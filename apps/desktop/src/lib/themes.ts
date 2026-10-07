/**
 * Colour themes, the single typed source for every palette. Each theme has a
 * dark and a light variant of five base colours; the rest (panel, muted,
 * subtle, lines) is mixed from these in globals.css.
 *
 * The same values are written out in two more places that cannot import this
 * file: globals.css (paints the app) and the inline script in index.html
 * (paints the splash before any JS loads). Keep all three in sync.
 */
import { contrast, hexToOklch, normalizeHex, oklchToHex } from '@/lib/color'

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

/**
 * The user's own theme: one accent colour, everything else derived. Not in
 * globals.css or index.html's table; appearance.ts writes its tokens inline on
 * <html>, and the ui store saves the derived palettes for index.html's splash.
 */
export const CUSTOM_THEME = 'custom'

export type ThemeChoice = ThemeId | typeof CUSTOM_THEME

export const DEFAULT_CUSTOM_ACCENT = '#3a6ff0'

/** Starting points offered next to the colour picker. */
export const ACCENT_SUGGESTIONS = ['#3a6ff0', '#7c4dff', '#d63c8a', '#e0512f', '#d9a21b', '#2f9e5b', '#0e93a8', '#6b7280']

/** Contrast the accent keeps against the canvas: it doubles as icon and link colour. */
const ACCENT_CONTRAST = 4.5

/**
 * The full light and dark palettes for one accent, built the way the themes
 * above are: a canvas and ink barely tinted with the accent's hue, a white
 * light-mode surface, and the accent itself wherever it already reads well.
 * When it does not (pale yellow on white, navy on near-black) its lightness is
 * walked toward the readable side, keeping its hue and chroma. Near-greys get
 * the ink as accent in the mode they fail, like Graphite. The text on the
 * accent is whichever of white and the tinted dark ink contrasts more.
 */
export function deriveCustomTheme(accentHex: string): { dark: Palette; light: Palette } {
  const accent = normalizeHex(accentHex) ?? DEFAULT_CUSTOM_ACCENT
  const { c, h } = hexToOklch(accent)
  // Greys stay neutral; vivid accents tint the neutrals a little, never more than the built-ins do.
  const tint = Math.min(c, 0.16) / 0.16
  const neutral = (l: number, chroma: number) => oklchToHex({ l, c: chroma * tint, h })

  const build = (mode: 'dark' | 'light'): Palette => {
    const dark = mode === 'dark'
    const bg = dark ? neutral(0.155, 0.02) : neutral(0.968, 0.01)
    const surface = dark ? neutral(0.2, 0.024) : '#ffffff'
    const fg = dark ? neutral(0.935, 0.018) : neutral(0.215, 0.045)
    const ink = neutral(0.17, 0.035)

    let tone = accent
    if (Math.min(contrast(tone, bg), contrast(tone, surface)) < ACCENT_CONTRAST) {
      if (c < 0.03) tone = fg
      else {
        const start = hexToOklch(accent)
        for (let l = start.l; dark ? l <= 0.96 : l >= 0.2; l += dark ? 0.01 : -0.01) {
          tone = oklchToHex({ ...start, l })
          if (Math.min(contrast(tone, bg), contrast(tone, surface)) >= ACCENT_CONTRAST) break
        }
      }
    }
    const onAccent = contrast('#ffffff', tone) >= contrast(ink, tone) ? '#ffffff' : ink
    return p(bg, surface, fg, tone, onAccent)
  }

  return { dark: build('dark'), light: build('light') }
}

/** The palette on screen for a theme choice and mode. */
export const resolvePalette = (choice: ThemeChoice, customAccent: string, mode: 'light' | 'dark'): Palette =>
  choice === CUSTOM_THEME ? deriveCustomTheme(customAccent)[mode] : findTheme(choice)[mode]
