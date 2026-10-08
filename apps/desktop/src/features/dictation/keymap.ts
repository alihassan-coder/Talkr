import type { Shortcut } from '@/lib/types'

/**
 * Keys the window itself sees, as Windows virtual-key codes (the portable code every backend
 * stores). Used to show keys live while a shortcut is recorded, and to record one in a plain
 * browser preview where there is no keyboard hook.
 */
const named: Record<string, [number, string]> = {
  Space: [0x20, 'Space'],
  Enter: [0x0d, 'Enter'],
  Tab: [0x09, 'Tab'],
  Backspace: [0x08, 'Backspace'],
  Escape: [0x1b, 'Esc'],
  Insert: [0x2d, 'Insert'],
  Delete: [0x2e, 'Delete'],
  Home: [0x24, 'Home'],
  End: [0x23, 'End'],
  PageUp: [0x21, 'Page Up'],
  PageDown: [0x22, 'Page Down'],
  ArrowLeft: [0x25, '←'],
  ArrowUp: [0x26, '↑'],
  ArrowRight: [0x27, '→'],
  ArrowDown: [0x28, '↓'],
  Pause: [0x13, 'Pause'],
  ScrollLock: [0x91, 'Scroll Lock'],
  CapsLock: [0x14, 'Caps Lock'],
  Semicolon: [0xba, ';'],
  Equal: [0xbb, '='],
  Comma: [0xbc, ','],
  Minus: [0xbd, '-'],
  Period: [0xbe, '.'],
  Slash: [0xbf, '/'],
  Backquote: [0xc0, '`'],
  BracketLeft: [0xdb, '['],
  Backslash: [0xdc, '\\'],
  BracketRight: [0xdd, ']'],
  Quote: [0xde, "'"],
}

/** The non-modifier key of a keyboard event, or null for a modifier or a key we do not know. */
export function keyFromCode(code: string): { key: number; keyLabel: string } | null {
  let m = /^Key([A-Z])$/.exec(code)
  if (m) return { key: m[1]!.charCodeAt(0), keyLabel: m[1]! }
  m = /^Digit([0-9])$/.exec(code)
  if (m) return { key: 0x30 + Number(m[1]), keyLabel: m[1]! }
  m = /^F([0-9]{1,2})$/.exec(code)
  if (m && Number(m[1]) >= 1 && Number(m[1]) <= 24) return { key: 0x6f + Number(m[1]), keyLabel: `F${m[1]}` }
  m = /^Numpad([0-9])$/.exec(code)
  if (m) return { key: 0x60 + Number(m[1]), keyLabel: `Num ${m[1]}` }
  const hit = named[code]
  return hit ? { key: hit[0], keyLabel: hit[1] } : null
}

const modifierCodes: Record<string, keyof Pick<Shortcut, 'ctrl' | 'shift' | 'alt' | 'win'>> = {
  ControlLeft: 'ctrl',
  ControlRight: 'ctrl',
  ShiftLeft: 'shift',
  ShiftRight: 'shift',
  AltLeft: 'alt',
  AltRight: 'alt',
  MetaLeft: 'win',
  MetaRight: 'win',
  OSLeft: 'win',
  OSRight: 'win',
}

export const modifierOf = (code: string) => modifierCodes[code] ?? null

export const emptyShortcut: Shortcut = { ctrl: false, shift: false, alt: false, win: false, key: null, keyLabel: null }

/** Add a pressed key to a chord being recorded. */
export function addKey(chord: Shortcut, code: string): Shortcut {
  const modifier = modifierOf(code)
  if (modifier) return { ...chord, [modifier]: true }
  const key = keyFromCode(code)
  return key ? { ...chord, ...key } : chord
}

export const isEmptyChord = (s: Shortcut) => !s.ctrl && !s.shift && !s.alt && !s.win && s.key === null
