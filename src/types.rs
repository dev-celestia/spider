use std::future::Future;
use std::pin::Pin;
use serde::{Deserialize, Serialize};

/// Phase 1 Output: Structural map of the website represented as a tree node graph.
///
/// Each node contains its absolute target URL, its recursive depth level relative
/// to the starting landing page, and a vector of child `SitemapNode` elements.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SitemapNode {
    /// Absolute URL of the page.
    pub url: String,
    /// Depth of the node in the sitemap traversal tree (0 for root).
    pub depth: usize,
    /// Vector of discovered child nodes scoping to the same domain host.
    pub children: Vec<SitemapNode>,
}

/// Phase 2 Output: Token-optimized Intermediate Representation (IR) for LLM ingestion.
///
/// Strips out HTML boilerplates, stylesheets, and scripts, formatting core document elements
/// into lightweight Markdown.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageIR {
    /// Absolute source URL of the page.
    pub url: String,
    /// Extracted page title (defaults to `"Untitled Page"` if missing).
    pub title: String,
    /// Compact Markdown-formatted text content suitable for prompt engineering or embeddings.
    pub markdown_ir: String,
}

/// Phase 4 Trait: Plug-and-play storage interface for processing results.
///
/// Implement this trait to sink processed `PageIR` payloads into vector databases,
/// local filesystems, cloud storage, or external APIs.
#[async_trait::async_trait]
pub trait StorageExporter: Send + Sync {
    /// Exports a single `PageIR` payload to the target destination.
    async fn export(&self, ir: &PageIR) -> Result<(), String>;
}

/// Phase 3 Callback Type: Asynchronous function closure invoked per `PageIR` payload.
///
/// Allows arbitrary real-time analysis, LLM processing, or summary logic before storage.
pub type AnalysisCallback = Box<
    dyn Fn(PageIR) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> + Send + Sync,
>;
