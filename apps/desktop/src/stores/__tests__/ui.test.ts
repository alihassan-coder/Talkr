import { afterEach, describe, expect, it } from 'vitest'
import { deriveCustomTheme, DEFAULT_CUSTOM_ACCENT } from '@/lib/themes'
import { UI_STORAGE_KEY, useUi } from '@/stores/ui'

const saved = () => JSON.parse(localStorage.getItem(UI_STORAGE_KEY) ?? 'null') as { state: Record<string, unknown> } | null

const rehydrateFrom = async (state: unknown) => {
  localStorage.setItem(UI_STORAGE_KEY, JSON.stringify({ state, version: 0 }))
  await useUi.persist.rehydrate()
}

afterEach(() => {
  useUi.setState({
    sidebarCollapsed: false,
    mode: 'system',
    theme: 'graphite',
    customAccent: DEFAULT_CUSTOM_ACCENT,
    zoom: 1,
    audioFormat: 'wav',
    textFormat: 'txt',
  })
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

  it('persists mode, theme and export formats, and only those fields', () => {
    useUi.getState().setMode('light')
    useUi.getState().setTheme('ocean')
    expect(saved()?.state).toEqual({
      sidebarCollapsed: false,
      mode: 'light',
      theme: 'ocean',
      customAccent: DEFAULT_CUSTOM_ACCENT,
      zoom: 1,
      audioFormat: 'wav',
      textFormat: 'txt',
    })
  })

  it('remembers the last audio and text format separately', () => {
    useUi.getState().rememberFormat('mp3')
    useUi.getState().rememberFormat('srt')
    expect(useUi.getState()).toMatchObject({ audioFormat: 'mp3', textFormat: 'srt' })
    useUi.getState().rememberFormat('flac')
    expect(useUi.getState()).toMatchObject({ audioFormat: 'flac', textFormat: 'srt' })
    expect(saved()?.state).toMatchObject({ audioFormat: 'flac', textFormat: 'srt' })
  })

  it('ignores stored export formats it does not know', async () => {
    useUi.setState({ audioFormat: 'mp3', textFormat: 'md' })
    await rehydrateFrom({ audioFormat: 'ogg', textFormat: 'docx' })
    expect(useUi.getState()).toMatchObject({ audioFormat: 'mp3', textFormat: 'md' })
    await rehydrateFrom({ audioFormat: 'flac', textFormat: 'vtt' })
    expect(useUi.getState()).toMatchObject({ audioFormat: 'flac', textFormat: 'vtt' })
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

  it('persists the custom accent and zoom, with the derived palette only for the custom theme', () => {
    useUi.getState().setCustomAccent('#2F9E5B')
    useUi.getState().setZoom(1.2)
    expect(saved()?.state).toMatchObject({ customAccent: '#2f9e5b', zoom: 1.2 })
    expect(saved()?.state.customPalette).toBeUndefined()
    useUi.getState().setTheme('custom')
    expect(saved()?.state.customPalette).toEqual(deriveCustomTheme('#2f9e5b'))
  })

  it('ignores an invalid custom accent and snaps zoom to a step', () => {
    useUi.getState().setCustomAccent('#2f9e5b')
    useUi.getState().setCustomAccent('not a colour')
    expect(useUi.getState().customAccent).toBe('#2f9e5b')
    useUi.getState().setZoom(9)
    expect(useUi.getState().zoom).toBe(1.5)
    useUi.getState().setZoom(1.04)
    expect(useUi.getState().zoom).toBe(1)
  })

  it('restores the custom theme, accent and zoom', async () => {
    await rehydrateFrom({ theme: 'custom', customAccent: '#D63C8A', zoom: 1.3 })
    expect(useUi.getState()).toMatchObject({ theme: 'custom', customAccent: '#d63c8a', zoom: 1.3 })
  })

  it('falls back for invalid stored accent and zoom', async () => {
    useUi.setState({ customAccent: '#2f9e5b', zoom: 1.1 })
    await rehydrateFrom({ customAccent: 'purple', zoom: 'big' })
    expect(useUi.getState()).toMatchObject({ customAccent: '#2f9e5b', zoom: 1 })
    await rehydrateFrom({ customAccent: 42 })
    expect(useUi.getState()).toMatchObject({ customAccent: '#2f9e5b', zoom: 1 })
  })

  it('survives corrupted storage', async () => {
    localStorage.setItem(UI_STORAGE_KEY, '{not json')
    await useUi.persist.rehydrate()
    expect(useUi.getState().theme).toBe('graphite')
  })
})
