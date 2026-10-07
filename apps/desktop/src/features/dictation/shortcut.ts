import type { DictationSettings, Shortcut } from '@/lib/types'

export const ctrlWin: Shortcut = { ctrl: true, shift: false, alt: false, win: true, key: null, keyLabel: null }
export const altShiftV: Shortcut = { ctrl: false, shift: true, alt: true, win: false, key: 0x56, keyLabel: 'V' }

/** Mirrors `DictationSettings::default()` in src-tauri/src/dictation/settings.rs. */
export const defaultDictation: DictationSettings = {
  enabled: false,
  shortcut: ctrlWin,
  mode: 'auto',
  pasteLastEnabled: true,
  pasteLastShortcut: altShiftV,
  model: null,
  language: null,
  keepWarm: true,
  microphone: null,
  insertMethod: 'auto',
  focusPolicy: 'original',
  restoreClipboard: true,
  smartSpacing: true,
  removeFillers: true,
  voiceCommands: true,
  vocabulary: [],
  replacements: [],
  appRules: [],
  overlayPosition: 'bottom',
  showTarget: true,
  sounds: true,
  saveHistory: true,
  launchAtLogin: false,
  closeToTray: true,
}

/** The keys of a shortcut, in the order Windows writes them: ["Ctrl", "Win"], ["Alt", "Shift", "V"]. */
export function shortcutKeys(s: Shortcut): string[] {
  const keys: string[] = []
  if (s.ctrl) keys.push('Ctrl')
  if (s.alt) keys.push('Alt')
  if (s.shift) keys.push('Shift')
  if (s.win) keys.push('Win')
  if (s.key !== null) keys.push(s.keyLabel ?? `Key ${s.key}`)
  return keys
}

export const describeShortcut = (s: Shortcut) => shortcutKeys(s).join(' + ')

const modifierCount = (s: Shortcut) => [s.ctrl, s.shift, s.alt, s.win].filter(Boolean).length

/** F1-F24, Pause, Scroll Lock, Insert: keys nobody types, fine on their own. */
const standalone = (vk: number) => (vk >= 0x70 && vk <= 0x87) || vk === 0x13 || vk === 0x91 || vk === 0x2d

const reserved = (vk: number) =>
  [0x10, 0x11, 0x12, 0x5b, 0x5c, 0x1b, 0x2c, 0x14, 0x90].includes(vk) || (vk >= 0xa0 && vk <= 0xa5) || (vk >= 0x01 && vk <= 0x06)

/** Why a shortcut cannot be used, worded for the user. Mirrors `Shortcut::problem` in Rust. */
export function shortcutProblem(s: Shortcut): string | null {
  if (s.key !== null) {
    if (reserved(s.key)) return 'That key cannot be part of a shortcut. Escape cancels dictation.'
    if (modifierCount(s) === 0 && !standalone(s.key))
      return 'Add Ctrl, Alt, Shift or Win. Only F-keys, Pause, Scroll Lock and Insert work alone.'
    return null
  }
  if (modifierCount(s) < 2) return 'Use at least two modifier keys (like Ctrl + Win), or a modifier and a key.'
  return null
}

export const sameKeys = (a: Shortcut, b: Shortcut) =>
  a.ctrl === b.ctrl && a.shift === b.shift && a.alt === b.alt && a.win === b.win && a.key === b.key
