import { beforeEach, describe, expect, it, vi } from 'vitest'
import { catalogFixture, emitEvent, flush, hardwareFixture, installedFixture, mockBackend, reject } from '@/test/tauri'

// The store keeps module state (subscription flag, cancelled jobs), so each test gets a fresh copy.
const load = async () => {
  vi.resetModules()
  const models = await import('@/stores/models')
  const toasts = await import('@/stores/toast')
  return { ...models, ...toasts }
}

const catalog = [catalogFixture({ id: 'whisper-base', name: 'Whisper Base' })]
const progress = (patch: Record<string, unknown>) => ({
  jobId: 'job-1',
  modelId: 'whisper-base',
  receivedBytes: 0,
  totalBytes: 0,
  bytesPerSec: 0,
  etaSec: null,
  state: 'downloading',
  error: null,
  ...patch,
})

let backendHandlers: Record<string, unknown>
beforeEach(() => {
  backendHandlers = {
    list_catalog: catalog,
    list_installed_models: [],
    get_hardware_info: hardwareFixture(),
    get_storage_usage: { modelsBytes: 5, audioBytes: 0, dbBytes: 0, totalBytes: 5 },
    download_model: 'job-1',
    cancel_download: null,
    delete_model: null,
    import_local_model: installedFixture({ id: 'mine', kind: 'stt', name: 'My model' }),
  }
})

describe('models store', () => {
  it('loads the catalog, installed models, hardware and storage', async () => {
    mockBackend(backendHandlers)
    const { useModels } = await load()
    await useModels.getState().refresh()
    expect(useModels.getState()).toMatchObject({ catalog, installedIds: [], modelsBytes: 5, loaded: true, error: null })
  })

  it('records a load error readably', async () => {
    mockBackend({ ...backendHandlers, list_catalog: reject('catalog missing') })
    const { useModels } = await load()
    await useModels.getState().refresh()
    expect(useModels.getState()).toMatchObject({ loaded: true, loading: false, error: 'catalog missing' })
  })

  it('uses the bundled catalog in a plain browser', async () => {
    const { useModels } = await load()
    await useModels.getState().refresh()
    expect(useModels.getState().catalog.length).toBeGreaterThan(5)
    expect(useModels.getState().catalog.some((m) => m.id.endsWith('-q5'))).toBe(true)
  })

  it('tracks download progress through to installed', async () => {
    const backend = mockBackend(backendHandlers)
    const { useModels, initModels, useToasts } = await load()
    initModels()
    await flush()
    await useModels.getState().download('whisper-base')
    expect(backend.argsOf('download_model')).toEqual([{ modelId: 'whisper-base' }])
    expect(useModels.getState().downloads['whisper-base']).toMatchObject({ jobId: 'job-1', state: 'queued' })

    await emitEvent('download://progress', progress({ receivedBytes: 50, totalBytes: 200, bytesPerSec: 10 }))
    expect(useModels.getState().downloads['whisper-base']).toMatchObject({ progress: 0.25, bytesDone: 50, speed: 10 })

    await emitEvent('download://progress', progress({ state: 'verifying', receivedBytes: 200, totalBytes: 200 }))
    expect(useModels.getState().downloads['whisper-base']?.progress).toBeNull()

    await emitEvent('download://progress', progress({ state: 'installed' }))
    expect(useModels.getState().downloads).toEqual({})
    expect(useToasts.getState().toasts.at(-1)?.message).toBe('Whisper Base is ready')
  })

  it('keeps the failure reason for the model', async () => {
    mockBackend(backendHandlers)
    const { useModels, initModels } = await load()
    initModels()
    await flush()
    await useModels.getState().download('whisper-base')
    await emitEvent('download://progress', progress({ state: 'failed', error: 'Not enough disk space to install Whisper Base.' }))
    expect(useModels.getState().downloads).toEqual({})
    expect(useModels.getState().failures).toEqual({ 'whisper-base': 'Not enough disk space to install Whisper Base.' })

    // Retrying clears it.
    await useModels.getState().download('whisper-base')
    expect(useModels.getState().failures).toEqual({})
  })

  it('records a rejected download command as a failure', async () => {
    mockBackend({ ...backendHandlers, download_model: reject('Not enough free memory to run whisper-base.') })
    const { useModels } = await load()
    await useModels.getState().download('whisper-base')
    expect(useModels.getState().downloads).toEqual({})
    expect(useModels.getState().failures['whisper-base']).toBe('Not enough free memory to run whisper-base.')
    useModels.getState().dismissFailure('whisper-base')
    expect(useModels.getState().failures).toEqual({})
  })

  it('cancels a download and ignores its late events', async () => {
    const backend = mockBackend(backendHandlers)
    const { useModels, initModels } = await load()
    initModels()
    await flush()
    await useModels.getState().download('whisper-base')
    await useModels.getState().cancel('whisper-base')
    expect(backend.argsOf('cancel_download')).toEqual([{ jobId: 'job-1' }])
    await emitEvent('download://progress', progress({ receivedBytes: 10, totalBytes: 100 }))
    expect(useModels.getState().downloads).toEqual({})
  })

  it('removes and imports models, then refreshes', async () => {
    const backend = mockBackend(backendHandlers)
    const { useModels, useToasts } = await load()
    await useModels.getState().refresh()
    await useModels.getState().remove('whisper-base')
    expect(backend.argsOf('delete_model')).toEqual([{ modelId: 'whisper-base' }])
    expect(useToasts.getState().toasts.at(-1)?.message).toBe('Whisper Base removed')

    await useModels.getState().importModel('C:\\m.bin', 'stt')
    expect(backend.argsOf('import_local_model')).toEqual([{ path: 'C:\\m.bin', kind: 'stt' }])
    expect(useToasts.getState().toasts.at(-1)?.message).toBe('My model imported')
    expect(backend.count('list_catalog')).toBe(3)
  })
})
