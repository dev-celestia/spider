The crate exposes three levels of API, all on the same engine.

## `Runner` — the crawler engine

Full option surface via `browser_crawler::Options` (every CLI flag is a field):

```rust
use browser_crawler::{Options, Runner, StandardWriter};
use std::sync::Arc;

let mut options = Options::with_defaults();
options.urls = vec!["https://example.com".into()];
options.max_depth = 2;
options.rate_limit = 50;
options.tech_detect = true;

let writer = Arc::new(StandardWriter::from_options(&options));
let mut runner = Runner::new(options)?;   // validates options
let summary = runner.run().await?;
println!("results: {}, failed: {}", summary.results, summary.failed);
```

Callbacks receive every emitted / skipped URL:

```rust
options.on_result = Some(Box::new(|result| { /* ... */ }));
options.on_skip_url = Some(Box::new(|url| { /* ... */ }));
```

Callbacks are not carried across `Options::clone()`. Lower-level entry points: `engine::execute(...)` (engine dispatch), `engine::common::Crawler` (queue worker pool), the `engine::common::PageFetch` trait (bring-your-own fetcher), and `output::StandardWriter` for screen/JSONL/template writers.

## `Browser::builder()` — the AI IR pipeline

The streaming IR facade: static or headless rendering, Markdown IR generation, per-page analysis callbacks, and pluggable storage exporters.

```rust
use std::time::Duration;
use browser_crawler::{Browser, RenderMode, WaitUntil, TimeoutStrategy};

let browser = Browser::builder()
    .start_url("https://example.com")
    .max_depth(2)
    .render_mode(RenderMode::Dynamic)
    .stealth(true)
    .wait_until(WaitUntil::NetworkIdle)
    .render_timeout(Duration::from_secs(10))
    .timeout_strategy(TimeoutStrategy::ExtractPartial)
    .on_page(|page_ir| async move {
        println!("Title: {}", page_ir.title);
        Ok(())
    })
    .build()?;

let summary = browser.run().await?;
```

One-off utilities: `crawl_single_page(url, &options)` and `browser.fetch_page(url)` return a single `PageIR`; `extract_links(base_url, html)` resolves a page's links without fetching.

## `CrawlSession` — UI / GUI integration

The embedding surface for Tauri, Electron, egui, or any UI host: spawn a crawl from a serde-serializable `CrawlConfig`, receive a typed event stream, and control it from UI buttons.

```rust
use browser_crawler::{CrawlConfig, CrawlSession, CrawlerEvent};

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
```

Full recipes (Tauri commands + JS listener, Electron sidecar, native UIs) live in `docs/UI_INTEGRATION.md`.

## Where the deep reference lives

| Document | Contents |
|----------|----------|
| `docs/FEATURES.md` | Every CLI flag, engine, filter, and output format |
| `docs/USAGE.md` | IR-library usage guide and quickstart |
| `docs/API.md` | Legacy `Browser::builder()` API reference |
| `src/types/options.rs` | The `Options` struct — the programmatic flag surface |
