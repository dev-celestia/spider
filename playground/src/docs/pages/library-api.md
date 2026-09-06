The crate exposes four levels of API, all on the same engine.

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

## `SiteMapper` — sitemap link trees

A focused depth-first mapper for rapid link discovery (CLI: `--sitemap-tree`). Instead of running the crawl pipeline, it fetches each page, collects same-host links, and recurses — producing a nested `SitemapNode` tree where each node is `{ url, depth, children }`. A link reachable from multiple parents appears once, under the first DFS path that reaches it.

```rust
use browser_crawler::{RenderOptions, SiteMapper};

let mapper = SiteMapper::builder()
    .max_depth(3)
    .max_pages(500)          // 0 = unlimited; stops DFS once the cap is hit
    .user_agent("MyAgent/1.0")
    .render_options(RenderOptions::default()) // RenderMode::Dynamic for headless
    .build();

if let Some(root) = mapper.map_site("https://example.com").await {
    println!("{}", serde_json::to_string_pretty(&root)?);
}
println!("fetched {} pages", mapper.pages_fetched());
```

`SiteMapper::new(max_depth)` is a shorthand for `builder().max_depth(n).build()`. For a flat, concurrent scan of the same site, use the crawl engine's `depth-first`/`breadth-first` strategies instead.

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

## Where the deep reference lives

| Source | Contents |
|--------|----------|
| `src/types/options.rs` | The `Options` struct — the programmatic flag surface |
| `src/mapper.rs` | `SiteMapper` / `SiteMapperBuilder` — sitemap tree generation |
| `src/session.rs` | `CrawlConfig`, `CrawlSession`, `CrawlerEvent` — embedding surface |
| `src/builder.rs` | `Browser::builder()` — the IR pipeline facade |
| In-app **Docs** view | End-user reference: flags, engines, filters, output |
