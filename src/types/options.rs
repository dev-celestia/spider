//! Port of the reference crawler `pkg/types/options.go` — the complete crawler options surface.
//!
//! Field names, defaults, and semantics mirror the reference crawler 1:1 so the CLI flags and
//! library options behave identically.

use std::collections::HashMap;
use std::time::Duration;

/// Port of the reference crawler `queue.Strategy` — crawl queue visit strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Strategy {
    /// Breadth-first queue — shallowest items are visited first (priority = depth).
    #[default]
    BreadthFirst,
    /// Depth-first (LIFO) stack — follow a path as deep as possible first.
    DepthFirst,
}

impl Strategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Strategy::BreadthFirst => "breadth-first",
            Strategy::DepthFirst => "depth-first",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "breadth-first" => Ok(Strategy::BreadthFirst),
            "depth-first" => Ok(Strategy::DepthFirst),
            other => Err(format!(
                "invalid strategy: {other} (must be one of depth-first, breadth-first)"
            )),
        }
    }
}

/// Which known files (robots.txt / sitemap.xml) to crawl.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum KnownFiles {
    #[serde(rename = "none")]
    #[default]
    None,
    #[serde(rename = "all")]
    All,
    #[serde(rename = "robotstxt")]
    RobotsTxt,
    #[serde(rename = "sitemapxml")]
    SitemapXml,
}

impl KnownFiles {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "all" => Ok(KnownFiles::All),
            "robotstxt" => Ok(KnownFiles::RobotsTxt),
            "sitemapxml" => Ok(KnownFiles::SitemapXml),
            other => Err(format!(
                "invalid known-files value: {other} (must be one of all, robotstxt, sitemapxml)"
            )),
        }
    }
}

/// Page load wait strategy for headless crawling (reference crawler `-pls`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageLoadStrategy {
    /// Wait until the DOM settles (no size change) — celestia default.
    #[default]
    Heuristic,
    /// Wait for full `load` event.
    Load,
    /// Wait for `DOMContentLoaded` plus `dom_wait_time` seconds.
    DomContentLoaded,
    /// Wait until the network is idle for 500ms.
    NetworkIdle,
    /// No waiting beyond navigation.
    None,
}

impl PageLoadStrategy {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "heuristic" => Ok(PageLoadStrategy::Heuristic),
            "load" => Ok(PageLoadStrategy::Load),
            "domcontentloaded" => Ok(PageLoadStrategy::DomContentLoaded),
            "networkidle" => Ok(PageLoadStrategy::NetworkIdle),
            "none" => Ok(PageLoadStrategy::None),
            other => Err(format!(
                "invalid page-load-strategy: {other} (must be one of heuristic, load, domcontentloaded, networkidle, none)"
            )),
        }
    }
}

/// Similarity dedup mode (reference crawler `-pcsm`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SimilarityMode {
    #[default]
    SimHash,
    TfIdf,
    Bm25,
}

impl SimilarityMode {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "simhash" => Ok(SimilarityMode::SimHash),
            "tfidf" => Ok(SimilarityMode::TfIdf),
            "bm25" => Ok(SimilarityMode::Bm25),
            other => Err(format!(
                "invalid page-content-similar-mode: {other} (must be one of simhash, tfidf, bm25)"
            )),
        }
    }
}

/// Callback invoked on every crawl result (reference crawler `OnResultCallback`).
pub type OnResultCallback =
    Box<dyn Fn(&crate::types::result::Result) + Send + Sync>;
/// Callback invoked for every URL skipped by filters/scope (reference crawler `OnSkipURLCallback`).
pub type OnSkipURLCallback = Box<dyn Fn(&str) + Send + Sync>;

