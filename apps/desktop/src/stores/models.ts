import { create } from 'zustand'
import {
  cancelDownload,
  deleteModel,
  downloadModel,
  getHardwareInfo,
  getStorageUsage,
  importLocalModel,
  isTauri,
  listCatalog,
  listInstalledModels,
  onDownloadProgress,
} from '@/lib/api'
import type {
  CatalogModel,
  DownloadProgress,
  DownloadState,
  HardwareInfo,
  InstalledModel,
  ModelKind,
} from '@/lib/types'
import { toast, toastError } from '@/stores/toast'
import sampleCatalog from '../../../../packages/model-catalog/catalog.json'

export type ActiveDownload = {
  jobId: string | null
  /** 0..1, null while unknown (starting, verifying, extracting) */
  progress: number | null
  bytesDone: number
  bytesTotal: number
  speed: number
  state: DownloadState
}

type ModelsState = {
  catalog: CatalogModel[]
  /** Everything on disk, including locally imported models that are not in the catalog. */
  installed: InstalledModel[]
  installedIds: string[]
  downloads: Record<string, ActiveDownload>
  hardware: HardwareInfo | null
  modelsBytes: number | null
  loading: boolean
  loaded: boolean
  error: string | null
  refresh: () => Promise<void>
  download: (modelId: string) => Promise<void>
  cancel: (modelId: string) => Promise<void>
  remove: (modelId: string) => Promise<void>
  importModel: (path: string, kind: ModelKind) => Promise<void>
}

/** Bundled catalog, so the screen can be previewed in a plain browser. */
const previewCatalog = (): CatalogModel[] =>
  sampleCatalog.models.map((m) => ({
    ...m,
    kind: m.kind as ModelKind,
    installed: false,
    installedVersion: null,
  }))

const errorMessage = (e: unknown) => (e instanceof Error ? e.message : String(e))

// Jobs the user cancelled: late progress events for them are ignored.
const cancelledJobs = new Set<string>()

export const useModels = create<ModelsState>((set, get) => {
  const nameOf = (modelId: string) => get().catalog.find((m) => m.id === modelId)?.name ?? modelId

  const dropDownload = (modelId: string) =>
    set((s) => {
      const downloads = { ...s.downloads }
      delete downloads[modelId]
      return { downloads }
    })

  return {
    catalog: [],
    installed: [],
    installedIds: [],
    downloads: {},
    hardware: null,
    modelsBytes: null,
    loading: false,
    loaded: false,
    error: null,

    refresh: async () => {
      if (!isTauri()) {
        set({ catalog: previewCatalog(), loaded: true })
        return
      }
      set({ loading: true, error: null })
      try {
        const [catalog, installed, hardware, storage] = await Promise.all([
          listCatalog(),
          listInstalledModels(),
          get().hardware ? Promise.resolve(get().hardware) : getHardwareInfo(),
          getStorageUsage().catch(() => null),
        ])
        set({
          catalog,
          installed,
          installedIds: installed.map((m) => m.id),
          hardware,
          modelsBytes: storage?.modelsBytes ?? null,
          loading: false,
          loaded: true,
        })
      } catch (e) {
        set({ loading: false, loaded: true, error: errorMessage(e) })
      }
    },

    download: async (modelId) => {
      if (!isTauri()) {
        toast('Downloads work in the Talkr desktop app')
        return
      }
      if (get().downloads[modelId]) return
      set((s) => ({
        downloads: {
          ...s.downloads,
          [modelId]: { jobId: null, progress: null, bytesDone: 0, bytesTotal: 0, speed: 0, state: 'queued' },
        },
      }))
      try {
        const jobId = await downloadModel({ modelId })
        set((s) => {
          const current = s.downloads[modelId]
          return current ? { downloads: { ...s.downloads, [modelId]: { ...current, jobId } } } : {}
        })
      } catch (e) {
        dropDownload(modelId)
        toastError(e)
      }
    },

    cancel: async (modelId) => {
      const job = get().downloads[modelId]
      if (!job) return
      if (job.jobId) cancelledJobs.add(job.jobId)
      dropDownload(modelId)
      try {
        // The backend also accepts a model id, which covers a job id that has not arrived yet.
        await cancelDownload({ jobId: job.jobId ?? modelId })
      } catch {
        // Already finished or never started: nothing to cancel.
      }
    },

    remove: async (modelId) => {
      const name = nameOf(modelId)
      try {
        await deleteModel({ modelId })
        toast(`${name} removed`)
      } catch (e) {
        toastError(e)
      }
      await get().refresh()
    },

    importModel: async (path, kind) => {
      try {
        const model = await importLocalModel({ path, kind })
        toast(`${model.name} imported`)
        await get().refresh()
      } catch (e) {
        toastError(e)
      }
    },
  }
})

function handleProgress(p: DownloadProgress) {
  if (cancelledJobs.has(p.jobId)) {
    if (p.state === 'cancelled' || p.state === 'failed' || p.state === 'installed') cancelledJobs.delete(p.jobId)
    return
  }
  const { getState, setState } = useModels
  const name = getState().catalog.find((m) => m.id === p.modelId)?.name ?? p.modelId

  const drop = () =>
    setState((s) => {
      const downloads = { ...s.downloads }
      delete downloads[p.modelId]
      return { downloads }
    })

  switch (p.state) {
    case 'installed':
      drop()
      toast(`${name} is ready`)
      void getState().refresh()
      return
    case 'failed':
      drop()
      toastError(p.error ? `${name}: ${p.error}` : `${name} failed to download`)
      return
    case 'cancelled':
      drop()
      return
    default: {
      const measurable = p.state === 'downloading' && p.totalBytes > 0
      setState((s) => ({
        downloads: {
          ...s.downloads,
          [p.modelId]: {
            jobId: p.jobId,
            progress: measurable ? p.receivedBytes / p.totalBytes : null,
            bytesDone: p.receivedBytes,
            bytesTotal: p.totalBytes,
            speed: p.bytesPerSec,
            state: p.state,
          },
        },
      }))
    }
  }
}

let initialized = false

/** Load models + hardware and subscribe to download events. Safe to call many times. */
export function initModels() {
  if (initialized) return
  initialized = true
  if (isTauri()) {
    onDownloadProgress(handleProgress).catch((e: unknown) => {
      initialized = false
      toastError(e)
    })
  }
  void useModels.getState().refresh()
}
