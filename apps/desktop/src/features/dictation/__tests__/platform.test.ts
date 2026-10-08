import { describe, expect, it } from 'vitest'
import { altShiftV, ctrlWin, describeShortcut, pasteKeys, shortcutKeys, shortcutProblem, spokenKeys } from '@/features/dictation/shortcut'
import type { KeyPlatform } from '@/features/dictation/shortcut'
import { addKey, emptyShortcut, keyFromCode } from '@/features/dictation/keymap'
import { liveState } from '@/features/dictation/state'
import { toAppId } from '@/features/dictation/utils'
import { defaultDictation } from '@/features/dictation/shortcut'
import { WAVE, WaveMotion, wavePath } from '@/overlay/wave'
import type { DictationStatus } from '@/lib/types'

const mac: KeyPlatform = { os: 'macos', metaKey: '⌘', modifierOnly: true }
const linux: KeyPlatform = { os: 'linux', metaKey: 'Super', modifierOnly: true }
const all = { ctrl: true, alt: true, shift: true, win: true, key: 0x4b, keyLabel: 'K' }

describe('keycaps per system', () => {
  it('uses each system’s names in its order', () => {
    expect(shortcutKeys(all)).toEqual(['Ctrl', 'Alt', 'Shift', 'Win', 'K'])
    expect(shortcutKeys(all, mac)).toEqual(['⌃', '⌥', '⇧', '⌘', 'K'])
    expect(shortcutKeys(ctrlWin, linux)).toEqual(['Ctrl', 'Super'])
  })

  it('writes and speaks them naturally', () => {
    expect(describeShortcut(altShiftV, mac)).toBe('⌥⇧V')
    expect(describeShortcut(altShiftV, linux)).toBe('Alt + Shift + V')
    expect(spokenKeys(ctrlWin, mac)).toBe('Control plus Command')
    expect(spokenKeys(ctrlWin)).toBe('Ctrl plus Win')
    expect(pasteKeys(mac)).toBe('⌘V')
  })

  it('words problems for the system', () => {
    expect(shortcutProblem({ ...emptyShortcut, key: 0x41, keyLabel: 'A' }, mac)).toMatch(/Add ⌃, ⌥, ⇧ or ⌘/)
    expect(shortcutProblem(ctrlWin, { ...mac, modifierOnly: false })).toMatch(/needs a key/)
    expect(shortcutProblem(emptyShortcut)).toMatch(/No keys/)
  })
})

describe('keys from the window', () => {
  it('maps key codes to virtual keys', () => {
    expect(keyFromCode('KeyV')).toEqual({ key: 0x56, keyLabel: 'V' })
    expect(keyFromCode('Digit7')).toEqual({ key: 0x37, keyLabel: '7' })
    expect(keyFromCode('F9')).toEqual({ key: 0x78, keyLabel: 'F9' })
    expect(keyFromCode('Space')).toEqual({ key: 0x20, keyLabel: 'Space' })
    expect(keyFromCode('ShiftLeft')).toBeNull()
  })

  it('builds a chord key by key', () => {
    const chord = ['ControlLeft', 'MetaLeft', 'Space'].reduce(addKey, emptyShortcut)
    expect(chord).toEqual({ ctrl: true, shift: false, alt: false, win: true, key: 0x20, keyLabel: 'Space' })
  })
})

describe('app ids', () => {
  it('follows how each system names apps', () => {
    expect(toAppId('Slack', 'windows')).toBe('slack.exe')
    expect(toAppId(' com.Microsoft.rdc.macos ', 'macos')).toBe('com.microsoft.rdc.macos')
    expect(toAppId('Remmina', 'linux')).toBe('remmina')
  })
})

describe('live state', () => {
  const base: DictationStatus = {
    supported: true,
    capabilities: {
      os: 'windows',
      supported: true,
      holdToTalk: true,
      modifierOnly: true,
      recordsShortcut: true,
      verifiesInsertion: true,
      insertsText: true,
      metaKey: 'Win',
      note: null,
    },
    permission: { state: 'notNeeded' },
    active: true,
    error: null,
    modelId: 'm',
    warm: true,
    hasLast: false,
  }
  const on = { ...defaultDictation, enabled: true }

  it('puts the most urgent first', () => {
    expect(liveState(base, on)).toBe('ready')
    expect(liveState({ ...base, warm: false }, on)).toBe('loading')
    expect(liveState({ ...base, active: false }, on)).toBe('starting')
    expect(liveState({ ...base, modelId: null }, on)).toBe('noModel')
    expect(liveState({ ...base, error: 'x' }, on)).toBe('error')
    expect(liveState(base, defaultDictation)).toBe('off')
    expect(liveState({ ...base, permission: { state: 'missing', title: 'A', detail: 'B', canRequest: true } }, on)).toBe('permission')
    expect(liveState({ ...base, supported: false }, on)).toBe('unsupported')
  })
})

describe('waveform', () => {
  it('draws one rounded stroke per bar', () => {
    const d = wavePath(new Array<number>(WAVE.bars).fill(1))
    expect(d.match(/M/g)).toHaveLength(WAVE.bars)
    // Full height reaches from cap to cap.
    expect(d.startsWith(`M${(WAVE.stroke / 2).toFixed(2)} ${(WAVE.stroke / 2).toFixed(2)}V`)).toBe(true)
  })

  it('rises with the voice and is tallest in the middle', () => {
    const motion = new WaveMotion(true)
    let bars: number[] = []
    for (let t = 0; t < 600; t += 16) bars = [...motion.step(t, 0.1).bars]
    const mid = Math.floor(WAVE.bars / 2)
    expect(bars[mid]).toBeGreaterThan(0.5)
    expect(bars[mid]).toBeGreaterThan(bars[0]!)
    const quiet = new WaveMotion(true)
    let rest: number[] = []
    for (let t = 0; t < 600; t += 16) rest = [...quiet.step(t, 0).bars]
    expect(Math.max(...rest)).toBeLessThan(0.2)
  })
})
