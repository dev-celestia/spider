//! Port of the reference crawler `pkg/engine/common` — the shared crawl engine: queue-based
//! worker pool, scope/filter pipeline, rate limiting, dedup, and output.

pub mod ratelimit;
pub mod scope;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dashmap::DashMap;

use crate::control::CrawlControl;
use crate::engine::parser::{self, ParserOptions};
use crate::output::{log, LogLevel, StandardWriter};
use crate::types::options::{KnownFiles, Options};
use crate::types::result::{now_rfc3339, Request, Response};
use crate::utils::extensions::ExtensionValidator;
use crate::utils::filters::{fingerprint_url, PathTrie, SimpleFilter, SimilarityIndex};
use crate::utils::formfill::{self, FormFillData, FormField, FormInput, FormSelect, FormTextArea};
use crate::utils::knownfiles;
use crate::utils::queue::{priority_rank, Item as QueueItem, Queue};
use crate::utils::techdetect;

use ratelimit::Pacer;
use scope::ScopeManager;

/// Fetcher abstraction over engines (standard HTTP / headless / hybrid).
#[async_trait]
pub trait PageFetch: Send + Sync {
    async fn fetch(&self, request: &Request) -> std::result::Result<Response, String>;
}

/// Shared crawl configuration and state (reference crawler `common.Shared`).
pub struct Crawler {
    pub options: Arc<Options>,
    pub scope: ScopeManager,
    pub extension_validator: ExtensionValidator,
    pub pacer: Arc<Pacer>,
    pub writer: Arc<StandardWriter>,
    pub parser_opts: ParserOptions,
    pub form_data: FormFillData,
    /// Custom field extraction rules (`-flc` + default email field).
    pub field_configs: Vec<crate::utils::fieldconfig::CompiledFieldConfig>,
    /// Unique URL/content filter shared across sessions (celestia global filter).
    pub simple_filter: Arc<SimpleFilter>,
    pub path_trie: Arc<Mutex<PathTrie>>,
    pub similarity: Arc<SimilarityIndex>,
    pub fetcher: Arc<dyn PageFetch>,
    /// Cooperative control: cancellation (ctrl-c or UI) and pause/resume.
    pub control: Arc<CrawlControl>,
    /// Internal stop flag set when the crawl completes or is cancelled.
    stop: Arc<AtomicBool>,
    /// Per-domain page counters for `max_domain_pages`.
    domain_pages: DashMap<String, usize>,
    /// Consecutive throttled responses per host for exponential backoff
    /// (reference crawler `Shared.hostBackoffs`).
    host_backoffs: DashMap<String, Arc<AtomicUsize>>,
    pub stats: Arc<CrawlStats>,
}

/// Aggregate crawl statistics.
#[derive(Debug, Default)]
pub struct CrawlStats {
    pub visited_urls: Mutex<Vec<String>>,
    pub failed: AtomicUsize,
    pub results: AtomicUsize,
    pub skipped: AtomicUsize,
}

/// Internal per-seed session (reference crawler `CrawlSession`).
struct Session {
    root_hostname: String,
    queue: Mutex<Queue>,
    inflight: AtomicUsize,
}

