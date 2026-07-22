use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use serde::{Deserialize, Serialize};

/// Rendering mode for HTML fetching and dynamic JS/CSS execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RenderMode {
    /// Fast static HTTP client (`reqwest`). Best for static HTML sites.
    #[default]
    Static,
    /// Headless Chrome browser execution via Chrome DevTools Protocol (CDP).
    /// Executes client-side JavaScript, CSS evaluations, and SPA framework hydration.
    Dynamic,
}

/// Wait lifecycle condition for dynamic page rendering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaitUntil {
    /// Waits until there are no active network requests for at least 500ms.
    /// Recommended for SPAs (React, Vue, Angular) fetching JSON data over API endpoints.
    NetworkIdle,
    /// Waits until the main HTML document parser finishes (DOMContentLoaded).
    DomContentLoaded,
    /// Waits until a specific CSS selector (e.g. `"main#content"`) appears in the DOM.
    Selector(String),
    /// Sleeps for a fixed duration before capturing the DOM snapshot.
    Delay(Duration),
}

impl Default for WaitUntil {
    fn default() -> Self {
        Self::NetworkIdle
    }
}

/// Action to execute when a dynamic page load or wait condition times out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TimeoutStrategy {
    /// Captures whatever DOM content has rendered up to the timeout moment,
    /// logs a warning, and proceeds to Phase 2 IR extraction without failing the crawl.
    #[default]
    ExtractPartial,
    /// Abandons headless Chrome and falls back to fetching raw static HTML via `reqwest`.
    FallbackToStatic,
    /// Skips IR extraction for the timed-out page and returns an error result.
    FailFast,
}

/// Configuration options for page rendering, wait strategies, and anti-bot stealth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOptions {
    pub render_mode: RenderMode,
    pub wait_until: WaitUntil,
    pub render_timeout: Duration,
    pub timeout_strategy: TimeoutStrategy,
    /// Enables anti-bot stealth mechanisms (masking `navigator.webdriver`, chrome flags, fingerprinting protection).
    pub stealth: bool,
    /// Enables debug inspection (logging CDP network responses and console errors).
    pub debug: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            render_mode: RenderMode::Static,
            wait_until: WaitUntil::NetworkIdle,
            render_timeout: Duration::from_secs(10),
            timeout_strategy: TimeoutStrategy::ExtractPartial,
            stealth: true,
            debug: false,
        }
    }
}

/// Summary metrics returned upon crawl completion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CrawlSummary {
    /// Total number of pages processed (fetched, converted to IR, analyzed, and exported).
    pub pages_processed: usize,
    /// Total characters/bytes of Markdown IR generated across all processed pages.
    pub total_ir_bytes: usize,
    /// List of all unique URLs visited during the crawl.
    pub visited_urls: Vec<String>,
}

/// Phase 1 Output: Structural map of the website represented as a tree node graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SitemapNode {
    pub url: String,
    pub depth: usize,
    pub children: Vec<SitemapNode>,
}

/// Phase 2 Output: Token-optimized Intermediate Representation (IR) for LLM ingestion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageIR {
    pub url: String,
    pub title: String,
    pub markdown_ir: String,
}

/// Phase 4 Trait: Plug-and-play storage interface for processing results.
#[async_trait::async_trait]
pub trait StorageExporter: Send + Sync {
    async fn export(&self, ir: &PageIR) -> Result<(), String>;
}

/// Phase 3 Callback Type: Asynchronous function closure invoked per `PageIR` payload.
pub type AnalysisCallback = Box<
    dyn Fn(PageIR) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> + Send + Sync,
>;
