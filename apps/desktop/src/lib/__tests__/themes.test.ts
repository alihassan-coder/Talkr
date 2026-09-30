import { describe, expect, it } from 'vitest'
import { DEFAULT_THEME, findTheme, isThemeId, migrateThemeId, themes } from '@/lib/themes'

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
