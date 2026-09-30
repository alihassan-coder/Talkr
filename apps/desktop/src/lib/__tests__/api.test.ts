import { describe, expect, it, vi } from 'vitest'
import * as api from '@/lib/api'
import { emitEvent, historyFixture, mockBackend } from '@/test/tauri'

type Case = [name: string, run: () => Promise<unknown>, cmd: string, args: Record<string, unknown>]

const cases: Case[] = [
  ['getHardwareInfo', () => api.getHardwareInfo(), 'get_hardware_info', {}],
  ['getEngineStatus', () => api.getEngineStatus(), 'get_engine_status', {}],
  ['getAppPaths', () => api.getAppPaths(), 'get_app_paths', {}],
  ['getSettings', () => api.getSettings(), 'get_settings', {}],
  ['updateSettings', () => api.updateSettings({ settings: { device: 'gpu' } }), 'update_settings', { settings: { device: 'gpu' } }],
  ['openDataFolder', () => api.openDataFolder(), 'open_data_folder', {}],
  ['getStorageUsage', () => api.getStorageUsage(), 'get_storage_usage', {}],
  ['runRetention', () => api.runRetention(), 'run_retention', {}],
  ['listCatalog', () => api.listCatalog(), 'list_catalog', {}],
  ['listInstalledModels', () => api.listInstalledModels(), 'list_installed_models', {}],
  ['downloadModel', () => api.downloadModel({ modelId: 'whisper-base' }), 'download_model', { modelId: 'whisper-base' }],
  ['cancelDownload', () => api.cancelDownload({ jobId: 'j1' }), 'cancel_download', { jobId: 'j1' }],
  ['deleteModel', () => api.deleteModel({ modelId: 'm' }), 'delete_model', { modelId: 'm' }],
  ['importLocalModel', () => api.importLocalModel({ path: '/x.bin', kind: 'stt' }), 'import_local_model', { path: '/x.bin', kind: 'stt' }],
  ['listVoices', () => api.listVoices({ modelId: 'kokoro' }), 'list_voices', { modelId: 'kokoro' }],
  [
    'synthesize',
    () => api.synthesize({ text: 'hi', modelId: 'kokoro', voiceId: 'af', speed: 1.2 }),
    'synthesize',
    { text: 'hi', modelId: 'kokoro', voiceId: 'af', speed: 1.2 },
  ],
  ['startRecording', () => api.startRecording(), 'start_recording', {}],
  ['stopRecording', () => api.stopRecording(), 'stop_recording', {}],
  ['getInputLevel', () => api.getInputLevel(), 'get_input_level', {}],
  [
    'transcribeFile',
    () => api.transcribeFile({ path: 'a.wav', modelId: 'w', language: 'de', translate: true }),
    'transcribe_file',
    { path: 'a.wav', modelId: 'w', language: 'de', translate: true },
  ],
  ['cancelJob', () => api.cancelJob({ jobId: 'j2' }), 'cancel_job', { jobId: 'j2' }],
  [
    'historyList (defaults)',
    () => api.historyList(),
    'history_list',
    { cursor: null, query: null, kind: null, favoritesOnly: false },
  ],
  [
    'historyList (filters)',
    () => api.historyList({ cursor: 50, query: 'plan', kind: 'stt', favoritesOnly: true }),
    'history_list',
    { cursor: 50, query: 'plan', kind: 'stt', favoritesOnly: true },
  ],
  ['historyGet', () => api.historyGet({ id: 'h' }), 'history_get', { id: 'h' }],
  ['historyDelete', () => api.historyDelete({ id: 'h' }), 'history_delete', { id: 'h' }],
  ['historyToggleFavorite', () => api.historyToggleFavorite({ id: 'h' }), 'history_toggle_favorite', { id: 'h' }],
  ['historyExport', () => api.historyExport({ id: 'h', format: 'srt' }), 'history_export', { id: 'h', format: 'srt' }],
  ['historyClear', () => api.historyClear(), 'history_clear', {}],
]

describe('api command wrappers', () => {
  it.each(cases)('%s calls the right command with the right args', async (_name, run, cmd, args) => {
    const backend = mockBackend({ [cmd]: 'ok' })
    await expect(run()).resolves.toBe('ok')
    expect(backend.calls).toEqual([{ cmd, args }])
  })

  it('rejects with the backend error string', async () => {
    mockBackend({ get_settings: () => Promise.reject('config.json is broken') })
    await expect(api.getSettings()).rejects.toBe('config.json is broken')
  })

  it('isTauri reflects the Tauri internals', () => {
    expect(api.isTauri()).toBe(false)
    mockBackend()
    expect(api.isTauri()).toBe(true)
  })
})

describe('api events', () => {
  it.each([
    ['onDownloadProgress', api.onDownloadProgress, 'download://progress'],
    ['onJobProgress', api.onJobProgress, 'job://progress'],
    ['onJobError', api.onJobError, 'job://error'],
    ['onTtsDone', api.onTtsDone, 'tts://done'],
    ['onSttDone', api.onSttDone, 'stt://done'],
    ['onMicLevel', api.onMicLevel, 'mic://level'],
  ] as const)('%s delivers the payload of %s', async (_name, subscribe, event) => {
    mockBackend()
    const cb = vi.fn()
    await (subscribe as (cb: (p: unknown) => void) => Promise<() => void>)(cb)
    await emitEvent(event, { hello: 'world' })
    expect(cb).toHaveBeenCalledWith({ hello: 'world' })
  })

  it('exposes the event names', () => {
    expect(api.EVENTS).toEqual({
      downloadProgress: 'download://progress',
      jobProgress: 'job://progress',
      jobError: 'job://error',
      ttsDone: 'tts://done',
      sttDone: 'stt://done',
      micLevel: 'mic://level',
    })
  })
})

describe('api helpers', () => {
  it('parses transcript segments and tolerates bad JSON', () => {
    const segments = [{ startMs: 0, endMs: 10, text: 'hi' }]
    expect(api.parseSegments(historyFixture({ id: 'a', segmentsJson: JSON.stringify(segments) }))).toEqual(segments)
    expect(api.parseSegments(historyFixture({ id: 'b', segmentsJson: null }))).toEqual([])
    expect(api.parseSegments(historyFixture({ id: 'c', segmentsJson: '{oops' }))).toEqual([])
  })

  it('builds audio URLs for relative and absolute paths', () => {
    mockBackend()
    expect(api.audioSrc('C:\\home\\', 'audio/a.wav')).toBe(`http://asset.localhost/${encodeURIComponent('C:\\home/audio/a.wav')}`)
    expect(api.audioSrc('C:\\home', 'D:\\music\\b.wav')).toBe(`http://asset.localhost/${encodeURIComponent('D:\\music\\b.wav')}`)
    expect(api.audioSrc('/home/me/.talkr', '/tmp/c.wav')).toBe(`http://asset.localhost/${encodeURIComponent('/tmp/c.wav')}`)
  })
})
