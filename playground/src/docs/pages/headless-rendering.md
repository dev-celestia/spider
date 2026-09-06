## Choosing a rendering mode

| Mode | Flag | When to use |
|------|------|-------------|
| Static | *(default)* | Server-rendered sites; fastest. |
| Dynamic | `--headless` | SPAs and JS-heavy pages; every page renders in headless Chrome. |
| Hybrid | `--hybrid` | Pages render headlessly, but discovered static resources (`.js`, `.css`, …) fetch over plain HTTP. |

Library-side, the same choice is `RenderMode::Static / Dynamic` (or `Hybrid`) on `RenderOptions` / the builder.

## Chrome launch options

| Flag | Description |
|------|-------------|
| `--system-chrome` | Use the locally installed Chrome. |
| `--system-chrome-path` | Path to a specific Chrome binary. |
| `--show-browser` | Show the browser window (headed debugging). |
| `--headless-options` | Extra Chrome args (`--window-size=500,700`). |
| `--no-sandbox` | Launch with `--no-sandbox` (needed in most containers). |
| `--chrome-data-dir` | Chrome `--user-data-dir` (session persistence). |
| `--no-incognito` | Launch without incognito (data dir governs state). |
| `--chrome-ws-url` | Attach to an already-running Chrome via its debugger WebSocket URL. |
| `--max-failure-count` | Max consecutive failures before stopping (default `10`). |

## Page-load strategies

| Flag | Description |
|------|-------------|
| `--page-load-strategy` | `heuristic` (DOM-settle polling — default), `load`, `domcontentloaded`, `networkidle`, `none`. |
| `--dom-wait-time` | Seconds to wait after DOMContentLoaded (default `5`). |
| `--time-stable` | Seconds to wait for page stability. |

Library-side these map to `WaitUntil` (`Load`, `DOMContentLoaded`, `NetworkIdle`, …), `TimeoutStrategy` (`ExtractPartial` keeps partially rendered content on timeout), and `render_timeout`.

## Stealth

Stealth is **enabled by default** for headless rendering (`Options::with_defaults()` sets `stealth: true`). It patches common headless-Chrome fingerprints: navigator properties, WebGL vendor, chrome runtime presence, permissions, plugins, and CDP-detection artifacts. Turn it off with `options.stealth = false` in the library if a target misbehaves; there is no CLI switch — every headless CLI crawl runs stealthed.

## XHR capture

`--xhr-extraction` records every XHR/fetch call the page makes (method + URL) and attaches them to the JSONL result — useful for mapping API surface without reading JS source.
