# UI Integration Guide

How to embed `browser-crawler` in UI-based applications (Tauri, Electron, Slint,
egui, GTK, …). Everything a UI needs lives in two modules:

| Module | What it provides |
|---|---|
| [`session`](../src/session.rs) | `CrawlConfig` (serde config) + `CrawlSession::spawn` + `CrawlSessionHandle` (cancel / pause / resume / events / join) |
| [`types::events`](../src/types/events.rs) | `CrawlerEvent` (tagged, serde-serializable event stream) + `SessionSummary` |
| [`control`](../src/control.rs) | `CrawlControl` — the shared cancel/pause flags if you drive `Runner` yourself |

## Design principles

1. **Config is data, events are data.** `CrawlConfig` and `CrawlerEvent` derive
   `serde::Serialize`/`Deserialize`, so the boundary between your UI process and
   the crawl engine is plain JSON — no trait objects, no callbacks crossing FFI.
2. **The host owns the runtime.** The library never installs signal handlers,
   prints progress you didn't ask for (set `silent: true`), or spawns threads
   you don't control. `CrawlSession::spawn` runs the crawl as one `tokio::spawn`
   task and hands you a handle.
3. **Multiple consumers + late joiners.** Events fan out over a
   `tokio::sync::broadcast` channel: one receiver per window, per WebSocket
   client, per log sink. Consumers that join after the crawl started hydrate
   from `session.snapshot()` first (counters + phase), then tail events.
4. **Graceful lifecycle.** `cancel()` is cooperative (workers finish their
   current page and drain); `pause()/resume()` idle the workers without losing
   the queue; `Finished` always arrives with a full `SessionSummary`.

## Quick start (5 lines)

```rust
use browser_crawler::{CrawlConfig, CrawlSession, CrawlerEvent, SessionSummary};

#[tokio::main]
async fn main() -> Result<(), String> {
    let config = CrawlConfig {
        urls: vec!["https://example.com".into()],
        max_depth: 2,
        ..Default::default()
    };
    let session = CrawlSession::spawn(config)?;
    let mut events = session.take_events().unwrap();

    // Drive the UI from events, in the background:
    tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            match event {
                CrawlerEvent::PageFetched { url, status_code, .. } =>
                    println!("fetched {url} ({status_code})"),
                CrawlerEvent::Finished { summary } => {
                    println!("done: {} pages", summary.results);
                    break;
                }
                _ => {}
            }
        }
    });

    // ...later, from a button handler:
    // session.pause();  session.resume();  session.cancel();

    let summary = session.join().await?;
    println!("visited {} URLs in {} ms", summary.visited_urls.len(), summary.duration_ms);
    Ok(())
}
```

## Tauri (v2)

The natural mapping: spawn in an **async command**, stream events back through a
Tauri `ipc::Channel` (or `app.emit` for multi-window broadcast), store the
handle in **managed state**.

Two Tauri-IPC rules the integration depends on:

- **Spawn from an `async` command.** Tauri runs `async` commands inside its
  tokio runtime and sync commands on a plain worker thread. `CrawlSession::spawn`
  needs the tokio context (`tokio::spawn`), so `start_crawl` must be
  `async fn` — a sync command would panic at runtime, not compile time.
- **Payload casing is consistent end to end.** `CrawlConfig` accepts camelCase
  (`maxDepth`), events serialize as `{"type": "page_fetched", "statusCode": 200}`
  (snake_case tag, camelCase fields), and `SessionSummary` returns camelCase.
  One convention on the JS side.

### Recommended: `ipc::Channel` (per-crawl, ordered, scoped to the caller)

```rust
use std::sync::Mutex;

use browser_crawler::{CrawlConfig, CrawlSession, CrawlSessionHandle, CrawlerEvent, SessionSummary};
use tauri::{ipc::Channel, State};

struct SessionState(Mutex<Option<CrawlSessionHandle>>);

#[tauri::command]
async fn start_crawl(
    config: CrawlConfig,
    on_event: Channel<CrawlerEvent>,   // passed from JS per invocation
    state: State<'_, SessionState>,
) -> Result<(), String> {
    let session = CrawlSession::spawn(config)?;
    let mut events = session.take_events().unwrap();

    // Bridge the broadcast stream into the IPC channel. Tauri serializes each
    // event to JSON on its way to the webview.
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = events.recv().await {
            if on_event.send(event).is_err() {
                break; // webview gone
            }
        }
    });

    *state.0.lock().unwrap() = Some(session);
    Ok(())
}

#[tauri::command]
fn pause_crawl(state: State<SessionState>) { state.0.lock().unwrap().as_ref().unwrap().pause(); }

#[tauri::command]
fn resume_crawl(state: State<SessionState>) { state.0.lock().unwrap().as_ref().unwrap().resume(); }

#[tauri::command]
fn cancel_crawl(state: State<SessionState>) { state.0.lock().unwrap().as_ref().unwrap().cancel(); }

// Hydrate a UI that joins after the crawl started (window reopen, second
// window): load the snapshot once, then tail events for live updates.
#[tauri::command]
fn crawl_snapshot(state: State<SessionState>) -> Result<browser_crawler::SessionSnapshot, String> {
    state.0.lock().unwrap().as_ref().ok_or("no active crawl")?.snapshot()
}

#[tauri::command]
async fn finish_crawl(state: State<'_, SessionState>) -> Result<SessionSummary, String> {
    let session = state.0.lock().unwrap().take().ok_or("no active crawl")?;
    session.join().await
}

// Optional: serve page content on demand. Keep a `BrowserPipeline` (or the
// `Browser`) in managed state alongside the session handle.
// #[tauri::command]
// async fn fetch_page_ir(url: String, pipeline: State<'_, BrowserPipeline>)
//     -> Result<browser_crawler::PageIR, String>
// {
//     pipeline.fetch_page(&url).await
// }

fn main() {
    tauri::Builder::default()
        .manage(SessionState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![start_crawl, pause_crawl, resume_crawl, cancel_crawl, crawl_snapshot, finish_crawl])
        .run(tauri::generate_context!())
        .expect("tauri run");
}
```

