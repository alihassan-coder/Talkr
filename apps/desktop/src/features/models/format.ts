import type { Backend, CatalogModel, HardwareInfo } from '@/lib/types'

export function formatBytes(bytes: number) {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`
  if (bytes >= 1e6) return `${Math.round(bytes / 1e6)} MB`
  if (bytes >= 1e3) return `${Math.round(bytes / 1e3)} KB`
  return `${bytes} B`
}

/** Installed RAM reads better rounded to whole binary gigabytes ("16 GB"). */
export function formatRam(bytes: number) {
  return `${Math.round(bytes / 1024 ** 3)} GB`
}

/**
 * "Whisper Small (Multilingual)" -> { base: "Whisper Small", variant: "Multilingual" }.
 * A trailing "(compressed)" is dropped: the Compressed badge says it
 * ("Whisper Small (English) (compressed)" -> "Whisper Small" + "English").
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
  'en-US': 'English (US)',
  'en-GB': 'English (UK)',
  de: 'German',
  fr: 'French',
  es: 'Spanish',
}

export function describeLanguages(languages: string[]) {
  const real = languages.filter((l) => l !== 'auto')
  const first = real[0]
  if (real.length === 1 && first) return LANGUAGE_NAMES[first] ?? first
  return `${real.length} languages`
}

/** "MIT (model) / GPL-3.0 (espeak-ng-data)" -> "MIT + GPL-3.0" */
export function shortLicense(license: string) {
  return license.replace(/\s*\([^)]*\)/g, '').replace(/\s*\/\s*/g, ' + ')
}

export const isRecommended = (m: CatalogModel) => m.tags.includes('recommended')

/** Quantized (q5) variants: smaller download, less memory, nearly the same accuracy. */
export const isCompressed = (m: Pick<CatalogModel, 'id' | 'tags'>) =>
  m.tags.includes('quantized') || m.tags.includes('low-memory') || /-q\d+(_\d+)?$/.test(m.id)

export const COMPRESSED_HINT = 'Compressed · uses less memory'

/** The compressed variant of `model` in `catalog` (e.g. whisper-small -> whisper-small-q5), if any. */
export const compressedAlternative = (model: CatalogModel, catalog: CatalogModel[]) =>
  isCompressed(model) ? null : (catalog.find((m) => m.id.startsWith(`${model.id}-q`) && isCompressed(m)) ?? null)

const TAG_LABELS: Record<string, string> = {
  'gpu-required': 'Needs a GPU',
  'gpu-recommended': 'Best with GPU',
  'best-quality': 'Best quality',
  accurate: 'Accurate',
  fast: 'Fast',
}

export const tagLabels = (m: CatalogModel) =>
  Object.keys(TAG_LABELS)
    .filter((tag) => m.tags.includes(tag))
    .map((tag) => TAG_LABELS[tag] ?? tag)

/** Recommended first, catalog order otherwise. */
export const sortModels = (models: CatalogModel[]) =>
  [...models].sort((a, b) => Number(isRecommended(b)) - Number(isRecommended(a)))

const BACKEND_LABELS: Record<Backend, string> = {
  metal: 'Metal',
  cuda: 'CUDA',
  vulkan: 'Vulkan',
  cpu: 'CPU',
}

export const backendLabel = (backend: Backend) => BACKEND_LABELS[backend]

export const isAccelerated = (hw: HardwareInfo) => hw.recommendedBackend !== 'cpu'

/** The GPU doing the work, or the first one found. */
export function primaryGpu(hw: HardwareInfo) {
  const preferred =
    hw.recommendedBackend === 'cuda'
      ? hw.gpus.find((g) => g.vendor === 'nvidia')
      : hw.recommendedBackend === 'metal'
        ? hw.gpus.find((g) => g.vendor === 'apple')
        : hw.gpus.find((g) => g.vendor !== 'unknown')
  return preferred ?? hw.gpus[0] ?? null
}

/** "Intel(R) Core(TM) i7-9750H CPU @ 2.60GHz" -> "Intel Core i7-9750H" */
export function shortCpuName(name: string) {
  return name
    .replace(/\((R|TM|C)\)/gi, '')
    .replace(/\s+CPU\s*@.*$/i, '')
    .replace(/\s+\d+-Core Processor$/i, '')
    .replace(/\s+/g, ' ')
    .trim()
}

export function formatSpeed(bytesPerSec: number) {
  return `${formatBytes(bytesPerSec)}/s`
}
