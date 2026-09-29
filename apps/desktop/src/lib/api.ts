// Typed wrappers around the Talkr Tauri commands and events.
// Commands reject with a string error message (the backend's AppError).

import { convertFileSrc, invoke } from '@tauri-apps/api/core'

export { convertFileSrc }
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type {
  AppPaths,
  CatalogModel,
  DownloadProgress,
  ExportFormat,
  HardwareInfo,
  HistoryItem,
  HistoryListParams,
  HistoryListResult,
  InstalledModel,
  JobDoneEvent,
  JobErrorEvent,
  JobProgressEvent,
  ModelKind,
  PartialSettings,
  RecordingResult,
  Settings,
  StorageUsage,
  TranscriptSegment,
  Voice,
} from './types'

/** True when running inside the Tauri webview (false in a plain browser during UI work). */
export const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

// ---------- system / settings ----------

export const getHardwareInfo = () => invoke<HardwareInfo>('get_hardware_info')

export const getAppPaths = () => invoke<AppPaths>('get_app_paths')

export const getSettings = () => invoke<Settings>('get_settings')

/** Merge `patch` into the settings, persist them and return the full updated settings. */
export const updateSettings = (args: { settings: PartialSettings }) => invoke<Settings>('update_settings', args)

/** Open ~/.talkr in the system file manager. */
export const openDataFolder = () => invoke<void>('open_data_folder')

export const getStorageUsage = () => invoke<StorageUsage>('get_storage_usage')

/** Apply the history retention policy now; resolves to the number of deleted items. */
export const runRetention = () => invoke<number>('run_retention')

// ---------- models ----------

export const listCatalog = () => invoke<CatalogModel[]>('list_catalog')

export const listInstalledModels = () => invoke<InstalledModel[]>('list_installed_models')

/** Start a download; resolves to the jobId. Track it with onDownloadProgress. */
export const downloadModel = (args: { modelId: string }) => invoke<string>('download_model', args)

/** Cancel a download by jobId (a modelId is accepted too). */
export const cancelDownload = (args: { jobId: string }) => invoke<void>('cancel_download', args)

export const deleteModel = (args: { modelId: string }) => invoke<void>('delete_model', args)

/** Import a local model file/folder (whisper .bin for 'stt', sherpa-onnx folder for 'tts'). */
export const importLocalModel = (args: { path: string; kind: ModelKind }) =>
  invoke<InstalledModel>('import_local_model', args)

// ---------- TTS ----------

/** Loads the model if needed (may take a few seconds the first time). */
export const listVoices = (args: { modelId: string }) => invoke<Voice[]>('list_voices', args)

/**
 * Start a synthesis job; resolves to the jobId. Result arrives via onTtsDone / onJobError,
 * progress via onJobProgress. `speed` defaults to settings.speechRate.
 */
export const synthesize = (args: { text: string; modelId: string; voiceId: string; speed?: number }) =>
  invoke<string>('synthesize', args)

// ---------- STT ----------

/** Start microphone capture; level updates arrive via onMicLevel. */
export const startRecording = () => invoke<void>('start_recording')

/** Stop capture and save a WAV. Resolves to { tempAudioPath, durationMs }; pass tempAudioPath to transcribeFile. */
export const stopRecording = () => invoke<RecordingResult>('stop_recording')

/** Current mic RMS level (0..1), 0 when not recording. */
export const getInputLevel = () => invoke<number>('get_input_level')

/**
 * Start a transcription job; resolves to the jobId. Result arrives via onSttDone / onJobError.
 * `path` may be absolute or relative to the Talkr home. `language` defaults to settings.sttLanguage, `translate` to false.
 */
export const transcribeFile = (args: {
  path: string
  modelId: string
  language?: string | null
  /** translate the transcript to English (multilingual models only) */
  translate?: boolean
}) =>
  invoke<string>('transcribe_file', args)

/** Cancel a running synthesize/transcribe job (it ends with a job://error event, cancelled: true). */
export const cancelJob = (args: { jobId: string }) => invoke<void>('cancel_job', args)

// ---------- history ----------

/** Newest first, 50 per page. */
export const historyList = (params: HistoryListParams = {}) =>
  invoke<HistoryListResult>('history_list', {
    cursor: params.cursor ?? null,
    query: params.query ?? null,
    kind: params.kind ?? null,
    favoritesOnly: params.favoritesOnly ?? false,
  })

/** Resolves to null when the id does not exist. */
export const historyGet = (args: { id: string }) => invoke<HistoryItem | null>('history_get', args)

export const historyDelete = (args: { id: string }) => invoke<void>('history_delete', args)

/** Resolves to the new favorite value. */
export const historyToggleFavorite = (args: { id: string }) => invoke<boolean>('history_toggle_favorite', args)

/** Opens a native save dialog; resolves to the saved path, or null if the user cancelled. */
export const historyExport = (args: { id: string; format: ExportFormat }) =>
  invoke<string | null>('history_export', args)

/** Delete all non-favorite items; resolves to the number deleted. */
export const historyClear = () => invoke<number>('history_clear')

// ---------- events ----------

export const EVENTS = {
  downloadProgress: 'download://progress',
  jobProgress: 'job://progress',
  jobError: 'job://error',
  ttsDone: 'tts://done',
  sttDone: 'stt://done',
  micLevel: 'mic://level',
} as const

export const onDownloadProgress = (cb: (p: DownloadProgress) => void): Promise<UnlistenFn> =>
  listen<DownloadProgress>(EVENTS.downloadProgress, (e) => cb(e.payload))

export const onJobProgress = (cb: (p: JobProgressEvent) => void): Promise<UnlistenFn> =>
  listen<JobProgressEvent>(EVENTS.jobProgress, (e) => cb(e.payload))

export const onJobError = (cb: (p: JobErrorEvent) => void): Promise<UnlistenFn> =>
  listen<JobErrorEvent>(EVENTS.jobError, (e) => cb(e.payload))

export const onTtsDone = (cb: (p: JobDoneEvent) => void): Promise<UnlistenFn> =>
  listen<JobDoneEvent>(EVENTS.ttsDone, (e) => cb(e.payload))

export const onSttDone = (cb: (p: JobDoneEvent) => void): Promise<UnlistenFn> =>
  listen<JobDoneEvent>(EVENTS.sttDone, (e) => cb(e.payload))

/** Mic RMS level (0..1), ~20 updates/s while recording, then a final 0. */
export const onMicLevel = (cb: (level: number) => void): Promise<UnlistenFn> =>
  listen<number>(EVENTS.micLevel, (e) => cb(e.payload))

// ---------- helpers ----------

export const parseSegments = (item: HistoryItem): TranscriptSegment[] => {
  if (!item.segmentsJson) return []
  try {
    return JSON.parse(item.segmentsJson) as TranscriptSegment[]
  } catch {
    return []
  }
}

/**
 * URL for an <audio> element. `audioPath` is a HistoryItem.audioPath / RecordingResult.tempAudioPath
 * (relative to `home` from getAppPaths). Only files under ~/.talkr/audio are in the asset scope.
 */
export const audioSrc = (home: string, audioPath: string) => {
  const isAbsolute = /^([a-zA-Z]:[\\/]|[\\/])/.test(audioPath)
  const full = isAbsolute ? audioPath : `${home.replace(/[\\/]+$/, '')}/${audioPath}`
  return convertFileSrc(full)
}
