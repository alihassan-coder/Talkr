import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { Pill } from '@/overlay/Pill'
import '@/overlay/overlay.css'

// The dictation pill's own window: transparent, click-through, never focused. See
// src-tauri/src/dictation/overlay.rs.
createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Pill />
  </StrictMode>,
)

// Follow theme changes made in the main window (same origin, so the same storage).
window.addEventListener('storage', (e) => {
  if (e.key === 'talkr.ui') window.location.reload()
})
