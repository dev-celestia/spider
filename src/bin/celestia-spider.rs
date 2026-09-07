//! The `celestia-spider` CLI binary, driving the Rust crawl engine.
//! driving the Rust crawl engine.

use std::time::Duration;

use clap::Parser;

use celestia_spider::output::configure_output;
use celestia_spider::runner::Runner;
use celestia_spider::types::options::{
    parse_custom_headers, KnownFiles, Options, PageLoadStrategy, SimilarityMode, Strategy,
};

#[derive(Parser, Debug)]
#[command(
    name = "celestia-spider",
    version = celestia_spider::runner::version(),
    about = "celestia-spider is a fast crawler for automation pipelines with headless and non-headless crawling.",
    long_about = None
)]
struct Cli {
    // ------------------------------------------------------------ input
    /// Target url / list to crawl
    #[arg(short = 'u', long = "list")]
    urls: Vec<String>,
    /// Resume scan using resume.cfg
    #[arg(long, default_value = "")]
    resume: String,
    /// Exclude host matching specified filter ('cdn', 'private-ips', cidr, ip, regex)
    #[arg(short = 'e', long)]
    exclude: Vec<String>,

    // ------------------------------------------------------ configuration
    /// List of custom resolver (file or comma separated)
    #[arg(short = 'r', long)]
    resolvers: Vec<String>,
    /// Maximum depth to crawl (default 3)
    #[arg(short = 'd', long, default_value_t = 3)]
    depth: i32,
    /// Enable endpoint parsing / crawling in javascript file
    #[arg(long = "js-crawl")]
    js_crawl: bool,
    /// Enable jsluice parsing in javascript file (memory intensive)
    #[arg(long = "jsluice")]
    jsluice: bool,
    /// Maximum duration to crawl the target for (s, m, h, d)
    #[arg(long = "crawl-duration", default_value = "")]
    crawl_duration: String,
    /// Enable crawling of known files (all, robotstxt, sitemapxml)
    #[arg(long = "known-files", default_value = "")]
    known_files: String,
    /// Maximum response size to read (default 4MB)
    #[arg(long = "max-response-size", default_value_t = 4 * 1024 * 1024)]
    max_response_size: usize,
    /// Time to wait for request in seconds (default 10)
    #[arg(long, default_value_t = 10)]
    timeout: u64,
    /// Time to wait until the page is stable in seconds (default 1)
    #[arg(long = "time-stable", default_value_t = 1)]
    time_stable: u64,
    /// Enable automatic form filling (experimental)
    #[arg(long = "automatic-form-fill")]
    automatic_form_fill: bool,
    /// Extract form, input, textarea & select elements in jsonl output
    #[arg(long = "form-extraction")]
    form_extraction: bool,
    /// Number of times to retry the request (default 1)
    #[arg(long, default_value_t = 1)]
    retry: i32,
    /// http/socks5 proxy to use
    #[arg(long, default_value = "")]
    proxy: String,
    /// Enable technology detection
    #[arg(long = "tech-detect")]
    tech_detect: bool,
    /// Custom header/cookie to include in all http request in header:value format (file)
    #[arg(short = 'H', long = "headers")]
    headers: Vec<String>,
    /// Path to the celestia-spider configuration file
    #[arg(long, default_value = "")]
    config: String,
    /// Path to custom form configuration file
    #[arg(long = "form-config", default_value = "")]
    form_config: String,
    /// Path to custom field configuration file
    #[arg(long = "field-config", default_value = "")]
    field_config: String,
    /// Visit strategy (depth-first, breadth-first) (default depth-first)
    #[arg(short = 's', long, default_value = "depth-first")]
    strategy: String,
    /// Build a DFS sitemap link tree (JSON) instead of crawling for results
    #[arg(long = "sitemap-tree")]
    sitemap_tree: bool,
    /// Ignore crawling same path with different query-param values
    #[arg(long = "ignore-query-params")]
    ignore_query_params: bool,
    /// Filter crawling of similar looking URLs (e.g., /users/123 and /users/456)
    #[arg(long = "filter-similar")]
    filter_similar: bool,
    /// Number of distinct values before a path position is treated as parameter (default 10)
    #[arg(long = "filter-similar-threshold", default_value_t = 10)]
    filter_similar_threshold: usize,
    /// Enable experimental client hello (ja3) tls randomization
    #[arg(long = "tls-impersonate")]
    tls_impersonate: bool,
    /// Disable following redirects (default false)
    #[arg(long = "disable-redirects")]
    disable_redirects: bool,
    /// Enable path climb (auto crawl parent paths)
    #[arg(long = "path-climb")]
    path_climb: bool,
    /// Enable knowledge base classification
    #[arg(long = "knowledge-base")]
    knowledge_base: bool,
    /// Enable secrets extractor in the knowledge base
    #[arg(long = "kb-secrets")]
    kb_secrets: bool,
    /// Validate detected secrets against their provider (sends live API calls)
    #[arg(long = "kb-validate-secrets")]
    kb_validate_secrets: bool,
    /// Enable endpoints extractor (classifies REST/GraphQL/SOAP/XHR requests)
    #[arg(long = "kb-endpoints")]
    kb_endpoints: bool,
    /// Maximum number of pages to crawl per domain (default unlimited)
    #[arg(long = "max-domain-pages", default_value_t = 0)]
    max_domain_pages: usize,

