import { useEffect, useMemo, useSyncExternalStore } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { isTauri } from '@/lib/api'
import { CUSTOM_THEME, resolvePalette, type Palette } from '@/lib/themes'
import { applyZoom, DEFAULT_ZOOM, stepZoom, zoomKey } from '@/lib/zoom'
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

/** Token name per palette field, as globals.css names them. */
const TOKENS: Record<keyof Palette, string> = {
  bg: '--color-bg',
  surface: '--color-surface',
  fg: '--color-fg',
  accent: '--color-accent',
  onAccent: '--color-on-accent',
}

/**
 * The custom theme has no rules in globals.css: its five base colours go inline
 * on <html>, which beats the per-theme rules. null clears them.
 */
export function setInlinePalette(palette: Palette | null) {
  const style = document.documentElement.style
  for (const [key, token] of Object.entries(TOKENS) as [keyof Palette, string][]) {
    if (palette) style.setProperty(token, palette[key])
    else style.removeProperty(token)
  }
}

/**
 * Writes `data-theme` / `data-mode` on <html> (globals.css maps them to the
 * colour tokens), plus the inline tokens of a custom theme, and keeps the
 * native title bar and window background in step. index.html sets the same
 * before first paint, so the first run here is usually a no-op.
 */
export function useApplyAppearance() {
  const mode = useUi((s) => s.mode)
  const theme = useUi((s) => s.theme)
  const customAccent = useUi((s) => s.customAccent)
  const resolved = useResolvedMode()
  const palette = useMemo(() => resolvePalette(theme, customAccent, resolved), [theme, customAccent, resolved])
  const custom = theme === CUSTOM_THEME

  useEffect(() => {
    const root = document.documentElement
    // Every frame while dragging the colour picker: no cross-fade, just the new colours.
    setInlinePalette(custom ? palette : null)
    if (root.dataset.theme === theme && root.dataset.mode === resolved) return
    // A short colour cross-fade, only while switching, so normal interactions stay snappy.
    if (root.dataset.theme) {
      root.classList.add('theme-fade')
      clearTimeout(fadeTimer)
      fadeTimer = setTimeout(() => root.classList.remove('theme-fade'), 320)
    }
    root.dataset.theme = theme
    root.dataset.mode = resolved
  }, [theme, resolved, custom, palette])

  useEffect(() => {
    if (!isTauri()) return
    // The native window and webview show this colour while resizing or loading, before
    // the page paints. tauri.conf.json keeps Graphite dark as the pre-JS default.
    getCurrentWebviewWindow()
      .setBackgroundColor(palette.bg)
      .catch(() => {})
  }, [palette.bg])

  useEffect(() => {
    if (!isTauri()) return
    // null hands the title bar back to the OS, which is what `system` means.
    getCurrentWindow()
      .setTheme(mode === 'system' ? null : mode)
      .catch(() => {})
  }, [mode])
}

/** Wheel distance per zoom step: one mouse-wheel notch, or a deliberate trackpad pinch. */
const WHEEL_STEP = 50

/**
 * Applies the saved zoom and keeps it in step, and handles the zoom shortcuts:
 * Ctrl/Cmd + / - / 0 and Ctrl + mouse wheel (also what a trackpad pinch sends).
 * They work everywhere, text fields included, as they do in a browser.
 */
export function useZoom() {
  const zoom = useUi((s) => s.zoom)

  useEffect(() => {
    void applyZoom(zoom)
  }, [zoom])

  useEffect(() => {
    const { setZoom } = useUi.getState()
    const onKey = (e: KeyboardEvent) => {
      const step = zoomKey(e)
      if (step === null) return
      e.preventDefault()
      setZoom(step === 0 ? DEFAULT_ZOOM : stepZoom(useUi.getState().zoom, step))
    }
    let wheel = 0
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return
      e.preventDefault()
      // Line-based deltas (Firefox, some mice) are about 33px a line.
      wheel += e.deltaMode === 1 ? e.deltaY * 33 : e.deltaY
      if (Math.abs(wheel) < WHEEL_STEP) return
      setZoom(stepZoom(useUi.getState().zoom, wheel < 0 ? 1 : -1))
      wheel = 0
    }
    window.addEventListener('keydown', onKey)
    window.addEventListener('wheel', onWheel, { passive: false })
    return () => {
      window.removeEventListener('keydown', onKey)
      window.removeEventListener('wheel', onWheel)
    }
  }, [])
}
