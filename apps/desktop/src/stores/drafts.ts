import { create } from 'zustand'

type DraftState = {
  /** The Speak box. Kept for the session, so switching screens never loses what was typed. */
  speakText: string
  setSpeakText: (text: string) => void
}

/** Unsent input that should survive navigation (pages remount on every route change). */
export const useDrafts = create<DraftState>((set) => ({
  speakText: '',
  setSpeakText: (speakText) => set({ speakText }),
}))
