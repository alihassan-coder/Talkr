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
  sttQuality: SttQuality
  speechRate: number
  /** 0 = keep forever */
  historyRetentionDays: number
  saveRecordings: boolean
  dictation: DictationSettings
}

// ---------- dictation ----------

/** A global shortcut: modifiers plus at most one key (a Windows virtual-key code). */
export interface Shortcut {
  ctrl: boolean
  shift: boolean
  alt: boolean
  win: boolean
  key: number | null
  /** How to show `key`, from the keyboard layout when it was recorded. */
  keyLabel: string | null
}

/** auto = hold to talk, tap for hands-free; hold = only while held; toggle = press to start and stop. */
export type ActivationMode = 'auto' | 'hold' | 'toggle'
export type InsertMethod = 'auto' | 'paste' | 'type'
export type AppMethod = 'auto' | 'paste' | 'type' | 'off'
/** When focus moved during dictation: go back to the starting field, use the current one, or only copy. */
export type FocusPolicy = 'original' | 'current' | 'copy'
export type OverlayPosition = 'bottom' | 'top'

export interface Replacement {
  from: string
  to: string
}

export interface AppRule {
  /** Executable name, lowercase ("slack.exe"). */
  app: string
  method: AppMethod
  /** Added by Talkr after pasting failed and typing worked. */
  learned: boolean
}

export interface DictationSettings {
  enabled: boolean
  shortcut: Shortcut
  mode: ActivationMode
  pasteLastEnabled: boolean
  pasteLastShortcut: Shortcut
  /** null = the default transcription model */
  model: string | null
  /** null = the transcription language setting */
  language: string | null
  keepWarm: boolean
  /** Microphone id; null = system default. Used by every recording. */
  microphone: string | null
  insertMethod: InsertMethod
  focusPolicy: FocusPolicy
  restoreClipboard: boolean
  smartSpacing: boolean
  removeFillers: boolean
  voiceCommands: boolean
  vocabulary: string[]
  replacements: Replacement[]
  appRules: AppRule[]
  overlayPosition: OverlayPosition
  showTarget: boolean
  sounds: boolean
  saveHistory: boolean
  launchAtLogin: boolean
  closeToTray: boolean
}

/** What dictation can do on this system (mirrors `backend::Capabilities`). */
export interface DictationCapabilities {
  os: 'windows' | 'macos' | 'linux'
  supported: boolean
  /** The shortcut's release is reported, so hold-to-talk works. */
  holdToTalk: boolean
  /** Shortcuts of modifiers alone (Ctrl + Win) work. */
  modifierOnly: boolean
  /** Talkr records the shortcut itself (false: the system's settings choose it). */
  recordsShortcut: boolean
  verifiesInsertion: boolean
  /** Text is typed into other apps (false: copied for the user to paste). */
  insertsText: boolean
  /** What the Win key is called here: "Win", "⌘" or "Super". */
  metaKey: string
  note: string | null
}

/** Something the system must still allow (mirrors `backend::Permission`). */
export type DictationPermission =
  | { state: 'notNeeded' }
  | { state: 'granted' }
  | { state: 'missing'; title: string; detail: string; canRequest: boolean }

export interface DictationStatus {
  /** Dictation works on this system. */
  supported: boolean
  capabilities: DictationCapabilities
  permission: DictationPermission
  /** The shortcut is being listened for. */
  active: boolean
  error: string | null
  /** The model dictation will use, if one is installed. */
  modelId: string | null
  /** That model is loaded and ready. */
  warm: boolean
  hasLast: boolean
  /** A dictation is recording right now. */
  recording: boolean
}

export interface Microphone {
  id: string
  name: string
  isDefault: boolean
}

/** `dictation://done`: what was dictated and whether it was typed in. */
export interface DictationDone {
  text: string
  inserted: boolean
  method: 'direct' | 'paste' | 'type' | null
  app: string | null
}

/** 'auto' = accurate for imported files, fast for in-app recordings. */
export type SttQuality = 'auto' | 'fast' | 'accurate'

/** Patch for `update_settings`. Omitted (or null) fields are left unchanged. */
export interface PartialSettings {
  device?: DevicePreference
  cpuThreads?: number
  defaultTtsModel?: string
  defaultVoice?: string
  defaultSttModel?: string
  sttLanguage?: string
  sttQuality?: SttQuality
  speechRate?: number
  historyRetentionDays?: number
  saveRecordings?: boolean
  /** Replaces the dictation settings as a whole. */
  dictation?: DictationSettings
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

/** Formats the backend writes itself (`history_export`). */
export type ExportFormat = 'txt' | 'md' | 'srt' | 'vtt' | 'json' | 'csv' | 'wav' | 'flac'

/** Every format the app can save: backend formats plus MP3, which the webview encodes. */
export type SaveFormat = ExportFormat | 'mp3'

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
