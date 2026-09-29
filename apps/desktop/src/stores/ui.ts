import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { DEFAULT_THEME, isThemeId, type ThemeId } from '@/lib/themes'

export type ColorMode = 'system' | 'light' | 'dark'

type UiState = {
  sidebarCollapsed: boolean
  mode: ColorMode
  theme: ThemeId
  toggleSidebar: () => void
  setMode: (mode: ColorMode) => void
  setTheme: (theme: ThemeId) => void
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
      setTheme: (theme) => set({ theme }),
    }),
    {
      name: UI_STORAGE_KEY,
      partialize: ({ sidebarCollapsed, mode, theme }) => ({ sidebarCollapsed, mode, theme }),
      // Hand-edited or stale values fall back to the defaults instead of breaking the page.
      merge: (persisted, current) => {
        const p = (persisted ?? {}) as Partial<UiState>
        return {
          ...current,
          sidebarCollapsed: p.sidebarCollapsed === true,
          mode: isMode(p.mode) ? p.mode : current.mode,
          theme: isThemeId(p.theme) ? p.theme : current.theme,
        }
      },
    },
  ),
)
