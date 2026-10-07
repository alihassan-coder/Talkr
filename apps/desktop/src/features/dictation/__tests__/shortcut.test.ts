import { describe, expect, it } from 'vitest'
import { altShiftV, ctrlWin, describeShortcut, sameKeys, shortcutKeys, shortcutProblem } from '@/features/dictation/shortcut'
import { recommendedModel, toExe } from '@/features/dictation/utils'
import { levelToHeight } from '@/overlay/level'
import type { Shortcut } from '@/lib/types'

const key = (patch: Partial<Shortcut>): Shortcut => ({ ctrl: false, shift: false, alt: false, win: false, key: null, keyLabel: null, ...patch })

describe('shortcuts', () => {
  it('lists keys in Windows order', () => {
    expect(shortcutKeys(ctrlWin)).toEqual(['Ctrl', 'Win'])
    expect(describeShortcut(altShiftV)).toBe('Alt + Shift + V')
    expect(shortcutKeys(key({ ctrl: true, key: 0x20, keyLabel: 'Space' }))).toEqual(['Ctrl', 'Space'])
  })

  it('applies the same rules as the backend', () => {
    expect(shortcutProblem(ctrlWin)).toBeNull()
    expect(shortcutProblem(altShiftV)).toBeNull()
    // One modifier alone would fire while typing.
    expect(shortcutProblem(key({ ctrl: true }))).toMatch(/two modifier/)
    // A letter alone too; an F-key is fine.
    expect(shortcutProblem(key({ key: 0x41, keyLabel: 'A' }))).toMatch(/Add Ctrl/)
    expect(shortcutProblem(key({ key: 0x78, keyLabel: 'F9' }))).toBeNull()
    // Escape cancels dictation.
    expect(shortcutProblem(key({ ctrl: true, key: 0x1b }))).toMatch(/Escape/)
  })

  it('compares keys, not labels', () => {
    expect(sameKeys(altShiftV, { ...altShiftV, keyLabel: 'v' })).toBe(true)
    expect(sameKeys(ctrlWin, altShiftV)).toBe(false)
  })
})

describe('dictation helpers', () => {
  it('turns app names into executables', () => {
    expect(toExe('Slack')).toBe('slack.exe')
    expect(toExe('  MSTSC.EXE ')).toBe('mstsc.exe')
    expect(toExe('C:\\Program Files\\App\\tool.exe')).toBe('tool.exe')
    expect(toExe('   ')).toBe('')
  })

  it('suggests a model that fits the computer', () => {
    expect(recommendedModel(true, 'auto')).toBe('whisper-large-v3-turbo-q5')
    expect(recommendedModel(false, 'en')).toBe('whisper-small-en-q5')
    expect(recommendedModel(false, 'auto')).toBe('whisper-small-q5')
  })

  it('maps microphone level to bar height on a log scale', () => {
    expect(levelToHeight(0)).toBe(0)
    expect(levelToHeight(Number.NaN)).toBe(0)
    expect(levelToHeight(0.001)).toBe(0)
    expect(levelToHeight(1)).toBe(1)
    const quiet = levelToHeight(0.01)
    const loud = levelToHeight(0.1)
    expect(quiet).toBeGreaterThan(0)
    expect(loud).toBeGreaterThan(quiet)
    expect(loud).toBeLessThan(1)
  })
})
