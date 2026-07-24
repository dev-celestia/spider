use dashmap::DashSet;
use reqwest::Client;
use scraper::{Html, Selector};
use std::collections::VecDeque;
use std::sync::Arc;
use url::Url;

use crate::exporter::FileStorageExporter;
use crate::renderer::PageFetcher;
use crate::transformer::transform_html_to_ir;
use crate::types::{AnalysisCallback, CrawlSummary, RenderOptions, SitemapNode, StorageExporter};

/// Internal task entry pushed onto the navigation queue.
#[derive(Debug, Clone)]
struct CrawlTask {
    url: String,
    depth: usize,
}

/// Builder for constructing [`BrowserPipeline`] instances.
#[derive(Default)]
pub struct BrowserPipelineBuilder {
    user_agent: String,
    render_options: RenderOptions,
    exporter: Option<Arc<dyn StorageExporter>>,
}

impl BrowserPipelineBuilder {
    /// Creates a new `BrowserPipelineBuilder` with default settings.
    pub fn new() -> Self {
        Self {
            user_agent: "RustAIBrowser/1.0".to_string(),
            render_options: RenderOptions::default(),
            exporter: None,
        }
    }

    /// Sets the HTTP User-Agent string.
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = ua.into();
        self
    }

    /// Sets page rendering configuration.
    pub fn render_options(mut self, options: RenderOptions) -> Self {
        self.render_options = options;
        self
    }

    /// Sets the Phase 4 [`StorageExporter`].
    pub fn exporter(mut self, exporter: Arc<dyn StorageExporter>) -> Self {
        self.exporter = Some(exporter);
        self
    }

    /// Builds the [`BrowserPipeline`].
    pub fn build(self) -> BrowserPipeline {
        let client = Client::builder()
            .user_agent(&self.user_agent)
            .build()
            .unwrap_or_default();

        let fetcher = PageFetcher::new(client, self.render_options);

        let exporter = self
            .exporter
            .unwrap_or_else(|| Arc::new(FileStorageExporter::default()));

        BrowserPipeline { fetcher, exporter }
    }
}

/// Browser pipeline engine orchestrating Phase 1–4 execution via queue-based streaming.
pub struct BrowserPipeline {
    fetcher: PageFetcher,
    exporter: Arc<dyn StorageExporter>,
}

/// Backwards-compatible type alias for [`BrowserPipeline`].
pub type CrawlerPipeline = BrowserPipeline;

impl BrowserPipeline {
    /// Creates a new `BrowserPipelineBuilder` instance.
    pub fn builder() -> BrowserPipelineBuilder {
        BrowserPipelineBuilder::new()
    }

    /// Creates a new pipeline equipped with a specified `StorageExporter`.
    pub fn new(exporter: Arc<dyn StorageExporter>) -> Self {
        Self::builder().exporter(exporter).build()
    }

    /// Fetches a single page URL and converts it to a token-optimized [`crate::types::PageIR`].
    pub async fn fetch_page(&self, url: &str) -> Result<crate::types::PageIR, String> {
        let html = self.fetcher.fetch_html(url).await?;
        Ok(transform_html_to_ir(url, &html))
    }

    /// Executes an interleaved, queue-based streaming crawl starting at `start_url`.
    ///
    /// Pushes `start_url` onto the navigation queue, pops tasks one by one to fetch HTML,
    /// extract `PageIR` markdown, run the analysis callback, export to disk/storage, scan for child links,
    /// and push unvisited links back to the queue until the queue is completely drained.
    pub async fn run_streaming_crawl(
        &self,
        start_url: &str,
        max_depth: usize,
        callback: &AnalysisCallback,
    ) -> Result<CrawlSummary, String> {
        let visited = DashSet::new();
        let mut queue = VecDeque::new();
        let mut pages_processed = 0;
        let mut total_ir_bytes = 0;
        let mut visited_urls = Vec::new();

        // 1. Push landing page task into queue
        visited.insert(start_url.to_string());
        queue.push_back(CrawlTask {
            url: start_url.to_string(),
            depth: 0,
        });

        let a_selector = Selector::parse("a[href]").unwrap();

        // 2. WHILE Queue is NOT Empty: POP Next CrawlTask
        while let Some(task) = queue.pop_front() {
            println!("[Streaming Task] Visiting URL (Depth {}): {}", task.depth, task.url);

            // Fetch HTML (Static or Dynamic Headless Chrome with Stealth)
            let html = match self.fetcher.fetch_html(&task.url).await {
                Ok(html) => html,
                Err(err) => {
                    eprintln!("[Warning] Failed to fetch URL '{}': {}", task.url, err);
                    continue;
                }
            };

            // Phase 2: Generate IR
            let ir = transform_html_to_ir(&task.url, &html);
            total_ir_bytes += ir.markdown_ir.len();
            pages_processed += 1;
            visited_urls.push(task.url.clone());

            // Phase 3: Execute user callback logic
            if let Err(err) = callback(ir.clone()).await {
                eprintln!("[Warning] Analysis callback failed for URL '{}': {}", task.url, err);
            }

            // Phase 4: Export to destination storage sink
            if let Err(err) = self.exporter.export(&ir).await {
                eprintln!("[Warning] Storage export failed for URL '{}': {}", task.url, err);
            }

            // Scan HTML for same-domain child links if depth limit permits
            if task.depth < max_depth {
                if let Ok(base_uri) = Url::parse(&task.url) {
                    let document = Html::parse_document(&html);
                    let mut found_count = 0;

                    for element in document.select(&a_selector) {
                        if let Some(href) = element.value().attr("href") {
                            if let Ok(joined) = base_uri.join(href) {
                                if joined.host() == base_uri.host()
                                    && (joined.scheme() == "http" || joined.scheme() == "https")
                                {
                                    let link = joined.to_string();
                                    if !visited.contains(&link) {
                                        visited.insert(link.clone());
                                        // PUSH discovered child task to navigation queue!
                                        queue.push_back(CrawlTask {
                                            url: link,
                                            depth: task.depth + 1,
                                        });
                                        found_count += 1;
                                    }
                                }
                            }
                        }
                    }

                    if found_count > 0 {
                        println!(
                            "[Discovery] Found {} new link(s) on {}. Pushed to queue (Remaining Queue: {}).",
                            found_count, task.url, queue.len()
                        );
                    }
                }
            }
        }

        Ok(CrawlSummary {
            pages_processed,
            total_ir_bytes,
            visited_urls,
        })
    }

    /// Asynchronously processes a pre-built `SitemapNode` graph recursively (legacy mode).
    pub async fn process_sitemap(
        &self,
        node: &SitemapNode,
        callback: &AnalysisCallback,
    ) -> Result<(), String> {
        println!("[Phase 2] Extracting IR: {}", node.url);

        if let Ok(html) = self.fetcher.fetch_html(&node.url).await {
            let ir = transform_html_to_ir(&node.url, &html);
            callback(ir.clone()).await?;
            self.exporter.export(&ir).await?;
        }

        for child in &node.children {
            Box::pin(self.process_sitemap(child, callback)).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PageIR;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestExporter {
        export_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl StorageExporter for TestExporter {
        async fn export(&self, _ir: &PageIR) -> Result<(), String> {
            self.export_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_pipeline_instantiation() {
        let count = Arc::new(AtomicUsize::new(0));
        let exporter = Arc::new(TestExporter {
            export_count: count.clone(),
        });
        let _pipeline = BrowserPipeline::new(exporter);
        assert_eq!(count.load(Ordering::SeqCst), 0);

        let _builder_pipeline = BrowserPipeline::builder().build();
    }
}
