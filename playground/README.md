# Celestia Playground

An interactive browser app for exercising the WebAssembly build of
[`browser-crawler`](../): paste HTML or fetch a URL and inspect the generated
**Markdown IR**, **links**, **forms**, and **JS endpoints** — all parsed locally
in WASM by the same Rust code paths the native crawler uses.

## Quick start

```bash
pnpm install
pnpm dev          # http://localhost:5173
```

Rebuilding the WASM module after changing the Rust crate:

```bash
pnpm build:wasm   # wasm-pack build --target web -> src/wasm/
```

The compiled module is committed under `src/wasm/`, so you only need a Rust
toolchain when the crate changes.

## App views

- **Playground** — two input modes (**Fetch URL** through a dev-only `/api/fetch`
  CORS proxy, or **Paste HTML** with a base URL) and five result tabs
  (Markdown IR, Links, Forms, JS endpoints, Raw JSON).
- **Docs** — complete in-app documentation of the `browser-crawler` **Rust library and
  CLI**: engines, scope & filters, rate limiting, headless rendering & stealth, JS
  crawling / forms / auth, output formats, the library API (`Runner`,
  `Browser::builder()`, `CrawlSession`), and the WASM core. Pages live as markdown
  under `src/docs/pages/` and are bundled into the app.

## Scripts

| Script | What it does |
|--------|--------------|
| `dev` | Vite dev server with HMR and the dev-only `/api/fetch` proxy |
| `build` | Typecheck (`tsc -b`) + production bundle into `dist/` |
| `preview` | Serve the production build locally |
| `lint` | ESLint |
| `build:wasm` | Rebuild the WASM bindings into `src/wasm/` |

> The production bundle has no `/api/fetch` proxy — reverse-proxy it yourself for
> **Fetch URL** mode to work in a deployment. **Paste HTML** mode works anywhere.

## GitHub Pages

The site auto-deploys on every push to `master` that touches `playground/**`
via [`.github/workflows/deploy-playground.yml`](../.github/workflows/deploy-playground.yml)
(there's also a manual **workflow_dispatch** trigger). The first time it runs,
enable Pages in the repo under **Settings → Pages → Source: GitHub Actions**.

The workflow builds with `VITE_PUBLIC_BASE=/<repo>/` so assets resolve under the
project-site subpath. To build a Pages-style bundle locally:

```bash
VITE_PUBLIC_BASE=/browser-crawler/ pnpm build && pnpm preview
```

Full documentation of the Rust library (setup, engines, filters, output, library
API, troubleshooting pointers) is built into the app — run `pnpm dev` and open the
**Docs** view.
