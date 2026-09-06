## JavaScript crawling

| Flag | Description |
|------|-------------|
| `--js-crawl` | Fetch and scrape discovered JS/CSS files for endpoints (regex extractor). |
| `--jsluice` | jsluice-style JS endpoint extraction (native Rust approximation; applies to inline scripts too). |
| `--xhr-extraction` | Capture XHR/fetch calls (method + URL) the page makes into JSONL output. |

The same extraction powers the playground's **JS endpoints** tab via the `extract_js_endpoints` WASM export.

## Form handling

| Flag | Description |
|------|-------------|
| `--automatic-form-fill` | Detect forms, fill fields from type/name heuristics, and enqueue GET/POST form navigations. |
| `--form-extraction` | Attach form metadata (`method`, `action`, `enctype`, `parameters`) to JSONL results. |
| `--form-config` | YAML file customizing fill values (`email`, `color`, `password`, `phone`, `placeholder`). |

The form parser is the same one the playground surfaces: every `<form>` becomes method, action, enctype, and a parameter list — see the **Forms** tab for a visual check.

## Authentication

| Flag | Description |
|------|-------------|
| `--auto-login user:pass` | On the seed page, the login form is filled and submitted before crawling. Also via the `AUTH_CREDENTIALS` env var. Requires a headless mode. |

## CAPTCHA solving

| Flag | Description |
|------|-------------|
| `--captcha-solver-provider` | Provider name; `capsolver` supported (also `CAPTCHA_SOLVER_PROVIDER` env). |
| `--captcha-solver-key` | Provider API key (also `CAPTCHA_SOLVER_KEY` env). |

Supported challenge types: reCAPTCHA, hCaptcha, Turnstile. Detection pairs with `--filter-page-type captcha` to skip pages that still fail.

## Technology detection & knowledge base

| Flag | Description |
|------|-------------|
| `--tech-detect` | Fingerprint technologies from headers/body (WordPress, Next.js, Nginx, Cloudflare, …) → `technologies` in JSONL. |
| `--knowledge-base` | Classify discovered endpoints and secrets into a knowledge base. |
| `--secrets` | Scan response bodies for secret patterns (keys, tokens). |
| `--validate-secrets` | Attempt low-risk validation of found secrets. |
| `--endpoints` | Emit discovered API endpoints in JSONL output. |

## Known files

| Flag | Description |
|------|-------------|
| `--known-files` | `robots` (crawl robots.txt), `sitemap` (parse sitemap.xml), `all` (both). |

## Resume & lifecycle

| Flag | Description |
|------|-------------|
| `--resume <file>` | Save pending queue state on stop/interrupt; re-run with the same flag to resume. |
| `Ctrl-C` | Graceful stop — the pending queue is written to the `--resume` file, if set. |
