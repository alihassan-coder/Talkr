import { useEffect } from 'react'
import { busyReason, initJobs, useJobs, type JobKind } from '@/stores/jobs'

/**
 * The background synthesis/transcription job of one kind. It lives in the jobs store, so
 * leaving the page does not lose its progress, Cancel or outcome: coming back shows them.
 *
 * A failure stays in `error` (shown on the page) until the next start or `dismissError`:
 * engine messages such as "ran out of memory" or "stopped unexpectedly" tell the user
 * what to do next, so they must not vanish with a toast.
 */
export function useJob(kind: JobKind) {
  const slot = useJobs((s) => s[kind])
  const blockedBy = useJobs((s) => busyReason(s, kind))
  const startJob = useJobs((s) => s.start)
  const cancelJob = useJobs((s) => s.cancel)
  const dismiss = useJobs((s) => s.dismissError)

  useEffect(() => {
    initJobs()
  }, [])

  return {
    ...slot,
    /** Why a new job cannot start now (another heavy job runs), or null. */
    blockedBy,
    start: (launch: () => Promise<string>, options?: { label?: string; clearResult?: boolean }) =>
      startJob(kind, launch, options),
    cancel: () => cancelJob(kind),
    dismissError: () => dismiss(kind),
  }
}
