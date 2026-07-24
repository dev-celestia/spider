use reqwest::Client;
use tokio::time::timeout;

use crate::stealth::{stealth_chrome_args, STEALTH_JS};
use crate::types::{RenderMode, RenderOptions, TimeoutStrategy, WaitUntil};

/// Unified HTML page fetcher supporting both static HTTP requests and Headless Chrome dynamic rendering with anti-bot stealth and debug logging.
pub struct PageFetcher {
    client: Client,
    options: RenderOptions,
}

impl PageFetcher {
    /// Creates a new `PageFetcher` with custom HTTP client and render options.
    pub fn new(client: Client, options: RenderOptions) -> Self {
        Self { client, options }
    }

    /// Fetches HTML source code for the specified URL according to the configured [`RenderMode`].
    pub async fn fetch_html(&self, url: &str) -> Result<String, String> {
        if self.options.debug {
            println!(
                "[DEBUG Renderer] Mode: {:?}, Stealth: {}, Timeout: {:?}, Wait: {:?}",
                self.options.render_mode, self.options.stealth, self.options.render_timeout, self.options.wait_until
            );
        }

        match self.options.render_mode {
            RenderMode::Static => self.fetch_static(url).await,
            RenderMode::Dynamic => match self.fetch_dynamic(url).await {
                Ok(html) => {
                    if self.options.debug {
                        let _ = tokio::fs::create_dir_all("out").await;
                        let _ = tokio::fs::write("out/debug_dump.html", &html).await;
                        println!(
                            "[DEBUG Dump] Dumped {} bytes of live rendered DOM to out/debug_dump.html",
                            html.len()
                        );
                    }
                    Ok(html)
                }
                Err(err) => {
                    eprintln!(
                        "[Warning] Headless Chrome execution error for {}: {}. Evaluating timeout strategy...",
                        url, err
                    );
                    match self.options.timeout_strategy {
                        TimeoutStrategy::FallbackToStatic | TimeoutStrategy::ExtractPartial => {
                            eprintln!("[Info] Falling back to static HTTP fetch for {}", url);
                            self.fetch_static(url).await
                        }
                        TimeoutStrategy::FailFast => Err(err),
                    }
                }
            },
        }
    }

    /// Fetches raw static HTML using `reqwest`.
    async fn fetch_static(&self, url: &str) -> Result<String, String> {
        if self.options.debug {
            println!("[DEBUG Network] GET {} (Static HTTP)", url);
        }

        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed for '{url}': {e}"))?;

        if self.options.debug {
            println!(
                "[DEBUG Network] Response Status: {} for {}",
                resp.status(),
                url
            );
        }

        resp.text()
            .await
            .map_err(|e| format!("Failed to read response body for '{url}': {e}"))
    }

