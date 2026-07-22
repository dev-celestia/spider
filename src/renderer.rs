use reqwest::Client;
use tokio::time::timeout;

use crate::stealth::{stealth_chrome_args, STEALTH_JS};
use crate::types::{RenderMode, RenderOptions, TimeoutStrategy, WaitUntil};

/// Unified HTML page fetcher supporting both static HTTP requests and Headless Chrome dynamic rendering with anti-bot stealth.
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
        match self.options.render_mode {
            RenderMode::Static => self.fetch_static(url).await,
            RenderMode::Dynamic => match self.fetch_dynamic(url).await {
                Ok(html) => Ok(html),
                Err(err) => {
                    eprintln!(
                        "[Warning] Headless Chrome execution encountered error for {}: {}. Evaluating timeout strategy...",
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
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed for '{url}': {e}"))?;

        resp.text()
            .await
            .map_err(|e| format!("Failed to read response body for '{url}': {e}"))
    }

    /// Fetches fully rendered DOM HTML using Headless Chrome CDP driver with anti-bot stealth mechanisms.
    async fn fetch_dynamic(&self, url: &str) -> Result<String, String> {
        let url_owned = url.to_string();
        let options = self.options.clone();

        // Offload blocking Headless Chrome CDP calls to a blocking thread task
        let render_task = tokio::task::spawn_blocking(move || -> Result<String, String> {
            use headless_chrome::{Browser as HeadlessBrowser, LaunchOptions};
            use std::ffi::OsStr;
            use std::time::{Duration, Instant};

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
                    let remaining = timeout_limit.saturating_sub(start_time.elapsed());
                    let _ = tab.wait_for_element_with_custom_timeout(selector, remaining);
                }
                WaitUntil::Delay(duration) => {
                    std::thread::sleep(duration);
                }
                WaitUntil::DomContentLoaded | WaitUntil::NetworkIdle => {
                    // Smart DOM settlement poller: poll DOM content length until DOM settles or timeout limit is reached
                    let poll_interval = Duration::from_millis(300);
                    let mut last_len = 0;
                    let mut stable_count = 0;

                    while start_time.elapsed() < timeout_limit {
                        if let Ok(content) = tab.get_content() {
                            let len = content.len();
                            // If DOM has rendered content (>300 chars) and length is stable across consecutive polls
                            if len > 300 && len == last_len {
                                stable_count += 1;
                                if stable_count >= 2 {
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

            // Always capture and return the live DOM content from Chrome!
            tab.get_content()
                .map_err(|e| format!("Failed to retrieve DOM content for '{url_owned}': {e}"))
        });

        // Add 2-second grace period to outer tokio timeout so inner Chrome thread finishes capturing DOM
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
