use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::exporter::FileStorageExporter;
use crate::mapper::SiteMapper;
use crate::pipeline::BrowserPipeline;
use crate::types::{AnalysisCallback, PageIR, StorageExporter};

/// High-level facade for configuring and running the 4-phase Browser pipeline.
pub struct Browser {
    start_url: String,
    mapper: SiteMapper,
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

    /// Asynchronously runs the 4-phase browser crawling pipeline.
    ///
    /// 1. Maps website structure starting from `start_url` (Phase 1).
    /// 2. Transforms raw HTML into `PageIR` markdown (Phase 2).
    /// 3. Triggers the analysis callback hook if provided (Phase 3).
    /// 4. Exports results to the configured storage sink (Phase 4).
    pub async fn run(&self) -> Result<(), String> {
        let sitemap = self
            .mapper
            .map_site(&self.start_url)
            .await
            .ok_or_else(|| format!("Failed to map site for URL: {}", self.start_url))?;

        let dummy_callback: AnalysisCallback = Box::new(|_| Box::pin(async { Ok(()) }));
        let callback_ref = self.callback.as_ref().unwrap_or(&dummy_callback);

        self.pipeline.process_sitemap(&sitemap, callback_ref).await
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
    /// Defaults to `./out`. If a custom `exporter` is supplied, `output_dir` is ignored.
    pub fn output_dir<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.output_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Sets a custom `StorageExporter` sink for Phase 4 export.
    pub fn exporter(mut self, exporter: Arc<dyn StorageExporter>) -> Self {
        self.exporter = Some(exporter);
        self
    }

    /// Sets an async callback function for Phase 3 content analysis.
    pub fn analysis_callback(mut self, callback: AnalysisCallback) -> Self {
        self.callback = Some(callback);
        self
    }

    /// Helper method allowing inline async closures for Phase 3 content analysis.
    ///
    /// # Examples
    /// ```
    /// use browser_crawler::Browser;
    ///
    /// let browser = Browser::builder()
    ///     .start_url("https://example.com")
    ///     .on_page(|page_ir| async move {
    ///         println!("Page URL: {}", page_ir.url);
    ///         Ok(())
    ///     })
    ///     .build()
    ///     .unwrap();
    /// ```
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
    ///
    /// Returns an error string if mandatory options (like `start_url`) are missing.
    pub fn build(self) -> Result<Browser, String> {
        let start_url = self
            .start_url
            .ok_or_else(|| "Missing required parameter 'start_url' for BrowserBuilder".to_string())?;

        let max_depth = self.max_depth.unwrap_or(2);
        let user_agent = self
            .user_agent
            .unwrap_or_else(|| "RustAIBrowser/1.0".to_string());

        let mapper = SiteMapper::builder()
            .max_depth(max_depth)
            .user_agent(&user_agent)
            .build();

        let exporter: Arc<dyn StorageExporter> = match self.exporter {
            Some(exp) => exp,
            None => {
                let out_dir = self.output_dir.unwrap_or_else(|| PathBuf::from("out"));
                Arc::new(FileStorageExporter::new(out_dir))
            }
        };

        let pipeline = BrowserPipeline::builder()
            .user_agent(&user_agent)
            .exporter(exporter)
            .build();

        Ok(Browser {
            start_url,
            mapper,
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
            .build();
        assert!(browser.is_ok());
    }
}
