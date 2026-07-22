# Browser & AI IR Library

[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A decoupled, 4-phase modular Rust library for web browsing and AI Intermediate Representation (IR) generation. Built around an **Interleaved Queue-Based Streaming Architecture** (`Browser::builder()`) with support for static HTTP fetching, **Playwright-style dynamic rendering**, and **Anti-Bot Stealth Mode** via Headless Chrome.

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
│ │ 4. Phase 2: Extract PageIR (Title, Noise Pruning, Markdown IR)                       │ │
│ └──────────────────────────────────────────┬───────────────────────────────────────────┘ │
│                                            │                                             │
│ ┌──────────────────────────────────────────▼───────────────────────────────────────────┐ │
│ │ 5. Phase 3: Execute Content Analysis Callback (.on_page / LLM Prompting)              │ │
│ └──────────────────────────────────────────┬───────────────────────────────────────────┘ │
│                                            │                                             │
│ ┌──────────────────────────────────────────▼───────────────────────────────────────────┐ │
│ │ 6. Phase 4: Export Payload to Disk (./out / Vector DB)                                │ │
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

## 🌟 Key Features

- **Queue-Based Push/Pop Navigation**: Discovered links are pushed onto a navigation queue during page scanning, and popped one by one for real-time processing and instant disk export.
- **Fluent Builder Pattern (`Browser::builder()`)**: Configure start URL, crawling depth, custom User-Agent, output directories, rendering modes, stealth flags, and callback hooks.
- **Anti-Bot Stealth Mode (`.stealth(true)`)**: Bypasses bot detection by stripping `--disable-blink-features=AutomationControlled`, masking `navigator.webdriver`, spoofing `window.chrome`, `navigator.plugins`, and WebGL vendor flags.
- **Playwright-Style Dynamic Rendering**: Executes client-side JavaScript, CSS layout evaluations, and SPA hydration (React, Vue, Angular) via Headless Chrome.
- **Playwright-Grade Wait Lifecycles (`WaitUntil`)**: Wait for `NetworkIdle` (0 active requests for 500ms), `DomContentLoaded`, or custom CSS selectors (`WaitUntil::Selector("main")`).
- **Token-Optimized AI IR**: Strips HTML scripts, styles, and wrapper noise into clean, compact Markdown suitable for LLMs.
- **Pluggable Storage Exporters**: Built-in file exporter (defaulting to `./out`) and extensible `StorageExporter` trait.

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

## 🚀 Quick Start (Queue Streaming Crawl)

Execute a full 4-phase crawl using the Builder API:

```rust
use std::time::Duration;
use browser_crawler::{Browser, RenderMode, TimeoutStrategy, WaitUntil};

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
        .on_page(|page_ir| async move {
            println!("[Phase 3] Processed URL: {}", page_ir.url);
            println!("Title: {}", page_ir.title);
            Ok(())
        })
        .build()?;

    // Execute the queue-based pipeline
    let summary = browser.run().await?;
    println!("Pages Processed: {}", summary.pages_processed);
    println!("Total IR Bytes: {}", summary.total_ir_bytes);
    println!("Visited URLs: {:?}", summary.visited_urls);
    Ok(())
}
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

## 🧪 Running Examples & Tests

### Basic Interleaved Crawl (Outputs to `./out`)
```bash
cargo run --example basic_crawl
```

### Dynamic Crawl Example (Headless Chrome)
```bash
cargo run --example dynamic_crawl
```

### Stealth Crawl Example
```bash
cargo run --example stealth_crawl
```

### Run Unit & Doc Tests
```bash
cargo test
```

---

## 📄 License

This project is licensed under the MIT License.
