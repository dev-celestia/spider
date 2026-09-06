# browser-crawler — End-User Feature Reference

This document lists **every end-user feature** of `browser-crawler`: the
`celestia-browser` CLI binary and the Rust library. The crawler is a 1:1 Rust
port of the reference Go crawler vendored under `reference/`.

- CLI binary: `cargo run --bin celestia-browser` (or `cargo build --release`; binary at
  `target/release/celestia-browser`)
- Library crate: `browser-crawler` (see [API.md](API.md) for the legacy
  `Browser::builder()` API and [USAGE.md](USAGE.md) for quickstart)

---

> **Flag syntax note:** this Rust port accepts the reference crawler's long flag names
> (prefixed with `--`) and its unambiguous single-char shorts (`-u`, `-d`,
> `-e`, `-H`, `-o`, `-c`, `-p`, `-j`, `-v`, `-s`). Multi-char celestia shorts
> like `-jc` are **long-only** here (`--js-crawl`), since the Rust CLI parser
> does not support multi-character short flags.

## Table of Contents

1. [Crawl Engines](#1-crawl-engines)
2. [Input](#2-input)
3. [Configuration](#3-configuration)
4. [Scope](#4-scope)
5. [Filters](#5-filters)
6. [Rate Limiting & Concurrency](#6-rate-limiting--concurrency)
7. [Headless Options](#7-headless-options)
8. [Output](#8-output)
9. [Field Selectors](#9-field-selectors)
10. [Known Files](#10-known-files)
11. [JavaScript Crawling](#11-javascript-crawling)
12. [Form Handling](#12-form-handling)
13. [Authentication & CAPTCHA](#13-authentication--captcha)
14. [Technology & Knowledge Base](#14-technology--knowledge-base)
15. [Resume & Lifecycle](#15-resume--lifecycle)
16. [Diagnostics](#16-diagnostics)
17. [Library API](#17-library-api)
18. [Feature Notes & Approximations](#18-feature-notes--approximations)

---

## 1. Crawl Engines

Three crawl engines, selected by flags (mutually exclusive):

| Engine   | Flag        | Behavior |
|----------|-------------|----------|
| Standard | *(default)* | Plain HTTP requests via `reqwest`. Fastest; parses raw HTML. |
| Headless | `--headless` | Every page renders in headless Chrome (live DOM, JS execution, stealth). |
| Hybrid   | `--hybrid`   | Pages render headlessly; static resources (`.js`, `.css`, …) are fetched over plain HTTP. |

Both headless modes share:
- **Stealth rendering** — anti-bot masking (`navigator.webdriver` removal,
  Chrome flag hardening, fingerprint scripts) enabled by default.
- **Live-DOM link extraction** — links are parsed from the *rendered* DOM, so
  SPA (React/Vue/Angular) navigation targets are discovered.
- **XHR capture** (`--xhr-extraction`) — fetch/XHR calls are intercepted with a
  document-start hook and reported in JSONL output (`xhr_requests`).

```bash
celestia -u https://example.com            # standard
celestia -u https://example.com --hl        # headless
celestia -u https://example.com --hh        # hybrid
```

## 2. Input

| Flag | Description |
|------|-------------|
| `--list` | Target URL(s). Repeatable; accepts a file path (one URL per line). |
| *(stdin)* | When no `--list` is given, URLs are read from stdin (one per line), e.g. `cat urls.txt \| celestia` |
| `--exclude` | Exclude hosts matching a filter (`cdn`, `private-ips`, cidr, ip, regex). |

```bash
celestia -u https://example.com -u https://other.com
echo "https://example.com" | celestia -d 3
```

## 3. Configuration

| Flag | Default | Description |
|------|---------|-------------|
| `--depth` | `3` | Maximum crawl depth. |
| `--crawl-duration` | `0` | Max duration (`30s`, `5m`, `1h`, `2d`). Stops the crawl when elapsed. |
| `--strategy` | `depth-first` | Queue strategy: `depth-first` (LIFO) or `breadth-first` (FIFO). |
| `--timeout` | `10` | Per-request timeout (seconds). |
| `--retry` | `1` | Retries per failed request. |
| `--max-response-size` | `4194304` | Max response body bytes to read. |
| `--time-stable` | `1` | Seconds to wait for page stability (headless). |
| `--disable-redirects` | off | Do not follow HTTP redirects. |
| `--proxy` | — | HTTP/SOCKS5 proxy URL. |
| `--headers` | — | Custom headers (`Key: Value`); repeatable or a file path. Sent on every request. |
| `--resolvers` | — | Custom resolvers (accepted; enforced best-effort). |
| `--ignore-query-params` | off | Treat `/page?a=1` and `/page?a=2` as the same URL. |
| `--max-domain-pages` | `0` | Cap pages crawled per domain (0 = unlimited). |
| `--path-climb` | off | Also crawl parent paths of discovered URLs (`/a/b/c` → `/a/`, `/a/b/`). |
| `--sitemap-tree` | off | Skip the crawl engine and build a DFS sitemap link tree from each `-u` target instead, emitting `SitemapNode` JSON (a single root object for one URL, an array for several). Honors `--depth` (tree depth) and `--headless` (dynamic rendering); use `-o` to write the JSON to a file. |
| `--config` | — | Celestia configuration file (accepted for compatibility). |

## 4. Scope

Controls which discovered URLs are followed:

| Flag | Default | Description |
|------|---------|-------------|
| `--crawl-scope` | — | Regex list; only matching URLs are followed. |
| `--crawl-out-scope` | — | Regex list; matching URLs are excluded. |
| `--field-scope` | `rdn` | Host-based scope: `dn` (contains root domain), `rdn` (registrable domain + subdomains — default), `fqdn` (exact host), or a custom regex. |
| `--no-scope` | off | Disable host-based default scope. |
| `--display-out-scope` | off | Log external URLs skipped by scope. |

```bash
celestia -u https://example.com --cs "/api/" --cos "logout"
celestia -u https://example.com --fs fqdn        # exact-host only
celestia -u https://example.com --fs "(a.io|b.com)"
```

## 5. Filters

Filter what is crawled and/or printed:

| Flag | Description |
|------|-------------|
| `--match-regex` | Regex list — output/crawl only matching URLs. |
| `--filter-regex` | Regex list — drop matching URLs. |
| `--extension-match` | Only URLs with these extensions (`--em php,html,js,none`; `none` = no extension). |
| `--extension-filter` | Ignore URLs with these extensions. |
| `--no-default-ext-filter` | Keep the default denylist (images, archives, …) unfiltered. |
| `--match-condition` | DSL expression — emit only results where truthy. |
| `--filter-condition` | DSL expression — drop results where truthy. |
| `--disable-unique-filter` | Disable exact-content dedup (MD5 of body). |
| `--filter-similar` | Filter similar-looking URLs by path-trie normalization (`/users/123` ≈ `/users/456`). |
| `--filter-similar-threshold` | Distinct values before a path position becomes a variable (default `10`). |
| `--page-content-similar` | Near-duplicate page filtering (SimHash). |
| `--similarity-deduplication` | Alias of `--page-content-similar`. |
| `--page-content-similar-mode` | `simhash` (default), `tfidf`, `bm25`. |
| `--page-content-similar-distance` | SimHash max Hamming distance (default `3`). |
| `--page-content-similar-threshold` | TF-IDF/BM25 min score 0–1 (default `0.85`). |
| `--page-content-similar-budget` | Pages fully processed per similarity cluster (default `1`). |
| `--filter-page-type` | Drop responses by heuristic type: `error`, `captcha`, `parked`. |

**DSL condition language** (`--match-condition`/`--filter-condition`): dotted result fields (`url`,
`method`, `tag`, `depth`, `status_code`, `content_length`, `body`, `words`,
`lines`, `request.endpoint`, `response.status_code`, …), literals, comparisons
(`== != < > <= >=`), boolean ops (`&& || !`), and functions `contains`,
`starts_with`, `ends_with`, `matches`, `upper`, `lower`, `len`, `words`,
`lines`:

```bash
celestia -u https://example.com --mdc "status_code == 200 && contains(url, '/api/')"
celestia -u https://example.com --fdc "matches(url, '\\.js$') || words(body) < 50"
```

## 6. Rate Limiting & Concurrency

| Flag | Default | Description |
|------|---------|-------------|
| `--concurrency` | `10` | Concurrent fetcher workers. |
| `--parallelism` | `10` | Concurrent input URLs processed. |
| `--delay` | `0` | Fixed delay between requests (seconds). |
| `--rate-limit` | `150` | Max requests/second (global token bucket). |
| `--rate-limit-minute` | `0` | Max requests/minute (global). |
| `--host-rate-limit` | `0` | Max requests/second per host. |
| `--host-rate-limit-minute` | `0` | Max requests/minute per host. |

## 7. Headless Options

| Flag | Description |
|------|-------------|
| `--headless` | Headless engine. |
| `--hybrid` | Hybrid engine. |
| `--system-chrome` | Use the locally installed Chrome. |
| `--system-chrome-path` | Path to a specific Chrome binary. |
| `--show-browser` | Show the browser window. |
| `--headless-options` | Extra Chrome args (`--window-size=500,700`). |
| `--no-sandbox` | Launch with `--no-sandbox`. |
| `--chrome-data-dir` | Chrome `--user-data-dir` (session persistence). |
| `--no-incognito` | Launch without incognito (data dir governs). |
| `--chrome-ws-url` | Attach to a running Chrome via its debugger WebSocket URL. |
| `--xhr-extraction` | Capture XHR/fetch calls (method + URL) into JSONL output. |
| `--max-failure-count` | Max consecutive failures before stopping (default `10`). |
| `--enable-diagnostics` | Enable engine diagnostics. |
| `--page-load-strategy` | Wait strategy: `heuristic` (DOM-settle polling, default), `load`, `domcontentloaded`, `networkidle`, `none`. |
| `--dom-wait-time` | Seconds after DOMContentLoaded (default `5`). |

## 8. Output

| Flag | Description |
|------|-------------|
| `--output` | Append results to a file (screen output always continues). |
| `--jsonl` | JSON Lines output: one JSON object per result (`request`, `response`, `timestamp`). |
| `--no-color` | Disable ANSI coloring. |
| `--silent` | Results only — no log lines. |
| `--verbose` | Decorate results: `[tag] [method] url [depth:n]`. |
| `--debug` | Debug logging (retries, render internals). |
| `--output-template` | Custom template with `{{.request.endpoint}}`--style JSON-path tokens. |
| `--list-output-fields` | Print the 14 field names and exit. |
| `--exclude-output-fields` | Remove fields from JSONL output (supports dotted paths like `response.body`). |
| `--omit-raw` | Omit raw request/response dumps. |
| `--omit-body` | Omit response body. |
| `--store-response` | Save raw request/response per host: `<dir>/<host>/<hash>.txt` + `index.txt`. |
| `--store-response-dir` | Custom store-response directory (default `output`). |
| `--no-clobber` | Never overwrite `--output` files (`out.txt` → `out-1.txt`). |
| `--error-log` / `--error-log` | File logging every failed request. |

JSONL result shape (fields with empty values are omitted; `endpoint` is the
URL, mirroring the reference crawler's key names):

```json
{"request":{"endpoint":"https://x.com/a","method":"GET","tag":"a","attribute":"href","source":"https://x.com/"},
 "response":{"status_code":200,"headers":{...},"body":"...","content_length":150,"technologies":["Nginx"],
             "forms":[...],"xhr_requests":[...],"stored_response_path":"...","knowledgebase":{...}},
 "timestamp":"2026-09-05T08:42:06Z"}
```

## 9. Field Selectors

`--f/--field` prints only the selected fields; `--store-field` appends the
same values to per-host files in `--store-field-dir`
(`<scheme>_<host>_<field>.txt`):

| Field | Meaning |
|-------|---------|
| `url` | Full URL |
| `path` | URL path |
| `fqdn` | Fully-qualified host name |
| `rdn` | Registrable domain (eTLD+1, via the Public Suffix List) |
| `rurl` | `scheme://host` |
| `qurl` | URL only when it has query parameters |
| `qpath` | `path?query` |
| `file` | Filename when the path has a dotted file |
| `ufile` | Full URL when the path has a dotted file |
| `key` | Query parameter names (one per line) |
| `value` | Query parameter values |
| `kv` | `key=value` pairs |
| `dir` | Directory part of the path |
| `udir` | `scheme://host` + directory |

```bash
celestia -u https://example.com --f "url,rdn,qurl"
celestia -u https://example.com --sf "kv" --sfd ./fields
```

## 10. Known Files

`--known-files` crawls `robots.txt` and/or `sitemap.xml` from the target
root and enqueues discovered paths (depth 2). Values: `all`, `robotstxt`,
`sitemapxml`. Depth is automatically raised to 3 when used.

## 11. JavaScript Crawling

| Flag | Description |
|------|-------------|
| `--js-crawl` | Fetch and scrape discovered JS/CSS files for endpoints (regex extractor). |
| `--jsluice` | jsluice-style JS endpoint extraction (native Rust approximation; applies to inline scripts too). |

Common third-party libraries (jQuery, React, …) are skipped automatically.

## 12. Form Handling

| Flag | Description |
|------|-------------|
| `--automatic-form-fill` | Detect forms, fill fields from type/name heuristics, and enqueue GET/POST form navigations. |
| `--form-extraction` | Attach form metadata (`method`, `action`, `enctype`, `parameters`) to JSONL results. |
| `--form-config` | YAML file customizing fill values (`email`, `color`, `password`, `phone`, `placeholder`). |

Default fill values: `celestia@example.org`, `#e66465`, `CelestiaP@assw0rd1`,
`2124567890`, `celestia`.

## 13. Authentication & CAPTCHA

| Flag | Description |
|------|-------------|
| `--auto-login` | `username:password` (or `AUTH_CREDENTIALS` env). On the seed page the login form is filled and submitted before crawling. Requires a headless mode. |
| `--captcha-solver-provider` | `CAPTCHA_SOLVER_PROVIDER` env. `capsolver` supported. |
| `--captcha-solver-key` | `CAPTCHA_SOLVER_KEY` env. |

CAPTCHA detection recognizes reCAPTCHA, hCaptcha and Cloudflare Turnstile
markers; with capsolver configured the token is solved and injected into the
page callback input.

## 14. Technology & Knowledge Base

| Flag | Description |
|------|-------------|
| `--tech-detect` | Fingerprint technologies from headers/body (WordPress, Next.js, Nginx, Cloudflare, …) → `technologies` in JSONL. |
| `--knowledge-base` | Enable classification analysis. |
| `--kb-secrets` | Scan responses for secret fingerprints (AWS/GitHub/Slack keys, JWTs, private keys, …). Matches are redacted in output. |
| `--kb-endpoints` | Classify endpoint paths found in responses as REST / GraphQL / SOAP / XHR. |
| `--kb-validate-secrets` | Accepted; live validation sends requests to credential providers — treat as opt-in. |

Results appear under `knowledgebase` in JSONL output.

## 15. Resume & Lifecycle

| Flag | Description |
|------|-------------|
| `--resume <file>` | Save pending queue state on stop/interrupt; re-run with the same flag to resume. |
| `Ctrl-C` | Graceful stop — pending queue is written to the `--resume` file, if set. |

Depth or duration requirement: at least one of `--depth` / `--crawl-duration` must be set.

## 16. Diagnostics

| Flag | Description |
|------|-------------|
| `--health-check` | Self-check: version, DNS resolution, Chrome availability, outbound TCP. |
| `--error-log` | Per-request failure log. |
| `--pprof-server` | Accepted (no-op — Go runtime feature). |

## 17. Library API

The Rust crate exposes the full option surface programmatically
(`browser_crawler::types::options::Options`), plus:

```rust
use browser_crawler::{Runner, Options, StandardWriter};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut options = Options::with_defaults();
    options.urls = vec!["https://example.com".into()];
    options.max_depth = 2;
    options.rate_limit = 50;

    let writer = Arc::new(StandardWriter::from_options(&options));
    let mut runner = Runner::new(options)?;
    let summary = runner.run().await?;
    println!("results: {}", summary.results);
    Ok(())
}
```

Key library entry points:

| Item | Purpose |
|------|---------|
| `Options` / `Options::with_defaults()` | Full celestia option surface (`src/types/options.rs`). |
| `Runner::new(options)` → `run()` | Validation + input parsing + engine dispatch + summary. |
| `engine::execute(...)` | Standard/headless/hybrid dispatch with input parallelism. |
| `engine::common::Crawler` | Queue worker pool, scope/filters, rate limiting, dedup. |
| `engine::common::PageFetch` | Implement your own fetcher engine (trait). |
| `engine::parser` | Navigation extraction (header + 30+ tag/attribute parsers). |
| `output::StandardWriter` | Screen / JSONL / template / field writers. |
| `output::fields` | 14 field selectors + per-host field storage. |
| `utils::*` | Queue strategies, scope manager, DSL evaluator, simhash, path trie, form fill, known files, tech detect, knowledge base. |
| `Browser::builder()` | Legacy high-level facade (unchanged; still works). |
| `crawl_single_page`, `PageFetcher` | Legacy single-page utilities. |

Callbacks: `options.on_result = Some(Box::new(|result| {...}))` and
`options.on_skip_url = Some(Box::new(|url| {...}))` receive every emitted /
skipped URL (note: callbacks are not carried across `Options::clone()`).

## 18. Feature Notes & Approximations

Features ported as **native Rust approximations** (flag-compatible, engine
divergent from the Go original):

- `--jsluice` (jsluice): regex-based JS endpoint extraction instead of the jsluice
  tree-sitter analyzer.
- Knowledge base (`--kb*`): built-in fingerprint regexes instead of `dit`/Titus
  models; the `-fpt` page-type filter uses body/status heuristics instead of the
  dit classifier (applied at output time, like the reference crawler).
- CAPTCHA solving: direct capsolver HTTP client instead of the Go SDK.
- `--tls-impersonate` (TLS impersonation): accepted; reqwest's TLS stack is used (no ja3
  randomization).
- `--pprof-server`: no-op (Go runtime feature).
- `--resolvers` (resolvers): accepted; not yet enforced by the HTTP stack.
- `--update` / `--disable-update-check`: the self-update mechanism and network
  version check are Go-ecosystem features; the flags are accepted (`-up` prints a
  notice and exits) but no update feed exists.
- `--config`: loaded as a simple `flag-name: value` file (goflags-style); only
  common scalar flags are mapped.
- Headless XHR capture uses a document-start JS hook (fetch/XHR wrappers), not a
  CDP network-event tap; captured XHR URLs are enqueued for crawling like the
  reference crawler.
- Onclick link discovery (`-ol`) extracts candidate URLs from `onclick`/
  `javascript:` attributes via injected JS instead of clicking elements and
  detecting navigation (hybrid `crawl.go` click simulation).
- Shadow-DOM traversal and JS-triggered navigation capture (hybrid CDP frame
  events) are covered by the live-DOM `get_content` extraction, not CDP
  `DOM.getDocument{Pierce}` walks.
- Headless responses report `status 200` + synthetic `content-type` (the
  headless_chrome crate does not expose the navigation response status).
- Stored-response file names use an MD5 URL hash (the reference crawler uses SHA-1),
  and raw request/response dumps are reconstructed from parsed fields where the
  engine does not capture wire bytes.
- `-noi` (no-incognito): the headless_chrome crate launches a regular profile,
  so the flag is inherently satisfied.
- Parallelism (`-p`) uses a sliding window over inputs (sizedwaitgroup semantics).

Everything else — scope rules (including the `dn` keyword), filter pipeline order
and stage placement (output-time match/filter regexes, extension validation, DSL
conditions, page-type filter), dedup semantics, `-fsu` URL fingerprinting (layer-1
segment normalization, per-host adaptive trie, sorted query keys), resume
(save on SIGINT / load with `-resume`), `-exclude` networkpolicy filtering,
known-files behavior, custom field extraction (`-flc` + default email field),
form-fill suggestions (placeholder precedence, faker resolution), field selectors,
output formats (template > JSONL > fields > screen precedence, file truncation,
decolorized file output), and default values — mirrors the reference crawler 1:1
and is covered by unit tests ported from the Go test suite.
