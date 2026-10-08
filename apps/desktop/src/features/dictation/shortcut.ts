import type { DictationCapabilities, DictationSettings, Shortcut } from '@/lib/types'

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

export type Os = DictationCapabilities['os']

/** What keycaps need to know about the system: its name for each modifier, and its rules. */
export interface KeyPlatform {
  os: Os
  /** The backend's name for the Win key ("Win", "⌘", "Super"). */
  metaKey: string
  /** Shortcuts of modifiers alone can be used. */
  modifierOnly: boolean
}

export const windowsKeys: KeyPlatform = { os: 'windows', metaKey: 'Win', modifierOnly: true }

type Modifier = 'ctrl' | 'alt' | 'shift' | 'win'
/** Every system lists modifiers in this order: Ctrl Alt Shift Win, and macOS's ⌃ ⌥ ⇧ ⌘. */
const ORDER: Modifier[] = ['ctrl', 'alt', 'shift', 'win']

const symbols: Record<Modifier, string> = { ctrl: '⌃', alt: '⌥', shift: '⇧', win: '⌘' }
const macNames: Record<Modifier, string> = { ctrl: 'Control', alt: 'Option', shift: 'Shift', win: 'Command' }

function modifierLabel(m: Modifier, p: KeyPlatform): string {
  if (p.os === 'macos') return symbols[m]
  if (m === 'win') return p.metaKey || (p.os === 'linux' ? 'Super' : 'Win')
  return { ctrl: 'Ctrl', alt: 'Alt', shift: 'Shift' }[m]
}

/** The keys of a shortcut as this system writes them: ["Ctrl", "Win"], ["⌥", "⇧", "V"]. */
export function shortcutKeys(s: Shortcut, p: KeyPlatform = windowsKeys): string[] {
  const keys = ORDER.filter((m) => s[m]).map((m) => modifierLabel(m, p))
  if (s.key !== null) keys.push(s.keyLabel ?? `Key ${s.key}`)
  return keys
}

/** The keys as a screen reader should say them ("Control plus Command"). */
export function spokenKeys(s: Shortcut, p: KeyPlatform = windowsKeys): string {
  if (p.os !== 'macos') return shortcutKeys(s, p).join(' plus ')
  const keys = ORDER.filter((m) => s[m]).map((m) => macNames[m])
  if (s.key !== null) keys.push(s.keyLabel ?? `Key ${s.key}`)
  return keys.join(' plus ')
}

/** One line of text: "Ctrl + Win", or "⌃⌘" on a Mac. */
export const describeShortcut = (s: Shortcut, p: KeyPlatform = windowsKeys) =>
  shortcutKeys(s, p).join(p.os === 'macos' ? '' : ' + ')

/** The paste shortcut people press themselves when text was only copied. */
export const pasteKeys = (p: KeyPlatform) => (p.os === 'macos' ? '⌘V' : 'Ctrl + V')

const modifierCount = (s: Shortcut) => ORDER.filter((m) => s[m]).length

/** F1-F24, Pause, Scroll Lock, Insert: keys nobody types, fine on their own. */
const standalone = (vk: number) => (vk >= 0x70 && vk <= 0x87) || vk === 0x13 || vk === 0x91 || vk === 0x2d

const reserved = (vk: number) =>
  [0x10, 0x11, 0x12, 0x5b, 0x5c, 0x1b, 0x2c, 0x14, 0x90].includes(vk) || (vk >= 0xa0 && vk <= 0xa5) || (vk >= 0x01 && vk <= 0x06)

/** "Ctrl, Alt, Shift or Win", in the system's own words. */
const modifierList = (p: KeyPlatform) => {
  const names = ORDER.map((m) => modifierLabel(m, p))
  return `${names.slice(0, -1).join(', ')} or ${names.at(-1)}`
}

/** Why a shortcut cannot be used, worded for the user. Mirrors `Shortcut::problem` in Rust. */
export function shortcutProblem(s: Shortcut, p: KeyPlatform = windowsKeys): string | null {
  if (s.key !== null) {
    if (reserved(s.key)) return 'That key cannot be part of a shortcut. Escape cancels dictation.'
    if (modifierCount(s) === 0 && !standalone(s.key))
      return `Add ${modifierList(p)}. Only F-keys, Pause, Scroll Lock and Insert work alone.`
    return null
  }
  if (modifierCount(s) === 0) return 'No keys were pressed.'
  if (!p.modifierOnly) {
    const example = describeShortcut({ ...s, key: 0x20, keyLabel: 'Space' }, p)
    return `This system needs a key with the modifiers, like ${example}.`
  }
  if (modifierCount(s) < 2) {
    const example = describeShortcut(ctrlWin, p)
    return `Use at least two modifier keys (like ${example}), or a modifier and a key.`
  }
  return null
}

export const sameKeys = (a: Shortcut, b: Shortcut) =>
  a.ctrl === b.ctrl && a.shift === b.shift && a.alt === b.alt && a.win === b.win && a.key === b.key
