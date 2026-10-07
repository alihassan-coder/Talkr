import { getCurrentWebview } from '@tauri-apps/api/webview'
import { isTauri } from '@/lib/api'

/** Interface zoom steps, as scale factors. index.html applies the saved one before first paint. */
export const ZOOM_LEVELS = [0.8, 0.9, 1, 1.1, 1.2, 1.3, 1.4, 1.5] as const

export const DEFAULT_ZOOM = 1

/** The nearest step to a saved value; 100% for anything that is not a number. */
export function normalizeZoom(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return DEFAULT_ZOOM
  return ZOOM_LEVELS.reduce<number>((best, level) => (Math.abs(level - value) < Math.abs(best - value) ? level : best), DEFAULT_ZOOM)
}

/** One step in or out from `current`, stopping at the ends. */
export function stepZoom(current: number, direction: 1 | -1): number {
  const index = ZOOM_LEVELS.indexOf(normalizeZoom(current) as (typeof ZOOM_LEVELS)[number])
  return ZOOM_LEVELS[Math.min(ZOOM_LEVELS.length - 1, Math.max(0, index + direction))] ?? DEFAULT_ZOOM
}

export const formatZoom = (level: number) => `${Math.round(level * 100)}%`

/** CSS zoom on <html>: the browser preview, and the fallback if the webview refuses. */
const setCssZoom = (level: number) => {
  document.documentElement.style.zoom = level === DEFAULT_ZOOM ? '' : String(level)
}

/**
 * Scales the whole interface. In the app this is the webview's own zoom (like a
 * browser's Ctrl +), so text stays crisp and layout reflows; the browser preview
 * uses CSS zoom.
 */
export async function applyZoom(level: number) {
  if (!isTauri()) return setCssZoom(level)
  try {
    await getCurrentWebview().setZoom(level)
    setCssZoom(DEFAULT_ZOOM)
  } catch {
    setCssZoom(level)
  }
}

/**
 * Ctrl/Cmd with + or =, - and 0. Shift is allowed since + is Shift+= on many
 * layouts; the numpad keys count too. Returns the step, 0 for reset, or null.
 */
export function zoomKey(e: Pick<KeyboardEvent, 'key' | 'code' | 'ctrlKey' | 'metaKey' | 'altKey'>): 1 | -1 | 0 | null {
  if (!(e.ctrlKey || e.metaKey) || e.altKey) return null
  if (e.key === '+' || e.key === '=' || e.code === 'NumpadAdd') return 1
  if (e.key === '-' || e.key === '_' || e.code === 'NumpadSubtract') return -1
  if (e.key === '0' || e.code === 'Numpad0') return 0
  return null
}
