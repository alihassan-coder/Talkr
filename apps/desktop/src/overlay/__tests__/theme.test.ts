import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { followTheme, THEME_RETRY_MS, THEME_SETTLE_MS, themeKey } from '@/overlay/theme'

const ui = (state: Record<string, unknown>) => JSON.stringify({ state, version: 0 })

describe('the pill follows the theme', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it('compares only what changes its look', () => {
    const base = { mode: 'dark', theme: 'violet', customAccent: '#ff0000', zoom: 1, sidebarCollapsed: false }
    expect(themeKey(ui(base))).toBe(themeKey(ui({ ...base, zoom: 1.2, sidebarCollapsed: true })))
    // The custom accent matters only while the custom theme is on.
    expect(themeKey(ui(base))).toBe(themeKey(ui({ ...base, customAccent: '#00ff00' })))
    expect(themeKey(ui({ ...base, theme: 'custom' }))).not.toBe(themeKey(ui({ ...base, theme: 'custom', customAccent: '#00ff00' })))
    expect(themeKey(ui(base))).not.toBe(themeKey(ui({ ...base, mode: 'light' })))
    expect(themeKey('not json')).toBe('unreadable')
    expect(themeKey(null)).toBe(themeKey(ui({})))
  })

  it('reloads once after a burst of changes, and not for other preferences', () => {
    let stored = ui({ mode: 'dark', theme: 'custom', customAccent: '#000000', zoom: 1 })
    const reload = vi.fn()
    const t = followTheme({ read: () => stored, busy: () => false, reload })
    stored = ui({ mode: 'dark', theme: 'custom', customAccent: '#000000', zoom: 1.1 })
    t.changed()
    vi.advanceTimersByTime(THEME_SETTLE_MS)
    expect(reload).not.toHaveBeenCalled()
    // Dragging the accent: many writes, one reload when it settles.
    for (let i = 1; i <= 20; i++) {
      stored = ui({ mode: 'dark', theme: 'custom', customAccent: `#0000${String(i).padStart(2, '0')}` })
      t.changed()
      vi.advanceTimersByTime(50)
    }
    expect(reload).not.toHaveBeenCalled()
    vi.advanceTimersByTime(THEME_SETTLE_MS)
    expect(reload).toHaveBeenCalledTimes(1)
  })

  it('waits for the pill to go before reloading', () => {
    let stored = ui({ mode: 'dark', theme: 'violet' })
    let up = true
    const reload = vi.fn()
    const t = followTheme({ read: () => stored, busy: () => up, reload })
    stored = ui({ mode: 'light', theme: 'violet' })
    t.changed()
    vi.advanceTimersByTime(THEME_SETTLE_MS + THEME_RETRY_MS * 3)
    expect(reload).not.toHaveBeenCalled()
    up = false
    vi.advanceTimersByTime(THEME_RETRY_MS)
    expect(reload).toHaveBeenCalledTimes(1)
    t.dispose()
  })
})