    // ------------------------------------------------------------- debug
    /// Run diagnostic check up
    #[arg(long = "health-check")]
    health_check: bool,
    /// File to write sent requests error log
    #[arg(long = "error-log", default_value = "")]
    error_log: String,
    /// Enable pprof server
    #[arg(long = "pprof-server")]
    pprof_server: bool,

    // ---------------------------------------------------------- headless
    /// Enable headless crawling (experimental)
    #[arg(long = "headless")]
    headless: bool,
    /// Enable headless hybrid crawling (experimental)
    #[arg(long = "hybrid")]
    hybrid: bool,
    /// Use local installed chrome browser instead of celestia installed
    #[arg(long = "system-chrome")]
    system_chrome: bool,
    /// Show the browser on the screen with headless mode
    #[arg(long = "show-browser")]
    show_browser: bool,
    /// Start headless chrome with additional options
    #[arg(long = "headless-options")]
    headless_options: Vec<String>,
    /// Start headless chrome in --no-sandbox mode
    #[arg(long = "no-sandbox")]
    no_sandbox: bool,
    /// Path to store chrome browser data
    #[arg(long = "chrome-data-dir", default_value = "")]
    chrome_data_dir: String,
    /// Use specified chrome browser for headless crawling
    #[arg(long = "system-chrome-path", default_value = "")]
    system_chrome_path: String,
    /// Start headless chrome without incognito mode
    #[arg(long = "no-incognito")]
    no_incognito: bool,
    /// Use chrome browser instance launched elsewhere with the debugger listening at this URL
    #[arg(long = "chrome-ws-url", default_value = "")]
    chrome_ws_url: String,
    /// Extract xhr request url,method in jsonl output
    #[arg(long = "xhr-extraction")]
    xhr_extraction: bool,
    /// Maximum number of consecutive action failures before stopping (default 10)
    #[arg(long = "max-failure-count", default_value_t = 10)]
    max_failure_count: i32,
    /// Enable diagnostics
    #[arg(long = "enable-diagnostics")]
    enable_diagnostics: bool,
    /// Page load strategy (heuristic, load, domcontentloaded, networkidle, none)
    #[arg(long = "page-load-strategy", default_value = "heuristic")]
    page_load_strategy: String,
    /// Time in seconds to wait after page load when using domcontentloaded strategy (default 5)
    #[arg(long = "dom-wait-time", default_value_t = 5)]
    dom_wait_time: u64,
    /// Captcha solver provider (e.g. capsolver)
    #[arg(long = "captcha-solver-provider", default_value = "")]
    captcha_solver_provider: String,
    /// Captcha solver provider api key
    #[arg(long = "captcha-solver-key", default_value = "")]
    captcha_solver_key: String,
    /// Automatic login with username:password (headless only)
    #[arg(long = "auto-login", default_value = "")]
    auto_login: String,
    /// Maximum number of onclick links to process per page (default 10)
    #[arg(long = "max-onclick-links", default_value_t = 10)]
    max_onclick_links: i32,

