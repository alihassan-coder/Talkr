import { useEffect, useState } from 'react'
import type { DictationStatus } from '@/lib/types'
import { isTauri, onDictationDone } from '@/lib/api'

const STORAGE_KEY = 'talkr.dictation.setup'

type Progress = { shortcut: boolean; tried: boolean; dismissed: boolean }

function load(): Progress {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? 'null') as Partial<Progress> | null
    return { shortcut: !!saved?.shortcut, tried: !!saved?.tried, dismissed: !!saved?.dismissed }
  } catch {
    return { shortcut: false, tried: false, dismissed: false }
  }
}

/** Setup progress, kept on this computer; a past dictation counts as having tried it. */
export function useSetup(status: DictationStatus) {
  const [progress, setProgress] = useState<Progress>(load)
  const update = (patch: Partial<Progress>) =>
    setProgress((p) => {
      const next = { ...p, ...patch }
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(next))
      } catch {
        // Private mode or full storage: setup simply shows again next time.
      }
      return next
    })

  useEffect(() => {
    if (!isTauri()) return
    const p = onDictationDone(() => update({ tried: true }))
    return () => void p.then((f) => f()).catch(() => {})
  }, [])

  const steps = {
    model: !!status.modelId,
    shortcut: progress.shortcut,
    tried: progress.tried || status.hasLast,
  }
  const complete = steps.model && steps.shortcut && steps.tried
  return { steps, complete, open: status.supported && !progress.dismissed && !complete, update }
}
