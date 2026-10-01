import { create } from 'zustand'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { cancelJob, isTauri, onJobError, onJobProgress, onSttDone, onTtsDone } from '@/lib/api'
import { errorText } from '@/lib/errors'
import type { HistoryItem } from '@/lib/types'
import { toast, toastError } from '@/stores/toast'

export type JobKind = 'tts' | 'stt'

type Outcome = { ok: true; item: HistoryItem } | { ok: false; error: string; cancelled: boolean }

export type JobResult = {
  item: HistoryItem
  /** What the page called the job when it started (e.g. the file name). */
  label: string
  /** Increases with every finished job, so pages can tell a new result from one they already showed. */
  seq: number
}

export type JobSlot = {
  running: boolean
  /** null while the command has not returned the job id yet. */
  jobId: string | null
  /** 0..1, null while unknown */
  progress: number | null
  /** Why the last job failed, until the next start or `dismissError`. */
  error: string | null
  label: string
  /** The user pressed Cancel; sent as soon as the job id is known. */
  cancelRequested: boolean
  result: JobResult | null
}

type JobsState = Record<JobKind, JobSlot> & {
  /** Start a job unless another heavy job runs. Resolves once the command returned (or failed). */
  start: (kind: JobKind, launch: () => Promise<string>, options?: { label?: string; clearResult?: boolean }) => Promise<void>
  cancel: (kind: JobKind) => Promise<void>
  dismissError: (kind: JobKind) => void
}

const idle = (): JobSlot => ({
  running: false,
  jobId: null,
  progress: null,
  error: null,
  label: '',
  cancelRequested: false,
  result: null,
})

const KINDS: JobKind[] = ['tts', 'stt']
const NAMES: Record<JobKind, string> = { tts: 'speech generation', stt: 'the transcription' }

/** Why a new `kind` job cannot start right now, or null when it can. One heavy job runs at a time. */
export function busyReason(state: Record<JobKind, JobSlot>, kind: JobKind): string | null {
  const other = kind === 'tts' ? 'stt' : 'tts'
  return state[other].running ? `Wait for ${NAMES[other]} to finish.` : null
}

let seq = 0
// Outcomes that arrived before the command resolved with its job id.
const early = new Map<string, Outcome>()

export const useJobs = create<JobsState>((set, get) => {
  const patch = (kind: JobKind, next: Partial<JobSlot>) => set((s) => ({ [kind]: { ...s[kind], ...next } }) as Partial<JobsState>)

  const sendCancel = async (jobId: string) => {
    try {
      await cancelJob({ jobId })
    } catch (err) {
      toastError(err)
    }
  }

  return {
    tts: idle(),
    stt: idle(),

    start: async (kind, launch, options = {}) => {
      const state = get()
      if (state[kind].running || busyReason(state, kind)) return
      patch(kind, {
        running: true,
        jobId: null,
        progress: null,
        error: null,
        label: options.label ?? '',
        cancelRequested: false,
        ...(options.clearResult ? { result: null } : {}),
      })
      early.clear()
      await listening()
      try {
        const jobId = await launch()
        const outcome = early.get(jobId)
        early.delete(jobId)
        if (outcome) {
          settle(kind, outcome)
          return
        }
        patch(kind, { jobId })
        if (get()[kind].cancelRequested) await sendCancel(jobId)
      } catch (err) {
        // Rejected before a job existed, e.g. "Not enough free memory to run …".
        patch(kind, { running: false, jobId: null, cancelRequested: false, error: errorText(err) })
      }
    },

    cancel: async (kind) => {
      const slot = get()[kind]
      if (!slot.running || slot.cancelRequested) return
      patch(kind, { cancelRequested: true })
      // Without an id yet, `start` sends the cancel once the command returns it.
      if (slot.jobId) await sendCancel(slot.jobId)
    },

    dismissError: (kind) => patch(kind, { error: null }),
  }
})

function settle(kind: JobKind, outcome: Outcome) {
  const slot = useJobs.getState()[kind]
  const done = { running: false, jobId: null, progress: null, cancelRequested: false }
  if (outcome.ok) {
    seq += 1
    useJobs.setState({ [kind]: { ...slot, ...done, result: { item: outcome.item, label: slot.label, seq } } })
  } else {
    useJobs.setState({ [kind]: { ...slot, ...done, error: outcome.cancelled ? null : errorText(outcome.error) } })
    if (outcome.cancelled) toast('Cancelled')
  }
}

function handleOutcome(jobId: string, outcome: Outcome) {
  const state = useJobs.getState()
  const kind = KINDS.find((k) => state[k].jobId === jobId)
  if (kind) settle(kind, outcome)
  else if (KINDS.some((k) => state[k].running && state[k].jobId === null)) early.set(jobId, outcome)
}

function handleProgress(jobId: string, value: number) {
  const state = useJobs.getState()
  const kind = KINDS.find((k) => state[k].jobId === jobId)
  if (kind) useJobs.setState({ [kind]: { ...state[kind], progress: value > 0 ? Math.min(1, value) : null } })
}

let subscription: Promise<UnlistenFn[]> | null = null

/** Subscribe to job events for the lifetime of the app. Safe to call many times. */
export function initJobs() {
  if (subscription || !isTauri()) return
  subscription = Promise.all([
    onJobProgress((e) => handleProgress(e.jobId, e.progress)),
    onJobError((e) => handleOutcome(e.jobId, { ok: false, error: e.error, cancelled: e.cancelled })),
    onTtsDone((e) => handleOutcome(e.jobId, { ok: true, item: e.historyItem })),
    onSttDone((e) => handleOutcome(e.jobId, { ok: true, item: e.historyItem })),
  ])
  subscription.catch((err: unknown) => {
    subscription = null
    toastError(err)
  })
}

/** Resolves once the event listeners are in place, so no outcome can be missed. */
async function listening() {
  initJobs()
  await subscription?.catch(() => {})
}

/** Test helper: drop all jobs and listeners. */
export async function resetJobs() {
  const current = subscription
  subscription = null
  early.clear()
  useJobs.setState({ tts: idle(), stt: idle() })
  if (current) {
    const unlisteners = await current.catch(() => [] as UnlistenFn[])
    for (const unlisten of unlisteners) {
      try {
        await unlisten()
      } catch {
        // The mocked event system may already be gone.
      }
    }
  }
}