    // ------------------------------------------------------------- scope
    /// In scope url regex to be followed by crawler
    #[arg(long = "crawl-scope")]
    crawl_scope: Vec<String>,
    /// Out of scope url regex to be excluded by crawler
    #[arg(long = "crawl-out-scope")]
    crawl_out_scope: Vec<String>,
    /// Pre-defined scope field (dn,rdn,fqdn) or custom regex (default rdn)
    #[arg(long = "field-scope", default_value = "rdn")]
    field_scope: String,
    /// Disables host based default scope
    #[arg(long = "no-scope")]
    no_scope: bool,
    /// Display external endpoint from scoped crawling
    #[arg(long = "display-out-scope")]
    display_out_scope: bool,

    // ------------------------------------------------------------ filter
    /// Regex or list of regex to match on output url (cli, file)
    #[arg(long = "match-regex")]
    match_regex: Vec<String>,
    /// Regex or list of regex to filter on output url (cli, file)
    #[arg(long = "filter-regex")]
    filter_regex: Vec<String>,
    /// Field to display in output (Deprecated: use -output-template instead)
    #[arg(short = 'f', long = "field", default_value = "")]
    field: String,
    /// Field to store in per-host output
    #[arg(long = "store-field", default_value = "")]
    store_field: String,
    /// Match output for given extension (eg, -em php,html,js,none)
    #[arg(long = "extension-match")]
    extension_match: Vec<String>,
    /// Filter output for given extension (eg, -ef png,css)
    #[arg(long = "extension-filter")]
    extension_filter: Vec<String>,
    /// Remove default extensions from the filter list
    #[arg(long = "no-default-ext-filter")]
    no_default_ext_filter: bool,
    /// Match response with dsl based condition
    #[arg(long = "match-condition", default_value = "")]
    match_condition: String,
    /// Filter response with dsl based condition
    #[arg(long = "filter-condition", default_value = "")]
    filter_condition: String,
    /// Disable duplicate content filtering
    #[arg(long = "disable-unique-filter")]
    disable_unique_filter: bool,
    /// Enable page content similarity filtering (after exact content dedup)
    #[arg(long = "page-content-similar")]
    page_content_similar: bool,
    /// Alias for -pcs (page content similarity)
    #[arg(long = "similarity-deduplication")]
    similarity_deduplication: bool,
    /// Similarity mode: simhash, tfidf, or bm25 (default simhash)
    #[arg(long = "page-content-similar-mode", default_value = "simhash")]
    page_content_similar_mode: String,
    /// Simhash max hamming distance (default 3)
    #[arg(long = "page-content-similar-distance", default_value_t = 3)]
    page_content_similar_distance: u32,
    /// Tfidf/bm25 min score 0-1 (default 0.85)
    #[arg(long = "page-content-similar-threshold", default_value = "0.85")]
    page_content_similar_threshold: String,
    /// Pages to fully process per similarity cluster (default 1)
    #[arg(long = "page-content-similar-budget", default_value_t = 1)]
    page_content_similar_budget: usize,
    /// Filter response with page type (e.g. error,captcha,parked)
    #[arg(long = "filter-page-type")]
    filter_page_type: Vec<String>,

    // -------------------------------------------------------- ratelimit
    /// Number of concurrent fetchers to use (default 10)
    #[arg(short = 'c', long, default_value_t = 10)]
    concurrency: usize,
    /// Number of concurrent inputs to process (default 10)
    #[arg(short = 'p', long, default_value_t = 10)]
    parallelism: usize,
    /// Request delay between each request in seconds (default 0)
    #[arg(long, default_value_t = 0)]
    delay: u64,
    /// Maximum requests to send per second (default 150)
    #[arg(long = "rate-limit", default_value_t = 150)]
    rate_limit: usize,
    /// Maximum number of requests to send per minute (default 0)
    #[arg(long = "rate-limit-minute", default_value_t = 0)]
    rate_limit_minute: usize,
    /// Maximum requests to send per second per host (default 0)
    #[arg(long = "host-rate-limit", default_value_t = 0)]
    host_rate_limit: usize,
    /// Maximum number of requests to send per minute per host (default 0)
    #[arg(long = "host-rate-limit-minute", default_value_t = 0)]
    host_rate_limit_minute: usize,

