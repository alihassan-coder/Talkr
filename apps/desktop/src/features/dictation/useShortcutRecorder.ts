import { useEffect, useRef, useState } from 'react'
import { captureShortcut, isTauri, onCaptureFailed, onShortcutCaptured, onShortcutCapturing } from '@/lib/api'
import type { Shortcut } from '@/lib/types'
import { errorText } from '@/lib/errors'
import { addKey, emptyShortcut, isEmptyChord } from '@/features/dictation/keymap'

/** How long recording waits for keys before giving up, so keys are never held hostage. */
export const RECORD_TIMEOUT_MS = 15_000

export type RecorderState =
  | { phase: 'idle' }
  | { phase: 'recording'; live: Shortcut }
  | { phase: 'failed'; message: string }
  | { phase: 'timedOut' }

/** Only one field records at a time: the keyboard hook has one capture. */
let activeStop: (() => void) | null = null

/**
 * Records a shortcut. In the app the keyboard hook does it (it sees the Win key and chords the
 * window would never get): it reports the keys held so far through `dictation://capturing`, the
 * result through `dictation://captured` (`null` for Escape), and `dictation://capture-failed`
 * when the system will not let Talkr listen. In a plain browser the window's own key events
 * record it, so the page can be tried without the backend.
 *
 * The listeners are attached before capture starts: a backend that cannot listen answers at
 * once, and that answer must not be missed.
 */
export function useShortcutRecorder(onRecorded: (s: Shortcut) => void) {
  const [state, setState] = useState<RecorderState>({ phase: 'idle' })
  const recorded = useRef(onRecorded)
  const stopCurrent = useRef<(() => void) | null>(null)

  useEffect(() => {
    recorded.current = onRecorded
  })

  // Let go of the keyboard if the field goes away mid-recording.
  useEffect(() => () => stopCurrent.current?.(), [])

  const start = () => {
    activeStop?.()
    const tauri = isTauri()
    const cleanups: (() => void)[] = []
    let over = false

    const end = (next: RecorderState, shortcut?: Shortcut, release = true) => {
      if (over) return
      over = true
      for (const c of cleanups.splice(0)) c()
      if (release && tauri) void captureShortcut({ active: false }).catch(() => {})
      if (activeStop === stop) activeStop = null
      stopCurrent.current = null
      setState(next)
      if (shortcut) recorded.current(shortcut)
    }
    const stop = () => end({ phase: 'idle' })
    activeStop = stop
    stopCurrent.current = stop
    setState({ phase: 'recording', live: emptyShortcut })

    const timer = setTimeout(() => end({ phase: 'timedOut' }), RECORD_TIMEOUT_MS)
    cleanups.push(() => clearTimeout(timer))

    // Keys as the window sees them: shown live, and the recording itself in a browser.
    let chord = emptyShortcut
    const held = new Set<string>()
    const down = (e: KeyboardEvent) => {
      e.preventDefault()
      e.stopPropagation()
      if (e.repeat) return
      if (!tauri && e.code === 'Escape' && isEmptyChord(chord)) return end({ phase: 'idle' })
      held.add(e.code)
      chord = addKey(chord, e.code)
      setState({ phase: 'recording', live: chord })
    }
    const up = (e: KeyboardEvent) => {
      e.preventDefault()
      held.delete(e.code)
      if (!tauri && held.size === 0 && !isEmptyChord(chord)) end({ phase: 'idle' }, chord)
    }
    window.addEventListener('keydown', down, true)
    window.addEventListener('keyup', up, true)
    cleanups.push(() => {
      window.removeEventListener('keydown', down, true)
      window.removeEventListener('keyup', up, true)
    })

    if (!tauri) return
    const unlistens: (() => void)[] = []
    cleanups.push(() => unlistens.splice(0).forEach((f) => f()))
    Promise.all([
      onShortcutCaptured((captured) => end({ phase: 'idle' }, captured ?? undefined, false)),
      onShortcutCapturing((live) => {
        if (!over) setState({ phase: 'recording', live })
      }),
      onCaptureFailed((message) => end({ phase: 'failed', message }, undefined, false)),
    ])
      .then((fs) => {
        if (over) return fs.forEach((f) => f())
        unlistens.push(...fs)
        return captureShortcut({ active: true })
      })
      .catch((e: unknown) => end({ phase: 'failed', message: errorText(e) }))
  }

  return {
    state,
    recording: state.phase === 'recording',
    start,
    stop: () => stopCurrent.current?.(),
    clear: () => setState({ phase: 'idle' }),
  }
}
