use std::sync::Arc;
use browser_crawler::{
    AnalysisCallback, CrawlerPipeline, PageIR, SiteMapper, StorageExporter,
};

struct LocalFileExporter;

#[async_trait::async_trait]
impl StorageExporter for LocalFileExporter {
    async fn export(&self, ir: &PageIR) -> Result<(), String> {
        let sanitized = ir
            .url
            .replace("https://", "")
            .replace("http://", "")
            .replace('/', "_");
        let filename = format!("out_{}.md", sanitized);
        tokio::fs::write(&filename, &ir.markdown_ir)
            .await
            .map_err(|e| e.to_string())?;
        println!("[Phase 4] Saved to disk: {}", filename);
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let start_url = "https://example.com";

    println!("--- Phase 1: Mapping Site Structure ---");
    let mapper = SiteMapper::new(2);
    let sitemap = mapper
        .map_site(start_url)
        .await
        .expect("Failed to map site");

    println!("Sitemap generated for URL: {}", sitemap.url);

    // 2. Define Phase 3 Hook
    let analysis_hook: AnalysisCallback = Box::new(|page_ir| {
        Box::pin(async move {
            println!("[Phase 3] Processing URL: {}", page_ir.url);
            println!("Extracted Text Size: {} chars", page_ir.markdown_ir.len());
            Ok(())
        })
    });

    // 3. Phase 2-4: Process Pipeline
    let exporter = Arc::new(LocalFileExporter);
    let pipeline = CrawlerPipeline::new(exporter);

    pipeline
        .process_sitemap(&sitemap, &analysis_hook)
        .await
        .expect("Failed to process sitemap pipeline");
}
