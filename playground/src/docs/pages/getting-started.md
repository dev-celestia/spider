## Build the CLI

```bash
cargo build --release
# binary: target/release/celestia-browser
```

Requires a Rust toolchain (2024 edition). Nothing else is needed for the standard engine; the headless engine uses your locally installed Chrome or one you point it at.

## Using the crate

```toml
[dependencies]
browser-crawler = "0.2"   # check the workspace version
tokio = { version = "1", features = ["full"] }
```

## CLI quick start

```bash
# Basic crawl (depth 3, default standard engine)
celestia-browser -u https://example.com

# JSONL output to file, silent logs
celestia-browser -u https://example.com -d 3 -j -silent -o results.jsonl

# Headless crawl with stealth + XHR extraction
celestia-browser -u https://spa.example.com --headless -j --xhr-extraction

# Scope, filters, and rate limiting
celestia-browser -u https://example.com -cs "/(api|docs)/" -fr "logout" -rl 50

# Known files + JS endpoint scraping
celestia-browser -u https://example.com --known-files all --js-crawl

# Crawl from a URL list (or stdin)
cat urls.txt | celestia-browser -d 2 -silent

# Sitemap tree: DFS link tree as JSON instead of per-page results
celestia-browser -u https://example.com -d 3 --sitemap-tree -o sitemap.json
```

Run `celestia-browser --help` for the full list. Flag syntax: long names use `--` (`--js-crawl`); unambiguous single-char shorts work too (`-u`, `-d`, `-o`, `-j`, `-v`, `-c`, `-p`, `-s`). Multi-character celestia shorts like `-jc` are **long-only** here (`--js-crawl`), since the Rust CLI parser does not support multi-character short flags.

## Library quick start (crawler engine)

```rust
use browser_crawler::{Options, Runner, StandardWriter};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut options = Options::with_defaults();
    options.urls = vec!["https://example.com".into()];
    options.max_depth = 2;
    options.rate_limit = 50;
    options.tech_detect = true;

    let writer = Arc::new(StandardWriter::from_options(&options));
    let mut runner = Runner::new(options)?;   // validates options
    let summary = runner.run().await?;        // full crawl with summary stats
    println!("results: {}, failed: {}", summary.results, summary.failed);
    Ok(())
}
```

## Library quick start (AI IR pipeline)

```rust
use std::time::Duration;
use browser_crawler::{Browser, RenderMode, WaitUntil};

#[tokio::main]
async fn main() -> Result<(), String> {
    let browser = Browser::builder()
        .start_url("https://example.com")
        .max_depth(2)
        .render_mode(RenderMode::Dynamic)
        .stealth(true)
        .wait_until(WaitUntil::NetworkIdle)
        .render_timeout(Duration::from_secs(10))
        .on_page(|page_ir| async move {
            println!("Title: {}", page_ir.title);
            Ok(())
        })
        .build()?;

    let summary = browser.run().await?;
    println!("Pages processed: {}", summary.pages_processed);
    Ok(())
}
```

## Tests

```bash
cargo test    # unit + doc + end-to-end suite
```

The end-to-end suite (`tests/e2e_crawl.rs`, `tests/e2e_cli.rs`) spins up a local HTTP test server and exercises the real engine and CLI binary: depth limits, scope regexes, extension/match/DSL filtering, known-files, JS crawling, form extraction and auto-fill, rate limiting, retries, JSONL output, and more. `tests/sitemap_tree.rs` covers `SiteMapper` against the same server.
