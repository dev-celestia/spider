//! End-to-end library tests: run the real crawl engine against a local HTTP
//! test server and assert on results, filters, output files, and server hits.

mod common;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use browser_crawler::engine::common::{Crawler, PageFetch};
use browser_crawler::engine::standard::StandardFetcher;
use browser_crawler::output::StandardWriter;
use browser_crawler::types::options::Options;

use common::{crawl_options, spawn, standard_site};

/// Run a crawl with `options` against `server`, collecting emitted result URLs.
async fn crawl_and_collect(
    options: &Options,
) -> (Arc<Crawler>, Arc<Mutex<Vec<String>>>) {
    let writer = Arc::new(StandardWriter::from_options(options));
    let cancel = Arc::new(AtomicBool::new(false));
    let results: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut options = options.clone();
    let collected = Arc::clone(&results);
    options.on_result = Some(Box::new(move |r| {
        collected.lock().unwrap().push(r.url().to_string());
    }));
    let fetcher: Arc<dyn PageFetch> = Arc::new(StandardFetcher::from_options(&options).unwrap());
    let crawler = Arc::new(Crawler::new(Arc::new(options), fetcher, writer, cancel).unwrap());
    let seed = crawler.options.urls[0].clone();
    crawler.crawl(&seed).await.unwrap();
    (Arc::clone(&crawler), results)
}

#[tokio::test]
async fn e2e_full_site_crawl_visits_all_pages() {
    let server = spawn(standard_site(None)).await;
    let options = crawl_options(&server.base_url);
    let (crawler, results) = crawl_and_collect(&options).await;

    let urls = results.lock().unwrap();
    assert!(urls.iter().any(|u| u.ends_with("/")), "index visited: {urls:?}");
    assert!(urls.iter().any(|u| u.ends_with("/about")), "about visited");
    assert!(urls.iter().any(|u| u.ends_with("/contact")), "contact visited");
    assert!(crawler.stats.results.load(std::sync::atomic::Ordering::SeqCst) >= 3);
    assert_eq!(crawler.stats.failed.load(std::sync::atomic::Ordering::SeqCst), 0);
    server.shutdown();
}

#[tokio::test]
async fn e2e_depth_limit_stops_recursion() {
    // Dedicated chain: index -> /about -> /contact (no direct shortcut).
    let mut routes = standard_site(None);
    routes.insert("/".to_string(), common::Route::html("<html><body><a href=\"/about\">about</a></body></html>"));
    let server = spawn(routes).await;
    let mut options = crawl_options(&server.base_url);
    options.max_depth = 1; // index (0) + /about (1); /contact would be depth 2
    let (_, results) = crawl_and_collect(&options).await;

    let urls = results.lock().unwrap();
    assert!(urls.iter().any(|u| u.ends_with("/about")));
    assert!(!urls.iter().any(|u| u.ends_with("/contact")), "depth respected: {urls:?}");
    server.shutdown();
}

#[tokio::test]
async fn e2e_default_extension_filter_skips_images() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.max_depth = 1;
    let (crawler, _) = crawl_and_collect(&options).await;

    assert_eq!(server.hits("/logo.png"), 0, "png filtered by default denylist");
    assert_eq!(server.hits("/about"), 1);
    // The skipped image counts as a filtered navigation.
    assert!(crawler.stats.skipped.load(std::sync::atomic::Ordering::SeqCst) >= 1);
    server.shutdown();
}

#[tokio::test]
async fn e2e_crawl_scope_regex_restricts_following() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.scope = vec!["/about".to_string()];
    let (_, results) = crawl_and_collect(&options).await;

    let urls = results.lock().unwrap();
    assert!(urls.iter().any(|u| u.ends_with("/about")));
    assert!(!urls.iter().any(|u| u.ends_with("/contact")), "contact outside scope: {urls:?}");
    assert!(!urls.iter().any(|u| u.ends_with("/form")), "form outside scope");
    server.shutdown();
}

#[tokio::test]
async fn e2e_out_of_scope_regex_excludes() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.out_of_scope = vec!["/users/".to_string()];
    let (_, results) = crawl_and_collect(&options).await;

    let urls = results.lock().unwrap();
    assert!(urls.iter().any(|u| u.ends_with("/about")));
    assert!(!urls.iter().any(|u| u.contains("/users/")), "users excluded: {urls:?}");
    server.shutdown();
}