    /// Fetches fully rendered DOM HTML using Headless Chrome CDP driver with anti-bot stealth mechanisms and debug inspection.
    async fn fetch_dynamic(&self, url: &str) -> Result<String, String> {
        let url_owned = url.to_string();
        let options = self.options.clone();

        // Offload blocking Headless Chrome CDP calls to a blocking thread task
        let render_task = tokio::task::spawn_blocking(move || -> Result<String, String> {
            use headless_chrome::{Browser as HeadlessBrowser, LaunchOptions};
            use std::ffi::OsStr;
            use std::time::{Duration, Instant};

            if options.debug {
                println!("[DEBUG Headless] Launching Chrome CDP for {}", url_owned);
            }

            let mut chrome_args = Vec::new();
            if options.stealth {
                for arg in stealth_chrome_args() {
                    chrome_args.push(OsStr::new(arg));
                }
            }

            let launch_options = LaunchOptions {
                headless: true,
                sandbox: false,
                args: chrome_args,
                ..Default::default()
            };

            let browser = HeadlessBrowser::new(launch_options)
                .map_err(|e| format!("Failed to launch Headless Chrome: {e}"))?;

            let tab = browser
                .new_tab()
                .map_err(|e| format!("Failed to open new browser tab: {e}"))?;

            if options.stealth {
                let _ = tab.evaluate(STEALTH_JS, false);
            }

            tab.navigate_to(&url_owned)
                .map_err(|e| format!("Failed to navigate to '{url_owned}': {e}"))?;

            if options.stealth {
                let _ = tab.evaluate(STEALTH_JS, false);
            }

            let start_time = Instant::now();
            let timeout_limit = options.render_timeout;

            // Perform smart wait according to WaitUntil option
            match options.wait_until {
                WaitUntil::Selector(ref selector) => {
                    if options.debug {
                        println!("[DEBUG Wait] Waiting for element selector: {}", selector);
                    }
                    let remaining = timeout_limit.saturating_sub(start_time.elapsed());
                    let _ = tab.wait_for_element_with_custom_timeout(selector, remaining);
                }
                WaitUntil::Delay(duration) => {
                    if options.debug {
                        println!("[DEBUG Wait] Sleeping for delay {:?}", duration);
                    }
                    std::thread::sleep(duration);
                }
                WaitUntil::DomContentLoaded | WaitUntil::NetworkIdle => {
                    if options.debug {
                        println!("[DEBUG Wait] Waiting for DOM settlement polling...");
                    }
                    let poll_interval = Duration::from_millis(300);
                    let mut last_len = 0;
                    let mut stable_count = 0;

                    while start_time.elapsed() < timeout_limit {
                        if let Ok(content) = tab.get_content() {
                            let len = content.len();
                            if len > 300 && len == last_len {
                                stable_count += 1;
                                if stable_count >= 2 {
                                    if options.debug {
                                        println!(
                                            "[DEBUG Settlement] DOM settled at {} bytes after {:?}",
                                            len,
                                            start_time.elapsed()
                                        );
                                    }
                                    break;
                                }
                            } else {
                                last_len = len;
                                stable_count = 0;
                            }
                        }
                        std::thread::sleep(poll_interval);
                    }
                }
            }

            tab.get_content()
                .map_err(|e| format!("Failed to retrieve DOM content for '{url_owned}': {e}"))
        });

        let max_task_timeout = self.options.render_timeout + std::time::Duration::from_secs(3);

        match timeout(max_task_timeout, render_task).await {
            Ok(Ok(result)) => result,
            Ok(Err(join_err)) => Err(format!("Task execution panicked: {join_err}")),
            Err(_) => Err(format!(
                "Headless Chrome rendering timed out after {:?}",
                self.options.render_timeout
            )),
        }
    }
}

/// Asynchronously fetches a single webpage and transforms it into a token-optimized [`PageIR`] payload.
///
/// This convenience function executes a one-off page fetch (using static HTTP or dynamic rendering
/// according to the provided [`RenderOptions`]) without constructing a full recursive crawling pipeline.
///
/// # Arguments
///
/// * `url` - The target web URL string to fetch and parse.
/// * `options` - Configuration options specifying render mode, timeouts, wait strategies, and anti-bot stealth.
///
/// # Errors
///
/// Returns `Err(String)` if HTTP connection fails, Headless Chrome execution encounters an unrecoverable error,
/// or network request times out.
///
/// # Examples
///
/// ```rust,no_run
/// use browser_crawler::{crawl_single_page, RenderOptions, RenderMode};
///
/// #[tokio::main]
/// async fn main() -> Result<(), String> {
///     let options = RenderOptions {
///         render_mode: RenderMode::Static,
///         ..Default::default()
///     };
///     let page_ir = crawl_single_page("https://example.com", &options).await?;
///     println!("Page Title: {}", page_ir.title);
///     println!("Markdown Content:\n{}", page_ir.markdown_ir);
///     Ok(())
/// }
/// ```
pub async fn crawl_single_page(url: &str, options: &RenderOptions) -> Result<crate::types::PageIR, String> {
    let client = Client::builder()
        .user_agent("RustAIBrowser/1.0")
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;
    let fetcher = PageFetcher::new(client, options.clone());
    let html = fetcher.fetch_html(url).await?;
    Ok(crate::transformer::transform_html_to_ir(url, &html))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_fetcher_creation() {
        let client = Client::new();
        let options = RenderOptions::default();
        let _fetcher = PageFetcher::new(client, options);
    }
}

