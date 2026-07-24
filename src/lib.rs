//! # browser-crawler
//! 
//! A high-performance Rust web browsing library and AI Intermediate Representation (IR) generator.
//! Built around a streaming builder architecture (`Browser::builder()`) with support for static HTTP fetching,
//! dynamic JavaScript rendering, anti-bot stealth mode via Headless Chrome, and one-off single page crawling utilities (`crawl_single_page`).
//! 
//! ## Interleaved Queue-Based Streaming Architecture
//! 
//! Pushes starting landing page onto navigation queue, pops tasks one by one to fetch HTML, extract `PageIR` markdown,
//! execute analysis callbacks, export payloads to disk/storage sinks, scan HTML for same-domain child links, and push
//! discovered tasks back onto the queue until draining completes.
//! 
//! ## Quick Stealth Dynamic Rendering Example
//! 
//! ```rust,no_run
//! use std::time::Duration;
//! use browser_crawler::{Browser, RenderMode, WaitUntil, TimeoutStrategy};
//! 
//! #[tokio::main]
//! async fn main() -> Result<(), String> {
//!     let browser = Browser::builder()
//!         .start_url("https://example.com")
//!         .render_mode(RenderMode::Dynamic)
//!         .stealth(true)
//!         .wait_until(WaitUntil::NetworkIdle)
//!         .render_timeout(Duration::from_secs(10))
//!         .timeout_strategy(TimeoutStrategy::ExtractPartial)
//!         .on_page(|page_ir| async move {
//!             println!("Analyzing URL: {}", page_ir.url);
//!             Ok(())
//!         })
//!         .build()?;
//! 
//!     let summary = browser.run().await?;
//!     println!("Pages Processed: {}", summary.pages_processed);
//!     Ok(())
//! }
//! ```
//!
//! ## Single Page Crawl Utility Example
//!
//! ```rust,no_run
//! use browser_crawler::{crawl_single_page, RenderOptions, RenderMode};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), String> {
//!     let options = RenderOptions {
//!         render_mode: RenderMode::Static,
//!         ..Default::default()
//!     };
//!     let page_ir = crawl_single_page("https://example.com", &options).await?;
//!     println!("Title: {}", page_ir.title);
//!     println!("Markdown IR:\n{}", page_ir.markdown_ir);
//!     Ok(())
//! }
//! ```

pub mod builder;
pub mod exporter;
pub mod mapper;
pub mod pipeline;
pub mod renderer;
pub mod stealth;
pub mod transformer;
pub mod types;

pub use builder::{Browser, BrowserBuilder};
pub use exporter::{FileStorageExporter, FileStorageExporterBuilder};
pub use mapper::{SiteMapper, SiteMapperBuilder};
pub use pipeline::{BrowserPipeline, BrowserPipelineBuilder, CrawlerPipeline};
pub use renderer::{crawl_single_page, PageFetcher};
pub use stealth::{stealth_chrome_args, STEALTH_JS};
pub use transformer::{extract_links, transform_html_to_ir};
pub use types::{
    AnalysisCallback, CrawlSummary, PageIR, RenderMode, RenderOptions, SitemapNode,
    StorageExporter, TimeoutStrategy, WaitUntil,
};