#[tokio::test]
async fn e2e_match_and_filter_regex_on_output() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.match_regex = vec![regex::Regex::new("/(about|contact)").unwrap()];
    let (_, results) = crawl_and_collect(&options).await;

    let urls = results.lock().unwrap();
    assert!(!urls.is_empty(), "matched pages emitted");
    // The seed URL is always crawled; all discovered pages respect the regex.
    assert!(
        urls.iter().filter(|u| !u.ends_with("/")).all(|u| u.contains("/about") || u.contains("/contact")),
        "{urls:?}"
    );
    server.shutdown();
}

#[tokio::test]
async fn e2e_ignore_query_params_dedupes() {
    let server = spawn(standard_site(None)).await;
    // With --ignore-query-params, /page?a=1 and /page?a=2 collapse to one fetch.
    let mut options = crawl_options(&server.base_url);
    options.ignore_query_params = true;
    options.max_depth = 1;
    let _ = crawl_and_collect(&options).await;
    assert_eq!(server.hits("/page"), 1, "query params ignored: {:?}", server.all_requests());

    // Without the flag both are fetched.
    let server2 = spawn(standard_site(None)).await;
    let options2 = {
        let mut o = crawl_options(&server2.base_url);
        o.max_depth = 1;
        o
    };
    let _ = crawl_and_collect(&options2).await;
    assert_eq!(server2.hits("/page"), 2, "both query variants fetched");
    server.shutdown();
    server2.shutdown();
}

#[tokio::test]
async fn e2e_filter_similar_collapses_variable_paths() {
    // Dedicated site: /site/{1,2,3} share one variable position (threshold 3);
    // root stays below threshold (2 top-level dirs: /about and /site).
    let mut routes = standard_site(None);
    routes.insert(
        "/".to_string(),
        common::Route::html(
            "<html><body><a href=\"/about\">about</a>             <a href=\"/site/1\">s1</a><a href=\"/site/2\">s2</a><a href=\"/site/3\">s3</a></body></html>",
        ),
    );
    for n in 1..=3 {
        routes.insert(
            format!("/site/{n}"),
            common::Route::html(format!("<html><body>site {n} unique body</body></html>")),
        );
    }
    let server = spawn(routes).await;
    let mut options = crawl_options(&server.base_url);
    options.filter_similar = true;
    options.filter_similar_threshold = 3;
    options.max_depth = 1;
    let _ = crawl_and_collect(&options).await;

    let all = server.all_requests();
    let site_hits = all.iter().filter(|p| p.starts_with("/site/")).count();
    assert_eq!(site_hits, 3, "site position collapsed after threshold: {all:?}");
    assert!(all.iter().any(|p| p == "/about"), "non-variable page still crawled: {all:?}");
    server.shutdown();
}

#[tokio::test]
async fn e2e_known_files_crawl_robots_and_sitemap() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.known_files = browser_crawler::types::options::KnownFiles::All;
    let (_, results) = crawl_and_collect(&options).await;

    let urls = results.lock().unwrap();
    assert!(urls.iter().any(|u| u.ends_with("/private")), "robots.txt disallow crawled: {urls:?}");
    assert!(urls.iter().any(|u| u.ends_with("/from-sitemap")), "sitemap loc crawled");
    server.shutdown();
}

#[tokio::test]
async fn e2e_js_crawl_extracts_js_endpoints() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.scrape_js_responses = true;
    options.max_depth = 2;
    let (_, results) = crawl_and_collect(&options).await;

    let urls = results.lock().unwrap();
    assert!(
        urls.iter().any(|u| u.contains("/api/secret/endpoint.json")),
        "JS endpoint discovered and crawled: {urls:?}"
    );
    server.shutdown();
}

#[tokio::test]
async fn e2e_no_js_crawl_skips_endpoints() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.max_depth = 2;
    let (_, results) = crawl_and_collect(&options).await;

    let urls = results.lock().unwrap();
    assert!(!urls.iter().any(|u| u.contains("/api/secret/")), "endpoints not scraped without -jc");
    server.shutdown();
}

