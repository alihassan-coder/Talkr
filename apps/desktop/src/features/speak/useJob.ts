import { useEffect, useEffectEvent, useRef, useState } from 'react'
import { cancelJob, isTauri, onJobError, onJobProgress, onSttDone, onTtsDone } from '@/lib/api'
import { errorText } from '@/lib/errors'
import type { HistoryItem } from '@/lib/types'
import { toast, toastError } from '@/stores/toast'

type Outcome = { ok: true; item: HistoryItem } | { ok: false; error: string; cancelled: boolean }

/**
 * Tracks one background synthesis/transcription job: its progress and outcome
 * arrive as events, so we match them against the job id returned by the command.
 *
 * A failure stays in `error` (shown on the page) until the next start or `dismissError`:
 * engine messages such as "ran out of memory" or "stopped unexpectedly" tell the user
 * what to do next, so they must not vanish with a toast.
 */
export function useJob(kind: 'tts' | 'stt', onDone: (item: HistoryItem) => void) {
  const [running, setRunning] = useState(false)
  const [progress, setProgress] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)
  const activeId = useRef<string | null>(null)
  // Outcomes that arrived before the command resolved with its job id.
  const early = useRef(new Map<string, Outcome>())

  const settle = (outcome: Outcome) => {
    activeId.current = null
    setRunning(false)
    setProgress(null)
    if (outcome.ok) onDone(outcome.item)
    else if (outcome.cancelled) toast('Cancelled')
    else setError(errorText(outcome.error))
  }

  const handleOutcome = useEffectEvent((jobId: string, outcome: Outcome) => {
    if (jobId === activeId.current) settle(outcome)
    else if (running) early.current.set(jobId, outcome)
  })

  const handleProgress = useEffectEvent((jobId: string, value: number) => {
    if (jobId === activeId.current) setProgress(value > 0 ? Math.min(1, value) : null)
  })

  useEffect(() => {
    if (!isTauri()) return
    const subscriptions = [
      onJobProgress((e) => handleProgress(e.jobId, e.progress)),
      onJobError((e) => handleOutcome(e.jobId, { ok: false, error: e.error, cancelled: e.cancelled })),
      (kind === 'tts' ? onTtsDone : onSttDone)((e) => handleOutcome(e.jobId, { ok: true, item: e.historyItem })),
    ]
    return () => {
      for (const sub of subscriptions) void sub.then((unlisten) => unlisten()).catch(() => {})
    }
  }, [kind])

  const start = async (launch: () => Promise<string>) => {
    early.current.clear()
    setError(null)
    setRunning(true)
    setProgress(null)
    try {
      const jobId = await launch()
      const outcome = early.current.get(jobId)
      if (outcome) settle(outcome)
      else activeId.current = jobId
    } catch (err) {
      // Rejected before a job existed, e.g. "Not enough free memory to run …".
      setRunning(false)
      setError(errorText(err))
    }
  }

  const cancel = async () => {
    const jobId = activeId.current
    if (!jobId) return
    try {
      await cancelJob({ jobId })
    } catch (err) {
      toastError(err)
    }
  }

  const dismissError = () => setError(null)

  return { running, progress, error, start, cancel, dismissError }
}