impl Crawler {
    /// Build a crawler from validated options and a fetcher engine.
    pub fn new(
        options: Arc<Options>,
        fetcher: Arc<dyn PageFetch>,
        writer: Arc<StandardWriter>,
        control: Arc<CrawlControl>,
    ) -> std::result::Result<Crawler, String> {
        let scope = ScopeManager::new(
            &options.scope,
            &options.out_of_scope,
            &options.field_scope,
            options.no_scope,
        )?;
        let extension_validator = ExtensionValidator::new(
            &options.extensions_match,
            &options.extension_filter,
            options.no_default_ext_filter,
        );
        let pacer = Pacer::from_options(
            options.rate_limit,
            options.rate_limit_minute,
            options.host_rate_limit,
            options.host_rate_limit_minute,
            options.delay,
        );
        let form_data = if options.form_config.is_empty() {
            FormFillData::default()
        } else {
            formfill::load_form_config(&options.form_config)?
        };
        // Custom field extraction: explicit `-flc` config, else the reference
        // crawler's default email field (output.go:100-103).
        let field_configs = if options.field_config.is_empty() {
            crate::utils::fieldconfig::default_field_configs()
        } else {
            crate::utils::fieldconfig::load_field_config(&options.field_config)?
        };
        let similarity_mode = options.page_content_similar_mode;
        let similarity_distance = options.page_content_similar_distance;
        let similarity_threshold = options.similarity_threshold;
        let similarity_budget = options.page_content_similar_budget;
        let filter_similar_threshold = options.filter_similar_threshold;
        let parser_opts = ParserOptions::from_options(&options);
        Ok(Crawler {
            options,
            scope,
            extension_validator,
            pacer: Arc::new(pacer),
            writer,
            parser_opts,
            form_data,
            field_configs,
            simple_filter: Arc::new(SimpleFilter::new()),
            path_trie: Arc::new(Mutex::new(PathTrie::new(filter_similar_threshold))),
            similarity: Arc::new(SimilarityIndex::new(
                similarity_mode,
                similarity_distance,
                similarity_threshold,
                similarity_budget,
            )),
            fetcher,
            control,
            stop: Arc::new(AtomicBool::new(false)),
            domain_pages: DashMap::new(),
            host_backoffs: DashMap::new(),
            stats: Arc::new(CrawlStats::default()),
        })
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst) || self.control.is_cancelled()
    }

    /// Crawl a single seed URL to completion (reference crawler `Crawler.Crawl`).
    pub async fn crawl(self: &Arc<Self>, seed: &str) -> std::result::Result<(), String> {
        let seed = seed.trim();
        let parsed =
            url::Url::parse(seed).map_err(|e| format!("invalid seed url `{seed}`: {e}"))?;
        let root_hostname = parsed.host_str().unwrap_or("").to_string();

        let strategy = self.options.strategy;
        let mut queue = Queue::new(strategy);
        let seed_item = QueueItem {
            method: "GET".to_string(),
            url: parsed.to_string(),
            body: String::new(),
            depth: 0,
            priority: priority_rank(0),
            skip_validation: true,
            custom_fields: Default::default(),
        };
        // The seed is pushed directly without consuming uniqueness (reference
        // crawler `queue.Push(..., SkipValidation: true)`), so a self-link on
        // the seed page is still crawled and path-climbed.
        queue.push(seed_item);

        let session = Arc::new(Session {
            root_hostname,
            queue: Mutex::new(queue),
            inflight: AtomicUsize::new(0),
        });

        // Known-files crawling (robots.txt / sitemap.xml) enqueued at depth 2.
        if self.options.known_files != KnownFiles::None {
            self.enqueue_known_files(&session, &parsed);
        }

        log(
            LogLevel::Info,
            &format!("Started crawling for => {}", parsed),
        );

        let start = Instant::now();
        let duration_limit = self.options.crawl_duration;
        let concurrency = self.options.concurrency.max(1);

        let mut workers = Vec::new();
        for _ in 0..concurrency {
            let crawler = Arc::clone(self);
            let session = Arc::clone(&session);
            workers.push(tokio::spawn(async move {
                loop {
                    if crawler.stopped() {
                        return;
                    }
                    // UI pause: workers idle here until resumed or cancelled.
                    crawler.control.wait_if_paused().await;
                    if crawler.stopped() {
                        return;
                    }

                    let item = session.queue.lock().unwrap().pop();
                    match item {
                        Some(item) => {
                            session.inflight.fetch_add(1, Ordering::SeqCst);
                            crawler.process_item(&session, item).await;
                            session.inflight.fetch_sub(1, Ordering::SeqCst);
                        }
                        None => {
                            if session.inflight.load(Ordering::SeqCst) == 0 {
                                return;
                            }
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    }
                }
            }));
        }

        // Supervisor: stop on cancellation / duration expiry / natural drain.
        loop {
            if self.control.is_cancelled() {
                log(LogLevel::Info, "Cancellation requested, stopping crawl");
                break;
            }
            self.control.wait_if_paused().await;
            if !duration_limit.is_zero() && start.elapsed() > duration_limit {
                log(LogLevel::Info, "Crawl duration expired, stopping crawl");
                break;
            }
            if session.queue.lock().unwrap().is_empty()
                && session.inflight.load(Ordering::SeqCst) == 0
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        self.stop.store(true, Ordering::SeqCst);
        for w in workers {
            let _ = w.await;
        }
        Ok(())
    }

    /// Enqueue known files (robots.txt / sitemap.xml) for the session root.
    fn enqueue_known_files(&self, session: &Arc<Session>, seed: &url::Url) {
        // Mirror the seed's scheme and authority (host[:port]).
        let base = format!(
            "{}://{}",
            seed.scheme(),
            seed.authority() // includes credentials/port if present
        );
        let mode = match self.options.known_files {
            KnownFiles::RobotsTxt => knownfiles::KnownFilesMode::RobotsTxt,
            KnownFiles::SitemapXml => knownfiles::KnownFilesMode::SitemapXml,
            _ => knownfiles::KnownFilesMode::All,
        };
        for u in knownfiles::known_file_urls(&base, mode) {
            session.queue.lock().unwrap().push(QueueItem {
                method: "GET".into(),
                url: u,
                body: String::new(),
                depth: 2,
                priority: priority_rank(2),
                skip_validation: false,
                custom_fields: Default::default(),
            });
        }
    }

    /// Process one queue item: fetch, parse, filter, enqueue children, emit output.
    async fn process_item(&self, session: &Arc<Session>, item: QueueItem) {
        let options = &self.options;

        if self.stopped() {
            return;
        }
        if options.max_depth > 0 && item.depth > options.max_depth {
            return;
        }

        let parsed = match url::Url::parse(&item.url) {
            Ok(u) => u,
            Err(_) => {
                if let Some(cb) = &options.on_skip_url {
                    cb(&item.url);
                }
                return;
            }
        };
        let host = parsed.host_str().unwrap_or("").to_string();

        // Dequeue-time extension validation (reference crawler Do →
        // ValidatePath). Known-files URLs are fetched by a dedicated client
        // outside the queue in the reference crawler, so they are exempt.
        let is_known_file = options.known_files != KnownFiles::None && {
            let p = parsed.path().to_lowercase();
            p.ends_with("/robots.txt") || p.ends_with("/sitemap.xml")
        };
        if !is_known_file && !self.extension_validator.validate_path(&item.url) {
            self.stats.skipped.fetch_add(1, Ordering::SeqCst);
            if let Some(cb) = &options.on_skip_url {
                cb(&item.url);
            }
            return;
        }

        // Dequeue-time scope validation (reference crawler Do → ValidateScope);
        // seeds skip it via `skip_validation`.
        if !item.skip_validation && !self.scope.validate(&parsed, &session.root_hostname) {
            self.stats.skipped.fetch_add(1, Ordering::SeqCst);
            return;
        }

        // Per-domain page cap (-mdp).
        if options.max_domain_pages > 0 {
            let mut count = self.domain_pages.entry(host.clone()).or_insert(0);
            if *count >= options.max_domain_pages {
                return;
            }
            *count += 1;
        }

        // Host backoff for previously throttled responses (429/503), then
        // rate limiting (-rl/-rlm/-hrl/-hrlm/-rd).
        self.apply_backoff(&host).await;
        self.pacer.wait(&host).await;

        let mut request = Request {
            method: item.method.clone(),
            url: item.url.clone(),
            body: item.body.clone(),
            depth: item.depth,
            root_hostname: session.root_hostname.clone(),
            custom_fields: item.custom_fields.clone(),
            ..Default::default()
        };

        // Fetch with retries (celestia retryablehttp).
        let mut fetch_result: std::result::Result<Response, String> = Err("not attempted".into());
        for attempt in 0..=options.retries.max(0) {
            if self.stopped() {
                return;
            }
            fetch_result = self.fetcher.fetch(&request).await;
            if fetch_result.is_ok() {
                break;
            }
            if attempt < options.retries.max(0) {
                log(
                    LogLevel::Debug,
                    &format!("retrying {} (attempt {})", item.url, attempt + 1),
                );
                tokio::time::sleep(Duration::from_millis(250 * (attempt as u64 + 1))).await;
            }
        }

        let mut response = match fetch_result {
            Ok(r) => {
                // Track throttle signals for host backoff (reference crawler
                // RecordThrottle / RecordSuccess).
                if r.status_code == 429 || r.status_code == 503 {
                    self.record_throttle(&host);
                } else {
                    self.record_success(&host);
                }
                r
            }
            Err(err) => {
                self.stats.failed.fetch_add(1, Ordering::SeqCst);
                self.writer.write_error(&item.url, &request.source, &err);
                log(LogLevel::Warning, &format!("failed to fetch {}: {}", item.url, err));
                let mut result = crate::types::result::Result {
                    timestamp: now_rfc3339(),
                    request: Some(request),
                    response: None,
                    error: err,
                };
                self.emit_result(&mut result);
                return;
            }
        };

        // Disable-redirects: redirect responses are output but not parsed
        // further (reference crawler base.go DisableRedirects check).
        let is_redirect = response.is_redirect();

        // Exact content dedup (-duf disables).
        if !options.disable_unique_filter
            && !self.simple_filter.unique_content(response.body.as_bytes())
        {
            self.stats.skipped.fetch_add(1, Ordering::SeqCst);
            return;
        }

        // Page content similarity (-pcs/-sdd with -pcsm/-pcst/-pcsn).
        if options.content_similarity_enabled() && self.similarity.is_similar(&item.url, &response.body)
        {
            self.stats.skipped.fetch_add(1, Ordering::SeqCst);
            return;
        }

        // Tech detection (-td).
        if options.tech_detect {
            response.technologies = techdetect::detect_technologies(&response);
        }

        // Knowledge base analysis (-kb / -kb-secrets / -kb-endpoints).
        if options.knowledge_base || options.secrets || options.endpoints {
            response.knowledge_base = Some(crate::utils::knowledgebase::analyze(&response));
        }

        // Parse the document once for navigation extraction and form metadata.
        let document = scraper::Html::parse_document(&response.body);

        // Form extraction (-fx).
        if options.form_extraction {
            response.forms = parser::extract_forms(&document);
        }

        // Navigation extraction (header + body parsers).
        let mut navs = parser::parse_html(&response, &document, &self.parser_opts);

        // Custom field regex extraction (-flc + default email field): matched
        // values are attached to a navigation request for the response URL.
        let custom_fields = crate::utils::fieldconfig::extract_custom_fields(
            &self.field_configs,
            &response.body,
            &response.headers,
        );
        if !custom_fields.is_empty() {
            // The extracted values ride along on this result's request as
            // well — the reference crawler emits them via a re-enqueued
            // request for the same URL, which content-dedup usually swallows.
            request.custom_fields.extend(custom_fields.clone());
            navs.push(Request {
                method: "GET".to_string(),
                url: response.source.clone(),
                depth: response.depth,
                custom_fields,
                ..Default::default()
            });
        }

        // Known-files parsing: robots.txt directives / sitemap.xml <loc> entries.
        if options.known_files != KnownFiles::None {
            let path = parsed.path().to_lowercase();
            if path.ends_with("/robots.txt") {
                navs.extend(knownfiles::parse_robots_txt(&response.source, &response.body));
            } else if path.ends_with("/sitemap.xml") {
                navs.extend(knownfiles::parse_sitemap_xml(&response.source, &response.body));
            }
        }

        // JS endpoint scraping from standalone JS/CSS responses (-jc/-jsl).
        if options.scrape_js_responses || options.scrape_jsluice_responses {
            navs.extend(parser::parse_js_file(&response, &self.parser_opts));
        }

        // Whole-body endpoint scraping (-jr): textual bodies are scraped with
        // the page body regex (reference crawler bodyScrapeEndpointsParser).
        if options.scrape_js_responses {
            for endpoint in crate::utils::regex::extract_body_endpoints(&response.body) {
                let mut req = Request::from_response(
                    &endpoint,
                    &response.source,
                    "body-scrape",
                    "body-scrape",
                    &response,
                );
                req.depth = response.depth;
                navs.push(req);
            }
        }

        // Automatic form fill (-aff).
        if options.automatic_form_fill {
            navs.extend(self.build_form_requests(&response, &document));
        }

        // Captured XHR requests (-xhr) and onclick-derived URLs from headless
        // rendering are enqueued for crawling (reference crawler enqueues both).
        navs.extend(response.xhr_requests.iter().cloned());
        for link in &response.onclick_links {
            navs.push(Request {
                method: "GET".to_string(),
                url: link.clone(),
                source: response.source.clone(),
                tag: "onclick".into(),
                depth: response.depth,
                ..Default::default()
            });
        }

        self.stats.visited_urls.lock().unwrap().push(item.url.clone());

        let mut result = crate::types::result::Result {
            timestamp: now_rfc3339(),
            request: Some(request),
            response: Some(response),
            error: String::new(),
        };

        if self.emit_result(&mut result) {
            self.stats.results.fetch_add(1, Ordering::SeqCst);
        } else {
            self.stats.skipped.fetch_add(1, Ordering::SeqCst);
        }

        // Enqueue discovered navigations (subject to scope/filters).
        if is_redirect {
            return;
        }
        if options.max_depth <= 0 || item.depth < options.max_depth {
            for nav in navs {
                self.enqueue_navigation(&session, &parsed, item.depth, nav);
            }
        }
    }

    /// Emit a result through the writer and the user callback. Returns false
    /// when the writer filtered the result out (no output produced).
    fn emit_result(&self, result: &mut crate::types::result::Result) -> bool {
        let wrote = self.writer.write(result).is_ok();
        if wrote {
            if let Some(cb) = &self.options.on_result {
                cb(result);
            }
        }
        wrote
    }

    /// Sleep when a host has accumulated throttle signals
    /// (reference crawler `ApplyBackoff`: exponential 1s→30s with jitter).
    async fn apply_backoff(&self, host: &str) {
        let consecutive = match self.host_backoffs.get(host) {
            Some(v) => v.load(Ordering::SeqCst),
            None => return,
        };
        if consecutive == 0 {
            return;
        }
        let delay_secs = (1.0f64 * 2f64.powi((consecutive as i32 - 1) as i32)).min(30.0);
        let jitter = delay_secs * 0.5 * rand_jitter();
        tokio::time::sleep(Duration::from_secs_f64(delay_secs + jitter)).await;
    }

    fn record_throttle(&self, host: &str) {
        let entry = self.host_backoffs.entry(host.to_string()).or_default();
        entry.fetch_add(1, Ordering::SeqCst);
    }

    fn record_success(&self, host: &str) {
        if let Some(v) = self.host_backoffs.get(host) {
            if v.load(Ordering::SeqCst) > 0 {
                v.fetch_sub(1, Ordering::SeqCst);
            }
        }
    }

    /// Enqueue a discovered navigation after the reference crawler's filter
    /// pipeline (reference crawler `Shared.Enqueue`): logout skip, depth check
    /// (uniqueness intentionally not consumed), uniqueness (bypassed for
    /// custom-field items), cycle detection, scope, push, then path climb.
    fn enqueue_navigation(
        &self,
        session: &Arc<Session>,
        base: &url::Url,
        depth: i32,
        mut nav: Request,
    ) {
        let options = &self.options;
        let child_depth = depth + 1;

        if nav.url.is_empty() {
            if let Some(cb) = &options.on_skip_url {
                cb(&nav.url);
            }
            return;
        }
        // Resolve relative URLs against the base.
        match base.join(&nav.url) {
            Ok(mut joined) => {
                joined.set_fragment(None);
                if options.ignore_query_params {
                    joined.set_query(None);
                }
                nav.url = joined.to_string();
            }
            Err(_) => {
                if let Some(cb) = &options.on_skip_url {
                    cb(&nav.url);
                }
                return;
            }
        }

        // Logout URLs are never crawled when auto-login is active
        // (reference crawler isLogoutURL).
        if !options.auth_credentials.is_empty() && is_logout_url(&nav.url) {
            return;
        }

        // When maximum depth is exceeded, output discovered URLs without
        // enqueuing them. Uniqueness is intentionally not consumed here so
        // that URLs can still be visited if later discovered at a valid
        // depth via another path (reference crawler base.go).
        if options.max_depth > 0 && child_depth > options.max_depth {
            let mut result = crate::types::result::Result {
                timestamp: now_rfc3339(),
                request: Some(nav),
                response: None,
                error: "max depth reached".to_string(),
            };
            self.emit_result(&mut result);
            return;
        }

        // Cycle detection.
        if SimpleFilter::is_cycle(&nav.url) {
            return;
        }

        // Unique URL filter (items with custom fields bypass the gate). With
        // -fsu the uniqueness key IS the structural fingerprint (reference
        // crawler fingerprints reqUrl before the uniqueness check).
        let mut key = visited_key(&nav.method, &nav.url, &nav.body, options.ignore_query_params);
        if options.filter_similar {
            let mut trie = self.path_trie.lock().unwrap();
            key = fingerprint_url(&nav.url, &mut trie);
        }
        if !self.simple_filter.unique_url(&key) && nav.custom_fields.is_empty() {
            return;
        }

        let _ = &options;

        // Scope validation (-cs/-cos/-fs/-ns): out-of-scope URLs are sent to
        // output without visiting when -do is set.
        let nav_url_string = nav.url.clone();
        match url::Url::parse(&nav.url) {
            Ok(u) => {
                if !self.scope.validate(&u, &session.root_hostname) {
                    if options.display_out_scope {
                        let mut result = crate::types::result::Result {
                            timestamp: now_rfc3339(),
                            request: Some(nav),
                            response: None,
                            error: "url out of scope".to_string(),
                        };
                        self.emit_result(&mut result);
                    }
                    self.stats.skipped.fetch_add(1, Ordering::SeqCst);
                    if let Some(cb) = &options.on_skip_url {
                        cb(&nav_url_string);
                    }
                    return;
                }
            }
            Err(_) => return,
        }

        session.queue.lock().unwrap().push(QueueItem {
            method: if nav.method.is_empty() { "GET".to_string() } else { nav.method.clone() },
            url: nav.url.clone(),
            body: nav.body.clone(),
            depth: child_depth,
            priority: priority_rank(child_depth),
            skip_validation: false,
            custom_fields: nav.custom_fields.clone(),
        });

        // Path climb (-pc): parent paths of every enqueued URL, one level
        // shallower than the URL that discovered them (reference crawler
        // ExtractParentPaths with parentDepth-1).
        if options.path_climb {
            let Ok(nav_url) = url::Url::parse(&nav.url) else { return };
            let path = nav_url.path();
            if path.is_empty() || path == "/" {
                return;
            }
            let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
            for i in 1..segments.len() {
                let ancestor = format!("/{}/", segments[..i].join("/"));
                let Ok(parent) = nav_url.join(&ancestor) else { continue };
                let parent_url = parent.to_string();
                if !self.simple_filter.unique_url(&parent_url) {
                    continue;
                }
                if !self.scope.validate(&parent, &session.root_hostname) {
                    continue;
                }
                let parent_depth = if child_depth > 0 { child_depth - 1 } else { 0 };
                session.queue.lock().unwrap().push(QueueItem {
                    method: if nav.method.is_empty() { "GET".to_string() } else { nav.method.clone() },
                    url: parent_url,
                    body: String::new(),
                    depth: parent_depth,
                    priority: priority_rank(parent_depth),
                    skip_validation: false,
                    custom_fields: Default::default(),
                });
            }
        }
    }

    /// Build form fill GET/POST navigation requests (reference crawler `bodyFormTagParser` + `-aff`).
    fn build_form_requests(&self, response: &Response, document: &scraper::Html) -> Vec<Request> {
        use scraper::Selector;
        let mut requests = Vec::new();
        let Ok(form_sel) = Selector::parse("form") else { return requests };
        let Ok(field_sel) = Selector::parse("input, select, textarea") else {
            return requests;
        };

        for form in document.select(&form_sel) {
            let method = form.value().attr("method").unwrap_or("GET").to_uppercase();
            let action = form.value().attr("action").unwrap_or("").to_string();
            let enctype = form
                .value()
                .attr("enctype")
                .unwrap_or("application/x-www-form-urlencoded")
                .to_string();

            let action_url = response.absolute_url(&action);
            if action_url.is_empty() {
                continue;
            }

            let mut fields: Vec<FormField> = Vec::new();
            for el in form.select(&field_sel) {
                let name = el.value().attr("name").unwrap_or("").to_string();
                match el.value().name() {
                    "select" => {
                        let options_list: Vec<(String, bool)> = el
                            .select(&Selector::parse("option").unwrap())
                            .map(|o| {
                                (
                                    o.value().attr("value").unwrap_or("").to_string(),
                                    o.value().attr("selected").is_some(),
                                )
                            })
                            .collect();
                        fields.push(FormField::Select(FormSelect { name, options: options_list }));
                    }
                    "textarea" => {
                        fields.push(FormField::TextArea(FormTextArea { name }));
                    }
                    _ => {
                        let attrs = el.value();
                        fields.push(FormField::Input(FormInput {
                            input_type: attrs.attr("type").unwrap_or("text").to_lowercase(),
                            name,
                            value: attrs.attr("value").unwrap_or("").to_string(),
                            placeholder: attrs.attr("placeholder").unwrap_or("").to_string(),
                            min: attrs.attr("min").and_then(|v| v.parse().ok()),
                            max: attrs.attr("max").and_then(|v| v.parse().ok()),
                            step: attrs.attr("step").and_then(|v| v.parse().ok()),
                        }));
                    }
                }
            }
            let suggestions = formfill::form_fill_suggestions(&fields, &self.form_data);
            if suggestions.is_empty() {
                continue;
            }

            let body = suggestions
                .iter()
                .map(|(k, v)| format!("{k}={}", encode_query(v)))
                .collect::<Vec<_>>()
                .join("&");

            let mut req =
                Request::from_response(&action_url, &response.source, "form", "action", response);
            req.method = method;
            match req.method.as_str() {
                "GET" => {
                    let sep = if req.url.contains('?') { '&' } else { '?' };
                    req.url = format!("{}{}{}", req.url, sep, body);
                }
                "POST" => {
                    req.body = body;
                    // Lowercase header keys (reference crawler marshals
                    // header keys lowercased).
                    req.headers.insert("content-type".to_string(), enctype);
                }
                _ => {}
            }
            requests.push(req);
        }
        requests
    }
}

/// Dedup key for a navigation (reference crawler `Request.RequestURL`, plus
/// `-iqp` query stripping).
fn visited_key(method: &str, url: &str, body: &str, ignore_query_params: bool) -> String {
    let url = if ignore_query_params {
        url::Url::parse(url)
            .ok()
            .map(|mut u| {
                u.set_query(None);
                u.to_string()
            })
            .unwrap_or_else(|| url.to_string())
    } else {
        url.to_string()
    };
    if method == "POST" {
        format!("{url}:{body}")
    } else {
        url
    }
}

/// Ancestor-path climb requests for `-pc` (moved into `enqueue_navigation`
/// to mirror the reference crawler's per-URL parent extraction).
fn encode_query(v: &str) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    utf8_percent_encode(v, NON_ALPHANUMERIC).to_string()
}