#[tokio::test]
async fn e2e_form_extraction_attaches_forms_to_jsonl() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.form_extraction = true;
    options.json = true;

    // Capture JSONL output via the writer by writing to a temp file.
    let tmp = std::env::temp_dir().join("e2e_forms.jsonl");
    let _ = std::fs::remove_file(&tmp);
    options.output_file = tmp.to_string_lossy().to_string();

    let writer = Arc::new(StandardWriter::from_options(&options));
    let cancel = Arc::new(AtomicBool::new(false));
    let fetcher: Arc<dyn PageFetch> = Arc::new(StandardFetcher::from_options(&options).unwrap());
    let crawler = Arc::new(Crawler::new(Arc::new(options.clone()), fetcher, writer, cancel).unwrap());
    crawler.crawl(&format!("{}/", server.base_url)).await.unwrap();

    let content = std::fs::read_to_string(&tmp).expect("jsonl written");
    let forms_line = content
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).expect("valid jsonl"))
        .find(|v| v["request"]["endpoint"].as_str().unwrap_or("").contains("/form"))
        .expect("form page in output");
    let forms = forms_line["response"]["forms"].as_array().expect("forms attached");
    assert_eq!(forms.len(), 1);
    assert_eq!(forms[0]["method"], "GET");
    assert_eq!(forms[0]["action"], "/submit");
    assert_eq!(forms[0]["parameters"][0], "q");
    let _ = std::fs::remove_file(&tmp);
    server.shutdown();
}

#[tokio::test]
async fn e2e_automatic_form_fill_submits_form() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.automatic_form_fill = true;
    options.max_depth = 2;
    let (_, results) = crawl_and_collect(&options).await;

    // The filled form navigation is enqueued as /submit?q=<placeholder>.
    let all = server.all_requests();
    assert!(
        all.iter().any(|p| p.starts_with("/submit?")),
        "form submitted with filled values: {all:?}"
    );
    let urls = results.lock().unwrap();
    assert!(urls.iter().any(|u| u.contains("/submit?")), "form result emitted");
    server.shutdown();
}

#[tokio::test]
async fn e2e_dsl_match_condition_filters_output() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.output_match_condition = "contains(url, '/about')".to_string();
    let (_, results) = crawl_and_collect(&options).await;

    let urls = results.lock().unwrap();
    assert!(!urls.is_empty());
    assert!(urls.iter().all(|u| u.contains("/about")), "dsl match: {urls:?}");
    server.shutdown();
}

#[tokio::test]
async fn e2e_page_type_filter_drops_errors() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.filter_page_type = vec!["error".to_string()];
    options.max_depth = 2;
    // /missing returns 404; request it directly as seed link via about? Use a
    // dedicated seed on a 404 path: the seed itself is filtered on response.
    options.urls = vec![format!("{}/missing", server.base_url)];
    let (_, results) = crawl_and_collect(&options).await;

    let urls = results.lock().unwrap();
    assert!(urls.is_empty(), "404 seed filtered by page-type: {urls:?}");
    server.shutdown();
}

#[tokio::test]
async fn e2e_jsonl_file_output_is_well_formed() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.json = true;
    options.max_depth = 1;
    let tmp = std::env::temp_dir().join("e2e_jsonl.jsonl");
    let _ = std::fs::remove_file(&tmp);
    options.output_file = tmp.to_string_lossy().to_string();

    let writer = Arc::new(StandardWriter::from_options(&options));
    let cancel = Arc::new(AtomicBool::new(false));
    let fetcher: Arc<dyn PageFetch> = Arc::new(StandardFetcher::from_options(&options).unwrap());
    let crawler = Arc::new(Crawler::new(Arc::new(options), fetcher, writer, cancel).unwrap());
    crawler.crawl(&format!("{}/", server.base_url)).await.unwrap();

    let content = std::fs::read_to_string(&tmp).expect("file written");
    let mut lines = content.lines().count();
    let mut saw_index = false;
    for line in std::fs::read_to_string(&tmp).unwrap().lines() {
        let v: serde_json::Value = serde_json::from_str(line).expect("each line is valid JSON");
        let endpoint = v["request"]["endpoint"].as_str().expect("endpoint key").to_string();
        if endpoint.ends_with('/') && endpoint.contains(&server.base_url) {
            saw_index = true;
            assert_eq!(v["response"]["status_code"], 200);
            assert!(v["timestamp"].as_str().is_some());
        }
        let _ = &mut lines;
    }
    assert!(saw_index, "index page present in jsonl");
    assert!(lines >= 4, "index + about + contact + app.js at minimum");
    let _ = std::fs::remove_file(&tmp);
    server.shutdown();
}