### Frontend (JS) side

```js
import { invoke, Channel } from "@tauri-apps/api/core";

// Channel: events arrive here in order, scoped to this crawl invocation.
const onEvent = new Channel();
onEvent.onmessage = (event) => {
  switch (event.type) {
    case "started":         /* event.engine, event.seeds */           break;
    case "page_fetched":    /* event.url, event.statusCode */         break;
    case "page_error":      /* event.url, event.error */              break;
    case "page_skipped":    /* event.url, event.reason */             break;
    case "link_discovered": /* event.url, event.from, event.depth */  break;
    case "progress":        /* event.results, event.skipped, event.failed */ break;
    case "paused": case "resumed": case "cancelled":                  break;
    case "finished":        /* event.summary (camelCase fields) */    break;
  }
};

await invoke("start_crawl", {
  config: { urls: ["https://example.com"], maxDepth: 2 }, // camelCase, matches CrawlConfig
  onEvent,
});
await invoke("pause_crawl");
await invoke("cancel_crawl");
const summary = await invoke("finish_crawl"); // { results, skipped, visitedUrls, durationMs, ... }

// Reopening window / late joiner: hydrate, then tail.
const snapshot = await invoke("crawl_snapshot");
// { seeds, engine, phase: "running"|"paused"|"cancelling"|"finished", results, skipped, failed }
```

### Alternative: `app.emit` broadcast

If several windows should observe the same crawl, keep the spawn in an async
command but forward with `app.emit("crawler://event", &event)` instead of a
`Channel`, and receive with `listen("crawler://event", ...)` from
`@tauri-apps/api/event`. Same payload shapes; every registered listener gets
every event.

### Transferring page content

Events intentionally carry metadata only (URL, status, length) — never page
bodies. When the UI wants the markdown IR of a page, invoke a command that
calls `pipeline.fetch_page(url)` / `browser.fetch_page(url)` and returns the
`PageIR` as the command's response. This keeps the event stream small and the
IPC boundary free of multi-MB payloads.

## Electron / Node sidecar

Use the wasm build (`wasm.rs`) for pure parsing (`transform_to_ir`,
`extract_links`, `extract_forms`, `extract_js_endpoints`) with **no network
process**. For full crawling, compile the library as a binary sidecar: extend
`celestia-browser` (or add a small `bin` that speaks JSONL on stdout) and
communicate via stdin/stdout — the same `CrawlerEvent` JSON payloads can be
written line-by-line and parsed in Node.

## egui / Slint / native UIs

No JSON needed — the same API works with in-process receivers:

```rust
let session = browser_crawler::session::spawn(config)?;
let mut events = session.subscribe();
// each frame:
while let Ok(event) = events.try_recv() {
    /* update UI state */
}
```

## Driving `Runner` directly

If you need the full `Options` surface (the complete reference-crawler flag set)
rather than `CrawlConfig`, use `Runner::with_control` + `set_event_channel`:

```rust
use browser_crawler::{CrawlControl, Runner};
use tokio::sync::broadcast;

let mut options = browser_crawler::Options::with_defaults();
options.urls = vec!["https://example.com".into()];
options.headless = true; // full flag surface available

let control = CrawlControl::default().shared();
let (events, _) = broadcast::channel(1024);

let mut runner = Runner::with_control(options, control.clone())?;
runner.set_event_channel(events.clone());
// hold `control` in your UI state: control.pause() / .resume() / .cancel()
let summary = runner.run().await?;
```

Note that `Runner::with_control` deliberately does **not** install a ctrl-C
handler — in a GUI process the ctrl-C listener would swallow the host's
handling (and on some platforms intercept window-close signals). CLI usage
keeps `Runner::new`, which installs it.

## Event reference

| Event | Fields | When |
|---|---|---|
| `started` | `seeds`, `engine` (`standard`/`headless`/`hybrid`) | once, at crawl start |
| `page_fetched` | `url`, `statusCode`, `depth`, `contentLength`, `forms`, `technologies` | every successful page |
| `page_error` | `url`, `error` | fetch failed after retries |
| `page_skipped` | `url`, `reason` | URL filtered by scope/filters |
| `link_discovered` | `url`, `from`, `depth` | (Browser pipeline) new link queued |
| `progress` | `results`, `skipped`, `failed` | accompanies result/skip/error events |
| `message` | `level`, `text` | human-readable log lines |
| `paused` / `resumed` / `cancelled` | — | control transitions |
| `finished` | `summary: SessionSummary` | once, at the very end |

## Testing your integration

`CrawlSession` is designed to be testable offline: spawn a session pointed at a
closed port (e.g. `http://127.0.0.1:1/`) and assert the event sequence — see
`src/session.rs` tests (`test_session_lifecycle_offline`,
`test_session_cancel_finishes_promptly`, `test_session_pause_resume_events`).
