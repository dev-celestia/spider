//! Port of the reference crawler `pkg/engine/common` — the shared crawl engine: queue-based
//! worker pool, scope/filter pipeline, rate limiting, dedup, and output.

pub mod ratelimit;
pub mod scope;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dashmap::DashMap;

use crate::engine::parser::{self, ParserOptions};
use crate::output::{log, LogLevel, StandardWriter};
use crate::types::options::{KnownFiles, Options};
use crate::types::result::{now_rfc3339, Request, Response};
use crate::utils::dsl;
use crate::utils::extensions::ExtensionValidator;
use crate::utils::filters::{PathTrie, SimpleFilter, SimilarityIndex};
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
    /// Unique URL/content filter shared across sessions (celestia global filter).
    pub simple_filter: Arc<SimpleFilter>,
    pub path_trie: Arc<Mutex<PathTrie>>,
    pub similarity: Arc<SimilarityIndex>,
    pub fetcher: Arc<dyn PageFetch>,
    /// Cooperative user cancellation (ctrl-c).
    pub cancel: Arc<AtomicBool>,
    /// Internal stop flag set when the crawl completes or is cancelled.
    stop: Arc<AtomicBool>,
    /// Per-domain page counters for `max_domain_pages`.
    domain_pages: DashMap<String, usize>,
    /// Path positions learned as variable by `-filter-similar`.
    variable_prefixes: Arc<Mutex<std::collections::HashSet<String>>>,
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
        cancel: Arc<AtomicBool>,
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
        let similarity_distance = options.page_content_similar_distance;
        let parser_opts = ParserOptions::from_options(&options);
        Ok(Crawler {
            options,
            scope,
            extension_validator,
            pacer: Arc::new(pacer),
            writer,
            parser_opts,
            form_data,
            simple_filter: Arc::new(SimpleFilter::new()),
            path_trie: Arc::new(Mutex::new(PathTrie::new())),
            similarity: Arc::new(SimilarityIndex::new(similarity_distance)),
            fetcher,
            cancel,
            stop: Arc::new(AtomicBool::new(false)),
            domain_pages: DashMap::new(),
            variable_prefixes: Arc::new(Mutex::new(std::collections::HashSet::new())),
            stats: Arc::new(CrawlStats::default()),
        })
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst) || self.cancel.load(Ordering::SeqCst)
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
        };
        let seed_key = visited_key(&seed_item.method, &seed_item.url, &seed_item.body, self.options.ignore_query_params);
        self.simple_filter.unique_url(&seed_key);
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
            if self.cancel.load(Ordering::SeqCst) {
                log(LogLevel::Info, "Cancellation requested, stopping crawl");
                break;
            }
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

        if let Some(path) = self.resume_path() {
            self.save_resume_state(&path, &session);
        }
        Ok(())
    }

    fn resume_path(&self) -> Option<String> {
        (!self.options.resume.is_empty()).then(|| self.options.resume.clone())
    }

    fn save_resume_state(&self, path: &str, session: &Session) {
        let queue = session.queue.lock().unwrap();
        let state = serde_json::json!({
            "queue": queue
                .items()
                .iter()
                .map(|i| serde_json::json!({
                    "method": i.method,
                    "url": i.url,
                    "body": i.body,
                    "depth": i.depth,
                }))
                .collect::<Vec<_>>(),
        });
        let _ = std::fs::write(path, serde_json::to_string(&state).unwrap_or_default());
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
            Err(_) => return,
        };
        let host = parsed.host_str().unwrap_or("").to_string();

        // Per-domain page cap (-mdp).
        if options.max_domain_pages > 0 {
            let mut count = self.domain_pages.entry(host.clone()).or_insert(0);
            if *count >= options.max_domain_pages {
                return;
            }
            *count += 1;
        }

        // Rate limiting (-rl/-rlm/-hrl/-hrlm/-rd).
        self.pacer.wait(&host).await;

        let request = Request {
            method: item.method.clone(),
            url: item.url.clone(),
            body: item.body.clone(),
            depth: item.depth,
            root_hostname: session.root_hostname.clone(),
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
            Ok(r) => r,
            Err(err) => {
                self.stats.failed.fetch_add(1, Ordering::SeqCst);
                self.writer.write_error(&item.url, &err);
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

        // Page-type filtering (-fpt error,captcha,parked).
        if page_type_filtered(&response, &options.filter_page_type) {
            self.stats.skipped.fetch_add(1, Ordering::SeqCst);
            return;
        }

        // Exact content dedup (-duf disables).
        if !options.disable_unique_filter
            && !self.simple_filter.unique_content(response.body.as_bytes())
        {
            self.stats.skipped.fetch_add(1, Ordering::SeqCst);
            return;
        }

        // Page content similarity (-pcs/-sdd).
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

        // Automatic form fill (-aff).
        if options.automatic_form_fill {
            navs.extend(self.build_form_requests(&response, &document));
        }

        // Path climb (-pc).
        if options.path_climb {
            navs.extend(path_climb_requests(&response, &parsed));
        }

        self.stats.results.fetch_add(1, Ordering::SeqCst);
        self.stats.visited_urls.lock().unwrap().push(item.url.clone());

        let mut result = crate::types::result::Result {
            timestamp: now_rfc3339(),
            request: Some(request),
            response: Some(response),
            error: String::new(),
        };

        // Output-level DSL match/filter conditions (-mdc/-fdc). These gate the
        // emitted result only — crawling of discovered navigations continues.
        let mut emit = true;
        if !options.output_match_condition.is_empty()
            || !options.output_filter_condition.is_empty()
        {
            let ctx = result_context(result.request.as_ref().unwrap(), result.response.as_ref().unwrap());
            if !options.output_match_condition.is_empty()
                && !dsl::eval_bool(&options.output_match_condition, &ctx).unwrap_or(false)
            {
                emit = false;
            }
            if emit
                && !options.output_filter_condition.is_empty()
                && dsl::eval_bool(&options.output_filter_condition, &ctx).unwrap_or(false)
            {
                emit = false;
            }
        }
        if emit {
            self.emit_result(&mut result);
        } else {
            self.stats.skipped.fetch_add(1, Ordering::SeqCst);
        }

        // Enqueue discovered navigations (subject to scope/filters).
        if options.max_depth <= 0 || item.depth < options.max_depth {
            for nav in navs {
                self.enqueue_navigation(&session, &parsed, item.depth, nav);
            }
        }
    }

    /// Emit a result through the writer and the user callback.
    fn emit_result(&self, result: &mut crate::types::result::Result) {
        self.writer.write(result);
        if let Some(cb) = &self.options.on_result {
            cb(result);
        }
    }

    /// Enqueue a discovered navigation after the reference crawler's filter pipeline.
    fn enqueue_navigation(
        &self,
        session: &Arc<Session>,
        base: &url::Url,
        depth: i32,
        mut nav: Request,
    ) {
        let options = &self.options;

        if nav.url.is_empty() {
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
            Err(_) => return,
        }

        let skip = |crawler: &Crawler, url: &str| {
            crawler.stats.skipped.fetch_add(1, Ordering::SeqCst);
            if options.display_out_scope {
                log(LogLevel::Info, &format!("[out-of-scope] {url}"));
            }
            if let Some(cb) = &options.on_skip_url {
                cb(url);
            }
        };

        // Scope validation (-cs/-cos/-fs/-ns).
        match url::Url::parse(&nav.url) {
            Ok(u) => {
                if !self.scope.validate(&u, &session.root_hostname) {
                    skip(self, &nav.url);
                    return;
                }
            }
            Err(_) => return,
        }

        // Match/filter regexes (-mr/-fr).
        if !options.match_regex.is_empty() && !options.match_regex.iter().any(|r| r.is_match(&nav.url))
        {
            skip(self, &nav.url);
            return;
        }
        if options.filter_regex.iter().any(|r| r.is_match(&nav.url)) {
            skip(self, &nav.url);
            return;
        }

        // Extension validation (-em/-ef/-ndef).
        if !self.extension_validator.validate_path(&nav.url) {
            skip(self, &nav.url);
            return;
        }

        // Cycle detection.
        if SimpleFilter::is_cycle(&nav.url) {
            return;
        }

        // Unique URL filter.
        let key = visited_key(&nav.method, &nav.url, &nav.body, options.ignore_query_params);
        if !self.simple_filter.unique_url(&key) {
            return;
        }

        // Filter-similar via path trie (-fsu/-fst).
        if options.filter_similar {
            let (normalized, collapsed_prefix) = {
                let mut trie = self.path_trie.lock().unwrap();
                let path = url::Url::parse(&nav.url)
                    .map(|u| u.path().to_string())
                    .unwrap_or_default();
                trie.normalize(&path, options.filter_similar_threshold)
            };
            let under_variable = {
                let mut vars = self.variable_prefixes.lock().unwrap();
                let path = url::Url::parse(&nav.url)
                    .map(|u| u.path().to_string())
                    .unwrap_or_default();
                // Skip URLs under already-known variable positions first, so
                // whichever sibling arrives first wins the position.
                let under = vars.iter().any(|p| {
                    path != *p && path.starts_with(&format!("{}/", p.trim_end_matches('/')))
                });
                // Learn newly-collapsed variable positions afterwards.
                if let Some(prefix) = collapsed_prefix {
                    vars.insert(prefix);
                }
                under
            };
            if under_variable {
                return;
            }
            if !self.simple_filter.unique_url(&format!("similar:{normalized}")) {
                return;
            }
        }

        let child_depth = depth + 1;
        if options.max_depth > 0 && child_depth > options.max_depth {
            return;
        }

        session.queue.lock().unwrap().push(QueueItem {
            method: if nav.method.is_empty() { "GET".to_string() } else { nav.method.clone() },
            url: nav.url.clone(),
            body: nav.body.clone(),
            depth: child_depth,
            priority: priority_rank(child_depth),
        });
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
                    req.headers.insert("Content-Type".to_string(), enctype);
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

/// Ancestor-path climb requests for `-pc`.
fn path_climb_requests(response: &Response, base: &url::Url) -> Vec<Request> {
    let path = base.path();
    if path.is_empty() || path == "/" {
        return Vec::new();
    }
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    let mut requests = Vec::new();
    for i in 1..segments.len() {
        let ancestor = format!("/{}/", segments[..i].join("/"));
        requests.push(Request::from_response(
            &ancestor,
            &response.source,
            "path-climb",
            "climb",
            response,
        ));
    }
    requests
}

fn encode_query(v: &str) -> String {
    use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
    utf8_percent_encode(v, NON_ALPHANUMERIC).to_string()
}

/// Page-type heuristics for `-fpt error,captcha,parked`.
fn page_type_filtered(response: &Response, filter: &[String]) -> bool {
    if filter.is_empty() {
        return false;
    }
    let body = response.body.to_lowercase();
    for f in filter {
        match f.as_str() {
            "error" => {
                if response.status_code >= 400
                    || body.contains("page not found")
                    || body.contains("internal server error")
                {
                    return true;
                }
            }
            "captcha" => {
                if body.contains("recaptcha")
                    || body.contains("hcaptcha")
                    || body.contains("turnstile")
                {
                    return true;
                }
            }
            "parked" => {
                if body.contains("domain is for sale") || body.contains("buy this domain") {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// JSON context for DSL conditions.
fn result_context(request: &Request, response: &Response) -> serde_json::Value {
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
    fn test_path_climb() {
        let resp = Response::default().with_source("https://x.com/a/b/c");
        let base = url::Url::parse("https://x.com/a/b/c").unwrap();
        let reqs = path_climb_requests(&resp, &base);
        assert_eq!(reqs.len(), 2);
        assert!(reqs[0].url.ends_with("/a/"));
        assert!(reqs[1].url.ends_with("/a/b/"));
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
