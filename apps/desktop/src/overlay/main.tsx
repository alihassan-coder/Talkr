import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { Pill } from '@/overlay/Pill'
import { isPillUp } from '@/overlay/state'
import { followTheme } from '@/overlay/theme'
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

// Follow theme changes made in the main window (same origin, so the same storage). The key is
// stores/ui.ts's UI_STORAGE_KEY, not imported: that would set up the whole store here.
const UI_STORAGE_KEY = 'talkr.ui'
const theme = followTheme({
  read: () => {
    try {
      return localStorage.getItem(UI_STORAGE_KEY)
    } catch {
      return null
    }
  },
  busy: isPillUp,
  reload: () => window.location.reload(),
})
window.addEventListener('storage', (e) => {
  if (e.key === UI_STORAGE_KEY) theme.changed()
})
