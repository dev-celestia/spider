import react from '@vitejs/plugin-react'
import { defineConfig, type Plugin } from 'vite'

// Dev-only middleware that fetches pages server-side so the browser UI can
// analyze sites that don't send permissive CORS headers. Production builds of
// the playground should reverse-proxy `/api/fetch` themselves.
function devFetchProxy(): Plugin {
  return {
    name: 'dev-fetch-proxy',
    configureServer(server) {
      server.middlewares.use('/api/fetch', async (req, res) => {
        const parsed = new URL(req.url ?? '/', 'http://localhost')
        const target = parsed.searchParams.get('url')
        const send = (status: number, payload: unknown) => {
          res.statusCode = status
          res.setHeader('content-type', 'application/json; charset=utf-8')
          res.end(JSON.stringify(payload))
        }
        if (!target || !/^https?:\/\//i.test(target)) {
          send(400, { error: 'expected ?url=<http(s) URL>' })
          return
        }
        try {
          const upstream = await fetch(target, {
            headers: {
              'user-agent':
                'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36',
              accept: 'text/html,application/xhtml+xml',
            },
            redirect: 'follow',
            signal: AbortSignal.timeout(20_000),
          })
          const body = await upstream.text()
          send(200, {
            status: upstream.status,
            finalUrl: upstream.url,
            contentType: upstream.headers.get('content-type') ?? '',
            body,
          })
        } catch (err) {
          send(502, { error: err instanceof Error ? err.message : String(err) })
        }
      })
    },
  }
}

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), devFetchProxy()],
})