    // ------------------------------------------------------------ output
    /// File to write output to
    #[arg(short = 'o', long, default_value = "")]
    output: String,
    /// Custom output template
    #[arg(long = "output-template", default_value = "")]
    output_template: String,
    /// Store http requests/responses
    #[arg(long = "store-response")]
    store_response: bool,
    /// Store http requests/responses to custom directory
    #[arg(long = "store-response-dir", default_value = "")]
    store_response_dir: String,
    /// Do not overwrite output file
    #[arg(long = "no-clobber")]
    no_clobber: bool,
    /// Store per-host field to custom directory
    #[arg(long = "store-field-dir", default_value = "")]
    store_field_dir: String,
    /// Omit raw requests/responses from jsonl output
    #[arg(long = "omit-raw")]
    omit_raw: bool,
    /// Omit response body from jsonl output
    #[arg(long = "omit-body")]
    omit_body: bool,
    /// List of fields to output in jsonl format
    #[arg(long = "list-output-fields")]
    list_output_fields: bool,
    /// Exclude fields from jsonl output
    #[arg(long = "exclude-output-fields")]
    exclude_output_fields: Vec<String>,
    /// Write output in jsonl format
    #[arg(short = 'j', long = "jsonl")]
    jsonl: bool,
    /// Output page content as Markdown instead of URLs/HTML
    #[arg(short = 'm', long = "markdown")]
    markdown: bool,
    /// Disable output content coloring (ANSI escape codes)
    #[arg(long = "no-color")]
    no_color: bool,
    /// Display output only
    #[arg(long)]
    silent: bool,
    /// Display verbose output
    #[arg(short = 'v', long)]
    verbose: bool,
    /// Display debug output
    #[arg(long)]
    debug: bool,
    /// Update the crawler to the latest version (no-op in the Rust build)
    #[arg(long)]
    update: bool,
    /// Disable automatic update check
    #[arg(long = "disable-update-check")]
    disable_update_check: bool,
    /// Do not print banner
    #[arg(long, hide = true)]
    _no_banner: bool,
}