/// Complete crawler options — mirrors the reference crawler `types.Options` field-for-field.
#[derive(Default)]
pub struct Options {
    /// Enable anti-bot stealth for headless rendering (default true).
    pub stealth: bool,
    // ------------------------------------------------------------- input
    /// Target URLs to crawl (`-u` / `-list`); may also come from stdin.
    pub urls: Vec<String>,
    /// Resume the scan from a state file written by a previous run (`-resume`).
    pub resume: String,
    /// Exclude host matching specified filter ('cdn', 'private-ips', cidr, ip, regex) (`-e`).
    pub exclude: Vec<String>,

    // -------------------------------------------------------- configuration
    /// Maximum depth to crawl (`-d`, default 3).
    pub max_depth: i32,
    /// Enable endpoint parsing/crawling in JavaScript files (`-jc`).
    pub scrape_js_responses: bool,
    /// Enable jsluice-style JS endpoint parsing (`-jsl`, native approximation).
    pub scrape_jsluice_responses: bool,
    /// Maximum duration to crawl the target for (`-ct`).
    pub crawl_duration: Duration,
    /// Enable crawling of known files: all, robotstxt, sitemapxml (`-kf`).
    pub known_files: KnownFiles,
    /// Maximum response size to read in bytes (`-mrs`, default 4MB).
    pub body_read_size: usize,
    /// Time to wait for a request in seconds (`-timeout`, default 10).
    pub timeout: u64,
    /// Time to wait until the page is stable in seconds (`-time-stable`, default 1).
    pub time_stable: u64,
    /// Enable automatic form filling (experimental) (`-aff`).
    pub automatic_form_fill: bool,
    /// Extract form/input/textarea/select elements in jsonl output (`-fx`).
    pub form_extraction: bool,
    /// Number of times to retry a failed request (`-retry`, default 1).
    pub retries: i32,
    /// HTTP/SOCKS5 proxy to use (`-proxy`).
    pub proxy: String,
    /// Enable technology detection (`-td`).
    pub tech_detect: bool,
    /// Custom headers to add to every request, `Key: Value` (`-H`).
    pub custom_headers: HashMap<String, String>,
    /// Path to the celestia configuration file (`-config`).
    pub config_file: String,
    /// Path to custom form fill configuration file (`-fc`).
    pub form_config: String,
    /// Path to custom field configuration file (`-flc`).
    pub field_config: String,
    /// Visit strategy: depth-first or breadth-first (`-s`, default depth-first).
    pub strategy: Strategy,
    /// Ignore crawling same path with different query-param values (`-iqp`).
    pub ignore_query_params: bool,
    /// Filter crawling of similar-looking URLs (`-fsu`).
    pub filter_similar: bool,
    /// Distinct values before a path position is treated as a parameter (`-fst`, default 10).
    pub filter_similar_threshold: usize,
    /// Experimental TLS ClientHello randomization (`-tlsi`, accepted; best-effort).
    pub tls_impersonate: bool,
    /// Disable following redirects (`-dr`).
    pub disable_redirects: bool,
    /// Enable path climb — auto crawl parent paths (`-pc`).
    pub path_climb: bool,
    /// Enable knowledge base classification (`-kb`, native approximation).
    pub knowledge_base: bool,
    /// Enable knowledge base secrets extractor (`-kb-secrets`).
    pub secrets: bool,
    /// Validate detected secrets against their provider (`-kb-validate-secrets`).
    pub validate_secrets: bool,
    /// Enable endpoints extractor for REST/GraphQL/SOAP/XHR (`-kb-endpoints`).
    pub endpoints: bool,
    /// Maximum number of pages to crawl per domain, 0 = unlimited (`-mdp`).
    pub max_domain_pages: usize,

    // ------------------------------------------------------------- debug
    /// Run diagnostic check-up (`-hc`).
    pub health_check: bool,
    /// File to write request error log to (`-elog`).
    pub error_log: String,
    /// Enable pprof server (`-pprof-server`, no-op in Rust with a warning).
    pub pprof_server: bool,

