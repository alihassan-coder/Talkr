import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { DEFAULT_THEME, migrateThemeId, type ThemeId } from '@/lib/themes'
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
  theme: ThemeId
  /** The audio and text formats last saved, offered first next time. */
  audioFormat: AudioFormat
  textFormat: TextFormat
  toggleSidebar: () => void
  setMode: (mode: ColorMode) => void
  setTheme: (theme: ThemeId) => void
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
      rememberFormat: (format) => set(isAudioFormat(format) ? { audioFormat: format } : { textFormat: format }),
    }),
    {
      name: UI_STORAGE_KEY,
      partialize: ({ sidebarCollapsed, mode, theme, audioFormat, textFormat }) => ({
        sidebarCollapsed,
        mode,
        theme,
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
          theme: migrateThemeId(p.theme) ?? current.theme,
          audioFormat: isAudioFormat(p.audioFormat) ? p.audioFormat : current.audioFormat,
          textFormat: isTextFormat(p.textFormat) ? p.textFormat : current.textFormat,
        }
      },
    },
  ),
)
