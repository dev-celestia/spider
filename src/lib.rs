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
//!
//! ## UI / GUI Integration (Tauri, Electron, egui, ...)
//!
//! For UI hosts, [`CrawlSession`] is the integration surface: a serde-serializable
//! [`CrawlConfig`], a broadcast [`CrawlerEvent`] stream, and a
//! [`crate::CrawlSessionHandle`] with `cancel` / `pause` / `resume` / `join`.
//!
//! ```rust,no_run
//! use browser_crawler::{CrawlConfig, CrawlSession, CrawlerEvent};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), String> {
//!     let session = CrawlSession::spawn(CrawlConfig {
//!         urls: vec!["https://example.com".into()],
//!         max_depth: 2,
//!         ..Default::default()
//!     })?;
//!     let mut events = session.take_events().unwrap();
//!
//!     // In a real UI: forward each event to the window (Tauri emit, IPC, ...).
//!     tokio::spawn(async move {
//!         while let Ok(event) = events.recv().await {
//!             if let CrawlerEvent::Finished { summary } = event {
//!                 println!("done: {} pages", summary.results);
//!                 break;
//!             }
//!         }
//!     });
//!
//!     // From a UI button: session.pause() / session.resume() / session.cancel()
//!     let summary = session.join().await?;
//!     println!("visited {} urls", summary.visited_urls.len());
//!     Ok(())
//! }
//! ```

#[cfg(not(target_arch = "wasm32"))]
pub mod builder;
#[cfg(not(target_arch = "wasm32"))]
pub mod control;
pub mod engine;
#[cfg(not(target_arch = "wasm32"))]
pub mod exporter;
#[cfg(not(target_arch = "wasm32"))]
pub mod mapper;
#[cfg(not(target_arch = "wasm32"))]
pub mod output;
#[cfg(not(target_arch = "wasm32"))]
pub mod pipeline;
#[cfg(not(target_arch = "wasm32"))]
pub mod renderer;
#[cfg(not(target_arch = "wasm32"))]
pub mod runner;
#[cfg(not(target_arch = "wasm32"))]
pub mod session;
#[cfg(not(target_arch = "wasm32"))]
pub mod stealth;
pub mod transformer;
pub mod types;
pub mod utils;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

#[cfg(not(target_arch = "wasm32"))]
pub use builder::{Browser, BrowserBuilder};
#[cfg(not(target_arch = "wasm32"))]
pub use control::CrawlControl;
#[cfg(not(target_arch = "wasm32"))]
pub use engine::common::{Crawler, PageFetch};
#[cfg(not(target_arch = "wasm32"))]
pub use exporter::{FileStorageExporter, FileStorageExporterBuilder};
#[cfg(not(target_arch = "wasm32"))]
pub use mapper::{SiteMapper, SiteMapperBuilder};
#[cfg(not(target_arch = "wasm32"))]
pub use output::{configure_output, StandardWriter};
#[cfg(not(target_arch = "wasm32"))]
pub use pipeline::{BrowserPipeline, BrowserPipelineBuilder, CrawlerPipeline};
#[cfg(not(target_arch = "wasm32"))]
pub use renderer::{crawl_single_page, PageFetcher};
#[cfg(not(target_arch = "wasm32"))]
pub use runner::{Runner, RunnerSummary};
#[cfg(not(target_arch = "wasm32"))]
pub use session::{CrawlConfig, CrawlSession, CrawlSessionHandle, SessionPhase, SessionSnapshot};
#[cfg(not(target_arch = "wasm32"))]
pub use stealth::{stealth_chrome_args, STEALTH_JS};
pub use transformer::{extract_links, transform_html_to_ir};
pub use types::result::Result as CrawlResult;
pub use types::events::{CrawlerEvent, SessionSummary};
pub use types::{
    AnalysisCallback, CrawlSummary, Options, PageIR, RenderMode, RenderOptions, Request, Response,
    SitemapNode, Strategy, StorageExporter, TimeoutStrategy, WaitUntil,
};
