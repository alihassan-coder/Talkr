import { CUSTOM_THEME } from '@/lib/themes'

/**
 * What of the saved UI preferences (`talkr.ui`) the pill's look depends on, as one comparable
 * string. Other writes to the same key (zoom, the sidebar, export formats) must not reload it.
 */
export function themeKey(raw: string | null): string {
  try {
    const parsed = (raw ? JSON.parse(raw) : null) as { state?: Record<string, unknown> } | null
    const s = parsed?.state ?? {}
    return JSON.stringify([s.mode ?? null, s.theme ?? null, s.theme === CUSTOM_THEME ? (s.customAccent ?? null) : null])
  } catch {
    return 'unreadable'
  }
}

/** How long the theme must stay put before the pill reloads (dragging the custom accent writes many times a second). */
export const THEME_SETTLE_MS = 400
/** How often a reload held back by a visible pill checks again. */
export const THEME_RETRY_MS = 1000

/**
 * Reload the pill's page when the theme changes in the main window: debounced, only for an
 * actual change of look, and never while the pill is up (a reload mid-dictation would leave an
 * empty, clickable window), so it waits for the pill to go.
 */
export function followTheme({
  read,
  busy,
  reload,
}: {
  /** The current `talkr.ui` value. */
  read: () => string | null
  /** The pill shows something. */
  busy: () => boolean
  reload: () => void
}): { changed: () => void; dispose: () => void } {
  const loaded = themeKey(read())
  let timer: ReturnType<typeof setTimeout> | undefined
  const check = () => {
    timer = undefined
    if (themeKey(read()) === loaded) return
    if (busy()) {
      timer = setTimeout(check, THEME_RETRY_MS)
      return
    }
    reload()
  }
  return {
    changed: () => {
      if (timer !== undefined) clearTimeout(timer)
      timer = setTimeout(check, THEME_SETTLE_MS)
    },
    dispose: () => {
      if (timer !== undefined) clearTimeout(timer)
    },
  }
}
