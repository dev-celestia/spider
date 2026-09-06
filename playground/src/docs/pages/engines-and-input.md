## Input

| Flag | Description |
|------|-------------|
| `--list` / `-u` | Target URL(s). Repeatable; accepts a file path (one URL per line). |
| stdin | Pipe URLs directly: `cat urls.txt | celestia-browser -d 2`. |
| `--exclude` | Exclude hosts matching a filter (`cdn`, `private-ips`, CIDR, ip, regex). |

## Core configuration

| Flag | Default | Description |
|------|---------|-------------|
| `--depth` / `-d` | `3` | Maximum crawl depth. |
| `--crawl-duration` | `0` | Max duration (`30s`, `5m`, `1h`, `2d`). Stops the crawl when elapsed. |
| `--strategy` | `depth-first` | Queue strategy: `depth-first` (LIFO) or `breadth-first` (FIFO). |
| `--timeout` | `10` | Per-request timeout (seconds). |
| `--retry` | `1` | Retries per failed request. |
| `--max-response-size` | `4194304` | Max response body bytes to read. |
| `--time-stable` | `1` | Seconds to wait for page stability (headless). |
| `--disable-redirects` | off | Do not follow HTTP redirects. |
| `--proxy` | — | HTTP/SOCKS5 proxy URL. |
| `--headers` / `-H` | — | Custom headers (`Key: Value`); repeatable or a file path. Sent on every request. |
| `--ignore-query-params` | off | Treat `/page?a=1` and `/page?a=2` as the same URL. |
| `--max-domain-pages` | `0` | Cap pages crawled per domain (0 = unlimited). |
| `--path-climb` | off | Also crawl parent paths of discovered URLs (`/a/b/c` → `/a/`, `/a/b/`). |
| `--config` | — | Celestia configuration file (accepted for compatibility). |

## The crawl loop

The engine runs a 4-phase interleaved queue:

1. Push the landing page task `(url, depth=0)` into the navigation queue.
2. While the queue is not empty, pop the next task.
3. Fetch HTML — static HTTP or dynamic headless Chrome.
4. Extract the `PageIR` (title, noise pruning, Markdown IR, links).
5. Execute the content-analysis callback (`on_page` / LLM prompting) and export the payload.
6. Scan the HTML for same-domain links; for each unvisited link within depth limits, mark visited and push `(link, depth+1)`.

At least one of `--depth` / `--crawl-duration` must be set — this is validated up front.