    // ---------------------------------------------------------- headless
    /// Enable headless crawling (`-hl`).
    pub headless: bool,
    /// Enable headless hybrid crawling (`-hh`).
    pub headless_hybrid: bool,
    /// Use locally installed Chrome instead of managed download (`-sc`).
    pub use_installed_chrome: bool,
    /// Show the browser window in headless mode (`-sb`).
    pub show_browser: bool,
    /// Extra Chrome command-line arguments (`-ho`).
    pub headless_optional_arguments: Vec<String>,
    /// Start Chrome in `--no-sandbox` mode (`-nos`).
    pub headless_no_sandbox: bool,
    /// Path for Chrome `--user-data-dir` (`-cdd`).
    pub chrome_data_dir: String,
    /// Use the specified Chrome binary (`-scp`).
    pub system_chrome_path: String,
    /// Start Chrome without incognito mode (`-noi`).
    pub headless_no_incognito: bool,
    /// Attach to a running Chrome debugger at this WebSocket URL (`-cwu`).
    pub chrome_ws_url: String,
    /// Extract XHR request URL/method in jsonl output (`-xhr`).
    pub xhr_extraction: bool,
    /// Max consecutive action failures before stopping (`-mfc`, default 10).
    pub max_failure_count: i32,
    /// Maximum number of onclick links to process per page (`-ol`, default 10).
    pub max_onclick_links: i32,
    /// Enable diagnostics (`-ed`).
    pub enable_diagnostics: bool,
    /// Page load strategy (heuristic, load, domcontentloaded, networkidle, none) (`-pls`).
    pub page_load_strategy: PageLoadStrategy,
    /// Seconds to wait after DOMContentLoaded (`-dwt`, default 5).
    pub dom_wait_time: u64,
    /// CAPTCHA solver provider (e.g. capsolver) (`-csp`).
    pub captcha_solver_provider: String,
    /// CAPTCHA solver API key (`-csk`).
    pub captcha_solver_api_key: String,
    /// Automatic login credentials `username:password` (`-al`).
    pub auth_credentials: String,

    // ------------------------------------------------------------- scope
    /// In-scope URL regexes to follow (`-cs`).
    pub scope: Vec<String>,
    /// Out-of-scope URL regexes to exclude (`-cos`).
    pub out_of_scope: Vec<String>,
    /// Pre-defined scope field (dn, rdn, fqdn) or custom regex (`-fs`, default rdn).
    pub field_scope: String,
    /// Disable host-based default scope (`-ns`).
    pub no_scope: bool,
    /// Display external endpoints from scoped crawling (`-do`).
    pub display_out_scope: bool,

    // ------------------------------------------------------------ filter
    /// Regex(es) to match output URLs (`-mr`).
    pub output_match_regex: Vec<String>,
    /// Regex(es) to filter output URLs (`-fr`).
    pub output_filter_regex: Vec<String>,
    /// Fields to display in output (`-f`, deprecated in favor of `-ot`).
    pub fields: String,
    /// Fields to store in separate per-host files (`-sf`).
    pub store_fields: String,
    /// Match output for the given extensions (`-em`).
    pub extensions_match: Vec<String>,
    /// Filter output for the given extensions (`-ef`).
    pub extension_filter: Vec<String>,
    /// Remove default extensions from the filter list (`-ndef`).
    pub no_default_ext_filter: bool,
    /// Match response with a DSL condition (`-mdc`).
    pub output_match_condition: String,
    /// Filter response with a DSL condition (`-fdc`).
    pub output_filter_condition: String,
    /// Disable duplicate content filtering (`-duf`).
    pub disable_unique_filter: bool,
    /// Enable page content similarity filtering after exact dedup (`-pcs`).
    pub page_content_similar: bool,
    /// Deprecated alias for `page_content_similar` (`-sdd`).
    pub similarity_deduplication: bool,
    /// Similarity mode: simhash, tfidf, or bm25 (`-pcsm`).
    pub page_content_similar_mode: SimilarityMode,
    /// SimHash max Hamming distance (`-pcsd`, default 3).
    pub page_content_similar_distance: u32,
    /// TF-IDF/BM25 min score 0-1 (`-pcst`, default "0.85").
    pub page_content_similar_threshold: String,
    /// Pages to fully process per similarity cluster (`-pcsn`, default 1).
    pub page_content_similar_budget: usize,
    /// Filter response by page type (error, captcha, parked) (`-fpt`).
    pub filter_page_type: Vec<String>,

