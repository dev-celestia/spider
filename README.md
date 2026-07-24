# Browser & AI IR Library

[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A high-performance Rust web browsing library and AI Intermediate Representation (IR) generator. Built around a streaming builder architecture (`Browser::builder()`) with support for static HTTP fetching, dynamic JavaScript rendering, and anti-bot stealth mode via Headless Chrome.

---

## 📐 Queue-Based Navigation & Rendering Pipeline

```
┌──────────────────────────────────────────────────────────────────────────────────────────┐
│ QUEUE-BASED INTERLEAVED CRAWL LOOP                                                        │
│                                                                                          │
│ ┌──────────────────────────────────────────────────────────────────────────────────────┐ │
│ │ 1. Push Landing Page Task (start_url, depth=0) into Navigation Queue                  │ │
│ └──────────────────────────────────────────┬───────────────────────────────────────────┘ │
│                                            │                                             │
│ ┌──────────────────────────────────────────▼───────────────────────────────────────────┐ │
│ │ 2. WHILE Queue is NOT Empty: POP Next CrawlTask (url, depth)                         │ │
│ └──────────────────────────────────────────┬───────────────────────────────────────────┘ │
│                                            │                                             │
│ ┌──────────────────────────────────────────▼───────────────────────────────────────────┐ │
│ │ 3. Fetch HTML (Static / Dynamic Headless Chrome with Stealth)                          │ │
│ └──────────────────────────────────────────┬───────────────────────────────────────────┘ │
│                                            │                                             │
│ ┌──────────────────────────────────────────▼───────────────────────────────────────────┐ │
│ │ 4. Extract PageIR (Title, Noise Pruning, Markdown IR, Thumbnails, Links)               │ │
│ └──────────────────────────────────────────┬───────────────────────────────────────────┘ │
│                                            │                                             │
│ ┌──────────────────────────────────────────▼───────────────────────────────────────────┐ │
│ │ 5. Execute Content Analysis Callback (.on_page / LLM Prompting)                        │ │
│ └──────────────────────────────────────────┬───────────────────────────────────────────┘ │
│                                            │                                             │
│ ┌──────────────────────────────────────────▼───────────────────────────────────────────┐ │
│ │ 6. Export Payload to Disk (./out / Vector DB)                                          │ │
│ └──────────────────────────────────────────┬───────────────────────────────────────────┘ │
│                                            │                                             │
│ ┌──────────────────────────────────────────▼───────────────────────────────────────────┐ │
│ │ 7. Scan HTML for Same-Domain Links (<a href="...">)                                   │ │
│ │    For each unvisited link (if depth < max_depth):                                     │ │
│ │    ├──> Mark Visited                                                                   │ │
│ │    └──> PUSH (link, depth + 1) onto Navigation Queue                                   │ │
│ └──────────────────────────────────────────┬───────────────────────────────────────────┘ │
│                                            │                                             │
│ ┌──────────────────────────────────────────▼───────────────────────────────────────────┐ │
│ │ 8. Repeat POP from Queue until Queue is Empty & Crawl Summary is Returned              │ │
│ └──────────────────────────────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

---

---

## 🌟 Key Features

- **Streaming Queue Navigation**: Discovered links are pushed onto a navigation queue during page scanning, and popped one by one for real-time processing and instant disk export.
- **Single Page Crawl Utility (`crawl_single_page`)**: One-off fetching and rendering of a single URL without setting up a full multi-page crawler.
- **Hyperlink Extraction (`extract_links`)**: Scans HTML for valid same-domain absolute links, stripping URL fragments (`#`).
- **Fluent Builder Pattern (`Browser::builder()`)**: Configure start URL, crawling depth, custom User-Agent, output directories, rendering modes, stealth flags, and callback hooks.
- **Anti-Bot Stealth Mode (`.stealth(true)`)**: Bypasses bot detection by stripping `--disable-blink-features=AutomationControlled`, masking `navigator.webdriver`, spoofing `window.chrome`, `navigator.plugins`, and WebGL vendor flags.
- **Dynamic Headless Rendering**: Executes client-side JavaScript, CSS layout evaluations, and SPA hydration (React, Vue, Angular) via Headless Chrome.
- **Advanced Wait Lifecycles (`WaitUntil`)**: Wait for `NetworkIdle` (0 active requests for 500ms), `DomContentLoaded`, or custom CSS selectors (`WaitUntil::Selector("main")`).
- **Debug Inspector Mode (`.debug(true)` / `--debug`)**: Logs Chrome CDP rendering steps, network responses, and DOM settlement events, dumping raw HTML to `./out/debug_dump.html`.
- **Token-Optimized AI IR**: Strips HTML scripts, styles, and wrapper noise into clean, compact Markdown (including article cards, links, and image thumbnails) suitable for LLMs.
- **Pluggable Storage Exporters**: Built-in file exporter (defaulting to `./out`) and extensible `StorageExporter` trait.

---

## 📚 Documentation

For complete detailed guides and API specifications, see:

- 📖 [**Full Usage Guide (`docs/USAGE.md`)**](file:///Users/arham/Desktop/project/browser-crawler/docs/USAGE.md)
- 📑 [**API Reference (`docs/API.md`)**](file:///Users/arham/Desktop/project/browser-crawler/docs/API.md)

---

## 📦 Installation

Add `browser-crawler` to your `Cargo.toml`:

```toml
[dependencies]
browser-crawler = { path = "." }
tokio = { version = "1.43", features = ["full"] }
async-trait = "0.1"
```

---

## 🚀 Quick Start

### 1. Multi-Page Streaming Crawl Example

```bash
cargo run --example example
```

#### Code Overview (`examples/example.rs`)

```rust
use std::time::Duration;
use browser_crawler::{Browser, RenderMode, TimeoutStrategy, WaitUntil};

#[tokio::main]
async fn main() -> Result<(), String> {
    let browser = Browser::builder()
        .start_url("https://0xbuffer.com/")
        .max_depth(1)
        .render_mode(RenderMode::Dynamic)
        .stealth(true)
        .wait_until(WaitUntil::Delay(Duration::from_secs(3)))
        .render_timeout(Duration::from_secs(12))
        .timeout_strategy(TimeoutStrategy::ExtractPartial)
        .on_page(|page_ir| async move {
            println!("[Process] Processed URL: {}", page_ir.url);
            println!("Title: {}", page_ir.title);
            Ok(())
        })
        .build()?;

    // Execute the streaming browser pipeline
    let summary = browser.run().await?;
    println!("Pages Processed: {}", summary.pages_processed);
    println!("Total IR Bytes: {}", summary.total_ir_bytes);
    println!("Visited URLs: {:?}", summary.visited_urls);
    Ok(())
}
```

### 2. Single Page Crawl & Link Extraction Utility

```bash
cargo run --example single_page
```

#### Code Overview (`examples/single_page.rs`)

```rust
use browser_crawler::{crawl_single_page, extract_links, Browser, RenderMode, RenderOptions};

#[tokio::main]
async fn main() -> Result<(), String> {
    // 1. One-off single page fetch
    let options = RenderOptions {
        render_mode: RenderMode::Static,
        ..Default::default()
    };
    let page_ir = crawl_single_page("https://example.com", &options).await?;
    println!("Title: {}", page_ir.title);

    // 2. Extract same-domain links
    let html = r#"<a href="/about">About</a><a href="https://example.com/docs">Docs</a>"#;
    let links = extract_links("https://example.com", html)?;
    println!("Extracted links: {:?}", links);

    // 3. Fetch single page using Browser instance
    let browser = Browser::builder().start_url("https://example.com").build()?;
    let fetched = browser.fetch_page("https://example.com/about").await?;
    println!("Fetched: {}", fetched.title);

    Ok(())
}
```

---

## 🛠️ Public Utility Functions

### `crawl_single_page`

```rust
pub async fn crawl_single_page(url: &str, options: &RenderOptions) -> Result<PageIR, String>
```

Asynchronously fetches a single page without setting up a multi-page crawler. Supports both static HTTP and dynamic Headless Chrome rendering.

### `extract_links`

```rust
pub fn extract_links(base_url: &str, html: &str) -> Result<Vec<String>, String>
```

Parses HTML and extracts all valid same-domain absolute hyperlinks, stripping URL fragments (`#`).

### `Browser::fetch_page`

```rust
pub async fn fetch_page(&self, url: &str) -> Result<PageIR, String>
```

Fetches a single page using a pre-configured `Browser` instance.

---

## 🔍 How to Use Debug Mode

Debug Mode logs Headless Chrome execution steps, CDP network status responses, and DOM settlement events. It also writes a raw DOM snapshot to `./out/debug_dump.html` for offline DOM inspection.

### Method 1: Via Terminal CLI Flag (`--debug`)

Pass `--debug` when running the example:

```bash
cargo run --example example -- --debug
```

---

## 📊 Summary Metrics (`CrawlSummary`)

When `browser.run().await` completes, it returns a `CrawlSummary` struct:

```rust
pub struct CrawlSummary {
    pub pages_processed: usize,
    pub total_ir_bytes: usize,
    pub visited_urls: Vec<String>,
}
```

---

## 🧪 Running Example Commands

### Default Dynamic Render Run (`https://0xbuffer.com/`)
```bash
cargo run --example example
```

### Single Page Utility Run
```bash
cargo run --example single_page
```

### Debug Inspection Mode
```bash
cargo run --example example -- --debug
```

### Fast Static HTTP Fetch Mode
```bash
cargo run --example example -- --static
```

### Target Custom URL
```bash
cargo run --example example -- https://example.com --debug
```

### Run Unit & Doc Tests
```bash
cargo test
```

---

## 📄 License

This project is licensed under the MIT License.

