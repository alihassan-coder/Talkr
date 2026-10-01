import { useEffect, useEffectEvent, useRef, useState } from 'react'
import { cx } from '@/lib/cx'
import { isTauri, onMicError, onMicLevel, startRecording, stopRecording } from '@/lib/api'
import { errorText } from '@/lib/errors'
import { toast, toastError } from '@/stores/toast'
import { formatDuration } from '@/features/speak/utils'

const BARS = 24
const silence = () => Array.from({ length: BARS }, () => 0)

/** RMS is small for normal speech; lift it so the meter feels alive. */
const toHeight = (level: number) => Math.min(1, Math.sqrt(Math.max(0, level)) * 2.2)

export function Recorder({
  onRecorded,
  disabled = false,
  disabledReason = null,
}: {
  onRecorded: (path: string, durationMs: number) => void
  disabled?: boolean
  /** Shown while disabled, e.g. "Wait for speech generation to finish." */
  disabledReason?: string | null
}) {
  const [recording, setRecording] = useState(false)
  const [busy, setBusy] = useState(false)
  const [startedAt, setStartedAt] = useState(0)
  const [now, setNow] = useState(0)
  const [levels, setLevels] = useState(silence)
  const [micError, setMicError] = useState<string | null>(null)
  const recordingRef = useRef(false)
  // startRecording() is in flight: the microphone may open after the user has left.
  const startingRef = useRef(false)
  const mountedRef = useRef(true)

  useEffect(() => {
    if (!recording) return
    const timer = setInterval(() => setNow(Date.now()), 200)
    const unlisten = onMicLevel((level) => setLevels((prev) => [...prev.slice(1), level]))
    return () => {
      clearInterval(timer)
      void unlisten.then((fn) => fn())
    }
  }, [recording])

  // Leaving the screen mid-recording releases the microphone. If it is still opening,
  // `start` releases it as soon as it opens.
  useEffect(() => {
    mountedRef.current = true
    return () => {
      mountedRef.current = false
      if (recordingRef.current) {
        recordingRef.current = false
        stopRecording().catch(() => {})
      }
    }
  }, [])

  // The microphone failed or was unplugged: the backend ended the recording.
  const handleMicError = useEffectEvent((message: string) => {
    if (!recordingRef.current && !startingRef.current) return
    recordingRef.current = false
    setRecording(false)
    setLevels(silence())
    setMicError(message ? errorText(message) : 'The microphone stopped working.')
    // Release whatever is left of the capture; it may already be gone.
    stopRecording().catch(() => {})
  })

  useEffect(() => {
    if (!isTauri()) return
    const unlisten = onMicError((e) => handleMicError(e.message))
    return () => {
      void unlisten.then((fn) => fn()).catch(() => {})
    }
  }, [])

  const start = async () => {
    if (!isTauri()) {
      toast('Recording works in the Talkr desktop app.')
      return
    }
    setBusy(true)
    setMicError(null)
    startingRef.current = true
    try {
      await startRecording()
      if (!mountedRef.current) {
        // The user left while the microphone was opening.
        stopRecording().catch(() => {})
        return
      }
      const t = Date.now()
      setStartedAt(t)
      setNow(t)
      setLevels(silence())
      recordingRef.current = true
      setRecording(true)
    } catch (err) {
      if (mountedRef.current) setMicError(errorText(err))
    } finally {
      startingRef.current = false
      if (mountedRef.current) setBusy(false)
    }
  }

  const stop = async () => {
    setBusy(true)
    try {
      const result = await stopRecording()
      onRecorded(result.tempAudioPath, result.durationMs)
    } catch (err) {
      toastError(err)
    } finally {
      recordingRef.current = false
      setRecording(false)
      setLevels(silence())
      setBusy(false)
    }
  }

  const elapsed = recording ? now - startedAt : 0

  return (
    <div className="flex flex-col items-center rounded-2xl border border-line bg-surface px-8 shadow-[var(--shadow-card)] py-12">
      <button
        type="button"
        onClick={() => void (recording ? stop() : start())}
        disabled={busy || (disabled && !recording)}
        aria-label={recording ? 'Stop recording' : 'Start recording'}
        aria-pressed={recording}
        className="group relative grid size-24 place-items-center rounded-full border border-line-strong transition-[border-color,transform] duration-300 ease-out-quint hover:border-line-strong active:scale-[0.97] disabled:opacity-40"
      >
        {recording ? (
          <span className="absolute inset-0 animate-ping rounded-full border border-line-strong [animation-duration:1.8s] motion-reduce:hidden" />
        ) : null}
        <span
          className={cx(
            'bg-accent transition-all duration-400 ease-out-quint',
            recording ? 'size-7 rounded-lg' : 'size-9 rounded-full group-hover:scale-105',
          )}
        />
      </button>

      <p className="mt-7 font-mono text-4xl font-light tabular-nums tracking-tight">{formatDuration(elapsed)}</p>
      <p className="mt-2 text-[13px] text-muted">
        {busy
          ? recording
            ? 'Saving…'
            : 'Opening microphone…'
          : recording
            ? 'Listening. Click to stop.'
            : disabled && disabledReason
              ? disabledReason
              : 'Click to record'}
      </p>
      {micError && !recording ? (
        <p role="alert" data-selectable className="mt-3 max-w-sm text-center text-[12.5px] leading-relaxed text-fg">
          {micError}
        </p>
      ) : null}

      <div className="mt-8 flex h-10 items-center gap-[3px]" aria-hidden="true">
        {levels.map((level, i) => {
          const h = recording ? toHeight(level) : 0
          return (
            <span
              key={i}
              className="w-[3px] rounded-full bg-accent transition-[height,opacity] duration-100"
              style={{ height: `${Math.max(8, h * 100)}%`, opacity: recording ? 0.35 + h * 0.65 : 0.15 }}
            />
          )
        })}
      </div>
    </div>
  )
}
