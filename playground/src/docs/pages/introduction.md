`browser-crawler` is a high-performance Rust web browsing library and AI Intermediate Representation (IR) generator. It ships as two artifacts:

- **`celestia-browser`** — a CLI binary for full site crawls (a 1:1 Rust port of the reference Go crawler vendored under `reference/`)
- **`browser-crawler`** — the library crate exposing the same engine programmatically

Both are built on one shared core: a 4-phase interleaved queue that fetches pages, extracts the token-optimized Markdown IR (`PageIR`), discovers same-domain links, and pushes them back onto the queue until limits are reached.

## Crawl engines

| Engine | Flag | Description |
|--------|------|-------------|
| Standard | *(default)* | Plain HTTP fetching via `reqwest` — fastest path for static sites. |
| Headless | `--headless` | Every page renders in headless Chrome: live DOM, JS execution, anti-bot stealth, XHR capture. |
| Hybrid | `--hybrid` | Pages render headlessly; static resources (`.js`, `.css`, …) fetch over plain HTTP. |

## What's on top of the engine

- **Scope & filter pipelines** — host-based scope via the Public Suffix List, in/out regexes, extension allow/deny lists, a DSL for match/filter conditions, exact-content dedup, SimHash near-duplicate filtering, path-trie similar-URL collapsing, and page-type heuristics (error/captcha/parked).
- **Rate limiting & concurrency** — global and per-host token buckets (per-second and per-minute), fixed delay, parallel fetcher workers.
- **JavaScript crawling** — endpoint extraction from discovered JS/CSS files.
- **Form handling** — detection, metadata extraction, heuristic auto-fill with YAML customization, and automatic login.
- **Auth & CAPTCHA** — `--auto-login user:pass` and a native capsolver client for reCAPTCHA/hCaptcha/Turnstile.
- **Output** — decorated screen output, JSONL, custom templates, 14 field selectors, per-host raw response storage.
- **Resume** — pending-queue state saved on Ctrl-C, resumable with the same flag.

## AI Intermediate Representation

Every fetched page is transformed into a `PageIR`: noise-pruned, cleaned text rendered as Markdown, plus the page title and metadata. The IR is designed to be fed directly to LLM pipelines with minimal token overhead — this is what the [WASM core](#wasm-core) exposes to the browser, and what `on_page` callbacks receive in the library.

## Repository map

| Path | Contents |
|------|----------|
| `src/runner.rs` | `Runner` — validation, input parsing, engine dispatch |
| `src/engine/` | Standard/headless/hybrid engines + shared queue core |
| `src/types/options.rs` | The full `Options` surface (every flag is a field) |
| `src/transformer.rs` | HTML → `PageIR` markdown IR generation |
| `src/session.rs` | `CrawlSession` — UI integration surface |
| `src/wasm.rs` | WASM bindings (browser core) |
| `docs/FEATURES.md` | Complete flag-by-flag end-user reference |
| `docs/USAGE.md` | IR-library usage guide |
| `docs/API.md` | Legacy `Browser::builder()` API reference |
| `docs/UI_INTEGRATION.md` | Tauri / Electron / egui embedding recipes |
