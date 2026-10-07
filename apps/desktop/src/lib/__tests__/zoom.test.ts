import { describe, expect, it } from 'vitest'
import { DEFAULT_ZOOM, formatZoom, normalizeZoom, stepZoom, ZOOM_LEVELS, zoomKey } from '@/lib/zoom'

const key = (key: string, mods: { ctrlKey?: boolean; metaKey?: boolean; altKey?: boolean } = {}, code = '') => ({
  key,
  code,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  ...mods,
})

describe('zoom levels', () => {
  it('runs from 80% to 150% with 100% as the default', () => {
    expect(ZOOM_LEVELS[0]).toBe(0.8)
    expect(ZOOM_LEVELS.at(-1)).toBe(1.5)
    expect(DEFAULT_ZOOM).toBe(1)
    expect(formatZoom(1.1)).toBe('110%')
  })

  it('normalizeZoom snaps to the nearest step and falls back to 100%', () => {
    expect(normalizeZoom(1.2)).toBe(1.2)
    expect(normalizeZoom(1.23)).toBe(1.2)
    expect(normalizeZoom(3)).toBe(1.5)
    expect(normalizeZoom(0.1)).toBe(0.8)
    expect(normalizeZoom('1.2')).toBe(1)
    expect(normalizeZoom(NaN)).toBe(1)
    expect(normalizeZoom(undefined)).toBe(1)
  })

  it('stepZoom moves one step and stops at the ends', () => {
    expect(stepZoom(1, 1)).toBe(1.1)
    expect(stepZoom(1, -1)).toBe(0.9)
    expect(stepZoom(1.5, 1)).toBe(1.5)
    expect(stepZoom(0.8, -1)).toBe(0.8)
  })
})

describe('zoomKey', () => {
  it('reads Ctrl or Cmd with +, =, - and 0', () => {
    expect(zoomKey(key('=', { ctrlKey: true }))).toBe(1)
    expect(zoomKey(key('+', { ctrlKey: true }))).toBe(1)
    expect(zoomKey(key('+', { metaKey: true }))).toBe(1)
    expect(zoomKey(key('-', { ctrlKey: true }))).toBe(-1)
    expect(zoomKey(key('-', { metaKey: true }))).toBe(-1)
    expect(zoomKey(key('0', { ctrlKey: true }))).toBe(0)
    expect(zoomKey(key('0', { metaKey: true }))).toBe(0)
  })

  it('reads the numpad keys', () => {
    expect(zoomKey(key('Add', { ctrlKey: true }, 'NumpadAdd'))).toBe(1)
    expect(zoomKey(key('Subtract', { ctrlKey: true }, 'NumpadSubtract'))).toBe(-1)
  })

  it('ignores plain keys, Alt combinations and other shortcuts', () => {
    expect(zoomKey(key('='))).toBeNull()
    expect(zoomKey(key('-'))).toBeNull()
    expect(zoomKey(key('0'))).toBeNull()
    expect(zoomKey(key('=', { ctrlKey: true, altKey: true }))).toBeNull()
    expect(zoomKey(key('0', { metaKey: true, altKey: true }))).toBeNull()
    expect(zoomKey(key('1', { ctrlKey: true }))).toBeNull()
    expect(zoomKey(key('b', { ctrlKey: true }))).toBeNull()
  })
})
