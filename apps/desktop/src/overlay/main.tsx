import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { Pill } from '@/overlay/Pill'
import { isTauri } from '@/lib/api'
import '@/overlay/overlay.css'

// The dictation pill's own window: transparent, click-through, never focused. See
// src-tauri/src/dictation/overlay.rs.
const root = createRoot(document.getElementById('root')!)

if (isTauri() || !import.meta.env.DEV) {
  root.render(
    <StrictMode>
      <Pill />
    </StrictMode>,
  )
} else {
  // In a plain browser during development, /overlay.html?state=listening shows a sample state.
  void import('@/overlay/Demo').then(({ Demo }) =>
    root.render(
      <StrictMode>
        <Demo />
      </StrictMode>,
    ),
  )
}

// Follow theme changes made in the main window (same origin, so the same storage).
window.addEventListener('storage', (e) => {
  if (e.key === 'talkr.ui') window.location.reload()
})