    // -------------------------------------------------------- ratelimit
    /// Number of concurrent fetchers (`-c`, default 10).
    pub concurrency: usize,
    /// Number of concurrent inputs to process (`-p`, default 10).
    pub parallelism: usize,
    /// Request delay between each request in seconds (`-rd`).
    pub delay: u64,
    /// Maximum requests per second (`-rl`, default 150).
    pub rate_limit: usize,
    /// Maximum requests per minute (`-rlm`).
    pub rate_limit_minute: usize,
    /// Maximum requests per second per host (`-hrl`).
    pub host_rate_limit: usize,
    /// Maximum requests per minute per host (`-hrlm`).
    pub host_rate_limit_minute: usize,

    // ------------------------------------------------------------ output
    /// File to write output to (`-o`).
    pub output_file: String,
    /// Custom output template (`-ot`).
    pub output_template: String,
    /// Store HTTP requests/responses (`-sr`).
    pub store_response: bool,
    /// Store HTTP requests/responses to a custom directory (`-srd`).
    pub store_response_dir: String,
    /// Do not overwrite an existing output file (`-ncb`).
    pub no_clobber: bool,
    /// Store per-host fields to a custom directory (`-sfd`).
    pub store_field_dir: String,
    /// Omit raw requests/responses from jsonl output (`-or`).
    pub omit_raw: bool,
    /// Omit the response body from jsonl output (`-ob`).
    pub omit_body: bool,
    /// List available output fields and exit (`-lof`).
    pub list_output_fields: bool,
    /// Exclude fields from jsonl output (`-eof`).
    pub exclude_output_fields: Vec<String>,
    /// Write output in jsonl format (`-j`).
    pub json: bool,
    /// Output page content as Markdown instead of URLs/HTML (`-md`).
    pub markdown: bool,
    /// Disable output coloring / ANSI codes (`-nc`).
    pub no_colors: bool,
    /// Display output only (`-silent`).
    pub silent: bool,
    /// Display verbose output (`-v`).
    pub verbose: bool,
    /// Display debug output (`-debug`).
    pub debug: bool,
    /// Show crawler version (`-version`).
    pub version: bool,
    /// Update the crawler to the latest version (`-up`; no-op in the Rust build).
    pub update: bool,
    /// Disable the automatic update check (`-duc`).
    pub disable_update_check: bool,

    // ---------------------------------------------------------- internal
    /// Compiled match regexes (from `output_match_regex`).
    pub match_regex: Vec<regex::Regex>,
    /// Compiled filter regexes (from `output_filter_regex`).
    pub filter_regex: Vec<regex::Regex>,
    /// Parsed similarity threshold (defaults to 0.85).
    pub similarity_threshold: f64,
    /// Read target URLs from stdin (no `-u` given, celestia stdin mode).
    pub urls_from_stdin: bool,
    /// Custom resolvers (accepted; enforced best-effort) (`-r`).
    pub resolvers: Vec<String>,
    /// Callback on each result.
    pub on_result: Option<OnResultCallback>,
    /// Callback on each skipped URL.
    pub on_skip_url: Option<OnSkipURLCallback>,
}

