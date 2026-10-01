// Types mirroring the serde-serialized structs of the Rust backend (src-tauri/src).
// Keep in sync with the Rust definitions; field casing follows each struct's `rename_all`.

// ---------- system / settings ----------

export type Backend = 'metal' | 'cuda' | 'vulkan' | 'cpu'
export type GpuVendor = 'nvidia' | 'amd' | 'intel' | 'apple' | 'unknown'

export interface GpuInfo {
  name: string
  vendor: GpuVendor
  vramBytes: number | null
}

export interface HardwareInfo {
  os: string
  arch: string
  cpuName: string
  cpuCores: number
  ramBytes: number
  gpus: GpuInfo[]
  recommendedBackend: Backend
}

export interface AppPaths {
  home: string
  models: string
  modelsStt: string
  modelsTts: string
  audio: string
  history: string
  cache: string
  cacheDownloads: string
  logs: string
  configFile: string
  dbFile: string
}

export type DevicePreference = 'auto' | 'gpu' | 'cpu'

export interface Settings {
  version: number
  device: DevicePreference
  /** 0 = auto (physical cores, max 8) */
  cpuThreads: number
  defaultTtsModel: string | null
  defaultVoice: string | null
  defaultSttModel: string | null
  /** 'auto' or an ISO language code */
  sttLanguage: string
  speechRate: number
  /** 0 = keep forever */
  historyRetentionDays: number
  saveRecordings: boolean
}

/** Patch for `update_settings`. Omitted (or null) fields are left unchanged. */
export interface PartialSettings {
  device?: DevicePreference
  cpuThreads?: number
  defaultTtsModel?: string
  defaultVoice?: string
  defaultSttModel?: string
  sttLanguage?: string
  speechRate?: number
  historyRetentionDays?: number
  saveRecordings?: boolean
}

/** Kind of compute device the speech engine found. */
export type EngineDeviceKind = 'cpu' | 'gpu' | 'igpu' | 'accelerator'

export interface EngineDevice {
  kind: EngineDeviceKind
  /** Backend name, e.g. "Vulkan0", "CUDA0" or "Metal". */
  name: string
  /** Human name, e.g. "NVIDIA GeForce RTX 3060". */
  description: string
  memoryFree: number
  memoryTotal: number
}

/** What the speech engine can run on (get_engine_status). */
export interface EngineStatus {
  /** Devices the engine reported; includes the CPU once the engine runs. */
  devices: EngineDevice[]
  /** A GPU build (or Metal) is installed and has not failed. */
  gpuAvailable: boolean
  /** The GPU engine crashed or could not start, so speech to text fell back to the CPU. */
  gpuFailed: boolean
  /** Whether speech to text uses the GPU with the current setting. */
  sttUsesGpu: boolean
}

export interface StorageUsage {
  modelsBytes: number
  audioBytes: number
  dbBytes: number
  totalBytes: number
}

// ---------- models ----------

export type ModelKind = 'stt' | 'tts'

export interface ModelFile {
  url: string
  sha256: string
  /** e.g. 'tar.bz2'; absent for single-file models */
  archive?: string
}

export interface CatalogModel {
  id: string
  kind: ModelKind
  /** 'whisper' | 'sherpa-onnx' */
  engine: string
  name: string
  description: string
  languages: string[]
  sizeBytes: number
  ramRecommendedBytes: number
  license: string
  homepage: string
  files: ModelFile[]
  tags: string[]
  installed: boolean
  installedVersion: string | null
}

export interface ModelManifest {
  id: string
  version: string
  sha256: string
  /** unix ms */
  installedAt: number
  sizeBytes: number
  files: string[]
}

export interface InstalledModel {
  id: string
  kind: ModelKind
  name: string
  /** 'whisper' | 'sherpa-onnx' (imported models get the default engine for their kind) */
  engine: string
  /** absolute model directory */
  path: string
  manifest: ModelManifest
}

export type DownloadState =
  | 'queued'
  | 'downloading'
  | 'verifying'
  | 'extracting'
  | 'installed'
  | 'failed'
  | 'cancelled'

export interface DownloadProgress {
  jobId: string
  modelId: string
  receivedBytes: number
  totalBytes: number
  bytesPerSec: number
  etaSec: number | null
  state: DownloadState
  /** set when state === 'failed' */
  error: string | null
}

// ---------- TTS / STT ----------

export interface Voice {
  /** pass as `voiceId` to synthesize */
  id: string
  name: string
  /** BCP-47-ish, may be '' when unknown */
  language: string
  gender: string | null
}

export interface RecordingResult {
  /** relative to the Talkr home; pass to transcribeFile */
  tempAudioPath: string
  durationMs: number
}

export interface TranscriptSegment {
  startMs: number
  endMs: number
  text: string
}

export interface Transcript {
  text: string
  segments: TranscriptSegment[]
  language: string | null
}

// ---------- history ----------

export type HistoryKind = 'tts' | 'stt'

export interface HistoryItem {
  /** equals the jobId that produced it */
  id: string
  kind: HistoryKind
  /** unix ms */
  createdAt: number
  title: string
  text: string
  /** relative to the Talkr home (or absolute for transcribed external files) */
  audioPath: string | null
  durationMs: number | null
  modelId: string
  voiceId: string | null
  language: string | null
  device: string
  processingMs: number
  favorite: boolean
  /** JSON-encoded TranscriptSegment[] (STT only); see parseSegments in api.ts */
  segmentsJson: string | null
}

export interface HistoryListResult {
  items: HistoryItem[]
  /** pass back as `cursor` to fetch the next page; null when there are no more */
  nextCursor: number | null
}

export interface HistoryListParams {
  cursor?: number | null
  /** full-text search (prefix match on words) */
  query?: string | null
  kind?: HistoryKind | null
  favoritesOnly?: boolean
}

export type ExportFormat = 'txt' | 'srt' | 'wav'

// ---------- events ----------

export interface JobProgressEvent {
  jobId: string
  /** 0..1 */
  progress: number
}

export interface JobDoneEvent {
  jobId: string
  historyItem: HistoryItem
}

export interface JobErrorEvent {
  jobId: string
  error: string
  cancelled: boolean
}

export interface MicErrorEvent {
  message: string
}
