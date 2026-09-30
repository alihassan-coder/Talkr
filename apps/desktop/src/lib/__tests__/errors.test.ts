import { describe, expect, it } from 'vitest'
import { errorText } from '@/lib/errors'

describe('errorText', () => {
  it('passes plain backend strings through', () => {
    expect(errorText('The speech engine ran out of memory.')).toBe('The speech engine ran out of memory.')
    expect(errorText('  padded  ')).toBe('padded')
  })

  it('unwraps JSON-encoded errors instead of showing raw JSON', () => {
    expect(errorText('{"message":"stopped unexpectedly"}')).toBe('stopped unexpectedly')
    expect(errorText('{"error":{"message":"nested"}}')).toBe('nested')
    expect(errorText('{"code":1}')).toBe('Something went wrong.')
  })

  it('keeps text that only looks like JSON', () => {
    expect(errorText('{not json')).toBe('{not json')
  })

  it('handles Error instances and objects', () => {
    expect(errorText(new Error('boom'))).toBe('boom')
    expect(errorText({ message: 'from object' })).toBe('from object')
    expect(errorText({ reason: 'why' })).toBe('why')
  })

  it('falls back for empty or unknown values', () => {
    expect(errorText('')).toBe('Something went wrong.')
    expect(errorText(null)).toBe('Something went wrong.')
    expect(errorText(undefined)).toBe('Something went wrong.')
    expect(errorText(new Error(''))).toBe('Something went wrong.')
  })
})