impl Clone for Options {
    fn clone(&self) -> Self {
        // Callbacks are non-Clone boxed closures; cloned options keep none.
        Options {
            urls: self.urls.clone(),
            resume: self.resume.clone(),
            exclude: self.exclude.clone(),
            max_depth: self.max_depth,
            scrape_js_responses: self.scrape_js_responses,
            scrape_jsluice_responses: self.scrape_jsluice_responses,
            crawl_duration: self.crawl_duration,
            known_files: self.known_files,
            body_read_size: self.body_read_size,
            timeout: self.timeout,
            time_stable: self.time_stable,
            automatic_form_fill: self.automatic_form_fill,
            form_extraction: self.form_extraction,
            retries: self.retries,
            proxy: self.proxy.clone(),
            tech_detect: self.tech_detect,
            custom_headers: self.custom_headers.clone(),
            config_file: self.config_file.clone(),
            form_config: self.form_config.clone(),
            field_config: self.field_config.clone(),
            strategy: self.strategy,
            ignore_query_params: self.ignore_query_params,
            filter_similar: self.filter_similar,
            filter_similar_threshold: self.filter_similar_threshold,
            tls_impersonate: self.tls_impersonate,
            disable_redirects: self.disable_redirects,
            path_climb: self.path_climb,
            knowledge_base: self.knowledge_base,
            secrets: self.secrets,
            validate_secrets: self.validate_secrets,
            endpoints: self.endpoints,
            max_domain_pages: self.max_domain_pages,
            health_check: self.health_check,
            error_log: self.error_log.clone(),
            pprof_server: self.pprof_server,
            headless: self.headless,
            headless_hybrid: self.headless_hybrid,
            use_installed_chrome: self.use_installed_chrome,
            show_browser: self.show_browser,
            headless_optional_arguments: self.headless_optional_arguments.clone(),
            headless_no_sandbox: self.headless_no_sandbox,
            chrome_data_dir: self.chrome_data_dir.clone(),
            system_chrome_path: self.system_chrome_path.clone(),
            headless_no_incognito: self.headless_no_incognito,
            chrome_ws_url: self.chrome_ws_url.clone(),
            xhr_extraction: self.xhr_extraction,
            max_failure_count: self.max_failure_count,
            max_onclick_links: self.max_onclick_links,
            enable_diagnostics: self.enable_diagnostics,
            page_load_strategy: self.page_load_strategy,
            dom_wait_time: self.dom_wait_time,
            captcha_solver_provider: self.captcha_solver_provider.clone(),
            captcha_solver_api_key: self.captcha_solver_api_key.clone(),
            auth_credentials: self.auth_credentials.clone(),
            scope: self.scope.clone(),
            out_of_scope: self.out_of_scope.clone(),
            field_scope: self.field_scope.clone(),
            no_scope: self.no_scope,
            display_out_scope: self.display_out_scope,
            output_match_regex: self.output_match_regex.clone(),
            output_filter_regex: self.output_filter_regex.clone(),
            fields: self.fields.clone(),
            store_fields: self.store_fields.clone(),
            extensions_match: self.extensions_match.clone(),
            extension_filter: self.extension_filter.clone(),
            no_default_ext_filter: self.no_default_ext_filter,
            output_match_condition: self.output_match_condition.clone(),
            output_filter_condition: self.output_filter_condition.clone(),
            disable_unique_filter: self.disable_unique_filter,
            page_content_similar: self.page_content_similar,
            similarity_deduplication: self.similarity_deduplication,
            page_content_similar_mode: self.page_content_similar_mode,
            page_content_similar_distance: self.page_content_similar_distance,
            page_content_similar_threshold: self.page_content_similar_threshold.clone(),
            page_content_similar_budget: self.page_content_similar_budget,
            filter_page_type: self.filter_page_type.clone(),
            concurrency: self.concurrency,
            parallelism: self.parallelism,
            delay: self.delay,
            rate_limit: self.rate_limit,
            rate_limit_minute: self.rate_limit_minute,
            host_rate_limit: self.host_rate_limit,
            host_rate_limit_minute: self.host_rate_limit_minute,
            output_file: self.output_file.clone(),
            output_template: self.output_template.clone(),
            store_response: self.store_response,
            store_response_dir: self.store_response_dir.clone(),
            no_clobber: self.no_clobber,
            store_field_dir: self.store_field_dir.clone(),
            omit_raw: self.omit_raw,
            omit_body: self.omit_body,
            list_output_fields: self.list_output_fields,
            exclude_output_fields: self.exclude_output_fields.clone(),
            json: self.json,
            markdown: self.markdown,
            no_colors: self.no_colors,
            silent: self.silent,
            verbose: self.verbose,
            debug: self.debug,
            version: self.version,
            update: self.update,
            disable_update_check: self.disable_update_check,
            match_regex: self.match_regex.clone(),
            filter_regex: self.filter_regex.clone(),
            similarity_threshold: self.similarity_threshold,
            urls_from_stdin: self.urls_from_stdin,
            resolvers: self.resolvers.clone(),
            stealth: self.stealth,
            // Non-Clone callbacks are dropped on clone (documented behavior).
            on_result: None,
            on_skip_url: None,
        }
    }
}

