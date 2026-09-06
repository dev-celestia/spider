# celestia-browser

[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A fast, full-featured web crawler in Rust with a CLI binary and a library crate
(`browser-crawler`). It ships a 1:1 port of the reference Go crawler (vendored
under [`reference/`](reference/)) — standard, headless, and hybrid crawl engines,
scope & filter pipelines, rate limiting, JavaScript crawling, form filling, and
JSONL/template output — plus a token-optimized AI Intermediate Representation
(IR) generator built on the same engine.

---

## ✨ Feature Highlights

### Crawler engines

| Engine | Flag | Description |
|--------|------|-------------|
| Standard | *(default)* | Plain HTTP fetching via `reqwest` — fastest path for static sites. |
| Headless | `--headless` | Every page renders in headless Chrome: live DOM, JS execution, anti-bot stealth, XHR capture. |
| Hybrid | `--hybrid` | Pages render headlessly; static resources (`.js`, `.css`, …) fetch over plain HTTP. |

### Crawl control

- **Depth & duration limits** (`--depth`, `--crawl-duration`), depth-first or
  breadth-first queue strategies (`--strategy`).
- **Scope pipeline** — host-based scope (`dn`/`rdn`/`fqdn`/custom regex via the
  Public Suffix List), in/out-of-scope URL regexes (`--crawl-scope`,
  `--crawl-out-scope`).
- **Filters** — match/filter regexes, extension allow/deny lists, DSL
  match/filter conditions (`--match-condition "status_code == 200"`), exact
  content dedup, SimHash near-duplicate page filtering, similar-URL path-trie
  filtering, page-type heuristics (error/captcha/parked).
- **Rate limiting** — global and per-host token buckets, per-second and
  per-minute, plus fixed request delay.
- **Concurrency** — parallel fetcher workers (`--concurrency`) and parallel
  input processing (`--parallelism`).
- **Known files** — `robots.txt` / `sitemap.xml` crawling (`--known-files all`).
- **JavaScript crawling** — endpoint extraction from discovered JS/CSS files
  (`--js-crawl`, `--jsluice`).
- **Forms** — automatic form detection, filling, and submission
  (`--automatic-form-fill`), form metadata extraction (`--form-extraction`),
  YAML-configurable fill values (`--form-config`).
- **Auth & CAPTCHA** — automatic login (`--auto-login user:pass`) and a native
  capsolver client for reCAPTCHA/hCaptcha/Turnstile.
- **Tech detection & knowledge base** — response fingerprinting
  (`--tech-detect`) and secrets/endpoints classification (`--knowledge-base`).
- **Resume** — pending-queue state saved on interrupt, resumable via `--resume`.

### Output

- Screen format with verbose `[tag] [method] url [depth:n]` decorations.
- **JSONL** (`--jsonl`) with field exclusion (`--exclude-output-fields`),
  raw/body omission (`--omit-raw`, `--omit-body`).
- **14 field selectors** (`--field url,fqdn,qurl,…`) and per-host field storage
  (`--store-field`, `--store-field-dir`).
- Raw request/response storage per host (`--store-response`), custom output
  templates (`--output-template`), error logging (`--error-log`).

> 📖 **Complete flag-by-flag documentation: [docs/FEATURES.md](docs/FEATURES.md)**

---

## 🚀 CLI Quick Start

```bash
# Build
cargo build --release          # binary: target/release/celestia-browser

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
```

Flag syntax: long names use `--` (`--js-crawl`); unambiguous single-char shorts
work too (`-u`, `-d`, `-o`, `-j`, `-v`, `-c`, `-p`, `-s`). Run
`celestia-browser --help` for the full list.

---

## 📚 Library Quick Start

### UI / GUI app integration (`CrawlSession`)

Embedding in Tauri, Electron, egui, or any UI app: spawn a crawl from a
serde-serializable config, receive a typed event stream, and control it
(cancel / pause / resume) from your UI:

```rust
use browser_crawler::{CrawlConfig, CrawlSession, CrawlerEvent};

#[tokio::main]
async fn main() -> Result<(), String> {
    let session = CrawlSession::spawn(CrawlConfig {
        urls: vec!["https://example.com".into()],
        max_depth: 2,
        ..Default::default()
    })?;
    let mut events = session.take_events().unwrap();

    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            if let CrawlerEvent::Finished { summary } = event {
                println!("done: {} pages", summary.results);
                break;
            }
        }
    });

    // session.pause(); session.resume(); session.cancel();
    let summary = session.join().await?;
    Ok(())
}
```

Full recipes (Tauri commands + JS listener, Electron sidecar, native UIs) in
[docs/UI_INTEGRATION.md](docs/UI_INTEGRATION.md).

### Crawler engine API (`Runner`)

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

Every option is programmatically available on
[`Options`](src/types/options.rs); callbacks `options.on_result` /
`options.on_skip_url` receive each emitted/skipped URL.

### AI IR library API (`Browser::builder()`)

The original streaming IR pipeline still runs on the shared engine — static or
headless rendering, Markdown IR generation, per-page analysis callbacks, and
pluggable storage exporters:

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

One-off utilities: `crawl_single_page(url, &options)`, `browser.fetch_page(url)`,
and `extract_links(base_url, html)`.

---

## 🌐 Web Playground (WASM)

The pure, I/O-free core of the library (HTML → `PageIR` markdown IR, link
extraction, form parsing, JS endpoint extraction) compiles to WebAssembly and
runs in the browser via an interactive visual test app in
[`playground/`](playground/) (Vite + React):

```bash
cargo install wasm-pack                     # one-time
rustup target add wasm32-unknown-unknown    # one-time
pnpm --dir playground build:wasm            # wasm-pack build -> playground/src/wasm
pnpm --dir playground dev                   # http://localhost:5173
```

The app lets you fetch a URL (through a dev-only `/api/fetch` proxy that
sidesteps CORS) or paste HTML, then inspect the generated markdown IR, all
resolved links with internal/external scope badges, extracted forms, and JS
endpoints — all processed locally in WebAssembly. The network, headless-Chrome,
and filesystem engines are compiled out on the `wasm32` target; the native
build is unaffected.

---

## 🏗️ Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│                         celestia-browser                            │
├──────────────────┬──────────────────┬──────────────────────────────┤
│  standard engine │  headless engine │       hybrid engine          │
│  (reqwest HTTP)  │  (Chrome CDP +   │  (browser pages + HTTP       │
│                  │   stealth, XHR,  │   sub-resources)             │
│                  │   forms, captcha)│                              │
├──────────────────┴──────────────────┴──────────────────────────────┤
│                     engine::common (shared core)                    │
│   worker pool · queue (DFS/BFS) · scope manager · filter pipeline   │
│   rate limiting · retries · dedup (URL/content/simhash/path-trie)   │
├─────────────────────────────────────────────────────────────────────┤
│  parser (30+ tag/attr + header parsers, JS endpoints, forms)        │
├─────────────────────────────────────────────────────────────────────┤
│  output (screen / JSONL / template / fields / store-response)       │
├─────────────────────────────────────────────────────────────────────┤
│  utils (scope, DSL, simhash, path-trie, formfill, knownfiles, tech) │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 📖 Documentation

| Document | Contents |
|----------|----------|
| [docs/FEATURES.md](docs/FEATURES.md) | **Complete end-user feature reference** — every flag, engine, filter, output format, and field selector |
| [docs/USAGE.md](docs/USAGE.md) | Legacy IR-library usage guide and quickstart |
| [docs/API.md](docs/API.md) | Legacy `Browser::builder()` IR-library API reference (crawler API: [`src/types/options.rs`](src/types/options.rs), [`src/runner.rs`](src/runner.rs)) |

## 🧪 Examples & Tests

```bash
cargo run --example example       # multi-page streaming IR crawl
cargo run --example single_page   # one-off page fetch + link extraction
cargo test                        # 175 tests: unit + doc + end-to-end
```

**End-to-end coverage** (`tests/e2e_crawl.rs`, `tests/e2e_cli.rs`): the suite
spins up a local HTTP test server and exercises the real crawl engine and the
`celestia-browser` binary — full-site crawls, depth limits, scope regexes,
extension/match/DSL filtering, known-files, JS endpoint crawling, form
extraction & auto-fill, ignore-query-params, similar-URL collapsing, rate
limiting, crawl-duration stop, retries, tech detection, knowledge-base
secrets, JSONL output, store-response dumps, stdin input, and CLI
error handling.

## 📄 License

This project is licensed under the MIT License.
