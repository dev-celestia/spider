//! # browser-crawler
//! 
//! A decoupled, 4-phase modular Rust library for web browsing, token-optimized HTML-to-Markdown 
//! Intermediate Representation (IR) generation, extensible content analysis callbacks, and pluggable storage sinks.
//! 
//! ## 4-Phase Modular Architecture
//! 
//! 1. **Phase 1: Sitemap Discovery** (`SiteMapper`) - Rapid link discovery and domain-scoped sitemap graph construction.
//! 2. **Phase 2: IR Data Extraction** (`transform_html_to_ir`) - Noise pruning and compact Markdown IR generation for LLM ingestion.
//! 3. **Phase 3: Content Analysis Hook** (`AnalysisCallback`) - Async callback execution per `PageIR` payload (e.g. LLM extraction, summarizing).
//! 4. **Phase 4: Export & Storage Interface** (`StorageExporter`, `FileStorageExporter`) - Modular storage exporter sinks (defaulting to `./out` directory, custom paths, Vector DBs, or cloud sinks).
//! 
//! ## Quick Builder Example
//! 
//! ```rust,no_run
//! use browser_crawler::Browser;
//! 
//! #[tokio::main]
//! async fn main() -> Result<(), String> {
//!     let browser = Browser::builder()
//!         .start_url("https://example.com")
//!         .max_depth(2)
//!         .output_dir("out")
//!         .on_page(|page_ir| async move {
//!             println!("Analyzing URL: {}", page_ir.url);
//!             Ok(())
//!         })
//!         .build()?;
//! 
//!     browser.run().await?;
//!     Ok(())
//! }
//! ```

pub mod builder;
pub mod exporter;
pub mod mapper;
pub mod pipeline;
pub mod transformer;
pub mod types;

pub use builder::{Browser, BrowserBuilder};
pub use exporter::{FileStorageExporter, FileStorageExporterBuilder};
pub use mapper::{SiteMapper, SiteMapperBuilder};
pub use pipeline::{BrowserPipeline, BrowserPipelineBuilder, CrawlerPipeline};
pub use transformer::transform_html_to_ir;
pub use types::{AnalysisCallback, PageIR, SitemapNode, StorageExporter};