impl Options {
    /// Default options mirroring the reference crawler `types.DefaultOptions`.
    pub fn with_defaults() -> Self {
        Options {
            stealth: true,
            max_depth: 3,
            body_read_size: 4 * 1024 * 1024,
            timeout: 10,
            time_stable: 1,
            retries: 1,
            strategy: Strategy::DepthFirst,
            field_scope: "rdn".to_string(),
            concurrency: 10,
            parallelism: 10,
            rate_limit: 150,
            page_load_strategy: PageLoadStrategy::Heuristic,
            dom_wait_time: 5,
            max_failure_count: 10,
            max_onclick_links: 10,
            filter_similar_threshold: 10,
            page_content_similar_mode: SimilarityMode::SimHash,
            page_content_similar_distance: 3,
            page_content_similar_threshold: "0.85".to_string(),
            page_content_similar_budget: 1,
            ..Default::default()
        }
    }

    /// Content similarity Layer-2 enabled (reference crawler `ContentSimilarityEnabled`).
    pub fn content_similarity_enabled(&self) -> bool {
        self.page_content_similar || self.similarity_deduplication
    }

    /// Parse the similarity threshold string (reference crawler `PageContentSimilarThreshold`).
    pub fn similarity_threshold(&self) -> f64 {
        if self.page_content_similar_threshold.is_empty() {
            return 0.85;
        }
        match self.page_content_similar_threshold.parse::<f64>() {
            Ok(v) if v > 0.0 && v <= 1.0 => v,
            _ => 0.85,
        }
    }

    /// Whether the headless (browser) engine is active in any form.
    pub fn is_headless(&self) -> bool {
        self.headless || self.headless_hybrid
    }

