import { useEffect, useSyncExternalStore } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { isTauri } from '@/lib/api'
import { useUi } from '@/stores/ui'

const DARK_QUERY = '(prefers-color-scheme: dark)'

const subscribeSystem = (onChange: () => void) => {
  const media = window.matchMedia(DARK_QUERY)
  media.addEventListener('change', onChange)
  return () => media.removeEventListener('change', onChange)
}

const systemIsDark = () => window.matchMedia(DARK_QUERY).matches

/** The mode actually on screen: `system` resolved against the OS setting, live. */
export function useResolvedMode(): 'light' | 'dark' {
  const mode = useUi((s) => s.mode)
  const dark = useSyncExternalStore(subscribeSystem, systemIsDark, () => true)
  if (mode !== 'system') return mode
  return dark ? 'dark' : 'light'
}

let fadeTimer: ReturnType<typeof setTimeout> | undefined

/**
 * Writes `data-theme` / `data-mode` on <html> (globals.css maps them to the two
 * colours) and keeps the native title bar in step. index.html sets the same
 * attributes before first paint, so the first run here is usually a no-op.
 */
export function useApplyAppearance() {
  const mode = useUi((s) => s.mode)
  const theme = useUi((s) => s.theme)
  const resolved = useResolvedMode()

  useEffect(() => {
    const root = document.documentElement
    if (root.dataset.theme === theme && root.dataset.mode === resolved) return
    // A short colour cross-fade, only while switching, so normal interactions stay snappy.
    if (root.dataset.theme) {
      root.classList.add('theme-fade')
      clearTimeout(fadeTimer)
      fadeTimer = setTimeout(() => root.classList.remove('theme-fade'), 320)
    }
    root.dataset.theme = theme
    root.dataset.mode = resolved
  }, [theme, resolved])

  useEffect(() => {
    if (!isTauri()) return
    // null hands the title bar back to the OS, which is what `system` means.
    getCurrentWindow()
      .setTheme(mode === 'system' ? null : mode)
      .catch(() => {})
  }, [mode])
}
