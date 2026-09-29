/**
 * Colour themes, the single typed source for every palette. Each theme has a
 * dark and a light variant of five base colours; the rest (panel, muted,
 * subtle, lines) is mixed from these in globals.css.
 *
 * The same values are written out in two more places that cannot import this
 * file: globals.css (paints the app) and the inline script in index.html
 * (paints the splash before any JS loads). Keep all three in sync.
 */
export type Palette = {
  /** Page canvas. */
  bg: string
  /** Cards, panels, popovers, inputs. */
  surface: string
  /** Ink: primary text and icons. */
  fg: string
  /** Primary buttons, selection, focus, progress. */
  accent: string
  /** Text and icons drawn on the accent. */
  onAccent: string
}

export type Theme = { id: string; name: string; mood: string; dark: Palette; light: Palette }

const p = (bg: string, surface: string, fg: string, accent: string, onAccent: string): Palette => ({
  bg,
  surface,
  fg,
  accent,
  onAccent,
})

export const themes = [
  {
    id: 'graphite',
    name: 'Graphite',
    mood: 'Monochrome',
    dark: p('#0b0b0c', '#141416', '#ededea', '#ededea', '#0b0b0c'),
    light: p('#f3f3f1', '#ffffff', '#121212', '#121212', '#ffffff'),
  },
  {
    id: 'nightfall',
    name: 'Nightfall',
    mood: 'Indigo night',
    dark: p('#0a0c15', '#121526', '#e4e7f7', '#8e9cff', '#0a0c15'),
    light: p('#f1f3fb', '#ffffff', '#121833', '#4353d6', '#ffffff'),
  },
  {
    id: 'forest',
    name: 'Forest',
    mood: 'Evergreen',
    dark: p('#09100c', '#101a14', '#e1ece4', '#5dd39e', '#06140d'),
    light: p('#eff4f0', '#ffffff', '#0f2419', '#187a4e', '#ffffff'),
  },
  {
    id: 'ember',
    name: 'Ember',
    mood: 'Warm charcoal',
    dark: p('#110c0a', '#1b1411', '#f4e9e1', '#ff8d52', '#1a0d06'),
    light: p('#faf5f1', '#ffffff', '#2a170d', '#b8480f', '#ffffff'),
  },
  {
    id: 'orchid',
    name: 'Orchid',
    mood: 'Violet dusk',
    dark: p('#0f0b14', '#19121f', '#eee6f6', '#d08cff', '#160a20'),
    light: p('#f7f3fa', '#ffffff', '#24142f', '#9440cc', '#ffffff'),
  },
  {
    id: 'dune',
    name: 'Dune',
    mood: 'Desert gold',
    dark: p('#12100b', '#1c1912', '#f0e7d4', '#e5b95f', '#1a1406'),
    light: p('#f7f2e8', '#fffdf8', '#2b2417', '#8a600c', '#ffffff'),
  },
  {
    id: 'ocean',
    name: 'Ocean',
    mood: 'Deep teal',
    dark: p('#081114', '#0f1b1f', '#e0eef1', '#45c9d8', '#041214'),
    light: p('#eef6f7', '#ffffff', '#0d2429', '#0b7383', '#ffffff'),
  },
  {
    id: 'rose',
    name: 'Rose',
    mood: 'Soft blush',
    dark: p('#130b0e', '#1d1216', '#f6e6eb', '#ff7d9c', '#1f0910'),
    light: p('#fbf3f5', '#ffffff', '#2d121a', '#bf2f58', '#ffffff'),
  },
] as const satisfies readonly Theme[]

export type ThemeId = (typeof themes)[number]['id']

export const DEFAULT_THEME: ThemeId = 'graphite'

export const isThemeId = (value: unknown): value is ThemeId => themes.some((t) => t.id === value)

/** Ids from earlier versions that were renamed. index.html has the same table. */
const renamed: Record<string, ThemeId> = { midnight: 'nightfall', plum: 'orchid', sand: 'dune' }

/** A saved theme id, upgraded if it was renamed; undefined if unknown. */
export const migrateThemeId = (value: unknown): ThemeId | undefined => {
  if (isThemeId(value)) return value
  return typeof value === 'string' && Object.hasOwn(renamed, value) ? renamed[value] : undefined
}

export const findTheme = (id: ThemeId): Theme => themes.find((t) => t.id === id) ?? themes[0]