    /// Validate options mirroring the reference crawler `validateOptions` in `internal/runner/options.go`.
    pub fn validate(&mut self) -> Result<(), String> {
        if self.max_depth <= 0 && self.crawl_duration.is_zero() {
            return Err("either max-depth or crawl-duration must be specified".to_string());
        }
        // Empty `urls` is allowed at validation time: URLs may still arrive via
        // stdin (celestia checks HasStdin in the runner).
        if self.headless && self.headless_hybrid {
            return Err("flags -hl (headless) and -hh (hybrid) are mutually exclusive".to_string());
        }
        // Hybrid + automatic form fill: force-disabled in the reference crawler
        // (form filling is handled via headless actions in the page context).
        if self.headless_hybrid && self.automatic_form_fill {
            self.automatic_form_fill = false;
            log_info("Automatic form fill (-aff) has been disabled for headless navigation.");
        }
        if !self.auth_credentials.is_empty() {
            if !self.auth_credentials.contains(':') {
                return Err("auth credentials must be in username:password format".to_string());
            }
            if !self.headless && !self.headless_hybrid {
                self.headless = true;
                log_info("Headless mode enabled automatically for authenticated crawling.");
            }
        }
        if (!self.headless_optional_arguments.is_empty()
            || self.headless_no_sandbox
            || !self.system_chrome_path.is_empty())
            && !self.headless
            && !self.headless_hybrid
        {
            return Err(
                "headless (-hl) or hybrid (-hh) mode is required if -ho, -nos or -scp are set"
                    .to_string(),
            );
        }
        if !self.system_chrome_path.is_empty() && !std::path::Path::new(&self.system_chrome_path).exists() {
            return Err("specified system chrome binary does not exist".to_string());
        }
        if !self.store_response_dir.is_empty() && !self.store_response {
            self.store_response = true;
        }
        for mr in &self.output_match_regex {
            let compiled = regex::Regex::new(mr)
                .map_err(|e| format!("Invalid value for match regex option: {e}"))?;
            self.match_regex.push(compiled);
        }
        for fr in &self.output_filter_regex {
            let compiled = regex::Regex::new(fr)
                .map_err(|e| format!("Invalid value for filter regex option: {e}"))?;
            self.filter_regex.push(compiled);
        }
        if self.known_files != KnownFiles::None && self.max_depth < 3 {
            self.max_depth = 3;
        }
        self.similarity_threshold = self.similarity_threshold();
        Ok(())
    }
}

/// Log an info line on native targets (no-op on wasm, where the output
/// module is not built).
pub fn log_info(msg: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    crate::output::log(crate::output::LogLevel::Info, msg);
    #[cfg(target_arch = "wasm32")]
    let _ = msg;
}

/// Parse `Key: Value` header strings into a map (reference crawler `ParseCustomHeaders`).
pub fn parse_custom_headers(headers: &[String]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for h in headers {
        if let Some((k, v)) = h.split_once(':') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

/// Parse a Go `time.Duration`-style string (`500ms`, `1h30m`, `30s`) plus the
/// celestia-specific `d` day suffix (`2d`). Empty input means no limit.
pub fn parse_go_duration(input: &str) -> Result<Duration, String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(Duration::ZERO);
    }
    let mut total = Duration::ZERO;
    let mut rest = input;
    while !rest.is_empty() {
        let digits = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(rest.len());
        if digits == 0 {
            return Err(format!("invalid duration: {input}"));
        }
        let value: f64 = rest[..digits]
            .parse()
            .map_err(|_| format!("invalid duration: {input}"))?;
        rest = &rest[digits..];
        let unit_end = rest
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or(rest.len());
        let unit = &rest[..unit_end];
        rest = &rest[unit_end..];
        let secs = match unit {
            "ns" => value / 1e9,
            "us" | "µs" => value / 1e6,
            "ms" => value / 1e3,
            "s" => value,
            "m" => value * 60.0,
            "h" => value * 3600.0,
            "d" => value * 86_400.0,
            _ => return Err(format!("invalid duration unit `{unit}` in: {input}")),
        };
        total += Duration::from_secs_f64(secs);
    }
    Ok(total)
}

