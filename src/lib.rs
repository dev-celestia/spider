//! # browser-crawler
//! 
//! A decoupled, 4-phase modular Rust library for web crawling, token-optimized HTML-to-Markdown 
//! Intermediate Representation (IR) generation, extensible content analysis callbacks, and pluggable storage sinks.
//! 
//! ## 4-Phase Modular Architecture
//! 
//! 1. **Phase 1: Sitemap Discovery** (`SiteMapper`) - Rapid link discovery and domain-scoped sitemap graph construction.
//! 2. **Phase 2: IR Data Extraction** (`transform_html_to_ir`) - Noise pruning and compact Markdown IR generation for LLM ingestion.
//! 3. **Phase 3: Content Analysis Hook** (`AnalysisCallback`) - Async callback execution per `PageIR` payload (e.g. LLM extraction, summarizing).
//! 4. **Phase 4: Export & Storage Interface** (`StorageExporter`) - Modular storage exporter sinks (Vector DBs, local filesystem, S3, custom data sinks).
//! 
//! ## Quick Example
//! 
//! ```rust,no_run
//! use std::sync::Arc;
//! use browser_crawler::{SiteMapper, CrawlerPipeline, PageIR, StorageExporter, AnalysisCallback};
//! 
//! struct StdoutExporter;
//! 
//! #[async_trait::async_trait]
//! impl StorageExporter for StdoutExporter {
//!     async fn export(&self, ir: &PageIR) -> Result<(), String> {
//!         println!("Exporting IR for {}: {} bytes", ir.url, ir.markdown_ir.len());
//!         Ok(())
//!     }
//! }
//! 
//! #[tokio::main]
//! async fn main() {
//!     let mapper = SiteMapper::new(2);
//!     let sitemap = mapper.map_site("https://example.com").await.unwrap();
//! 
//!     let callback: AnalysisCallback = Box::new(|page_ir| {
//!         Box::pin(async move {
//!             println!("Analyzing URL: {}", page_ir.url);
//!             Ok(())
//!         })
//!     });
//! 
//!     let exporter = Arc::new(StdoutExporter);
//!     let pipeline = CrawlerPipeline::new(exporter);
//!     pipeline.process_sitemap(&sitemap, &callback).await.unwrap();
//! }
//! ```

pub mod mapper;
pub mod pipeline;
pub mod transformer;
pub mod types;

pub use mapper::SiteMapper;
pub use pipeline::CrawlerPipeline;
pub use transformer::transform_html_to_ir;
pub use types::{AnalysisCallback, PageIR, SitemapNode, StorageExporter};
