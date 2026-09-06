//! Port of the reference crawler `pkg/engine/headless` — headless Chrome crawl engine:
//! stealth rendering, page-load strategies, XHR capture via document-start JS
//! hooks, automatic form fill, auto-login, and CAPTCHA detection/solving
//! (native capsolver client).

pub mod captcha;

use std::ffi::OsStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::control::CrawlControl;
use crate::engine::common::{Crawler, PageFetch};
use crate::output::{log, LogLevel, StandardWriter};
use crate::stealth::{stealth_chrome_args, STEALTH_JS};
use crate::types::options::{Options, PageLoadStrategy};
use crate::types::result::{Headers, Request, Response};
use crate::utils::formfill::{self, FormFillData};

/// Headless Chrome fetcher (celestia headless engine).
pub struct HeadlessFetcher {
    options: Arc<Options>,
    /// Consecutive fetch failures for `-mfc` (max-failure-count).
    consecutive_failures: std::sync::atomic::AtomicUsize,
    control: Arc<CrawlControl>,
}

impl HeadlessFetcher {
    pub fn new(options: Arc<Options>, control: Arc<CrawlControl>) -> std::result::Result<Self, String> {
        Ok(HeadlessFetcher {
            options,
            consecutive_failures: std::sync::atomic::AtomicUsize::new(0),
            control,
        })
    }

    /// Should XHR interception be installed for this request.
    fn xhr_enabled(&self) -> bool {
        self.options.xhr_extraction || self.options.headless_hybrid
    }
}

