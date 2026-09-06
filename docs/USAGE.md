# Full Usage Documentation for `browser-crawler`

`browser-crawler` is a high-performance Rust web browsing library and AI Intermediate Representation (IR) generator designed for single-page parsing, site link discovery, and multi-page streaming crawls.

---

## Table of Contents

1. [Overview & Architecture](#overview--architecture)
2. [Public Functions & Explanation](#public-functions--explanation)
   - [`crawl_single_page`](#crawl_single_page)
   - [`extract_links`](#extract_links)
   - [`transform_html_to_ir`](#transform_html_to_ir)
   - [`Browser::fetch_page`](#browserfetch_page)
3. [Full Crawler Usage (`Browser::builder()`)](#full-crawler-usage-browserbuilder)
4. [Rendering Modes & Execution Strategies](#rendering-modes--execution-strategies)
5. [Anti-Bot Stealth & Debug Inspector](#anti-bot-stealth--debug-inspector)
6. [Pluggable Storage Exporters](#pluggable-storage-exporters)
7. [Full Examples](#full-examples)

---

## Overview & Architecture

`browser-crawler` operates on a 4-phase interleaved queue architecture:

```
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│ QUEUE-BASED INTERLEAVED CRAWL LOOP                                                        │
│                                                                                          │
│ 1. Push Landing Page Task (start_url, depth=0) into Navigation Queue                     │
│ 2. WHILE Queue is NOT Empty: POP Next CrawlTask (url, depth)                            │
│ 3. Fetch HTML (Static HTTP or Dynamic Headless Chrome with Stealth)                      │
│ 4. Extract PageIR (Title, Noise Pruning, Markdown IR, Thumbnails, Links)                  │
│ 5. Execute Content Analysis Callback (.on_page / LLM Prompting)                           │
│ 6. Export Payload to Disk (./out / StorageExporter)                                      │
│ 7. Scan HTML for Same-Domain Links (<a href="...">)                                      │
│    For each unvisited link (if depth < max_depth):                                        │
│    ├──> Mark Visited                                                                      │
│    └──> PUSH (link, depth + 1) onto Navigation Queue                                      │
│ 8. Repeat until Queue is Empty                                                            │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Public Functions & Explanation

### `crawl_single_page`

```rust
pub async fn crawl_single_page(url: &str, options: &RenderOptions) -> Result<PageIR, String>
```

#### Detailed Function Explanation

`crawl_single_page` is a convenience utility function for executing a one-off fetch and transformation of a single URL into a token-optimized `PageIR` payload. 

Unlike initializing a full `Browser` instance with recursive queue crawling, `crawl_single_page` performs a single, direct page render according to the provided `RenderOptions`. It supports both static HTTP fetching (via `reqwest`) and full JavaScript SPA execution (via Headless Chrome with anti-bot stealth).

#### Parameters

- `url` (`&str`): The target webpage URL to fetch and process.
- `options` (`&RenderOptions`): Configuration specifying render mode (`Static` vs `Dynamic`), timeouts, wait lifecycles (`NetworkIdle`, `Selector`), and stealth settings.

#### Return Value

- `Ok(PageIR)`: Contains the canonical URL (`url`), extracted page title (`title`), and cleaned Markdown content (`markdown_ir`).
- `Err(String)`: Returns an error message if the network request fails or Headless Chrome execution times out.

#### Example Usage

```rust,no_run
use browser_crawler::{crawl_single_page, RenderOptions, RenderMode};

#[tokio::main]
async fn main() -> Result<(), String> {
    let options = RenderOptions {
        render_mode: RenderMode::Static,
        ..Default::default()
    };

    let page_ir = crawl_single_page("https://example.com", &options).await?;
    println!("Title: {}", page_ir.title);
    println!("Markdown IR:\n{}", page_ir.markdown_ir);
    Ok(())
}
```

---

### `extract_links`

```rust
pub fn extract_links(base_url: &str, html: &str) -> Result<Vec<String>, String>
```

#### Detailed Function Explanation

`extract_links` parses a raw HTML string and extracts all valid, same-domain absolute hyperlinks (`<a href="...">`).

It resolves relative links against `base_url`, strips anchor fragments (`#...`), filters out cross-domain targets and invalid protocols (`javascript:`, `mailto:`), and eliminates duplicates.

#### Parameters

- `base_url` (`&str`): The absolute base URL used to resolve relative paths and enforce domain scoping.
- `html` (`&str`): The raw HTML string to scan.

#### Return Value

- `Ok(Vec<String>)`: A vector of unique, fully qualified same-domain URLs.
- `Err(String)`: Returns an error string if `base_url` is invalid.

#### Example Usage

```rust
use browser_crawler::extract_links;

let html = r#"
    <a href="/about">About Us</a>
    <a href="https://example.com/docs">Docs</a>
    <a href="https://external.org">External</a>
"#;

let links = extract_links("https://example.com/home", html).unwrap();
assert_eq!(links, vec!["https://example.com/about", "https://example.com/docs"]);
```

---

### `transform_html_to_ir`

```rust
pub fn transform_html_to_ir(url: &str, html: &str) -> PageIR
```

#### Detailed Function Explanation

Converts verbose HTML into a lightweight Markdown `PageIR` payload by stripping script/style/wrapper elements while preserving headings, paragraphs, lists, links, inline code, and images.

#### Example Usage

```rust
use browser_crawler::transform_html_to_ir;

let html = "<html><head><title>My Title</title></head><body><h1>Hello</h1><p>World</p></body></html>";
let ir = transform_html_to_ir("https://example.com", html);
assert_eq!(ir.title, "My Title");
assert!(ir.markdown_ir.contains("# Hello"));
```

---

### `SiteMapper::map_site`

```rust
pub async fn map_site(&self, start_url: &str) -> Option<SitemapNode>
```

#### Detailed Function Explanation

Builds a structural map of a website as a `SitemapNode` tree by traversing it **depth-first** (recursive): starting from `start_url`, every same-host link is followed to its maximum depth before backtracking. A link reachable from multiple parents appears only once — under the first DFS path that reaches it. Returns `None` if the initial request fails.

Configure via the builder: `SiteMapper::builder().max_depth(n).max_pages(m).render_options(..).build()`. `max_pages` (`0` = unlimited) caps total fetches as a safety net for large sites; `max_depth` defaults to `2`.

#### Example Usage

```rust
use browser_crawler::SiteMapper;

#[tokio::main]
async fn main() {
    let mapper = SiteMapper::builder().max_depth(2).max_pages(100).build();
    if let Some(tree) = mapper.map_site("https://example.com").await {
        println!("{}", serde_json::to_string_pretty(&tree).unwrap());
    }
}
```

---

### `Browser::fetch_page`

```rust
pub async fn fetch_page(&self, url: &str) -> Result<PageIR, String>
```

#### Detailed Function Explanation

`fetch_page` is an async method on the `Browser` struct that uses the browser's pre-configured rendering options, user-agent, and fetch settings to retrieve and convert a single target page into a `PageIR` struct.

#### Example Usage

```rust,no_run
use browser_crawler::{Browser, RenderMode};

#[tokio::main]
async fn main() -> Result<(), String> {
    let browser = Browser::builder()
        .start_url("https://example.com")
        .render_mode(RenderMode::Dynamic)
        .stealth(true)
        .build()?;

    let page_ir = browser.fetch_page("https://example.com/about").await?;
    println!("Title: {}", page_ir.title);
    Ok(())
}
```

---

## Full Crawler Usage (`Browser::builder()`)

For multi-page streaming crawls, use `Browser::builder()`:

```rust,no_run
use std::time::Duration;
use browser_crawler::{Browser, RenderMode, WaitUntil, TimeoutStrategy};

#[tokio::main]
async fn main() -> Result<(), String> {
    let browser = Browser::builder()
        .start_url("https://example.com")
        .max_depth(2)
        .render_mode(RenderMode::Dynamic)
        .stealth(true)
        .wait_until(WaitUntil::NetworkIdle)
        .render_timeout(Duration::from_secs(10))
        .timeout_strategy(TimeoutStrategy::ExtractPartial)
        .output_dir("./out")
        .on_page(|page_ir| async move {
            println!("Processed: {} - {}", page_ir.url, page_ir.title);
            Ok(())
        })
        .build()?;

    let summary = browser.run().await?;
    println!("Pages Processed: {}", summary.pages_processed);
    println!("Total IR Bytes: {}", summary.total_ir_bytes);
    Ok(())
}
```

---

## Rendering Modes & Execution Strategies

| Mode | Enum Variant | Description | Best Used For |
| :--- | :--- | :--- | :--- |
| **Static** | `RenderMode::Static` | Fast HTTP requests via `reqwest`. No JavaScript execution. | Server-rendered HTML, blogs, documentation sites |
| **Dynamic** | `RenderMode::Dynamic` | Full Headless Chrome execution with CDP rendering and stealth evasion. | SPAs (React, Vue, Angular), JS-hydrated sites |

### Wait Lifecycles (`WaitUntil`)

- `WaitUntil::NetworkIdle`: Waits for 0 active network requests for 500ms.
- `WaitUntil::DomContentLoaded`: Waits for HTML parser completion.
- `WaitUntil::Selector("main")`: Waits until a specific CSS selector appears in the DOM.
- `WaitUntil::Delay(Duration)`: Pauses execution for a fixed duration.

---

## Anti-Bot Stealth & Debug Inspector

- **Stealth Mode (`.stealth(true)`)**: Suppresses Chrome automation flags, spoofs `navigator.webdriver = undefined`, mocks `window.chrome` and WebGL vendors.
- **Debug Inspector (`.debug(true)`)**: Logs CDP network responses and DOM settlement progress, dumping live rendered HTML to `./out/debug_dump.html`.

---

## Pluggable Storage Exporters

Implement the `StorageExporter` trait to send extracted `PageIR` payloads to custom databases, vector stores, or S3 sinks:

```rust
use browser_crawler::{PageIR, StorageExporter};

pub struct MyCustomExporter;

#[async_trait::async_trait]
impl StorageExporter for MyCustomExporter {
    async fn export(&self, ir: &PageIR) -> Result<(), String> {
        println!("Exporting {} to database...", ir.url);
        Ok(())
    }
}
```

---

## Full Examples

Run the included example binaries:

```bash
# Multi-page crawl example
cargo run --example example

# Single-page crawl & link extraction example
cargo run --example single_page
```
