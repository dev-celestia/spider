## Host scope

Before any regex filtering applies, URLs are scope-checked against the seed host:

| Flag | Description |
|------|-------------|
| `--field-scope` | `dn` (contains root domain), `rdn` (registrable domain + subdomains — default), `fqdn` (exact host), or a custom regex. Resolved via the Public Suffix List. |
| `--no-scope` | Disable host-based default scope. |
| `--display-out-scope` | Log external URLs skipped by scope. |

## Scope regexes

| Flag | Description |
|------|-------------|
| `--crawl-scope` / `-cs` | Regex list; only matching URLs are followed. |
| `--crawl-out-scope` | Regex list; matching URLs are excluded from following. |

## URL filters

| Flag | Description |
|------|-------------|
| `--match-regex` | Regex list — output/crawl only matching URLs. |
| `--filter-regex` / `-fr` | Regex list — drop matching URLs. |
| `--extension-match` / `-em` | Only URLs with these extensions (`php,html,js,none`; `none` = no extension). |
| `--extension-filter` / `-ef` | Ignore URLs with these extensions. |
| `--no-default-ext-filter` | Keep the default denylist (images, archives, …) unfiltered. |
| `--match-condition` | DSL expression — emit only results where truthy. |
| `--filter-condition` | DSL expression — drop results where truthy. |
| `--filter-page-type` | Drop responses by heuristic type: `error`, `captcha`, `parked`. |

DSL conditions compare response fields, e.g. `--match-condition "status_code == 200"`.

## Deduplication

| Flag | Description |
|------|-------------|
| *(default)* | Exact-content dedup — MD5 of the body. Disable with `--disable-unique-filter`. |
| `--filter-similar` | Collapse similar-looking URLs by path-trie normalization (`/users/123` ≈ `/users/456`). |
| `--filter-similar-threshold` | Distinct values before a path position becomes a variable (default `10`). |
| `--page-content-similar` | Near-duplicate page filtering (SimHash). `--similarity-deduplication` is an alias. |
| `--page-content-similar-mode` | `simhash` (default), `tfidf`, `bm25`. |
| `--page-content-similar-distance` | SimHash max Hamming distance (default `3`). |
| `--page-content-similar-threshold` | TF-IDF/BM25 min score 0–1 (default `0.85`). |
| `--page-content-similar-budget` | Pages fully processed per similarity cluster (default `1`). |

## Rate limiting & concurrency

| Flag | Default | Description |
|------|---------|-------------|
| `--concurrency` / `-c` | `10` | Concurrent fetcher workers. |
| `--parallelism` / `-p` | `10` | Concurrent input URLs processed. |
| `--delay` | `0` | Fixed delay between requests (seconds). |
| `--rate-limit` / `-rl` | `150` | Max requests/second (global token bucket). |
| `--rate-limit-minute` | `0` | Max requests/minute (global). |
| `--host-rate-limit` | `0` | Max requests/second per host. |
| `--host-rate-limit-minute` | `0` | Max requests/minute per host. |
