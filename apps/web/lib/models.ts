import catalog from '@talkr/model-catalog/catalog.json'

export type CatalogModel = {
  id: string
  kind: 'stt' | 'tts'
  engine: string
  name: string
  description: string
  languages: string[]
  sizeBytes: number
  license: string
  homepage: string
  tags: string[]
}

export const models = catalog.models as CatalogModel[]
export const sttModels = models.filter((m) => m.kind === 'stt')
export const ttsModels = models.filter((m) => m.kind === 'tts')
export const largestModelBytes = Math.max(...models.map((m) => m.sizeBytes))

const WORDS = ['zero', 'one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight', 'nine', 'ten', 'eleven', 'twelve', 'thirteen', 'fourteen', 'fifteen', 'sixteen', 'seventeen', 'eighteen', 'nineteen', 'twenty']

export function countInWords(n: number) {
  return WORDS[n] ?? String(n)
}

export function formatBytes(bytes: number) {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`
  return `${Math.round(bytes / 1e6)} MB`
}

/**
 * "Whisper Small (Multilingual)" -> { base: "Whisper Small", variant: "Multilingual" }.
 * A trailing "(compressed)" is dropped; `isCompressed` covers it.
 */
export function splitName(name: string) {
  const match = name.match(/^(.*?)\s*((?:\([^()]*\)\s*)+)$/)
  if (!match) return { base: name, variant: null }
  const groups = [...(match[2] ?? '').matchAll(/\(([^()]*)\)/g)]
    .map((g) => (g[1] ?? '').trim())
    .filter((g) => g !== '' && g.toLowerCase() !== 'compressed')
  return { base: match[1] || name, variant: groups.length ? groups.join(' · ') : null }
}

const LANGUAGE_NAMES: Record<string, string> = {
  en: 'English',
  'en-GB': 'English (UK)',
  de: 'German',
  fr: 'French',
  es: 'Spanish',
}

export function describeLanguages(languages: string[]) {
  const real = languages.filter((l) => l !== 'auto')
  if (real.length === 1) return LANGUAGE_NAMES[real[0]!] ?? real[0]!
  return `${real.length} languages`
}

/** "MIT (model) / GPL-3.0 (espeak-ng-data)" -> "MIT + GPL-3.0" */
export function shortLicense(license: string) {
  return license.replace(/\s*\([^)]*\)/g, '').replace(/\s*\/\s*/g, ' + ')
}

export const isRecommended = (m: CatalogModel) => m.tags.includes('recommended')

/** Quantized variants: smaller, and they need less memory. */
export const isCompressed = (m: CatalogModel) => m.tags.includes('quantized') || m.tags.includes('low-memory')
