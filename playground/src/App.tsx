import { useEffect, useMemo, useState } from 'react'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import init, {
  extract_all_links,
  extract_forms,
  extract_js_endpoints,
  transform_to_ir,
  version,
} from './wasm'

interface PageIR {
  url: string
  title: string
  markdown_ir: string
}

interface Form {
  method: string
  action: string
  enctype: string
  parameters: string[]
}

interface Analysis {
  pageIr: PageIR
  links: { url: string; internal: boolean }[]
  forms: Form[]
  endpoints: string[]
}

type SourceMode = 'url' | 'html'
type ResultTab = 'markdown' | 'links' | 'forms' | 'endpoints' | 'json'

const HOME = 'https://example.com'

function analyze(base: string, html: string): Analysis {
  const pageIr = transform_to_ir(base, html) as PageIR
  const baseHost = safeHost(base)
  const links = extract_all_links(base, html)
    .filter((link, i, all) => all.indexOf(link) === i)
    .map((url) => ({ url, internal: safeHost(url) === baseHost }))
  const forms = extract_forms(html) as Form[]
  const endpoints = [...new Set(extract_js_endpoints(html))]
  return { pageIr, links, forms, endpoints }
}

function safeHost(url: string): string {
  try {
    return new URL(url).host
  } catch {
    return ''
  }
}

function StatusBadge({ state }: { state: 'loading' | 'ready' | 'error' }) {
  const [label, cls] =
    state === 'ready'
      ? [`wasm v${version()} loaded`, 'ok']
      : state === 'loading'
        ? ['loading wasm…', 'wait']
        : ['wasm failed to load', 'bad']
  return <span className={`badge ${cls}`}>{label}</span>
}

