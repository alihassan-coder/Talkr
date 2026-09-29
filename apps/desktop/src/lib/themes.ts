/**
 * Colour themes. Each one is still exactly two colours, `bg` and `fg`, with a
 * dark and a light variant. The same values live in globals.css (which paints
 * the app); these copies only draw the swatches in Settings.
 */
export type Palette = { bg: string; fg: string }

export type Theme = { id: string; name: string; dark: Palette; light: Palette }

export const themes = [
  { id: 'graphite', name: 'Graphite', dark: { bg: '#0b0b0c', fg: '#ededea' }, light: { bg: '#f5f4f0', fg: '#121212' } },
  { id: 'midnight', name: 'Midnight', dark: { bg: '#0a0e17', fg: '#e3e9f5' }, light: { bg: '#eef2f9', fg: '#0e1a33' } },
  { id: 'forest', name: 'Forest', dark: { bg: '#0a110d', fg: '#e2ece4' }, light: { bg: '#eef4ef', fg: '#10231a' } },
  { id: 'ember', name: 'Ember', dark: { bg: '#120c09', fg: '#f3e5dc' }, light: { bg: '#f8efe8', fg: '#2d150b' } },
  { id: 'plum', name: 'Plum', dark: { bg: '#100b13', fg: '#ece3f3' }, light: { bg: '#f5eff8', fg: '#24142f' } },
  { id: 'sand', name: 'Sand', dark: { bg: '#12100b', fg: '#efe5d1' }, light: { bg: '#f6f0e3', fg: '#2a2316' } },
] as const satisfies readonly Theme[]

export type ThemeId = (typeof themes)[number]['id']

export const DEFAULT_THEME: ThemeId = 'graphite'

export const isThemeId = (value: unknown): value is ThemeId => themes.some((t) => t.id === value)
