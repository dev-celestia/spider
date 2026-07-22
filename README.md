# Browser & AI IR Library

[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A decoupled, 4-phase modular Rust library for web browsing and AI Intermediate Representation (IR) generation. Built around an elegant **Builder Pattern** (`Browser::builder()`) for fast async link discovery, noise-free Markdown extraction for Large Language Models (LLMs), real-time content analysis hooks, and pluggable storage sinks.

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

- **Fluent Builder Pattern (`Browser::builder()`)**: Configure start URL, crawling depth, custom User-Agent, output directories, and callback hooks via a unified, readable API.
- **Modular Sub-Builders**: Fine-grained sub-builders (`SiteMapper::builder()`, `BrowserPipeline::builder()`, `FileStorageExporter::builder()`) for custom pipelines.
- **Default Output Directory (`./out`)**: Standard built-in file exporter automatically creates and targets `./out`, with support for custom directory paths.
- **Fast Non-Blocking Discovery**: Concurrent link discovery with thread-safe visited tracking (`DashSet`) and domain host scoping.
- **Token-Optimized AI IR**: Strips HTML scripts, styles, and wrapper noise into clean, compact Markdown suitable for LLM prompt context and vector embeddings.
- **Asynchronous Analysis Hooks**: Inline `.on_page(|page_ir| async move { ... })` callbacks for real-time LLM inference, summarization, or entity extraction.

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

## 🚀 Quick Start (Builder Pattern)

Execute a full 4-phase crawl in a few lines of clean Rust code:

```rust
use browser_crawler::Browser;

#[tokio::main]
async fn main() -> Result<(), String> {
    // Configure and build the Browser instance using the fluent Builder API
    let browser = Browser::builder()
        .start_url("https://example.com")
        .max_depth(2)
        .user_agent("RustAIBrowser/1.0")
        .output_dir("out") // Defaults to "./out"
        .on_page(|page_ir| async move {
            println!("[Phase 3] Processing URL: {}", page_ir.url);
            println!("Extracted Text Size: {} chars", page_ir.markdown_ir.len());
            Ok(())
        })
        .build()?;

    // Execute the 4-phase pipeline
    browser.run().await?;
    Ok(())
}
```

---

## 📚 Builder API & Modular Components

### Builder Summary Table

| Builder Type | Entry Point | Primary Configuration Methods |
| --- | --- | --- |
| **`BrowserBuilder`** | `Browser::builder()` | `.start_url()`, `.max_depth()`, `.user_agent()`, `.output_dir()`, `.on_page()`, `.exporter()`, `.build()` |
| **`SiteMapperBuilder`** | `SiteMapper::builder()` | `.max_depth()`, `.user_agent()`, `.build()` |
| **`BrowserPipelineBuilder`** | `BrowserPipeline::builder()` | `.user_agent()`, `.exporter()`, `.build()` |
| **`FileStorageExporterBuilder`** | `FileStorageExporter::builder()` | `.output_dir()`, `.build()` |

---

### Phase 1: Sitemap Discovery (`SiteMapper::builder()`)

`SiteMapper` traverses internal hyperlinks recursively while maintaining high execution speed by avoiding full text parsing during mapping.

```rust
use browser_crawler::SiteMapper;

let mapper = SiteMapper::builder()
    .max_depth(3)
    .user_agent("CustomBot/1.0")
    .build();

if let Some(sitemap) = mapper.map_site("https://example.com").await {
    println!("Root URL: {}, Children: {}", sitemap.url, sitemap.children.len());
}
```

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

### Phase 3: Content Analysis Hooks (`.on_page(...)` / `AnalysisCallback`)

`BrowserBuilder` allows inline async closures via `.on_page(...)`:

```rust
let browser = Browser::builder()
    .start_url("https://example.com")
    .on_page(|ir| async move {
        println!("Analyzing Page Title: {}", ir.title);
        // Integrate OpenAI, Anthropic, or local LLM inference here
        Ok(())
    })
    .build()?;
```

---

### Phase 4: Storage Exporters (`FileStorageExporter::builder()` / `StorageExporter`)

#### Using `FileStorageExporter`

- **Default directory (`./out`)**:
  ```rust
  let browser = Browser::builder()
      .start_url("https://example.com")
      .output_dir("out") // Defaults to "./out"
      .build()?;
  ```
- **Custom output directory**:
  ```rust
  let exporter = FileStorageExporter::builder()
      .output_dir("my_custom_folder")
      .build();

  let browser = Browser::builder()
      .start_url("https://example.com")
      .exporter(Arc::new(exporter))
      .build()?;
  ```

#### Implementing a Custom Vector DB Exporter:

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

### `Browser` & `BrowserBuilder`
```rust
pub struct Browser { /* ... */ }
pub struct BrowserBuilder { /* ... */ }
```

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

## 🧪 Running Examples & Tests

### Default Run (Outputs to `./out`)
```bash
cargo run --example basic_crawl
```

### Custom Directory Run
```bash
cargo run --example basic_crawl -- my_export_dir
```

### Run Unit & Doc Tests
```bash
cargo test
```

---

## 📄 License

This project is licensed under the MIT License.
