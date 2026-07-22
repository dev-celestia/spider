# Browser Crawler & AI IR Library

[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A decoupled, 4-phase modular Rust web crawling and AI Intermediate Representation (IR) generation library. Built for high performance async link discovery, noise-free Markdown extraction for Large Language Models (LLMs), real-time content analysis hooks, and plug-and-play storage sinks.

---

## 📐 Architecture Overview

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│ PHASE 1: Sitemap Discovery                                                       │
│ Landing Page ──> Fast Link Extractor ──> Recursive Sitemap Node Graph             │
└────────────────────────────────────────┬─────────────────────────────────────────┘
                                         │
┌────────────────────────────────────────▼─────────────────────────────────────────┐
│ PHASE 2: IR Data Extraction                                                      │
│ Node URL ──> Fetch HTML ──> Prune Noise (CSS/JS) ──> Convert to Compact Markdown │
└────────────────────────────────────────┬─────────────────────────────────────────┘
                                         │
┌────────────────────────────────────────▼─────────────────────────────────────────┐
│ PHASE 3: Content Analysis Hook                                                   │
│ PageIR Payload ──> Async Callback ──> Optional LLM Prompting / Summary Logic     │
└────────────────────────────────────────┬─────────────────────────────────────────┘
                                         │
┌────────────────────────────────────────▼─────────────────────────────────────────┐
│ PHASE 4: Export & Storage Interface                                              │
│ Output Stream ──> Vector Database / Local Storage / Custom Data Sink             │
└──────────────────────────────────────────────────────────────────────────────────┘
```

---

## 🌟 Key Features

- **Decoupled 4-Phase Modular Design**: Clear separation between sitemap mapping, IR transformation, inline analysis callbacks, and data export.
- **Fast Non-Blocking Discovery**: Concurrent link discovery with thread-safe visited tracking (`DashSet`) and domain host scoping.
- **Token-Optimized AI IR**: Strips HTML scripts, styles, and wrapper noise into clean, compact Markdown suitable for LLM prompt context and vector embeddings.
- **Asynchronous Analysis Hooks**: Plug custom async callback logic (LLM summarization, entity extraction, content filtering) into Phase 3.
- **Pluggable Storage Exporters**: Easily export results to Vector DBs, local filesystem, S3, or custom data sinks using the `StorageExporter` trait.

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

Here is a complete example building a sitemap, running an analysis hook, and exporting results to local `.md` files:

```rust
use std::sync::Arc;
use browser_crawler::{
    AnalysisCallback, CrawlerPipeline, PageIR, SiteMapper, StorageExporter,
};

// 1. Define Phase 4 Exporter
struct LocalFileExporter;

#[async_trait::async_trait]
impl StorageExporter for LocalFileExporter {
    async fn export(&self, ir: &PageIR) -> Result<(), String> {
        let filename = format!("out_{}.md", ir.url.replace("https://", "").replace('/', "_"));
        tokio::fs::write(&filename, &ir.markdown_ir)
            .await
            .map_err(|e| e.to_string())?;
        println!("[Phase 4] Saved to disk: {}", filename);
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let start_url = "https://example.com";

    // 2. Phase 1: Sitemap Discovery
    let mapper = SiteMapper::new(2);
    let sitemap = mapper.map_site(start_url).await.expect("Failed to map site");

    // 3. Phase 3: Content Analysis Hook
    let analysis_hook: AnalysisCallback = Box::new(|page_ir| {
        Box::pin(async move {
            println!("[Phase 3] Processing URL: {}", page_ir.url);
            println!("Extracted Text Size: {} chars", page_ir.markdown_ir.len());
            Ok(())
        })
    });

    // 4. Phase 2-4: Process Pipeline
    let exporter = Arc::new(LocalFileExporter);
    let pipeline = CrawlerPipeline::new(exporter);

    pipeline.process_sitemap(&sitemap, &analysis_hook).await.unwrap();
}
```

---

## 📚 Phase-by-Phase Documentation

### Phase 1: Sitemap Discovery (`SiteMapper`)

`SiteMapper` traverses internal hyperlinks recursively while maintaining high execution speed by avoiding full text parsing during mapping.

```rust
use browser_crawler::SiteMapper;

let mapper = SiteMapper::new(3); // max depth = 3
if let Some(sitemap) = mapper.map_site("https://example.com").await {
    println!("Root URL: {}, Children: {}", sitemap.url, sitemap.children.len());
}
```

**Key Behaviors:**
- **Same-Domain Scoping**: Only links sharing the same domain host as the root URL are recursed.
- **Visited Deduplication**: Concurrent `Arc<DashSet<String>>` set prevents cycles or duplicate requests.

---

### Phase 2: HTML-to-IR Transformer (`transform_html_to_ir`)

Converts raw HTML string content into token-efficient `PageIR`.

```rust
use browser_crawler::transform_html_to_ir;

let html = r#"
    <html>
      <head><title>Documentation Page</title></head>
      <body>
        <h1>API Reference</h1>
        <p>This is the core document context.</p>
        <ul>
          <li>Feature 1</li>
          <li>Feature 2</li>
        </ul>
      </body>
    </html>
"#;

let page_ir = transform_html_to_ir("https://example.com/docs", html);

assert_eq!(page_ir.title, "Documentation Page");
assert!(page_ir.markdown_ir.contains("# API Reference"));
assert!(page_ir.markdown_ir.contains("* Feature 1"));
```

---

### Phase 3: Content Analysis Hooks (`AnalysisCallback`)

`AnalysisCallback` provides an async callback mechanism allowing developers to run LLM summarization, sentiment classification, or validation logic before saving data.

```rust
use browser_crawler::AnalysisCallback;

let callback: AnalysisCallback = Box::new(|ir| {
    Box::pin(async move {
        println!("Analyzing Page Title: {}", ir.title);
        // Integrate OpenAI, Anthropic, or local LLM inference here
        Ok(())
    })
});
```

---

### Phase 4: Pluggable Storage Exporters (`StorageExporter`)

Implement `StorageExporter` to stream `PageIR` results into any database or storage service.

#### Vector Database Exporter Example:

```rust
use browser_crawler::{StorageExporter, PageIR};

struct VectorDbExporter {
    db_client: MyVectorDbClient,
}

#[async_trait::async_trait]
impl StorageExporter for VectorDbExporter {
    async fn export(&self, ir: &PageIR) -> Result<(), String> {
        let embedding = generate_embedding(&ir.markdown_ir).await?;
        self.db_client
            .insert_document(&ir.url, &ir.title, &ir.markdown_ir, embedding)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
```

---

## 🛠️ Data Structures Reference

### `SitemapNode`
```rust
pub struct SitemapNode {
    pub url: String,
    pub depth: usize,
    pub children: Vec<SitemapNode>,
}
```

### `PageIR`
```rust
pub struct PageIR {
    pub url: String,
    pub title: String,
    pub markdown_ir: String,
}
```

---

## 🧪 Running Tests & Examples

### Run Unit Tests
```bash
cargo test
```

### Run Basic Crawl Example
```bash
cargo run --example basic_crawl
```

### Build Documentation
```bash
cargo doc --open
```

---

## 📄 License

This project is licensed under the MIT License.
