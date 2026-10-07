import { describe, expect, it } from 'vitest'
import { contrast } from '@/lib/color'
import {
  CUSTOM_THEME,
  DEFAULT_CUSTOM_ACCENT,
  DEFAULT_THEME,
  deriveCustomTheme,
  findTheme,
  isThemeId,
  migrateThemeId,
  resolvePalette,
  themes,
} from '@/lib/themes'

const HEX = /^#[0-9a-f]{6}$/

describe('themes', () => {
  it('has eight themes with unique ids', () => {
    expect(themes).toHaveLength(8)
    expect(new Set(themes.map((t) => t.id)).size).toBe(themes.length)
  })

  it.each(themes.map((t) => [t.id, t] as const))('%s has complete light and dark palettes', (_id, theme) => {
    for (const mode of ['light', 'dark'] as const) {
      const palette = theme[mode]
      for (const key of ['bg', 'surface', 'fg', 'accent', 'onAccent'] as const) {
        expect(palette[key]).toMatch(HEX)
      }
    }
    expect(theme.name).not.toBe('')
    expect(theme.mood).not.toBe('')
  })

  it('defaults to graphite', () => {
    expect(DEFAULT_THEME).toBe('graphite')
    expect(isThemeId(DEFAULT_THEME)).toBe(true)
  })

  it('isThemeId accepts only known ids', () => {
    expect(isThemeId('ocean')).toBe(true)
    expect(isThemeId('midnight')).toBe(false)
    expect(isThemeId(undefined)).toBe(false)
    expect(isThemeId(42)).toBe(false)
  })

  it('migrates renamed ids and keeps current ones', () => {
    expect(migrateThemeId('midnight')).toBe('nightfall')
    expect(migrateThemeId('plum')).toBe('orchid')
    expect(migrateThemeId('sand')).toBe('dune')
    expect(migrateThemeId('forest')).toBe('forest')
  })

  it('returns undefined for unknown or non-string values', () => {
    expect(migrateThemeId('neon')).toBeUndefined()
    expect(migrateThemeId('toString')).toBeUndefined()
    expect(migrateThemeId(null)).toBeUndefined()
    expect(migrateThemeId({})).toBeUndefined()
  })

  it('findTheme returns the theme, or the first one for an unknown id', () => {
    expect(findTheme('rose').name).toBe('Rose')
    expect(findTheme('nope' as never)).toBe(themes[0])
  })
})

describe('custom theme', () => {
  const accents = ['#3a6ff0', '#ffd400', '#0a1a3c', '#121212', '#f2f2f2', '#2f9e5b', '#ff00ff', '#6b7280']

  it.each(accents)('derives complete, readable light and dark palettes from %s', (accent) => {
    const derived = deriveCustomTheme(accent)
    for (const mode of ['light', 'dark'] as const) {
      const palette = derived[mode]
      for (const key of ['bg', 'surface', 'fg', 'accent', 'onAccent'] as const) {
        expect(palette[key]).toMatch(HEX)
      }
      // Body text, the accent as text or icon, and text drawn on the accent all read.
      expect(contrast(palette.fg, palette.bg)).toBeGreaterThanOrEqual(12)
      expect(contrast(palette.accent, palette.bg)).toBeGreaterThanOrEqual(4.5)
      expect(contrast(palette.accent, palette.surface)).toBeGreaterThanOrEqual(4.5)
      expect(contrast(palette.onAccent, palette.accent)).toBeGreaterThanOrEqual(4.5)
    }
    expect(derived.light.bg).not.toBe(derived.dark.bg)
  })

  it('keeps an accent that already reads well as it is', () => {
    expect(deriveCustomTheme('#2563eb').light.accent).toBe('#2563eb')
    expect(deriveCustomTheme('#8e9cff').dark.accent).toBe('#8e9cff')
  })

  it('falls back to the default accent for invalid input', () => {
    expect(deriveCustomTheme('not a colour')).toEqual(deriveCustomTheme(DEFAULT_CUSTOM_ACCENT))
  })

  it('resolvePalette returns built-in or derived palettes', () => {
    expect(resolvePalette('ocean', '#ff0000', 'dark')).toEqual(findTheme('ocean').dark)
    expect(resolvePalette(CUSTOM_THEME, '#ff0000', 'light')).toEqual(deriveCustomTheme('#ff0000').light)
  })
})
