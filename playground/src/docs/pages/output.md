## Screen output

Default format is one URL per line; `-v`/`--verbose` decorates results as `[tag] [method] url [depth:n]`. `--silent` prints results only; `--no-color` strips ANSI; `--debug` logs retries and render internals.

## JSONL output

`--jsonl` / `-j` writes one JSON object per result (`request`, `response`, `timestamp`) to `--output` / `-o` (screen output continues alongside). Related controls:

| Flag | Description |
|------|-------------|
| `--omit-raw` | Omit raw request/response dumps. |
| `--omit-body` | Omit response body. |
| `--exclude-output-fields` | Remove fields from JSONL output (supports dotted paths like `response.body`). |
| `--no-clobber` | Never overwrite `--output` files (`out.txt` → `out-1.txt`). |
| `--output-template` | Custom template with `{{.request.endpoint}}`-style JSON-path tokens. |

## Field selectors

`--list-output-fields` prints the 14 field names:

`url`, `path`, `fqdn`, `rdn`, `rurl`, `qurl`, `qpath`, `file`, `ufile`, `key`, `value`, `kv`, `dir`, `udir`

| Flag | Description |
|------|-------------|
| `--fields` / `-f` | Comma-separated field list for output (`url,fqdn,qurl,…`). |
| `--store-field` | Per-host field storage for the given field. |
| `--store-field-dir` | Directory for stored fields. |

## Raw response storage

| Flag | Description |
|------|-------------|
| `--store-response` | Save raw request/response per host: `<dir>/<host>/<hash>.txt` + `index.txt`. |
| `--store-response-dir` | Custom directory (default `output`). |

## Errors & diagnostics

| Flag | Description |
|------|-------------|
| `--error-log` | File logging every failed request. |
| `--health-check` | Self-check: version, DNS resolution, Chrome availability, outbound TCP. |

## AI IR exporters (library)

The `Browser::builder()` pipeline exports each `PageIR` through pluggable storage exporters — JSON files, markdown, or any custom `StorageExporter` — with per-page `on_page` callbacks for analysis, LLM prompting, or custom sinks. See the **Library API** page for the builder in action.
