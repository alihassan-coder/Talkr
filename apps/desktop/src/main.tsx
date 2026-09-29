import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { HashRouter } from 'react-router'
import { App } from '@/App'
import '@/styles/globals.css'

// Hash routing: the bundled app is served from a custom protocol, so there is no server to rewrite deep links.
createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <HashRouter>
      <App />
    </HashRouter>
  </StrictMode>,
)
