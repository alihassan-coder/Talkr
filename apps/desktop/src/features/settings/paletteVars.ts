import type { CSSProperties } from 'react'
import type { Palette } from '@/lib/themes'

type Mode = 'light' | 'dark'

/** Mix percentages per mode, the same as globals.css uses for the app. */
const MIX: Record<Mode, { panel: string; muted: string; subtle: string; line: string }> = {
  dark: { panel: '2.5%', muted: '62%', subtle: '46%', line: '10%' },
  light: { panel: '3.5%', muted: '66%', subtle: '50%', line: '12%' },
}

/**
 * Custom properties that paint an `.ap-palette` subtree (see appearance.css) in
 * one palette and mode, independent of the app's own theme.
 */
export function paletteVars(palette: Palette, mode: Mode): CSSProperties {
  const mix = MIX[mode]
  return {
    '--p-bg': palette.bg,
    '--p-surface': palette.surface,
    '--p-fg': palette.fg,
    '--p-accent': palette.accent,
    '--p-on-accent': palette.onAccent,
    '--p-mix-panel': mix.panel,
    '--p-mix-muted': mix.muted,
    '--p-mix-subtle': mix.subtle,
    '--p-mix-line': mix.line,
  } as CSSProperties
}