#[tokio::test]
async fn e2e_rate_limit_paces_requests() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.rate_limit = 1; // 1 request/second
    options.max_depth = 1;
    options.timeout = 30;
    let start = std::time::Instant::now();
    let _ = crawl_and_collect(&options).await;
    // index + at least 3 discovered pages => >= 3s of pacing at 1 rps.
    // Use a conservative lower bound to avoid CI flakiness.
    assert!(
        start.elapsed() >= std::time::Duration::from_millis(1500),
        "rate limit enforced, elapsed: {:?}",
        start.elapsed()
    );
    server.shutdown();
}

#[tokio::test]
async fn e2e_max_domain_pages_caps_crawl() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.max_domain_pages = 2;
    let (crawler, results) = crawl_and_collect(&options).await;

    let urls = results.lock().unwrap();
    assert!(
        urls.len() <= 2,
        "domain page cap enforced: {urls:?} (skipped={})",
        crawler.stats.skipped.load(std::sync::atomic::Ordering::SeqCst)
    );
    server.shutdown();
}

#[tokio::test]
async fn e2e_path_climb_enqueues_ancestors() {
    // /a/b/c page links only to itself; path-climb must discover /a/ and /a/b/.
    let mut routes = standard_site(None);
    routes.insert("/a/b/c".to_string(), common::Route::html("<html><body>deep</body></html>"));
    routes.insert("/a/".to_string(), common::Route::html("<html><body>level a</body></html>"));
    routes.insert("/a/b/".to_string(), common::Route::html("<html><body>level a-b</body></html>"));
    let server = spawn(routes).await;
    let mut options = crawl_options(&server.base_url);
    options.path_climb = true;
    options.max_depth = 3;
    options.urls = vec![format!("{}/a/b/c", server.base_url)];
    let (_, results) = crawl_and_collect(&options).await;

    let urls = results.lock().unwrap();
    assert!(urls.iter().any(|u| u.ends_with("/a/")), "ancestor /a/ climbed: {urls:?}");
    assert!(urls.iter().any(|u| u.ends_with("/a/b/")), "ancestor /a/b/ climbed");
    server.shutdown();
}

#[tokio::test]
async fn e2e_crawl_duration_stops_crawl() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.max_depth = 0; // depth unlimited
    options.crawl_duration = std::time::Duration::from_millis(300);
    let start = std::time::Instant::now();
    let _ = crawl_and_collect(&options).await;
    assert!(
        start.elapsed() <= std::time::Duration::from_millis(5000),
        "crawl stopped by duration: {:?}",
        start.elapsed()
    );
    server.shutdown();
}

#[tokio::test]
async fn e2e_strategy_breadth_first_visits_level_order() {
    let server = spawn(standard_site(None)).await;
    let mut options = crawl_options(&server.base_url);
    options.strategy = browser_crawler::types::options::Strategy::BreadthFirst;
    options.max_depth = 1;
    let (crawler, _) = crawl_and_collect(&options).await;

    // All depth-1 pages were discovered; breadth-first order is exercised by
    // the queue unit tests — here assert completion and no failures.
    assert_eq!(crawler.stats.failed.load(std::sync::atomic::Ordering::SeqCst), 0);
    server.shutdown();
}