/// Logout URL detection when auto-login is active
/// (reference crawler `logoutURLPattern`).
fn is_logout_url(url: &str) -> bool {
    let url = url.to_lowercase();
    for kw in [
        "logout", "log-out", "log_out", "signout", "sign-out", "sign_out", "deconnexion",
        "cerrar sesion", "abmelden", "uitloggen", "ausloggen", "disconnect", "end-session",
        "end session", "wyloguj", "sign-off",
    ] {
        if url.contains(kw) {
            return true;
        }
    }
    false
}

/// Small random jitter fraction in [0, 1) for backoff (no external rng dep).
fn rand_jitter() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 1000) as f64 / 1000.0
}

/// JSON context for DSL conditions.
pub fn result_context(request: &Request, response: &Response) -> serde_json::Value {
    serde_json::json!({
        "url": request.url,
        "method": request.method,
        "tag": request.tag,
        "attribute": request.attribute,
        "source": request.source,
        "depth": request.depth,
        "status_code": response.status_code,
        "content_length": response.content_length,
        "body": response.body,
        "words": response.body.split_whitespace().count(),
        "lines": response.body.lines().count(),
        "technologies": response.technologies,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::page_type_filtered;

    #[test]
    fn test_page_type_filter() {
        let mut r = Response::default();
        r.status_code = 404;
        r.body = "Page Not Found".into();
        assert!(page_type_filtered(&r, &["error".to_string()]));
        assert!(!page_type_filtered(&r, &["captcha".to_string()]));
        assert!(!page_type_filtered(&r, &[]));

        let mut r = Response::default();
        r.body = "verify you are human: recaptcha".into();
        assert!(page_type_filtered(&r, &["captcha".to_string()]));
    }

    #[test]
    fn test_result_context() {
        let ctx = result_context(
            &Request { url: "https://x.com".into(), ..Default::default() },
            &Response { status_code: 200, body: "a b\nc".into(), ..Default::default() },
        );
        assert_eq!(ctx["words"], 3);
        assert_eq!(ctx["lines"], 2);
        assert_eq!(ctx["status_code"], 200);
    }

    #[test]
    fn test_is_logout_url() {
        assert!(is_logout_url("https://x.com/logout"));
        assert!(is_logout_url("https://x.com/sign-out"));
        assert!(!is_logout_url("https://x.com/login"));
    }

    #[test]
    fn test_encode_query() {
        assert_eq!(encode_query("hello world"), "hello%20world");
    }

    #[test]
    fn test_visited_key() {
        assert_eq!(visited_key("GET", "https://x.com", "", false), "https://x.com");
        assert_eq!(visited_key("POST", "https://x.com", "a=1", false), "https://x.com:a=1");
        assert_eq!(
            visited_key("GET", "https://x.com?p=1", "", true),
            visited_key("GET", "https://x.com", "", true)
        );
    }
}
