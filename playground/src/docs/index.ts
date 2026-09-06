import introduction from './pages/introduction.md?raw'
import gettingStarted from './pages/getting-started.md?raw'
import enginesAndInput from './pages/engines-and-input.md?raw'
import scopeFilters from './pages/scope-filters-rate-limiting.md?raw'
import headlessRendering from './pages/headless-rendering.md?raw'
import jsFormsAuth from './pages/js-forms-auth.md?raw'
import output from './pages/output.md?raw'
import libraryApi from './pages/library-api.md?raw'
import wasmCore from './pages/wasm-core.md?raw'

export interface DocPage {
  id: string
  title: string
  body: string
}

export const DOC_PAGES: DocPage[] = [
  { id: 'introduction', title: 'Introduction', body: introduction },
  { id: 'getting-started', title: 'Getting started', body: gettingStarted },
  { id: 'engines-and-input', title: 'Engines & input', body: enginesAndInput },
  { id: 'scope-filters', title: 'Scope, filters & rate limits', body: scopeFilters },
  { id: 'headless-rendering', title: 'Headless rendering & stealth', body: headlessRendering },
  { id: 'js-forms-auth', title: 'JS crawling, forms & auth', body: jsFormsAuth },
  { id: 'output', title: 'Output & diagnostics', body: output },
  { id: 'library-api', title: 'Library API', body: libraryApi },
  { id: 'wasm-core', title: 'WASM core & playground', body: wasmCore },
]
