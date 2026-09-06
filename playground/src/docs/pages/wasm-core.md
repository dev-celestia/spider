The pure, I/O-free core of the library — HTML parsing, `PageIR` markdown IR generation, link extraction, form parsing, and JS endpoint extraction — compiles to **WebAssembly** on the `wasm32` target. The network engine, headless Chrome, and filesystem exporters are compiled out; the native build is unaffected.

## Exports

| Function | Signature | Description |
|----------|-----------|-------------|
| `transform_to_ir` | `(url, html) → PageIR` | Raw HTML → `{ url, title, markdown_ir }` plain JS object. |
| `extract_all_links` | `(base_url, html) → string[]` | Every HTTP(S) link resolved against `base_url`. |
| `extract_links` | `(base_url, html) → string[]` | Same-host links only, matching the crawler's queue-discovery semantics. |
| `extract_forms` | `(html) → Form[]` | Forms as `{ method, action, enctype, parameters }`. |
| `extract_js_endpoints` | `(content) → string[]` | jsluice-style endpoint candidates from JS source. |
| `version` | `() → string` | Crate version. |

Build with:

```bash
cargo install wasm-pack                     # one-time
rustup target add wasm32-unknown-unknown    # one-time
pnpm --dir playground build:wasm            # emits into playground/src/wasm
```

## The playground

This app *is* the visual test harness for the WASM core. The **Playground** view fetches a URL through a dev-only CORS proxy or takes pasted HTML, runs it through the module, and shows the Markdown IR, resolved links with internal/external badges, forms, and JS endpoints.

Use it to sanity-check transformer and parser changes without writing a Rust test: rebuild with `pnpm --dir playground build:wasm`, reload, paste a problematic page, and inspect the IR.

## Why the split matters

Everything behind `transform_to_ir` is deterministic and side-effect free, so the exact same code paths run in three contexts: the CLI, the Rust library, and this browser module. A rendering difference you see here reproduces natively — and vice versa.
