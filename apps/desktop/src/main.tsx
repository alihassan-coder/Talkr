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

// Fade out the launch screen from index.html once the first frame is on screen. It stays up
// for at least a beat so it reads as a logo, not a flicker.
const splash = document.getElementById('splash')
if (splash) {
  const shownFor = performance.now()
  setTimeout(
    () =>
      requestAnimationFrame(() => {
        splash.classList.add('out')
        splash.addEventListener('transitionend', () => splash.remove(), { once: true })
        // transitionend can be skipped (hidden window, no animation); index.html's launch
        // background only lets go of <html> once the splash node is gone.
        setTimeout(() => splash.remove(), 600)
      }),
    Math.max(0, 450 - shownFor),
  )
}
