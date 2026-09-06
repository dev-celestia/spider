import { useEffect, useMemo, useRef, useState } from 'react'
import { marked } from 'marked'
import DOMPurify from 'dompurify'
import { DOC_PAGES } from './docs'

interface TocEntry {
  id: string
  text: string
  level: 2 | 3
}

function slugify(text: string): string {
  return text
    .toLowerCase()
    .trim()
    .replace(/[^\w\s-]/g, '')
    .replace(/\s+/g, '-')
}

export default function Docs() {
  const [pageId, setPageId] = useState(DOC_PAGES[0].id)
  const [activeHeading, setActiveHeading] = useState('')
  const [toc, setToc] = useState<TocEntry[]>([])
  const bodyRef = useRef<HTMLDivElement>(null)

  const page = DOC_PAGES.find((p) => p.id === pageId) ?? DOC_PAGES[0]

  // Memoize the prop object itself: a fresh { __html } each render makes React
  // re-set innerHTML, which would wipe the heading ids assigned below.
  const html = useMemo(
    () => ({ __html: DOMPurify.sanitize(marked.parse(page.body) as string) }),
    [page],
  )

  // Assign ids to h2/h3 in the rendered markdown and derive the
  // "on this page" table of contents from them.
  useEffect(() => {
    const root = bodyRef.current
    if (!root) return
    const entries: TocEntry[] = []
    const used = new Set<string>()
    root.querySelectorAll('h2, h3').forEach((h) => {
      let id = slugify(h.textContent ?? '')
      let n = 2
      while (used.has(id)) id = `${slugify(h.textContent ?? '')}-${n++}`
      used.add(id)
      h.id = id
      entries.push({
        id,
        text: h.textContent ?? '',
        level: h.tagName === 'H2' ? 2 : 3,
      })
    })
    setToc(entries)
  }, [html])

  // Scroll spy: highlight the heading closest above the viewport top.
  useEffect(() => {
    const onScroll = () => {
      let current = toc[0]?.id ?? ''
      for (const entry of toc) {
        const el = document.getElementById(entry.id)
        if (el && el.getBoundingClientRect().top <= 96) current = entry.id
      }
      setActiveHeading(current)
    }
    window.addEventListener('scroll', onScroll, { passive: true })
    onScroll()
    return () => window.removeEventListener('scroll', onScroll)
  }, [toc])

  function selectPage(id: string) {
    setPageId(id)
    setActiveHeading('')
    window.scrollTo({ top: 0 })
  }

  function jumpTo(id: string) {
    document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  }

  return (
    <div className="docs">
      <aside className="docs-sidebar">
        <p className="docs-heading">Documentation</p>
        <nav aria-label="Documentation pages">
          <ul className="docs-menu">
            {DOC_PAGES.map((p) => (
              <li key={p.id}>
                <button
                  className={p.id === pageId ? 'active' : ''}
                  aria-current={p.id === pageId ? 'page' : undefined}
                  onClick={() => selectPage(p.id)}
                >
                  {p.title}
                </button>
              </li>
            ))}
          </ul>
        </nav>
      </aside>

      <article className="docs-content">
        <div className="markdown" ref={bodyRef} dangerouslySetInnerHTML={html} />
      </article>

      {toc.length > 0 && (
        <aside className="docs-toc">
          <p className="docs-heading">On this page</p>
          <nav aria-label="On this page">
            <ul>
              {toc.map((entry) => (
                <li key={entry.id}>
                  <a
                    href={`#${entry.id}`}
                    className={`${entry.level === 3 ? 'l3' : ''} ${entry.id === activeHeading ? 'active' : ''}`}
                    onClick={(e) => {
                      e.preventDefault()
                      jumpTo(entry.id)
                    }}
                  >
                    {entry.text}
                  </a>
                </li>
              ))}
            </ul>
          </nav>
        </aside>
      )}
    </div>
  )
}
