import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { normalizeHex } from '@/lib/color'
import {
  CUSTOM_THEME,
  DEFAULT_CUSTOM_ACCENT,
  DEFAULT_THEME,
  deriveCustomTheme,
  migrateThemeId,
  type ThemeChoice,
} from '@/lib/themes'
import { DEFAULT_ZOOM, normalizeZoom } from '@/lib/zoom'
import type { SaveFormat } from '@/lib/types'

export type ColorMode = 'system' | 'light' | 'dark'

export type AudioFormat = Extract<SaveFormat, 'wav' | 'flac' | 'mp3'>
export type TextFormat = Exclude<SaveFormat, AudioFormat>

const AUDIO_FORMATS: readonly AudioFormat[] = ['wav', 'flac', 'mp3']
const TEXT_FORMATS: readonly TextFormat[] = ['txt', 'md', 'srt', 'vtt', 'json', 'csv']
export const isAudioFormat = (value: unknown): value is AudioFormat => AUDIO_FORMATS.includes(value as AudioFormat)
const isTextFormat = (value: unknown): value is TextFormat => TEXT_FORMATS.includes(value as TextFormat)

type UiState = {
  sidebarCollapsed: boolean
  mode: ColorMode
  theme: ThemeChoice
  /** Accent of the custom theme, `#rrggbb`. Kept when another theme is picked. */
  customAccent: string
  /** Interface zoom, one of ZOOM_LEVELS. Per device, like the rest of this store. */
  zoom: number
  /** The audio and text formats last saved, offered first next time. */
  audioFormat: AudioFormat
  textFormat: TextFormat
  toggleSidebar: () => void
  setMode: (mode: ColorMode) => void
  setTheme: (theme: ThemeChoice) => void
  /** Sets the custom accent (ignored unless it is a valid hex colour). */
  setCustomAccent: (hex: string) => void
  setZoom: (zoom: number) => void
  rememberFormat: (format: SaveFormat) => void
}

/** Storage key. index.html reads it too, to paint the right theme before React loads. */
export const UI_STORAGE_KEY = 'talkr.ui'

const isMode = (value: unknown): value is ColorMode => value === 'system' || value === 'light' || value === 'dark'

/** Window-level preferences: layout and appearance. Kept in localStorage, not config.json. */
export const useUi = create<UiState>()(
  persist(
    (set) => ({
      sidebarCollapsed: false,
      mode: 'system',
      theme: DEFAULT_THEME,
      toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
      setMode: (mode) => set({ mode }),
      audioFormat: 'wav',
      textFormat: 'txt',
      setTheme: (theme) => set({ theme }),
      customAccent: DEFAULT_CUSTOM_ACCENT,
      setCustomAccent: (hex) => {
        const customAccent = normalizeHex(hex)
        if (customAccent) set({ customAccent })
      },
      zoom: DEFAULT_ZOOM,
      setZoom: (zoom) => set({ zoom: normalizeZoom(zoom) }),
      rememberFormat: (format) => set(isAudioFormat(format) ? { audioFormat: format } : { textFormat: format }),
    }),
    {
      name: UI_STORAGE_KEY,
      partialize: ({ sidebarCollapsed, mode, theme, customAccent, zoom, audioFormat, textFormat }) => ({
        sidebarCollapsed,
        mode,
        theme,
        customAccent,
        // Derived, saved only for index.html, which paints the splash before this code loads.
        // Never read back: the accent is the source of truth.
        customPalette: theme === CUSTOM_THEME ? deriveCustomTheme(customAccent) : undefined,
        zoom,
        audioFormat,
        textFormat,
      }),
      // Hand-edited or stale values fall back to the defaults instead of breaking the page.
      merge: (persisted, current) => {
        const p = (persisted ?? {}) as Partial<UiState>
        return {
          ...current,
          sidebarCollapsed: p.sidebarCollapsed === true,
          mode: isMode(p.mode) ? p.mode : current.mode,
          theme: p.theme === CUSTOM_THEME ? CUSTOM_THEME : (migrateThemeId(p.theme) ?? current.theme),
          customAccent: (typeof p.customAccent === 'string' && normalizeHex(p.customAccent)) || current.customAccent,
          zoom: p.zoom === undefined ? current.zoom : normalizeZoom(p.zoom),
          audioFormat: isAudioFormat(p.audioFormat) ? p.audioFormat : current.audioFormat,
          textFormat: isTextFormat(p.textFormat) ? p.textFormat : current.textFormat,
        }
      },
    },
  ),
)
