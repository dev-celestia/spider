# API Reference (`browser-crawler`)

Complete API reference for all public modules, structs, enums, functions, and traits exposed by the `browser-crawler` crate.

---

## Crate Root Exports (`browser_crawler::*`)

### Core Types & Structs

#### `Browser`

High-level facade for executing queue-based streaming web crawls or fetching single pages.

```rust
pub struct Browser;
```

##### Associated Methods

- `pub fn builder() -> BrowserBuilder`  
  Creates a default [`BrowserBuilder`] instance.

- `pub async fn run(&self) -> Result<CrawlSummary, String>`  
  Executes the 4-phase queue-based streaming browser pipeline starting at `start_url`. Returns summary metrics on completion.

- `pub async fn fetch_page(&self, url: &str) -> Result<PageIR, String>`  
  Fetches and transforms a single URL into a [`PageIR`] payload using the browser's configured renderer options.

---

#### `BrowserBuilder`

Fluent builder for constructing and configuring a [`Browser`] instance.

```rust
pub struct BrowserBuilder;
```

##### Methods

- `pub fn start_url(mut self, url: impl Into<String>) -> Self`
- `pub fn max_depth(mut self, depth: usize) -> Self`
- `pub fn user_agent(mut self, ua: impl Into<String>) -> Self`
- `pub fn output_dir<P: AsRef<Path>>(mut self, dir: P) -> Self`
- `pub fn exporter(mut self, exporter: Arc<dyn StorageExporter>) -> Self`
- `pub fn render_mode(mut self, mode: RenderMode) -> Self`
- `pub fn wait_until(mut self, wait: WaitUntil) -> Self`
- `pub fn wait_for_selector(mut self, selector: impl Into<String>) -> Self`
- `pub fn render_timeout(mut self, timeout: Duration) -> Self`
- `pub fn timeout_strategy(mut self, strategy: TimeoutStrategy) -> Self`
- `pub fn stealth(mut self, enabled: bool) -> Self`
- `pub fn debug(mut self, enabled: bool) -> Self`
- `pub fn analysis_callback(mut self, callback: AnalysisCallback) -> Self`
- `pub fn on_page<F, Fut>(mut self, f: F) -> Self where F: Fn(PageIR) -> Fut + Send + Sync + 'static, Fut: Future<Output = Result<(), String>> + Send + 'static`
- `pub fn build(self) -> Result<Browser, String>`

---

### Public Functions

#### `crawl_single_page`

```rust
pub async fn crawl_single_page(url: &str, options: &RenderOptions) -> Result<PageIR, String>
```

**Description:** Asynchronously fetches a single webpage and transforms it into a token-optimized [`PageIR`] payload using static HTTP or dynamic Headless Chrome rendering.

---

#### `extract_links`

```rust
pub fn extract_links(base_url: &str, html: &str) -> Result<Vec<String>, String>
```

**Description:** Scans raw HTML source code and extracts all unique, same-domain absolute hyperlinks, resolving relative URLs against `base_url` and stripping URL fragments (`#`).

---

#### `transform_html_to_ir`

```rust
pub fn transform_html_to_ir(url: &str, html: &str) -> PageIR
```

**Description:** Converts raw HTML source into a clean Markdown [`PageIR`] representation by pruning scripts, styles, and wrapper noise.

#### `SiteMapper`

```rust
pub struct SiteMapper { /* ... */ }
```

**Description:** Rapid sitemap mapper for Phase 1 link discovery. Traverses a site **depth-first** (recursive), building a [`SitemapNode`](#sitemapnode) tree; a link reachable from multiple parents appears only once under the first DFS path that reaches it.

```rust
// Builder: max_depth (default 2), max_pages (0 = unlimited), user_agent, render_options
let mapper = SiteMapper::builder().max_depth(2).max_pages(100).build();
let root: Option<SitemapNode> = mapper.map_site("https://example.com").await;
```

---

### Data Models & Enums

#### `PageIR`

```rust
pub struct PageIR {
    pub url: String,
    pub title: String,
    pub markdown_ir: String,
}
```

#### `CrawlSummary`

```rust
pub struct CrawlSummary {
    pub pages_processed: usize,
    pub total_ir_bytes: usize,
    pub visited_urls: Vec<String>,
}
```

#### `SitemapNode`

```rust
pub struct SitemapNode {
    pub url: String,
    pub depth: usize,
    pub children: Vec<SitemapNode>,
}
```

**Description:** Structural map of a website as a tree node graph, produced by [`SiteMapper`](#sitemapper).

#### `RenderMode`

```rust
pub enum RenderMode {
    Static,  // Fast HTTP client (reqwest)
    Dynamic, // Headless Chrome CDP browser
}
```

#### `WaitUntil`

```rust
pub enum WaitUntil {
    NetworkIdle,
    DomContentLoaded,
    Selector(String),
    Delay(Duration),
}
```

#### `TimeoutStrategy`

```rust
pub enum TimeoutStrategy {
    ExtractPartial,
    FallbackToStatic,
    FailFast,
}
```

---

### Storage Trait

#### `StorageExporter`

```rust
#[async_trait::async_trait]
pub trait StorageExporter: Send + Sync {
    async fn export(&self, ir: &PageIR) -> Result<(), String>;
}
```