export default function App() {
  const [wasmState, setWasmState] = useState<'loading' | 'ready' | 'error'>('loading')
  const [mode, setMode] = useState<SourceMode>('url')
  const [url, setUrl] = useState(HOME)
  const [html, setHtml] = useState('')
  const [base, setBase] = useState(HOME)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [analysis, setAnalysis] = useState<Analysis | null>(null)
  const [tab, setTab] = useState<ResultTab>('markdown')

  useEffect(() => {
    init()
      .then(() => setWasmState('ready'))
      .catch((err) => {
        console.error(err)
        setWasmState('error')
      })
  }, [])

  const renderedMarkdown = useMemo(() => {
    if (!analysis || tab !== 'markdown') return ''
    return DOMPurify.sanitize(marked.parse(analysis.pageIr.markdown_ir) as string)
  }, [analysis, tab])

  async function run() {
    setError(null)
    setBusy(true)
    try {
      if (mode === 'url') {
        const resp = await fetch(`/api/fetch?url=${encodeURIComponent(url)}`)
        const payload = await resp.json()
        if (!resp.ok) throw new Error(payload.error ?? `proxy returned ${resp.status}`)
        if (payload.status >= 400) {
          throw new Error(`site returned HTTP ${payload.status} for ${payload.finalUrl ?? url}`)
        }
        setAnalysis(analyze(payload.finalUrl ?? url, payload.body))
        setUrl(payload.finalUrl ?? url)
      } else {
        if (!html.trim()) throw new Error('paste some HTML first')
        setAnalysis(analyze(base, html))
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
      setAnalysis(null)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="app">
      <header className="header">
        <div>
          <h1>Celestia Playground</h1>
          <p className="subtitle">
            browser-crawler compiled to WebAssembly — turn pages into markdown IR, links, forms,
            and JS endpoints, entirely in your browser.
          </p>
        </div>
        <StatusBadge state={wasmState} />
      </header>

      <section className="card">
        <div className="segmented" role="tablist" aria-label="input source">
          <button
            className={mode === 'url' ? 'active' : ''}
            onClick={() => setMode('url')}
            disabled={busy}
          >
            Fetch URL
          </button>
          <button
            className={mode === 'html' ? 'active' : ''}
            onClick={() => setMode('html')}
            disabled={busy}
          >
            Paste HTML
          </button>
        </div>

        {mode === 'url' ? (
          <form
            className="url-row"
            onSubmit={(e) => {
              e.preventDefault()
              void run()
            }}
          >
            <input
              type="url"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://example.com"
              aria-label="page URL"
            />
            <button type="submit" className="primary" disabled={busy || wasmState !== 'ready'}>
              {busy ? 'Fetching…' : 'Fetch & analyze'}
            </button>
          </form>
        ) : (
          <div className="html-row">
            <input
              type="url"
              value={base}
              onChange={(e) => setBase(e.target.value)}
              placeholder="Base URL for resolving links"
              aria-label="base URL"
            />
            <textarea
              value={html}
              onChange={(e) => setHtml(e.target.value)}
              placeholder="<html>… paste page source here …</html>"
              rows={6}
              aria-label="HTML source"
            />
            <div>
              <button
                type="button"
                className="primary"
                onClick={() => void run()}
                disabled={busy || wasmState !== 'ready'}
              >
                {busy ? 'Analyzing…' : 'Analyze HTML'}
              </button>
            </div>
          </div>
        )}
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
      </section>

      {analysis && (
        <section className="card results">
          <div className="summary">
            <h2 title={analysis.pageIr.url}>{analysis.pageIr.title || '(untitled page)'}</h2>
            <div className="chips">
              <button
                className={`chip ${tab === 'links' ? 'active' : ''}`}
                onClick={() => setTab('links')}
              >
                {analysis.links.length} links
              </button>
              <button
                className={`chip ${tab === 'forms' ? 'active' : ''}`}
                onClick={() => setTab('forms')}
              >
                {analysis.forms.length} forms
              </button>
              <button
                className={`chip ${tab === 'endpoints' ? 'active' : ''}`}
                onClick={() => setTab('endpoints')}
              >
                {analysis.endpoints.length} endpoints
              </button>
            </div>
          </div>

          <nav className="tabs" role="tablist">
            {(
              [
                ['markdown', 'Markdown IR'],
                ['links', 'Links'],
                ['forms', 'Forms'],
                ['endpoints', 'JS endpoints'],
                ['json', 'Raw JSON'],
              ] as [ResultTab, string][]
            ).map(([key, label]) => (
              <button
                key={key}
                role="tab"
                aria-selected={tab === key}
                className={tab === key ? 'active' : ''}
                onClick={() => setTab(key)}
              >
                {label}
              </button>
            ))}
          </nav>

          <div className="panel">
            {tab === 'markdown' && (
              <div className="markdown" dangerouslySetInnerHTML={{ __html: renderedMarkdown }} />
            )}

            {tab === 'links' && (
              <table className="data">
                <thead>
                  <tr>
                    <th>#</th>
                    <th>URL</th>
                    <th>Scope</th>
                  </tr>
                </thead>
                <tbody>
                  {analysis.links.map((link, i) => (
                    <tr key={link.url + i}>
                      <td className="num">{i + 1}</td>
                      <td className="mono wrap">{link.url}</td>
                      <td>
                        <span className={`tag ${link.internal ? 'in' : 'out'}`}>
                          {link.internal ? 'internal' : 'external'}
                        </span>
                      </td>
                    </tr>
                  ))}
                  {analysis.links.length === 0 && (
                    <tr>
                      <td colSpan={3} className="empty">
                        No links found.
                      </td>
                    </tr>
                  )}
                </tbody>
              </table>
            )}

            {tab === 'forms' && (
              <div className="forms">
                {analysis.forms.map((form, i) => (
                  <article key={i} className="form-card">
                    <header>
                      <span className="tag in">{(form.method || 'GET').toUpperCase()}</span>
                      <code className="wrap">{form.action || '(no action)'}</code>
                      {form.enctype && form.enctype !== 'application/x-www-form-urlencoded' && (
                        <span className="dim mono">{form.enctype}</span>
                      )}
                    </header>
                    <ul>
                      {form.parameters.map((p) => (
                        <li key={p}>
                          <code>{p}</code>
                        </li>
                      ))}
                      {form.parameters.length === 0 && <li className="dim">no parameters</li>}
                    </ul>
                  </article>
                ))}
                {analysis.forms.length === 0 && <p className="empty">No forms found.</p>}
              </div>
            )}

            {tab === 'endpoints' && (
              <ul className="endpoint-list">
                {analysis.endpoints.map((ep, i) => (
                  <li key={i} className="mono">
                    {ep}
                  </li>
                ))}
                {analysis.endpoints.length === 0 && (
                  <li className="empty">No endpoints extracted from the page source.</li>
                )}
              </ul>
            )}

            {tab === 'json' && <pre className="json">{JSON.stringify(analysis.pageIr, null, 2)}</pre>}
          </div>
        </section>
      )}

      <footer className="footer">
        Fetching goes through a dev-only proxy (<code>/api/fetch</code>) to bypass CORS; everything
        else — parsing, IR generation, link &amp; form extraction — runs in WebAssembly locally.
      </footer>
    </div>
  )
}
