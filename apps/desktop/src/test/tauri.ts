import { act } from '@testing-library/react'
import { emit } from '@tauri-apps/api/event'
import { mockConvertFileSrc, mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import type { InvokeArgs } from '@tauri-apps/api/core'
import type {
  AppPaths,
  CatalogModel,
  HardwareInfo,
  HistoryItem,
  InstalledModel,
  Settings,
} from '@/lib/types'

export type Call = { cmd: string; args: Record<string, unknown> }
type Handler = unknown | ((args: Record<string, unknown>) => unknown)

/**
 * Fake Tauri backend built on the official `mockIPC`, with event mocking on so the
 * app's `listen` subscriptions receive what `emitEvent` sends. Every command call is
 * recorded; an unmocked command rejects, which makes a missing stub obvious.
 */
export function mockBackend(handlers: Record<string, Handler> = {}) {
  const calls: Call[] = []
  mockWindows('main')
  mockConvertFileSrc('windows')
  mockIPC(
    (cmd: string, payload?: InvokeArgs) => {
      const args = (payload ?? {}) as Record<string, unknown>
      calls.push({ cmd, args })
      if (!(cmd in handlers)) return Promise.reject(`unmocked command: ${cmd}`)
      const handler = handlers[cmd]
      return typeof handler === 'function' ? (handler as (a: Record<string, unknown>) => unknown)(args) : handler
    },
    { shouldMockEvents: true },
  )
  return {
    calls,
    /** Arguments of every call to `cmd`, in order. */
    argsOf: (cmd: string) => calls.filter((c) => c.cmd === cmd).map((c) => c.args),
    count: (cmd: string) => calls.filter((c) => c.cmd === cmd).length,
  }
}

/** Emit a backend event and let React process it. */
export async function emitEvent(event: string, payload: unknown) {
  await act(async () => {
    await emit(event, payload)
  })
}

/** Wait until the app has subscribed to `event` (listen is async). */
export async function flush() {
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0))
  })
}

/** A command that rejects the way the backend does: with a plain string. */
export const reject = (message: string) => () => Promise.reject(message)

// ---------- fixtures ----------

export const settingsFixture = (patch: Partial<Settings> = {}): Settings => ({
  version: 1,
  device: 'auto',
  cpuThreads: 0,
  defaultTtsModel: null,
  defaultVoice: null,
  defaultSttModel: null,
  sttLanguage: 'auto',
  speechRate: 1,
  historyRetentionDays: 0,
  saveRecordings: true,
  ...patch,
})

export const pathsFixture: AppPaths = {
  home: 'C:\\Users\\me\\.talkr',
  models: 'C:\\Users\\me\\.talkr\\models',
  modelsStt: 'C:\\Users\\me\\.talkr\\models\\stt',
  modelsTts: 'C:\\Users\\me\\.talkr\\models\\tts',
  audio: 'C:\\Users\\me\\.talkr\\audio',
  history: 'C:\\Users\\me\\.talkr\\history',
  cache: 'C:\\Users\\me\\.talkr\\cache',
  cacheDownloads: 'C:\\Users\\me\\.talkr\\cache\\downloads',
  logs: 'C:\\Users\\me\\.talkr\\logs',
  configFile: 'C:\\Users\\me\\.talkr\\config.json',
  dbFile: 'C:\\Users\\me\\.talkr\\talkr.db',
}

export const hardwareFixture = (patch: Partial<HardwareInfo> = {}): HardwareInfo => ({
  os: 'windows',
  arch: 'x86_64',
  cpuName: 'Intel(R) Core(TM) i7-9750H CPU @ 2.60GHz',
  cpuCores: 6,
  ramBytes: 16 * 1024 ** 3,
  gpus: [{ name: 'NVIDIA GeForce RTX 3060', vendor: 'nvidia', vramBytes: 12 * 1024 ** 3 }],
  recommendedBackend: 'vulkan',
  ...patch,
})

export const installedFixture = (patch: Partial<InstalledModel> & Pick<InstalledModel, 'id' | 'kind'>): InstalledModel => ({
  name: patch.id,
  engine: patch.kind === 'stt' ? 'whisper' : 'sherpa-onnx',
  path: `C:\\Users\\me\\.talkr\\models\\${patch.kind}\\${patch.id}`,
  manifest: { id: patch.id, version: '1', sha256: 'abc', installedAt: 0, sizeBytes: 1000, files: [] },
  ...patch,
})

export const catalogFixture = (patch: Partial<CatalogModel> & Pick<CatalogModel, 'id'>): CatalogModel => ({
  kind: 'stt',
  engine: 'whisper',
  name: patch.id,
  description: `${patch.id} description`,
  languages: ['en'],
  sizeBytes: 150_000_000,
  ramRecommendedBytes: 1_000_000_000,
  license: 'MIT',
  homepage: 'https://example.com',
  files: [],
  tags: [],
  installed: false,
  installedVersion: null,
  ...patch,
})

export const historyFixture = (patch: Partial<HistoryItem> & Pick<HistoryItem, 'id'>): HistoryItem => ({
  kind: 'stt',
  createdAt: Date.now() - 60_000 * 5,
  title: `Item ${patch.id}`,
  text: `Text of ${patch.id}`,
  audioPath: null,
  durationMs: 12_000,
  modelId: 'whisper-base-en',
  voiceId: null,
  language: 'en',
  device: 'cpu',
  processingMs: 900,
  favorite: false,
  segmentsJson: null,
  ...patch,
})