fn main() {
    let cli = Cli::parse();

    // Sitemap-tree mode: DFS link tree via SiteMapper instead of the crawl
    // engine. Runs before options mapping so `cli` fields are still owned.
    if cli.sitemap_tree {
        run_sitemap_tree(&cli);
        std::process::exit(0);
    }

    let mut options = Options::with_defaults();
    options.urls = cli.urls;
    options.resume = cli.resume;
    options.exclude = cli.exclude;
    options.resolvers = cli.resolvers;
    options.max_depth = cli.depth;
    options.scrape_js_responses = cli.js_crawl;
    options.scrape_jsluice_responses = cli.jsluice;
    options.crawl_duration = match parse_duration_arg(&cli.crawl_duration) {
        Ok(d) => d,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    };
    options.known_files = if cli.known_files.is_empty() {
        KnownFiles::None
    } else {
        match KnownFiles::parse(&cli.known_files) {
            Ok(k) => k,
            Err(err) => {
                eprintln!("error: {err}");
                std::process::exit(1);
            }
        }
    };
    options.body_read_size = cli.max_response_size;
    options.timeout = cli.timeout;
    options.time_stable = cli.time_stable;
    options.automatic_form_fill = cli.automatic_form_fill;
    options.form_extraction = cli.form_extraction;
    options.retries = cli.retry;
    options.proxy = cli.proxy;
    options.tech_detect = cli.tech_detect;
    options.custom_headers = parse_custom_headers(&resolve_file_inputs(&cli.headers));
    options.config_file = cli.config;
    if !options.config_file.is_empty() {
        let config_path = options.config_file.clone();
        if let Err(err) = apply_config_file(&mut options, &config_path) {
            eprintln!("error: could not read config file: {err}");
            std::process::exit(1);
        }
    }
    options.form_config = cli.form_config;
    options.field_config = cli.field_config;
    options.strategy = match Strategy::parse(&cli.strategy) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    };
    options.ignore_query_params = cli.ignore_query_params;
    options.filter_similar = cli.filter_similar;
    options.filter_similar_threshold = cli.filter_similar_threshold;
    options.tls_impersonate = cli.tls_impersonate;
    options.disable_redirects = cli.disable_redirects;
    options.path_climb = cli.path_climb;
    options.knowledge_base = cli.knowledge_base;
    options.secrets = cli.kb_secrets || cli.knowledge_base;
    options.validate_secrets = cli.kb_validate_secrets;
    options.endpoints = cli.kb_endpoints || cli.knowledge_base;
    options.max_domain_pages = cli.max_domain_pages;
    options.health_check = cli.health_check;
    options.error_log = cli.error_log;
    options.pprof_server = cli.pprof_server;
    options.headless = cli.headless;
    options.headless_hybrid = cli.hybrid;
    options.use_installed_chrome = cli.system_chrome;
    options.show_browser = cli.show_browser;
    options.headless_optional_arguments = cli.headless_options;
    options.headless_no_sandbox = cli.no_sandbox;
    options.chrome_data_dir = cli.chrome_data_dir;
    options.system_chrome_path = cli.system_chrome_path;
    options.headless_no_incognito = cli.no_incognito;
    options.chrome_ws_url = cli.chrome_ws_url;
    options.xhr_extraction = cli.xhr_extraction;
    options.max_failure_count = cli.max_failure_count;
    options.max_onclick_links = cli.max_onclick_links;
    options.enable_diagnostics = cli.enable_diagnostics;
    options.page_load_strategy = match PageLoadStrategy::parse(&cli.page_load_strategy) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    };
    options.dom_wait_time = cli.dom_wait_time;
    options.captcha_solver_provider = cli
        .captcha_solver_provider
        .is_empty()
        .then(|| std::env::var("CAPTCHA_SOLVER_PROVIDER").unwrap_or_default())
        .unwrap_or(cli.captcha_solver_provider);
    options.captcha_solver_api_key = cli
        .captcha_solver_key
        .is_empty()
        .then(|| std::env::var("CAPTCHA_SOLVER_KEY").unwrap_or_default())
        .unwrap_or(cli.captcha_solver_key);
    options.auth_credentials = cli
        .auto_login
        .is_empty()
        .then(|| std::env::var("AUTH_CREDENTIALS").unwrap_or_default())
        .unwrap_or(cli.auto_login);
    options.scope = resolve_file_inputs(&cli.crawl_scope);
    options.out_of_scope = resolve_file_inputs(&cli.crawl_out_scope);
    options.field_scope = cli.field_scope;
    options.no_scope = cli.no_scope;
    options.display_out_scope = cli.display_out_scope;
    options.output_match_regex = resolve_file_inputs(&cli.match_regex);
    options.output_filter_regex = resolve_file_inputs(&cli.filter_regex);
    options.fields = cli.field;
    options.store_fields = cli.store_field;
    options.extensions_match = cli.extension_match;
    options.extension_filter = cli.extension_filter;
    options.no_default_ext_filter = cli.no_default_ext_filter;
    options.output_match_condition = cli.match_condition;
    options.output_filter_condition = cli.filter_condition;
    options.disable_unique_filter = cli.disable_unique_filter;
    options.page_content_similar = cli.page_content_similar || cli.similarity_deduplication;
    options.similarity_deduplication = cli.similarity_deduplication;
    options.page_content_similar_mode = match SimilarityMode::parse(&cli.page_content_similar_mode) {
        Ok(m) => m,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    };
    options.page_content_similar_distance = cli.page_content_similar_distance;
    options.page_content_similar_threshold = cli.page_content_similar_threshold;
    options.page_content_similar_budget = cli.page_content_similar_budget;
    options.filter_page_type = cli.filter_page_type;
    options.concurrency = cli.concurrency;
    options.parallelism = cli.parallelism;
    options.delay = cli.delay;
    options.rate_limit = cli.rate_limit;
    options.rate_limit_minute = cli.rate_limit_minute;
    options.host_rate_limit = cli.host_rate_limit;
    options.host_rate_limit_minute = cli.host_rate_limit_minute;
    options.output_file = cli.output;
    options.output_template = cli.output_template;
    options.store_response = cli.store_response;
    options.store_response_dir = cli.store_response_dir;
    options.no_clobber = cli.no_clobber;
    options.store_field_dir = cli.store_field_dir;
    options.omit_raw = cli.omit_raw;
    options.omit_body = cli.omit_body;
    options.list_output_fields = cli.list_output_fields;
    options.exclude_output_fields = cli.exclude_output_fields;
    options.json = cli.jsonl;
    options.markdown = cli.markdown;
    options.no_colors = cli.no_color;
    options.silent = cli.silent;
    options.verbose = cli.verbose;
    options.debug = cli.debug;
    options.urls_from_stdin = options.urls.is_empty();

    // Update check flags: self-update is a Go-ecosystem feature and is a no-op
    // here; the -duc flag suppresses the check that would run in Go.
    if cli.update {
        eprintln!("[INF] self-update is not supported in the Rust build; rebuild with cargo instead");
        std::process::exit(0);
    }
    if !cli.disable_update_check {
        // Reference crawler performs a network version check here; the Rust
        // build ships no update feed, so nothing to check (silent no-op).
    }

    configure_output(&options);

    // List output fields exits before the banner/runner (reference crawler
    // handles -lof first in main).
    if options.list_output_fields {
        celestia_spider::output::list_output_fields();
        std::process::exit(0);
    }
    if options.health_check {
        celestia_spider::runner::health_check();
        std::process::exit(0);
    }

    // Banner (reference crawler showBanner; suppressed by -silent / --no-banner).
    if !options.silent && !cli._no_banner {
        println!("celestia-spider v{} - fast crawler for automation pipelines", celestia_spider::runner::version());
    }

    // Cleanup: resume files older than 10 days are removed at startup
    // (reference crawler cleanupOldResumeFiles).
    celestia_spider::runner::cleanup_old_resume_files(10);

    let mut runner = match Runner::new(options) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    let summary = match runtime.block_on(runner.run()) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("error: {err}");
            std::process::exit(1);
        }
    };

    if !runner.options.silent {
        eprintln!(
            "[INF] done: {} results, {} skipped, {} failed{}",
            summary.results,
            summary.skipped,
            summary.failed,
            if summary.cancelled { " (cancelled)" } else { "" }
        );
    }

    // Post-run housekeeping (reference crawler main.go): dedupe lines in the
    // store-field dir and remove the resume file after a successful run.
    celestia_spider::output::dedupe_lines_in_dir("celestia_field");
    runner.remove_resume_file();
}

