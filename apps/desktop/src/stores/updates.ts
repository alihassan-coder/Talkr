import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { Update } from '@tauri-apps/plugin-updater'
import { isTauri, stopEngine } from '@/lib/api'

/** How often a running app looks for a new release, after the check shortly after launch. */
export const CHECK_EVERY_MS = 6 * 60 * 60 * 1000
/** Launch is busy (models, engine probe); the first check waits a little. */
export const FIRST_CHECK_DELAY_MS = 8_000

export type UpdatePhase =
  | 'idle'
  | 'checking'
  /** The last check found nothing newer. */
  | 'current'
  | 'available'
  | 'downloading'
  | 'installing'
  | 'failed'

type UpdateState = {
  phase: UpdatePhase
  /** The release on offer (set from 'available' on). */
  update: Update | null
  downloaded: number
  total: number | null
  /** Why the last check or install failed. */
  error: string | null
  lastChecked: number | null
  /** The banner was closed for this version until the app restarts. */
  dismissedVersion: string | null
  // Persisted preferences:
  autoCheck: boolean
  /** "Skip this version": never offered again on its own (Settings still shows it). */
  skippedVersion: string | null

  /** Look for a new release. `manual` checks report failures (automatic ones only log them). */
  check: (manual?: boolean) => Promise<void>
  install: () => Promise<void>
  dismiss: () => void
  skip: () => void
  setAutoCheck: (on: boolean) => void
}

/** Update problems never interrupt the user: they go to the log file. */
async function logWarning(message: string) {
  try {
    const { warn } = await import('@tauri-apps/plugin-log')
    await warn(message)
  } catch {
    console.warn(message)
  }
}

const initial = {
  phase: 'idle' as UpdatePhase,
  update: null,
  downloaded: 0,
  total: null,
  error: null,
  lastChecked: null,
  dismissedVersion: null,
}

export const useUpdates = create<UpdateState>()(
  persist(
    (set, get) => ({
      ...initial,
      autoCheck: true,
      skippedVersion: null,

      check: async (manual = false) => {
        const { phase } = get()
        if (phase === 'checking' || phase === 'downloading' || phase === 'installing') return
        set({ phase: 'checking', error: null })
        try {
          const { check } = await import('@tauri-apps/plugin-updater')
          const update = await check()
          set({ update, phase: update ? 'available' : 'current', lastChecked: Date.now() })
        } catch (e) {
          void logWarning(`Update check failed: ${String(e)}`)
          // A quiet background check that fails keeps whatever was found before.
          set({
            phase: get().update ? 'available' : manual ? 'failed' : 'idle',
            error: manual ? 'Could not reach the update server. Check your connection and try again.' : null,
          })
        }
      },

      install: async () => {
        const { update, phase } = get()
        if (!update || phase === 'downloading' || phase === 'installing') return
        set({ phase: 'downloading', downloaded: 0, total: null, error: null })
        try {
          let downloaded = 0
          await update.download((event) => {
            if (event.event === 'Started') set({ total: event.data.contentLength ?? null })
            else if (event.event === 'Progress') {
              downloaded += event.data.chunkLength
              set({ downloaded })
            }
          })
          set({ phase: 'installing' })
          // The Windows installer has to replace the engine executable, which is locked while it runs.
          await stopEngine()
          await update.install()
          // Windows quits for the installer on its own; macOS and Linux need the relaunch.
          const { relaunch } = await import('@tauri-apps/plugin-process')
          await relaunch()
        } catch (e) {
          void logWarning(`Update to ${update.version} failed: ${String(e)}`)
          set({ phase: 'failed', error: 'The update could not be installed. Try again, or download it from the website.' })
        }
      },

      dismiss: () => set({ dismissedVersion: get().update?.version ?? null }),
      skip: () => set({ skippedVersion: get().update?.version ?? null }),
      setAutoCheck: (autoCheck) => set({ autoCheck }),
    }),
    {
      name: 'talkr.updates',
      partialize: ({ autoCheck, skippedVersion }) => ({ autoCheck, skippedVersion }),
      merge: (persisted, current) => {
        const p = (persisted ?? {}) as Partial<UpdateState>
        return {
          ...current,
          autoCheck: p.autoCheck !== false,
          skippedVersion: typeof p.skippedVersion === 'string' ? p.skippedVersion : null,
        }
      },
    },
  ),
)

/** Whether the banner should offer the update right now. */
export function shouldOffer(s: Pick<UpdateState, 'phase' | 'update' | 'dismissedVersion' | 'skippedVersion'>) {
  if (!s.update) return false
  if (s.phase === 'downloading' || s.phase === 'installing') return true
  if (s.phase === 'failed') return true
  return s.phase === 'available' && s.update.version !== s.dismissedVersion && s.update.version !== s.skippedVersion
}

/**
 * Only the packaged app checks: not the browser preview (no Tauri), not `tauri dev` and not tests
 * (both run a development build).
 */
export const canCheckForUpdates = () => isTauri() && import.meta.env.PROD

/**
 * Check shortly after launch, then every `CHECK_EVERY_MS` while automatic checks are on.
 * Returns a cleanup function.
 */
export function startUpdateChecks(): () => void {
  let interval: ReturnType<typeof setInterval> | undefined
  const run = () => {
    if (useUpdates.getState().autoCheck) void useUpdates.getState().check()
  }
  const first = setTimeout(() => {
    run()
    interval = setInterval(run, CHECK_EVERY_MS)
  }, FIRST_CHECK_DELAY_MS)
  return () => {
    clearTimeout(first)
    clearInterval(interval)
  }
}

/** Test helper: forget everything but the persisted preferences' defaults. */
export function resetUpdates() {
  useUpdates.setState({ ...initial, autoCheck: true, skippedVersion: null })
}