#[tokio::test]
async fn e2e_tech_detect_fingerprinted_in_output() {
    let mut routes = standard_site(None);
    routes.insert(
        "/tech".to_string(),
        common::Route {
            status: 200,
            content_type: "text/html",
            body: "<html><body>wp-content themes</body></html>".to_string(),
        },
    );
    let server = spawn(routes).await;
    let mut options = crawl_options(&server.base_url);
    options.tech_detect = true;
    options.json = true;
    options.max_depth = 1;
    options.urls = vec![format!("{}/tech", server.base_url)];

    let tmp = std::env::temp_dir().join("e2e_tech.jsonl");
    let _ = std::fs::remove_file(&tmp);
    options.output_file = tmp.to_string_lossy().to_string();
    let writer = Arc::new(StandardWriter::from_options(&options));
    let cancel = Arc::new(AtomicBool::new(false));
    let fetcher: Arc<dyn PageFetch> = Arc::new(StandardFetcher::from_options(&options).unwrap());
    let crawler = Arc::new(Crawler::new(Arc::new(options), fetcher, writer, cancel).unwrap());
    crawler.crawl(&crawler.options.urls[0].clone()).await.unwrap();

    let content = std::fs::read_to_string(&tmp).unwrap();
    assert!(content.contains("WordPress"), "tech detected: {content}");
    let _ = std::fs::remove_file(&tmp);
    server.shutdown();
}

#[tokio::test]
async fn e2e_knowledge_base_secrets_extracted() {
    let mut routes = standard_site(None);
    routes.insert(
        "/leak".to_string(),
        common::Route::html("<html><body>key = AKIAIOSFODNN7EXAMPLE</body></html>"),
    );
    let server = spawn(routes).await;
    let mut options = crawl_options(&server.base_url);
    options.secrets = true;
    options.json = true;
    options.max_depth = 1;
    options.urls = vec![format!("{}/leak", server.base_url)];

    let tmp = std::env::temp_dir().join("e2e_kb.jsonl");
    let _ = std::fs::remove_file(&tmp);
    options.output_file = tmp.to_string_lossy().to_string();
    let writer = Arc::new(StandardWriter::from_options(&options));
    let cancel = Arc::new(AtomicBool::new(false));
    let fetcher: Arc<dyn PageFetch> = Arc::new(StandardFetcher::from_options(&options).unwrap());
    let crawler = Arc::new(Crawler::new(Arc::new(options), fetcher, writer, cancel).unwrap());
    crawler.crawl(&crawler.options.urls[0].clone()).await.unwrap();

    let content = std::fs::read_to_string(&tmp).unwrap();
    let v: serde_json::Value = serde_json::from_str(content.lines().next().unwrap()).unwrap();
    let kb = &v["response"]["knowledgebase"];
    let secret = &kb["secrets"][0];
    assert_eq!(secret["type"], "AWS Access Key", "secret kind found: {kb}");
    assert_eq!(secret["match"], "[REDACTED]", "secret value redacted in kb output");
    let _ = std::fs::remove_file(&tmp);
    server.shutdown();
}

#[tokio::test]
async fn e2e_scope_cross_host_external_skipped() {
    // Two servers with DIFFERENT hostnames: main binds 127.0.0.1, the external
    // link uses `localhost` so DNS-based rdn scope treats them as distinct hosts.
    let external = spawn(standard_site(None)).await;
    let external_url = external.base_url.replace("127.0.0.1", "localhost");
    let main = spawn(standard_site(Some(&format!("{}/", external_url)))).await;

    let mut options = crawl_options(&main.base_url);
    let (_, results) = crawl_and_collect(&options).await;

    let urls = results.lock().unwrap();
    assert!(
        !urls.iter().any(|u| u.contains("localhost")),
        "external host out of scope by default: {urls:?}"
    );
    assert_eq!(external.hits("/"), 0, "external server never fetched");
    main.shutdown();
    external.shutdown();
}

#[tokio::test]
async fn e2e_retry_on_failed_fetch() {
    // Connection-refused target: the crawler retries once, then records a failure.
    let mut options = crawl_options("http://127.0.0.1:1");
    options.max_depth = 0;
    options.retries = 1;
    options.timeout = 5;
    let start = std::time::Instant::now();
    let (crawler, _) = crawl_and_collect(&options).await;

    assert_eq!(crawler.stats.failed.load(std::sync::atomic::Ordering::SeqCst), 1);
    // Retry backoff (250ms) plus connection attempts must have elapsed.
    assert!(start.elapsed() >= std::time::Duration::from_millis(200), "{:?}", start.elapsed());
}