/// Run the DFS sitemap-tree mode: fetch each `-u` target with `SiteMapper`,
/// following same-host links depth-first, and emit the resulting
/// `SitemapNode` tree as JSON (array of roots when multiple URLs are given).
fn run_sitemap_tree(cli: &Cli) {
    use celestia_spider::mapper::SiteMapper;
    use celestia_spider::types::{RenderMode, RenderOptions};

    if cli.urls.is_empty() {
        eprintln!("error: --sitemap-tree requires at least one target url (-u)");
        std::process::exit(1);
    }

    let render_options = if cli.headless {
        RenderOptions {
            render_mode: RenderMode::Dynamic,
            ..RenderOptions::default()
        }
    } else {
        RenderOptions::default()
    };

    let mapper = SiteMapper::builder()
        .max_depth(cli.depth.max(0) as usize)
        .render_options(render_options)
        .build();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    let roots: Vec<celestia_spider::types::SitemapNode> = runtime.block_on(async {
        let mut roots = Vec::new();
        for url in &cli.urls {
            match mapper.map_site(url).await {
                Some(node) => roots.push(node),
                None => eprintln!("[WRN] failed to map {url}"),
            }
        }
        roots
    });

    if !cli.silent {
        eprintln!(
            "[INF] sitemap: {} root(s), {} pages fetched",
            roots.len(),
            mapper.pages_fetched()
        );
    }

    let json = if roots.len() == 1 {
        serde_json::to_string_pretty(&roots[0]).expect("serialize sitemap tree")
    } else {
        serde_json::to_string_pretty(&roots).expect("serialize sitemap tree")
    };

    if cli.output.is_empty() {
        println!("{json}");
    } else if let Err(err) = std::fs::write(&cli.output, format!("{json}\n")) {
        eprintln!("error: could not write {}: {err}", cli.output);
        std::process::exit(1);
    }
}

