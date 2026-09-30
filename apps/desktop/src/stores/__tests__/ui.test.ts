import { afterEach, describe, expect, it } from 'vitest'
import { UI_STORAGE_KEY, useUi } from '@/stores/ui'

const saved = () => JSON.parse(localStorage.getItem(UI_STORAGE_KEY) ?? 'null') as { state: Record<string, unknown> } | null

const rehydrateFrom = async (state: unknown) => {
  localStorage.setItem(UI_STORAGE_KEY, JSON.stringify({ state, version: 0 }))
  await useUi.persist.rehydrate()
}

afterEach(() => {
  useUi.setState({ sidebarCollapsed: false, mode: 'system', theme: 'graphite' })
})

describe('ui store', () => {
  it('starts with the defaults', () => {
    const s = useUi.getState()
    expect(s.sidebarCollapsed).toBe(false)
    expect(s.mode).toBe('system')
    expect(s.theme).toBe('graphite')
  })

  it('toggles the sidebar and persists it', () => {
    useUi.getState().toggleSidebar()
    expect(useUi.getState().sidebarCollapsed).toBe(true)
    expect(saved()?.state.sidebarCollapsed).toBe(true)
    useUi.getState().toggleSidebar()
    expect(saved()?.state.sidebarCollapsed).toBe(false)
  })

  it('persists mode and theme, and only those fields', () => {
    useUi.getState().setMode('light')
    useUi.getState().setTheme('ocean')
    expect(saved()?.state).toEqual({ sidebarCollapsed: false, mode: 'light', theme: 'ocean' })
  })

  it('restores saved preferences', async () => {
    await rehydrateFrom({ sidebarCollapsed: true, mode: 'dark', theme: 'rose' })
    expect(useUi.getState()).toMatchObject({ sidebarCollapsed: true, mode: 'dark', theme: 'rose' })
  })

  it('migrates renamed theme ids', async () => {
    await rehydrateFrom({ theme: 'midnight' })
    expect(useUi.getState().theme).toBe('nightfall')
    await rehydrateFrom({ theme: 'plum' })
    expect(useUi.getState().theme).toBe('orchid')
    await rehydrateFrom({ theme: 'sand' })
    expect(useUi.getState().theme).toBe('dune')
  })

  it('falls back to defaults for invalid stored values', async () => {
    useUi.setState({ mode: 'dark', theme: 'forest' })
    await rehydrateFrom({ sidebarCollapsed: 'yes', mode: 'sepia', theme: 'neon' })
    // Unknown mode/theme keep the current values; a non-boolean flag reads as expanded.
    expect(useUi.getState()).toMatchObject({ sidebarCollapsed: false, mode: 'dark', theme: 'forest' })
  })

  it('survives corrupted storage', async () => {
    localStorage.setItem(UI_STORAGE_KEY, '{not json')
    await useUi.persist.rehydrate()
    expect(useUi.getState().theme).toBe('graphite')
  })
})
