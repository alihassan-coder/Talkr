import { describe, expect, it } from 'vitest'
import { contrast, hexToOklch, normalizeHex, oklchToHex } from '@/lib/color'

describe('normalizeHex', () => {
  it('accepts 3 and 6 digit hex, with or without #, in any case', () => {
    expect(normalizeHex('#3A6FF0')).toBe('#3a6ff0')
    expect(normalizeHex('3a6ff0')).toBe('#3a6ff0')
    expect(normalizeHex('  #36F ')).toBe('#3366ff')
    expect(normalizeHex('fff')).toBe('#ffffff')
  })

  it('rejects anything else', () => {
    for (const bad of ['', '#', '#12', '#1234', '#12345', '#1234567', '#ggg000', 'blue', 'rgb(0,0,0)', '##123456']) {
      expect(normalizeHex(bad)).toBeNull()
    }
  })
})

describe('contrast', () => {
  it('is 21 for black on white and 1 for a colour on itself, in either order', () => {
    expect(contrast('#000000', '#ffffff')).toBeCloseTo(21, 5)
    expect(contrast('#ffffff', '#000000')).toBeCloseTo(21, 5)
    expect(contrast('#3a6ff0', '#3a6ff0')).toBe(1)
  })

  it('picks the more readable foreground for light and dark colours', () => {
    expect(contrast('#ffffff', '#1d4ed8')).toBeGreaterThan(contrast('#111111', '#1d4ed8'))
    expect(contrast('#111111', '#facc15')).toBeGreaterThan(contrast('#ffffff', '#facc15'))
  })
})

describe('OKLCH round trip', () => {
  it.each(['#3a6ff0', '#121212', '#ffffff', '#000000', '#e5b95f', '#bf2f58'])('%s survives the round trip', (hex) => {
    expect(oklchToHex(hexToOklch(hex))).toBe(hex)
  })

  it('brings out-of-gamut colours back into sRGB', () => {
    expect(oklchToHex({ l: 0.7, c: 0.5, h: 150 })).toMatch(/^#[0-9a-f]{6}$/)
  })
})