/// Parse `--key=value` / bare `--key` chrome args into a map
/// (reference crawler `ParseHeadlessOptionalArguments`).
pub fn parse_headless_optional_arguments(args: &[String]) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();
    let mut last_key = String::new();
    for v in args {
        if v.is_empty() {
            continue;
        }
        if let Some((k, val)) = v.split_once('=') {
            map.insert(k.trim().to_string(), val.trim().to_string());
            last_key = k.trim().to_string();
        } else if !v.starts_with("--") {
            // Continuation of the previous key's value (e.g. --window-size / 500,700).
            let entry = map.entry(last_key.clone()).or_default();
            if entry.is_empty() {
                entry.push_str(v);
            } else {
                entry.push(',');
                entry.push_str(v);
            }
        } else {
            map.insert(v.clone(), String::new());
            last_key = v.clone();
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_match_celestia() {
        let o = Options::with_defaults();
        assert_eq!(o.max_depth, 3);
        assert_eq!(o.body_read_size, 4 * 1024 * 1024);
        assert_eq!(o.timeout, 10);
        assert_eq!(o.retries, 1);
        assert_eq!(o.strategy, Strategy::DepthFirst);
        assert_eq!(o.field_scope, "rdn");
        assert_eq!(o.concurrency, 10);
        assert_eq!(o.rate_limit, 150);
    }

    #[test]
    fn test_strategy_parse() {
        assert_eq!(Strategy::parse("breadth-first").unwrap(), Strategy::BreadthFirst);
        assert_eq!(Strategy::parse("depth-first").unwrap(), Strategy::DepthFirst);
        assert!(Strategy::parse("nope").is_err());
    }

    #[test]
    fn test_strategy_serde_matches_parse_strings() {
        // UI configs (Tauri IPC JSON) pass the same strings the CLI uses.
        let json = serde_json::to_string(&Strategy::BreadthFirst).unwrap();
        assert_eq!(json, r#""breadth-first""#);
        let back: Strategy = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Strategy::BreadthFirst);
    }

    #[test]
    fn test_parse_custom_headers() {
        let m = parse_custom_headers(&["A: b".into(), "c:d".into()]);
        assert_eq!(m.get("A").unwrap(), "b");
        assert_eq!(m.get("c").unwrap(), "d");
    }

    #[test]
    fn test_parse_headless_args() {
        let m = parse_headless_optional_arguments(&[
            "--user-agent=chrome".into(),
            "--window-size".into(),
            "500,700".into(),
        ]);
        assert_eq!(m.get("--user-agent").unwrap(), "chrome");
        assert_eq!(m.get("--window-size").unwrap(), "500,700");
    }

    #[test]
    fn test_validate_requires_depth_or_duration() {
        let mut o = Options::with_defaults();
        o.urls = vec!["https://example.com".into()];
        o.max_depth = 0;
        assert!(o.validate().is_err());

        let mut o = Options::with_defaults();
        o.urls = vec!["https://example.com".into()];
        o.max_depth = 0;
        o.crawl_duration = Duration::from_secs(10);
        assert!(o.validate().is_ok());
    }

    #[test]
    fn test_validate_known_files_bumps_depth() {
        let mut o = Options::with_defaults();
        o.urls = vec!["https://example.com".into()];
        o.max_depth = 2;
        o.known_files = KnownFiles::All;
        o.validate().unwrap();
        assert_eq!(o.max_depth, 3);
    }

    #[test]
    fn test_parse_go_duration() {
        assert_eq!(parse_go_duration("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_go_duration("1h30m").unwrap(), Duration::from_secs(5400));
        assert_eq!(parse_go_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_go_duration("2d").unwrap(), Duration::from_secs(172_800));
        assert_eq!(parse_go_duration("").unwrap(), Duration::ZERO);
        assert!(parse_go_duration("bogus").is_err());
    }

    #[test]
    fn test_validate_headless_only_options_require_headless() {
        let mut o = Options::with_defaults();
        o.urls = vec!["https://example.com".into()];
        o.headless_no_sandbox = true;
        assert!(o.validate().is_err());

        let mut o = Options::with_defaults();
        o.headless_no_sandbox = true;
        o.headless = true;
        assert!(o.validate().is_ok());
    }

    #[test]
    fn test_validate_auth_enables_headless() {
        let mut o = Options::with_defaults();
        o.auth_credentials = "user:pass".into();
        o.validate().unwrap();
        assert!(o.headless);
    }

    #[test]
    fn test_validate_hybrid_disables_form_fill() {
        let mut o = Options::with_defaults();
        o.headless_hybrid = true;
        o.automatic_form_fill = true;
        o.validate().unwrap();
        assert!(!o.automatic_form_fill);
    }
}