#[async_trait]
impl PageFetch for HeadlessFetcher {
    async fn fetch(&self, request: &Request) -> std::result::Result<Response, String> {
        // Stop the crawl after too many consecutive headless failures
        // (reference crawler max-failure-count in the headless loop).
        if self.options.max_failure_count > 0
            && self.consecutive_failures.load(std::sync::atomic::Ordering::SeqCst)
                >= self.options.max_failure_count as usize
        {
            log(
                LogLevel::Warning,
                &format!(
                    "stopping crawl: max consecutive failures ({}) exceeded",
                    self.options.max_failure_count
                ),
            );
            self.control.cancel();
            return Err("max failure count exceeded".into());
        }

        let options = Arc::clone(&self.options);
        let request = request.clone();
        let xhr_enabled = self.xhr_enabled();
        let max_wait = Duration::from_secs(self.options.timeout.max(1) + 60);

        // Render on a blocking thread (headless_chrome is sync).
        let render = tokio::task::spawn_blocking(move || {
            render_page(&options, &request, xhr_enabled)
        });
        let result = match tokio::time::timeout(max_wait, render).await {
            Ok(Ok(result)) => result,
            Ok(Err(join_err)) => Err(format!("render task failed: {join_err}")),
            Err(_) => Err("headless rendering timed out".into()),
        };
        match &result {
            Ok(_) => self
                .consecutive_failures
                .store(0, std::sync::atomic::Ordering::SeqCst),
            Err(_) => {
                self.consecutive_failures
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
        result
    }
}

/// Render a page in headless Chrome and extract the live DOM + XHR records.
fn render_page(
    options: &Options,
    request: &Request,
    xhr_enabled: bool,
) -> std::result::Result<Response, String> {
    // Browser pool: reuse a running Chrome across requests
    // (reference crawler `browserPool`); the browser returns to the pool
    // when this render finishes.
    let browser = match pop_pooled_browser() {
        Some(b) => b,
        None => launch_browser(options)?,
    };
    let render_result = render_with_browser(options, request, xhr_enabled, &browser);
    push_pooled_browser(browser);
    render_result
}

/// Max browsers kept alive in the pool (idle ones beyond this are closed).
const BROWSER_POOL_MAX: usize = 8;

/// Process-wide browser pool (reference crawler `rod.NewPool[BrowserPage]`).
static BROWSER_POOL: std::sync::Mutex<Vec<headless_chrome::Browser>> =
    std::sync::Mutex::new(Vec::new());

fn pop_pooled_browser() -> Option<headless_chrome::Browser> {
    BROWSER_POOL.lock().ok()?.pop()
}

fn push_pooled_browser(browser: headless_chrome::Browser) {
    if let Ok(mut pool) = BROWSER_POOL.lock() {
        if pool.len() < BROWSER_POOL_MAX {
            pool.push(browser);
            return;
        }
    }
    // Dropping the browser closes the Chrome process.
    drop(browser);
}

fn render_with_browser(
    options: &Options,
    request: &Request,
    xhr_enabled: bool,
    browser: &headless_chrome::Browser,
) -> std::result::Result<Response, String> {
    let tab = browser
        .new_tab()
        .map_err(|e| format!("failed to open new tab: {e}"))?;

    if options.stealth {
        let _ = tab.evaluate(STEALTH_JS, false);
    }

    // Install XHR/fetch hooks before any page script runs (celestia hijack).
    if xhr_enabled {
        let _ = tab.call_method(
            headless_chrome::protocol::cdp::Page::AddScriptToEvaluateOnNewDocument {
                source: xhr_hook_js(),
                world_name: None,
                include_command_line_api: None,
                run_immediately: None,
            },
        );
    }

    tab.navigate_to(&request.url)
        .map_err(|e| format!("failed to navigate to '{}': {e}", request.url))?;
    let _ = tab.wait_until_navigated();

    // Auto-login on the seed page (-al).
    if !options.auth_credentials.is_empty() && request.depth == 0 {
        let Some((user, pass)) = options.auth_credentials.split_once(':') else {
            return Err("auth credentials must be in username:password format".into());
        };
        perform_login(&tab, user, pass);
        let _ = tab.wait_until_navigated();
    }

    // Page load strategy wait (-pls / -dwt / -time-stable).
    apply_page_load_strategy(&tab, options);

    // CAPTCHA detection & optional solving.
    let content = tab
        .get_content()
        .map_err(|e| format!("failed to get DOM content: {e}"))?;
    if captcha::looks_like_captcha(&content) {
        log(LogLevel::Info, &format!("CAPTCHA detected on {}", request.url));
        if !options.captcha_solver_provider.is_empty() && !options.captcha_solver_api_key.is_empty()
        {
            let solved = captcha::solve_with_provider_blocking(
                &options.captcha_solver_provider,
                &options.captcha_solver_api_key,
                &content,
                &request.url,
            );
            if let Ok(token) = solved {
                let _ = tab.evaluate(&captcha::token_injection_js(&token), false);
                log(LogLevel::Info, "CAPTCHA token injected");
            }
        }
    }

    // Automatic form fill + submit (-aff), using the form fill config data.
    if options.automatic_form_fill {
        let fill_js = form_fill_js(&formfill_values_js(options));
        let _ = tab.evaluate(&fill_js, false);
        let _ = tab.evaluate("document.querySelectorAll('form')[0] && document.querySelectorAll('form')[0].requestSubmit ? document.querySelectorAll('form')[0].requestSubmit() : document.querySelectorAll('form')[0] && document.querySelectorAll('form')[0].submit()", false);
        let _ = tab.wait_until_navigated();
    }

    // Stable wait after interactions (reference crawler caps timeStable at
    // timeout/2 for the hybrid engine).
    let stable_cap = Duration::from_secs(options.timeout.max(1) / 2).max(Duration::from_secs(1));
    let stable_wait = Duration::from_secs(options.time_stable.max(0)).min(stable_cap);
    std::thread::sleep(stable_wait);

    // Simulate clicks on onclick links capped by -ol (reference crawler
    // hybrid crawl.go onclick simulation; native JS approximation).
    let onclick_links = collect_onclick_links(&tab, options);

    // Collect XHR records captured by the hook (-xhr).
    let mut xhr_requests = Vec::new();
    if xhr_enabled {
        if let Ok(result) = tab.evaluate("JSON.stringify(window.__celestia_xhr || [])", false) {
            if let Some(value) = result.value {
                if let Some(list) = value.as_str().and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()) {
                    if let Some(arr) = list.as_array() {
                        for entry in arr {
                            xhr_requests.push(Request {
                                method: entry
                                    .get("method")
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("GET")
                                    .to_uppercase(),
                                url: entry
                                    .get("url")
                                    .and_then(|u| u.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                source: request.url.clone(),
                                tag: "xhr".into(),
                                depth: request.depth,
                                root_hostname: request.root_hostname.clone(),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
    }

    let html = tab
        .get_content()
        .map_err(|e| format!("failed to get DOM content: {e}"))?;
    let content_length = html.len() as i64;
    let mut headers = Headers::new();
    headers.insert("content-type".into(), "text/html; charset=utf-8".into());

    Ok(Response {
        depth: request.depth,
        status_code: 200,
        headers,
        body: html,
        content_length,
        root_hostname: request.root_hostname.clone(),
        source: request.url.clone(),
        xhr_requests,
        onclick_links,
        ..Default::default()
    })
}

/// Launch or connect to Chrome per the options (celestia headless browser pool).
fn launch_browser(options: &Options) -> std::result::Result<headless_chrome::Browser, String> {
    use headless_chrome::{Browser, LaunchOptions};

    // Attach to an external Chrome instance (-cwu).
    if !options.chrome_ws_url.is_empty() {
        return Browser::connect(normalize_ws_url(&options.chrome_ws_url))
            .map_err(|e| format!("failed to connect to Chrome at {}: {e}", options.chrome_ws_url));
    }

    let mut chrome_args: Vec<&OsStr> = Vec::new();
    if options.stealth {
        for arg in stealth_chrome_args() {
            chrome_args.push(OsStr::new(arg));
        }
    }
    // Additional Chrome arguments (-ho).
    let owned_args: Vec<String> = options
        .headless_optional_arguments
        .iter()
        .map(|a| a.trim_start_matches("--").to_string())
        .collect();
    for a in &owned_args {
        chrome_args.push(OsStr::new(a));
    }
    if options.headless_no_sandbox {
        chrome_args.push(OsStr::new("no-sandbox"));
    }
    // Proxy through the browser (-proxy; reference crawler sets --proxy-server).
    let proxy_arg: String;
    if !options.proxy.is_empty() {
        proxy_arg = format!("proxy-server={}", options.proxy);
        chrome_args.push(OsStr::new(&proxy_arg));
    }

    let path = if !options.system_chrome_path.is_empty() {
        Some(std::path::PathBuf::from(&options.system_chrome_path))
    } else if options.use_installed_chrome {
        detect_system_chrome()
    } else {
        None
    };

    let launch = LaunchOptions {
        headless: !options.show_browser,
        sandbox: !options.headless_no_sandbox,
        args: chrome_args,
        path,
        user_data_dir: (!options.chrome_data_dir.is_empty())
            .then(|| std::path::PathBuf::from(&options.chrome_data_dir)),
        ..LaunchOptions::default()
    };

    Browser::new(launch).map_err(|e| format!("failed to launch headless Chrome: {e}"))
}

/// Normalize a Chrome debugger URL for `Browser::connect` (ws:// or host:port).
fn normalize_ws_url(input: &str) -> String {
    if input.starts_with("ws://") || input.starts_with("wss://") {
        input.to_string()
    } else if let Some(rest) = input.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if let Some(rest) = input.strip_prefix("https://") {
        format!("wss://{rest}")
    } else {
        format!("ws://{input}")
    }
}

/// Locate a system Chrome binary (-sc).
fn detect_system_chrome() -> Option<std::path::PathBuf> {
    for candidate in [
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ] {
        let p = std::path::PathBuf::from(candidate);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Apply the page load wait strategy to the tab.
fn apply_page_load_strategy(tab: &Arc<headless_chrome::Tab>, options: &Options) {
    let deadline = Instant::now() + Duration::from_secs(options.timeout.max(1));
    match options.page_load_strategy {
        PageLoadStrategy::None => {}
        PageLoadStrategy::Load => {
            // wait_until_navigated already ran; nothing further.
        }
        PageLoadStrategy::DomContentLoaded => {
            let wait = Duration::from_secs(options.dom_wait_time.max(0));
            let until = std::cmp::min(Instant::now() + wait, deadline);
            while Instant::now() < until {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        PageLoadStrategy::NetworkIdle => {
            // Poll resource entry count until stable for ~500ms.
            let mut last = 0usize;
            let mut stable = 0;
            while Instant::now() < deadline {
                let count = tab
                    .evaluate(
                        "window.performance.getEntriesByType('resource').length",
                        false,
                    )
                    .ok()
                    .and_then(|r| r.value)
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                if count == last && count > 0 {
                    stable += 1;
                    if stable >= 3 {
                        break;
                    }
                } else {
                    stable = 0;
                    last = count;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        PageLoadStrategy::Heuristic => {
            // Poll DOM size stability (existing renderer heuristic).
            let mut last_len = 0usize;
            let mut stable_count = 0;
            while Instant::now() < deadline {
                let len = tab
                    .get_content()
                    .map(|c| c.len())
                    .unwrap_or(0);
                if len > 300 && len == last_len {
                    stable_count += 1;
                    if stable_count >= 2 {
                        break;
                    }
                } else {
                    last_len = len;
                    stable_count = 0;
                }
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
}

/// Fill and submit a login form with the given credentials (-al).
fn perform_login(tab: &Arc<headless_chrome::Tab>, user: &str, pass: &str) {
    let js = format!(
        r#"
(function() {{
  var u = {user:?}, p = {pass:?};
  var inputs = Array.from(document.querySelectorAll('input'));
  var userInput = inputs.find(i => /user|email|login/i.test(i.name + ' ' + i.id + ' ' + (i.type||'')));
  var passInput = inputs.find(i => i.type === 'password');
  if (userInput) {{ userInput.value = u; userInput.dispatchEvent(new Event('input', {{bubbles: true}})); }}
  if (passInput) {{ passInput.value = p; passInput.dispatchEvent(new Event('input', {{bubbles: true}})); }}
  var form = (userInput || passInput || {{}}).form || document.querySelector('form');
  if (form) {{ form.requestSubmit ? form.requestSubmit() : form.submit(); }}
}})();
"#,
        user = user,
        pass = pass
    );
    let _ = tab.evaluate(&js, false);
}

/// Extract URLs from onclick handlers / javascript: links, capped by
/// `-ol` (native approximation of the reference crawler's onclick click
/// simulation in hybrid/crawl.go).
fn collect_onclick_links(tab: &Arc<headless_chrome::Tab>, options: &Options) -> Vec<String> {
    if options.max_onclick_links <= 0 {
        return Vec::new();
    }
    let js = format!(
        r#"
(function(max) {{
  var out = [];
  var seen = {{}};
  var re = /['"]((?:https?:\/\/|\/)[^'"]+)['"]/g;
  var els = document.querySelectorAll('a[onclick], [onclick], a[href^="javascript:"]');
  for (var i = 0; i < els.length && out.length < max; i++) {{
    var text = (els[i].getAttribute('onclick') || '') + ' ' + (els[i].getAttribute('href') || '');
    var m;
    while ((m = re.exec(text)) !== null && out.length < max) {{
      var u = m[1];
      if (!seen[u]) {{ seen[u] = true; out.push(u); }}
    }}
  }}
  return out;
}})({max});
"#,
        max = options.max_onclick_links
    );
    let mut links = Vec::new();
    if let Ok(result) = tab.evaluate(&js, false) {
        if let Some(value) = result.value {
            if let Some(arr) = value.as_array() {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        links.push(s.to_string());
                    }
                }
            }
        }
    }
    links
}

/// JSON object of input-type → fill value from the form fill data
/// (reference crawler `-fc` FormFillSuggestions used by processForm).
fn formfill_values_js(options: &Options) -> String {
    let data = if options.form_config.is_empty() {
        FormFillData::default()
    } else {
        formfill::load_form_config(&options.form_config).unwrap_or_default()
    };
    let object = serde_json::json!({
        "email": data.email,
        "password": data.password,
        "tel": data.phone,
        "color": data.color,
        "text": data.placeholder,
        "search": data.placeholder,
        "placeholder": data.placeholder,
    });
    object.to_string()
}

/// Document-start XHR/fetch interception JS (celestia hijack hook).
fn xhr_hook_js() -> String {
    r#"
window.__celestia_xhr = window.__celestia_xhr || [];
(function() {
  var record = function(method, url) {
    try { window.__celestia_xhr.push({method: method, url: String(url)}); } catch (e) {}
  };
  var origOpen = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function(method, url) {
    record(method || 'GET', url);
    return origOpen.apply(this, arguments);
  };
  if (window.fetch) {
    var origFetch = window.fetch;
    window.fetch = function(input, init) {
      try {
        var url = typeof input === 'string' ? input : (input && input.url) || '';
        var method = (init && init.method) || (input && input.method) || 'GET';
        record(method, url);
      } catch (e) {}
      return origFetch.apply(this, arguments);
    };
  }
})();
"#
    .to_string()
}

/// JS that fills visible inputs using the provided values map
/// (celestia -aff page step, driven by `-fc` form config data).
fn form_fill_js(values_json: &str) -> String {
    format!(
        r#"
(function() {{
  var values = {values_json};
  var inputs = document.querySelectorAll('input:not([type=hidden]), textarea');
  inputs.forEach(function(i) {{
    if (i.value) return;
    var t = (i.type || 'text').toLowerCase();
    if (values[t]) {{ i.value = values[t]; }}
    else if (i.placeholder) {{ i.value = i.placeholder; }}
    else {{ i.value = values.text || 'celestia'; }}
    i.dispatchEvent(new Event('input', {{bubbles: true}}));
  }});
}})();
"#
    )
}

/// Build and run a headless-engine crawler for one seed.
pub async fn crawl_headless(
    options: Arc<Options>,
    writer: Arc<StandardWriter>,
    control: Arc<CrawlControl>,
    seed: &str,
) -> std::result::Result<Arc<Crawler>, String> {
    let fetcher = Arc::new(HeadlessFetcher::new(Arc::clone(&options), control.clone())?);
    let crawler = Arc::new(Crawler::new(options, fetcher, writer, control)?);
    crawler.crawl(seed).await?;
    Ok(crawler)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_ws_url() {
        assert_eq!(normalize_ws_url("ws://x:9222/dev"), "ws://x:9222/dev");
        assert_eq!(normalize_ws_url("http://127.0.0.1:9222"), "ws://127.0.0.1:9222");
        assert_eq!(normalize_ws_url("127.0.0.1:9222"), "ws://127.0.0.1:9222");
    }

    #[test]
    fn test_xhr_hook_js_contains_hook() {
        let js = xhr_hook_js();
        assert!(js.contains("__celestia_xhr"));
        assert!(js.contains("XMLHttpRequest"));
        assert!(js.contains("fetch"));
    }

    #[test]
    fn test_fetcher_new() {
        let o = Arc::new(Options::with_defaults());
        assert!(HeadlessFetcher::new(o, Arc::new(CrawlControl::default())).is_ok());
    }
}