/// Parse a reference-crawler-style duration argument (`30s`, `5m`, `1h`,
/// `1h30m`, `500ms`, `2d`) — see `parse_go_duration`.
pub fn parse_duration_arg(input: &str) -> Result<Duration, String> {
    celestia_spider::types::options::parse_go_duration(input)
}

/// Apply a `--config` file: simple `flag-name: value` lines (goflags-style
/// config file), mapping flag names onto options. Unknown keys are ignored.
fn apply_config_file(options: &mut Options, path: &str) -> Result<(), String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("could not read {path}: {e}"))?;
    let mut headers: Vec<String> = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().trim_matches('"');
        let value = value.trim().trim_matches('"').to_string();
        match key {
            "depth" | "d" => options.max_depth = value.parse().unwrap_or(options.max_depth),
            "concurrency" | "c" => options.concurrency = value.parse().unwrap_or(options.concurrency),
            "parallelism" | "p" => options.parallelism = value.parse().unwrap_or(options.parallelism),
            "timeout" => options.timeout = value.parse().unwrap_or(options.timeout),
            "retry" => options.retries = value.parse().unwrap_or(options.retries),
            "rate-limit" => options.rate_limit = value.parse().unwrap_or(options.rate_limit),
            "delay" => options.delay = value.parse().unwrap_or(options.delay),
            "proxy" => options.proxy = value,
            "output" | "o" => options.output_file = value,
            "field-scope" => options.field_scope = value,
            "strategy" | "s" => {
                if let Ok(s) = Strategy::parse(&value) {
                    options.strategy = s;
                }
            }
            "headers" | "H" => headers.push(value),
            "known-files" => {
                if let Ok(k) = KnownFiles::parse(&value) {
                    options.known_files = k;
                }
            }
            "js-crawl" => options.scrape_js_responses = value == "true",
            "jsonl" | "j" => options.json = value == "true",
            "silent" => options.silent = value == "true",
            "verbose" => options.verbose = value == "true",
            "no-color" => options.no_colors = value == "true",
            _ => {}
        }
    }
    for (k, v) in parse_custom_headers(&headers) {
        options.custom_headers.insert(k, v);
    }
    Ok(())
}

/// Resolve `-H`/`-cs` style inputs: if the value is a file path, read one
/// value per line (celestia FileStringSliceOptions).
fn resolve_file_inputs(values: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for v in values {
        if std::path::Path::new(v).is_file() {
            if let Ok(content) = std::fs::read_to_string(v) {
                for line in content.lines() {
                    let line = line.trim();
                    if !line.is_empty() {
                        out.push(line.to_string());
                    }
                }
                continue;
            }
        }
        // Comma-separated expansion for scope-like flags.
        for part in v.split(',') {
            let part = part.trim();
            if !part.is_empty() {
                out.push(part.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_arg() {
        assert_eq!(parse_duration_arg("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration_arg("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration_arg("1h").unwrap(), Duration::from_secs(3600));
        assert_eq!(parse_duration_arg("2d").unwrap(), Duration::from_secs(172_800));
        assert_eq!(parse_duration_arg("1h30m").unwrap(), Duration::from_secs(5400));
        assert_eq!(parse_duration_arg("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration_arg("").unwrap(), Duration::ZERO);
        assert!(parse_duration_arg("bogus").is_err());
    }

    #[test]
    fn test_resolve_file_inputs_inline() {
        let v = resolve_file_inputs(&["/api/,/admin/".into()]);
        assert_eq!(v, vec!["/api/", "/admin/"]);
    }

    #[test]
    fn test_resolve_file_inputs_file() {
        let tmp = std::env::temp_dir().join("bc_cli_inputs.txt");
        std::fs::write(&tmp, "https://a.com\nhttps://b.com\n").unwrap();
        let v = resolve_file_inputs(&[tmp.to_string_lossy().to_string()]);
        assert_eq!(v, vec!["https://a.com", "https://b.com"]);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_custom_headers_map() {
        let m: std::collections::HashMap<String, String> =
            parse_custom_headers(&["A: b".into()]);
        assert_eq!(m.get("A").unwrap(), "b");
    }
}
