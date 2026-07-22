use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::exporter::FileStorageExporter;
use crate::pipeline::BrowserPipeline;
use crate::types::{
    AnalysisCallback, CrawlSummary, PageIR, RenderMode, RenderOptions, StorageExporter,
    TimeoutStrategy, WaitUntil,
};

/// High-level facade for configuring and running the 4-phase Browser pipeline.
pub struct Browser {
    start_url: String,
    max_depth: usize,
    pipeline: BrowserPipeline,
    callback: Option<AnalysisCallback>,
}

impl Browser {
    /// Creates a new `BrowserBuilder` instance.
    ///
    /// # Examples
    /// ```
    /// use browser_crawler::Browser;
    ///
    /// let builder = Browser::builder();
    /// ```
    pub fn builder() -> BrowserBuilder {
        BrowserBuilder::default()
    }

    /// Asynchronously runs the 4-phase queue-based streaming browser pipeline.
    pub async fn run(&self) -> Result<CrawlSummary, String> {
        let dummy_callback: AnalysisCallback = Box::new(|_| Box::pin(async { Ok(()) }));
        let callback_ref = self.callback.as_ref().unwrap_or(&dummy_callback);

        self.pipeline
            .run_streaming_crawl(&self.start_url, self.max_depth, callback_ref)
            .await
    }
}

/// Fluent builder for constructing a [`Browser`] instance.
#[derive(Default)]
pub struct BrowserBuilder {
    start_url: Option<String>,
    max_depth: Option<usize>,
    user_agent: Option<String>,
    output_dir: Option<PathBuf>,
    exporter: Option<Arc<dyn StorageExporter>>,
    callback: Option<AnalysisCallback>,
    render_options: RenderOptions,
}

impl BrowserBuilder {
    /// Sets the target starting URL for crawling.
    pub fn start_url(mut self, url: impl Into<String>) -> Self {
        self.start_url = Some(url.into());
        self
    }

    /// Sets the maximum recursive crawling depth (default is `2`).
    pub fn max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Sets the HTTP User-Agent string.
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// Sets the target directory path for file export.
    pub fn output_dir<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.output_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Sets a custom `StorageExporter` sink for Phase 4 export.
    pub fn exporter(mut self, exporter: Arc<dyn StorageExporter>) -> Self {
        self.exporter = Some(exporter);
        self
    }

    /// Sets the rendering mode (`RenderMode::Static` vs `RenderMode::Dynamic`).
    pub fn render_mode(mut self, mode: RenderMode) -> Self {
        self.render_options.render_mode = mode;
        self
    }

    /// Sets the wait condition for dynamic JS rendering.
    pub fn wait_until(mut self, wait: WaitUntil) -> Self {
        self.render_options.wait_until = wait;
        self
    }

    /// Shorthand to wait until a specific CSS selector appears in the live DOM.
    pub fn wait_for_selector(mut self, selector: impl Into<String>) -> Self {
        self.render_options.wait_until = WaitUntil::Selector(selector.into());
        self
    }

    /// Sets the timeout limit for page rendering (default is 10 seconds).
    pub fn render_timeout(mut self, timeout: Duration) -> Self {
        self.render_options.render_timeout = timeout;
        self
    }

    /// Sets the recovery action executed when page rendering times out.
    pub fn timeout_strategy(mut self, strategy: TimeoutStrategy) -> Self {
        self.render_options.timeout_strategy = strategy;
        self
    }

    /// Enables or disables anti-bot stealth mode (default is `true`).
    pub fn stealth(mut self, enabled: bool) -> Self {
        self.render_options.stealth = enabled;
        self
    }

    /// Enables or disables Debug Inspector mode (default is `false`).
    /// Logs Chrome CDP network responses and browser console errors during rendering.
    pub fn debug(mut self, enabled: bool) -> Self {
        self.render_options.debug = enabled;
        self
    }

    /// Sets an async callback function for Phase 3 content analysis.
    pub fn analysis_callback(mut self, callback: AnalysisCallback) -> Self {
        self.callback = Some(callback);
        self
    }

    /// Helper method allowing inline async closures for Phase 3 content analysis.
    pub fn on_page<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(PageIR) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let callback: AnalysisCallback = Box::new(move |ir| Box::pin(f(ir)));
        self.callback = Some(callback);
        self
    }

    /// Builds and validates the [`Browser`] instance.
    pub fn build(self) -> Result<Browser, String> {
        let start_url = self
            .start_url
            .ok_or_else(|| "Missing required parameter 'start_url' for BrowserBuilder".to_string())?;

        let max_depth = self.max_depth.unwrap_or(2);
        let user_agent = self
            .user_agent
            .unwrap_or_else(|| "RustAIBrowser/1.0".to_string());

        let exporter: Arc<dyn StorageExporter> = match self.exporter {
            Some(exp) => exp,
            None => {
                let out_dir = self.output_dir.unwrap_or_else(|| PathBuf::from("out"));
                Arc::new(FileStorageExporter::new(out_dir))
            }
        };

        let pipeline = BrowserPipeline::builder()
            .user_agent(&user_agent)
            .render_options(self.render_options)
            .exporter(exporter)
            .build();

        Ok(Browser {
            start_url,
            max_depth,
            pipeline,
            callback: self.callback,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_builder_missing_start_url() {
        let result = Browser::builder().build();
        match result {
            Err(err) => assert!(err.contains("start_url")),
            Ok(_) => panic!("Expected error due to missing start_url"),
        }
    }

    #[test]
    fn test_browser_builder_defaults() {
        let browser = Browser::builder()
            .start_url("https://example.com")
            .render_mode(RenderMode::Dynamic)
            .stealth(true)
            .debug(true)
            .wait_for_selector("main")
            .build();
        assert!(browser.is_ok());
    }
}
