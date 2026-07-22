//! # browser-crawler
//! 
//! A high-performance Rust web browsing library and AI Intermediate Representation (IR) generator.
//! Built around a streaming builder architecture (`Browser::builder()`) with support for static HTTP fetching,
//! dynamic JavaScript rendering, and anti-bot stealth mode via Headless Chrome.
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
pub use renderer::PageFetcher;
pub use stealth::{stealth_chrome_args, STEALTH_JS};
pub use transformer::transform_html_to_ir;
pub use types::{
    AnalysisCallback, CrawlSummary, PageIR, RenderMode, RenderOptions, SitemapNode,
    StorageExporter, TimeoutStrategy, WaitUntil,
};
