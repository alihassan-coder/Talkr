import { describe, expect, it } from 'vitest'
import catalog from '@talkr/model-catalog/catalog.json'
import {
  countInWords,
  describeLanguages,
  formatBytes,
  isCompressed,
  isRecommended,
  largestModelBytes,
  models,
  shortLicense,
  splitName,
  sttModels,
  ttsModels,
} from '@/lib/models'

describe('catalog views', () => {
  it('exposes every catalog model, split by kind', () => {
    expect(models).toHaveLength(catalog.models.length)
    expect(sttModels.length + ttsModels.length).toBe(models.length)
    expect(sttModels.every((m) => m.kind === 'stt')).toBe(true)
    expect(ttsModels.every((m) => m.kind === 'tts')).toBe(true)
  })

  it('knows the largest model', () => {
    expect(largestModelBytes).toBe(Math.max(...catalog.models.map((m) => m.sizeBytes)))
  })

  it('has at least one recommended model of each kind', () => {
    expect(sttModels.some(isRecommended)).toBe(true)
    expect(ttsModels.some(isRecommended)).toBe(true)
  })

  it('marks the compressed (q5) Whisper variants', () => {
    const compressed = models.filter(isCompressed)
    expect(compressed.length).toBeGreaterThan(0)
    for (const m of compressed) expect(m.id).toMatch(/-q\d/)
    expect(isCompressed({ ...models[0]!, tags: ['fast'] })).toBe(false)
  })
})

describe('formatBytes', () => {
  it('formats sizes in MB and GB', () => {
    expect(formatBytes(59_707_625)).toBe('60 MB')
    expect(formatBytes(147_964_211)).toBe('148 MB')
    expect(formatBytes(999_000_000)).toBe('999 MB')
    expect(formatBytes(1_000_000_000)).toBe('1.0 GB')
    expect(formatBytes(1_624_555_275)).toBe('1.6 GB')
  })

  it('formats every catalog size', () => {
    for (const m of models) expect(formatBytes(m.sizeBytes)).toMatch(/^\d+(\.\d)? (MB|GB)$/)
  })
})

describe('text helpers', () => {
  it('splits names and drops the (compressed) suffix', () => {
    expect(splitName('Whisper Small (Multilingual)')).toEqual({ base: 'Whisper Small', variant: 'Multilingual' })
    expect(splitName('Whisper Small (English) (compressed)')).toEqual({ base: 'Whisper Small', variant: 'English' })
    expect(splitName('Kokoro English')).toEqual({ base: 'Kokoro English', variant: null })
  })

  it('never leaves a broken parenthesis in catalog names', () => {
    for (const m of models) {
      const { base, variant } = splitName(m.name)
      expect(`${base} ${variant ?? ''}`).not.toMatch(/[()]/)
    }
  })

  it('describes languages', () => {
    expect(describeLanguages(['en'])).toBe('English')
    expect(describeLanguages(['auto', 'en', 'de'])).toBe('2 languages')
    expect(describeLanguages(['sw'])).toBe('sw')
  })

  it('shortens licenses', () => {
    expect(shortLicense('MIT (model) / GPL-3.0 (espeak-ng-data)')).toBe('MIT + GPL-3.0')
  })

  it('counts in words up to twenty', () => {
    expect(countInWords(0)).toBe('zero')
    expect(countInWords(17)).toBe('seventeen')
    expect(countInWords(21)).toBe('21')
  })
})
