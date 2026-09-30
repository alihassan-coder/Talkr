import { describe, expect, it } from 'vitest'
import {
  COMPRESSED_HINT,
  backendLabel,
  compressedAlternative,
  describeLanguages,
  formatBytes,
  formatRam,
  formatSpeed,
  isAccelerated,
  isCompressed,
  isRecommended,
  primaryGpu,
  shortCpuName,
  shortLicense,
  sortModels,
  splitName,
  tagLabels,
} from '@/features/models/format'
import { catalogFixture, hardwareFixture } from '@/test/tauri'

describe('formatBytes / formatSpeed / formatRam', () => {
  it('uses decimal units', () => {
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(2_000)).toBe('2 KB')
    expect(formatBytes(147_964_211)).toBe('148 MB')
    expect(formatBytes(1_624_555_275)).toBe('1.6 GB')
  })
  it('formats speed', () => {
    expect(formatSpeed(3_500_000)).toBe('4 MB/s')
  })
  it('rounds RAM to whole binary gigabytes', () => {
    expect(formatRam(16 * 1024 ** 3)).toBe('16 GB')
    expect(formatRam(15.6 * 1024 ** 3)).toBe('16 GB')
  })
})

describe('splitName', () => {
  it('splits a variant', () => {
    expect(splitName('Whisper Small (Multilingual)')).toEqual({ base: 'Whisper Small', variant: 'Multilingual' })
  })
  it('drops a trailing (compressed)', () => {
    expect(splitName('Whisper Small (English) (compressed)')).toEqual({ base: 'Whisper Small', variant: 'English' })
    expect(splitName('Whisper Tiny (compressed)')).toEqual({ base: 'Whisper Tiny', variant: null })
  })
  it('keeps commas inside a variant and handles no parentheses', () => {
    expect(splitName('Piper English (Lessac, Medium)')).toEqual({ base: 'Piper English', variant: 'Lessac, Medium' })
    expect(splitName('Kokoro Multi-language')).toEqual({ base: 'Kokoro Multi-language', variant: null })
  })
})

describe('languages and licenses', () => {
  it('describes languages', () => {
    expect(describeLanguages(['en'])).toBe('English')
    expect(describeLanguages(['auto', 'de'])).toBe('German')
    expect(describeLanguages(['xx'])).toBe('xx')
    expect(describeLanguages(['en', 'de', 'fr'])).toBe('3 languages')
  })
  it('shortens licenses', () => {
    expect(shortLicense('MIT (model) / GPL-3.0 (espeak-ng-data)')).toBe('MIT + GPL-3.0')
    expect(shortLicense('Apache-2.0')).toBe('Apache-2.0')
  })
})

describe('tags', () => {
  const recommended = catalogFixture({ id: 'whisper-base', tags: ['recommended', 'fast'] })
  const plain = catalogFixture({ id: 'whisper-small', tags: ['accurate', 'gpu-recommended'] })
  const compressed = catalogFixture({ id: 'whisper-small-q5', tags: ['accurate', 'quantized', 'low-memory'] })

  it('labels known tags in a fixed order', () => {
    expect(tagLabels(plain)).toEqual(['Best with GPU', 'Accurate'])
    expect(tagLabels(recommended)).toEqual(['Fast'])
  })
  it('sorts recommended models first, keeping catalog order otherwise', () => {
    expect(sortModels([plain, recommended, compressed]).map((m) => m.id)).toEqual([
      'whisper-base',
      'whisper-small',
      'whisper-small-q5',
    ])
    expect(isRecommended(recommended)).toBe(true)
  })
  it('detects compressed models by tag or id', () => {
    expect(isCompressed(compressed)).toBe(true)
    expect(isCompressed({ id: 'whisper-medium-q5_1', tags: [] })).toBe(true)
    expect(isCompressed(plain)).toBe(false)
    expect(COMPRESSED_HINT).toBe('Compressed · uses less memory')
  })
  it('finds the compressed alternative of a model', () => {
    const catalog = [recommended, plain, compressed]
    expect(compressedAlternative(plain, catalog)?.id).toBe('whisper-small-q5')
    expect(compressedAlternative(recommended, catalog)).toBeNull()
    expect(compressedAlternative(compressed, catalog)).toBeNull()
  })
})

describe('hardware', () => {
  it('labels backends', () => {
    expect(backendLabel('vulkan')).toBe('Vulkan')
    expect(backendLabel('metal')).toBe('Metal')
    expect(backendLabel('cpu')).toBe('CPU')
  })
  it('knows when the machine is accelerated', () => {
    expect(isAccelerated(hardwareFixture())).toBe(true)
    expect(isAccelerated(hardwareFixture({ recommendedBackend: 'cpu' }))).toBe(false)
  })
  it('picks the GPU doing the work', () => {
    const hw = hardwareFixture({
      recommendedBackend: 'metal',
      gpus: [
        { name: 'Other', vendor: 'unknown', vramBytes: null },
        { name: 'Apple M2', vendor: 'apple', vramBytes: null },
      ],
    })
    expect(primaryGpu(hw)?.name).toBe('Apple M2')
    expect(primaryGpu(hardwareFixture({ gpus: [] }))).toBeNull()
    expect(primaryGpu(hardwareFixture())?.name).toBe('NVIDIA GeForce RTX 3060')
  })
  it('shortens CPU names', () => {
    expect(shortCpuName('Intel(R) Core(TM) i7-9750H CPU @ 2.60GHz')).toBe('Intel Core i7-9750H')
    expect(shortCpuName('AMD Ryzen 7 5800X 8-Core Processor')).toBe('AMD Ryzen 7 5800X')
  })
})
