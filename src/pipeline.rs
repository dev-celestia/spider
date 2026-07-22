use reqwest::Client;
use std::sync::Arc;

use crate::transformer::transform_html_to_ir;
use crate::types::{AnalysisCallback, SitemapNode, StorageExporter};

/// Crawler pipeline engine orchestrating Phase 2–4 execution.
///
/// Fetches HTML content for URLs in a `SitemapNode` graph, converts them into `PageIR` markdown,
/// executes user analysis callbacks, and exports payloads to storage sinks.
pub struct CrawlerPipeline {
    client: Client,
    exporter: Arc<dyn StorageExporter>,
}

impl CrawlerPipeline {
    /// Creates a new pipeline equipped with a specified `StorageExporter`.
    pub fn new(exporter: Arc<dyn StorageExporter>) -> Self {
        Self {
            client: Client::builder()
                .user_agent("RustAICrawler/1.0")
                .build()
                .unwrap_or_default(),
            exporter,
        }
    }

    /// Asynchronously processes a `SitemapNode` graph recursively.
    ///
    /// For each node:
    /// 1. Fetches HTML source over HTTP/HTTPS.
    /// 2. Converts HTML into `PageIR` markdown (Phase 2).
    /// 3. Invokes the `callback` closure (Phase 3).
    /// 4. Dispatches the `PageIR` to `exporter` (Phase 4).
    /// 5. Recursively traverses child sitemap nodes.
    pub async fn process_sitemap(
        &self,
        node: &SitemapNode,
        callback: &AnalysisCallback,
    ) -> Result<(), String> {
        println!("[Phase 2] Extracting IR: {}", node.url);

        // Fetch & transform page content
        if let Ok(resp) = self.client.get(&node.url).send().await {
            if let Ok(html) = resp.text().await {
                // Phase 2: Generate IR
                let ir = transform_html_to_ir(&node.url, &html);

                // Phase 3: Execute user callback logic
                callback(ir.clone()).await?;

                // Phase 4: Export to destination sink
                self.exporter.export(&ir).await?;
            }
        }

        // Traverse remaining nodes sequentially or via task parallelism
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
        let _pipeline = CrawlerPipeline::new(exporter);
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }
}
