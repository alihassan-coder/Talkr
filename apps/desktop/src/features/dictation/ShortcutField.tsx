import { useEffect, useEffectEvent, useRef, useState } from 'react'
import { Keyboard, RotateCcw } from 'lucide-react'
import { captureShortcut, isTauri, onShortcutCaptured } from '@/lib/api'
import type { Shortcut } from '@/lib/types'
import { Button } from '@/components/ui'
import { cx } from '@/lib/cx'
import { shortcutKeys, shortcutProblem } from '@/features/dictation/shortcut'

/** The keys of a shortcut as keycaps. */
export function Keys({ shortcut, size = 'md' }: { shortcut: Shortcut; size?: 'md' | 'lg' }) {
  const keys = shortcutKeys(shortcut)
  return (
    <span className="inline-flex items-center gap-1" aria-label={keys.join(' plus ')}>
      {keys.map((k, i) => (
        <span key={`${k}-${i}`} className="inline-flex items-center gap-1" aria-hidden="true">
          {i > 0 ? <span className="text-[11px] text-subtle">+</span> : null}
          <kbd
            className={cx(
              'inline-flex items-center justify-center rounded-md border border-line-strong bg-surface font-mono font-medium text-fg shadow-[inset_0_-1.5px_0_var(--color-line-strong)]',
              size === 'lg' ? 'h-8 min-w-8 px-2.5 text-[13px]' : 'h-6 min-w-6 px-1.5 text-[11.5px]',
            )}
          >
            {k}
          </kbd>
        </span>
      ))}
    </span>
  )
}

/**
 * Shows a shortcut and records a new one. Recording happens in Talkr's keyboard hook, so it sees
 * the Windows key and combinations the window itself would never receive. `validate` adds checks
 * beyond the shortcut's own (it must differ from the other one).
 */
export function ShortcutField({
  label,
  value,
  fallback,
  onChange,
  validate,
  disabled = false,
}: {
  label: string
  value: Shortcut
  fallback: Shortcut
  onChange: (s: Shortcut) => void
  validate?: (s: Shortcut) => string | null
  disabled?: boolean
}) {
  const [recording, setRecording] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)
  const recordingRef = useRef(false)

  const onCaptured = useEffectEvent((captured: Shortcut | null) => {
    recordingRef.current = false
    setRecording(false)
    if (!captured) return
    const issue = shortcutProblem(captured) ?? validate?.(captured) ?? null
    setProblem(issue)
    if (!issue) onChange(captured)
  })

  useEffect(() => {
    if (!recording || !isTauri()) return
    recordingRef.current = true
    const unlisten = onShortcutCaptured(onCaptured)
    captureShortcut({ active: true }).catch(() => setRecording(false))
    // Give up after a while so keys are never held hostage.
    const timeout = setTimeout(() => setRecording(false), 15_000)
    return () => {
      clearTimeout(timeout)
      void unlisten.then((f) => f())
      if (recordingRef.current) {
        recordingRef.current = false
        void captureShortcut({ active: false }).catch(() => {})
      }
    }
  }, [recording])

  const isDefault = shortcutKeys(value).join() === shortcutKeys(fallback).join()

  return (
    <div className="flex flex-col items-end gap-1.5">
      <div className="flex items-center gap-2">
        {recording ? (
          <span
            role="status"
            className="inline-flex h-8 items-center gap-2 rounded-full border border-accent/60 bg-accent/10 px-3 text-[12.5px] text-fg"
          >
            <span className="size-1.5 animate-pulse rounded-full bg-accent" />
            Press the new shortcut…
            <span className="font-mono text-[10.5px] text-subtle">Esc cancels</span>
          </span>
        ) : (
          <Keys shortcut={value} />
        )}
        <Button
          size="sm"
          variant={recording ? 'ghost' : 'secondary'}
          disabled={disabled || !isTauri()}
          icon={recording ? undefined : <Keyboard className="size-3.5" strokeWidth={2} />}
          aria-label={recording ? `Stop recording ${label}` : `Change ${label}`}
          onClick={() => {
            setProblem(null)
            setRecording((r) => !r)
          }}
        >
          {recording ? 'Cancel' : 'Change'}
        </Button>
        {!recording && !isDefault ? (
          <Button
            size="sm"
            variant="ghost"
            aria-label={`Reset ${label}`}
            icon={<RotateCcw className="size-3.5" strokeWidth={2} />}
            onClick={() => {
              const issue = validate?.(fallback) ?? null
              setProblem(issue)
              if (!issue) onChange(fallback)
            }}
          />
        ) : null}
      </div>
      {problem ? (
        <p role="alert" className="max-w-72 text-right text-[12px] leading-snug text-accent">
          {problem}
        </p>
      ) : null}
    </div>
  )
}
