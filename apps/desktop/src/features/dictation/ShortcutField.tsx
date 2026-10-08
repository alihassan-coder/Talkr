import { useEffect, useState } from 'react'
import { Keyboard, RotateCcw } from 'lucide-react'
import type { Shortcut } from '@/lib/types'
import { Button } from '@/components/ui'
import { cx } from '@/lib/cx'
import { sameKeys, shortcutProblem } from '@/features/dictation/shortcut'
import { isEmptyChord } from '@/features/dictation/keymap'
import { useKeyPlatform } from '@/features/dictation/platform'
import { Keycap, Keys } from '@/features/dictation/Keycaps'
import { RECORD_TIMEOUT_MS, useShortcutRecorder } from '@/features/dictation/useShortcutRecorder'

/**
 * Shows a shortcut and records a new one. `validate` adds checks beyond the shortcut's own (it
 * must differ from the other shortcut). Every outcome is said out loud: recorded, refused (and
 * why), cancelled by timeout, or the keyboard could not be listened to.
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
  const platform = useKeyPlatform()
  const [problem, setProblem] = useState<string | null>(null)
  const [justSaved, setJustSaved] = useState(false)

  const accept = (s: Shortcut) => {
    const issue = shortcutProblem(s, platform) ?? validate?.(s) ?? null
    setProblem(issue)
    if (issue) return
    if (!sameKeys(s, value)) onChange(s)
    setJustSaved(true)
  }
  const recorder = useShortcutRecorder(accept)
  const { state } = recorder

  useEffect(() => {
    if (!justSaved) return
    const t = setTimeout(() => setJustSaved(false), 1800)
    return () => clearTimeout(t)
  }, [justSaved])

  const isDefault = sameKeys(value, fallback)
  const message =
    state.phase === 'failed'
      ? `Talkr could not listen to the keyboard: ${state.message}`
      : state.phase === 'timedOut'
        ? 'No keys were pressed, so the shortcut was kept.'
        : problem

  return (
    <div className="flex flex-col items-end gap-2">
      <div className="flex items-center gap-2">
        {state.phase === 'recording' ? (
          <span
            role="status"
            aria-live="polite"
            className="dict-recorder relative inline-flex h-9 min-w-[214px] items-center gap-2.5 overflow-hidden rounded-full border border-accent/55 bg-accent/[0.07] pl-3 pr-3.5 text-[12.5px] text-fg"
          >
            <span className="dict-rec-dot size-2 shrink-0 rounded-full bg-accent" aria-hidden="true" />
            {isEmptyChord(state.live) ? (
              <span className="flex-1 font-medium">Press the new shortcut…</span>
            ) : (
              <span className="flex-1">
                <Keys shortcut={state.live} size="sm" pressed />
              </span>
            )}
            <Keycap label="Esc" size="sm" />
            <span className="sr-only">Escape cancels.</span>
            <span
              aria-hidden="true"
              className="dict-countdown absolute inset-x-0 bottom-0 h-[2px] origin-left bg-accent/60"
              style={{ animationDuration: `${RECORD_TIMEOUT_MS}ms` }}
            />
          </span>
        ) : (
          <span className={cx('transition-opacity', disabled && 'opacity-50')}>
            <Keys shortcut={value} pressed={justSaved} />
          </span>
        )}
        <Button
          size="sm"
          variant={state.phase === 'recording' ? 'ghost' : 'secondary'}
          disabled={disabled}
          icon={state.phase === 'recording' ? undefined : <Keyboard className="size-3.5" strokeWidth={2} />}
          aria-label={state.phase === 'recording' ? `Stop recording ${label}` : `Change ${label}`}
          onClick={() => {
            setProblem(null)
            setJustSaved(false)
            if (state.phase === 'recording') recorder.stop()
            else recorder.start()
          }}
        >
          {state.phase === 'recording' ? 'Cancel' : 'Change'}
        </Button>
        {state.phase !== 'recording' && !isDefault ? (
          <Button
            size="sm"
            variant="ghost"
            aria-label={`Reset ${label}`}
            title="Reset to the default"
            disabled={disabled}
            icon={<RotateCcw className="size-3.5" strokeWidth={2} />}
            onClick={() => {
              recorder.clear()
              accept(fallback)
            }}
          />
        ) : null}
      </div>
      {message ? (
        <p role="alert" className="dict-warn-text max-w-80 animate-rise text-right text-[12px] leading-snug">
          {message}
        </p>
      ) : justSaved ? (
        <p role="status" className="animate-rise text-right text-[12px] text-muted">
          Shortcut saved
        </p>
      ) : null}
    </div>
  )
}
