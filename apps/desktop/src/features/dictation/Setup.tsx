import type { ReactNode } from 'react'
import { Check, Power } from 'lucide-react'
import type { DictationStatus, Settings, Shortcut } from '@/lib/types'
import { Button, Card } from '@/components/ui'
import { cx } from '@/lib/cx'
import type { useSetup } from '@/features/dictation/useSetup'
import { ModelDownload } from '@/features/dictation/Hero'
import { Keys } from '@/features/dictation/Keycaps'
import { ShortcutField } from '@/features/dictation/ShortcutField'
import { ctrlWin, describeShortcut, sameKeys } from '@/features/dictation/shortcut'
import { useKeyPlatform } from '@/features/dictation/platform'

function Step({
  n,
  title,
  done,
  current,
  children,
}: {
  n: number
  title: string
  done: boolean
  current: boolean
  children: ReactNode
}) {
  return (
    <li className="dict-step relative flex gap-4" data-done={done} data-current={current} aria-current={current ? 'step' : undefined}>
      <span
        className={cx(
          'dict-step-mark relative z-10 grid size-7 shrink-0 place-items-center rounded-full border text-[12px] font-semibold transition-colors duration-300',
          done ? 'border-accent bg-accent text-on-accent' : current ? 'border-accent/70 bg-surface text-fg' : 'border-line-strong bg-surface text-subtle',
        )}
      >
        {done ? <Check className="size-3.5 animate-pop" strokeWidth={2.75} aria-label="Done" /> : n}
      </span>
      <div className="min-w-0 flex-1 pb-5">
        <p className={cx('pt-1 text-[13.5px] font-medium tracking-[-0.005em]', !current && !done && 'text-muted', done && 'text-muted line-through decoration-line-strong')}>
          {title}
        </p>
        <div className="dict-collapse" data-open={current}>
          <div>
            <div className="pt-2.5">{children}</div>
          </div>
        </div>
      </div>
    </li>
  )
}

/**
 * First-run setup: a model, a shortcut, one try. It follows what the user actually did (the
 * model appears, a dictation arrives) and folds away once everything is done.
 */
export function Setup({
  settings,
  status,
  setup,
  onShortcut,
  onEnable,
  validateShortcut,
}: {
  settings: Settings
  status: DictationStatus
  setup: ReturnType<typeof useSetup>
  onShortcut: (s: Shortcut) => void
  onEnable: () => void
  validateShortcut: (s: Shortcut) => string | null
}) {
  const platform = useKeyPlatform()
  const caps = status.capabilities
  const d = settings.dictation
  const { steps } = setup
  const doneCount = [steps.model, steps.shortcut, steps.tried].filter(Boolean).length
  const current = !steps.model ? 1 : !steps.shortcut ? 2 : 3

  return (
    <Card className="overflow-hidden">
      <div className="flex items-center justify-between gap-4 border-b border-line px-5 py-3.5">
        <div className="flex items-center gap-3">
          <p className="text-[13.5px] font-semibold tracking-[-0.01em]">Get set up</p>
          <span className="font-mono text-[11px] text-subtle">{doneCount} of 3</span>
          <span className="flex gap-1" aria-hidden="true">
            {[steps.model, steps.shortcut, steps.tried].map((on, i) => (
              <span key={i} className={cx('h-1 w-6 rounded-full transition-colors duration-500', on ? 'bg-accent' : 'bg-fg/10')} />
            ))}
          </span>
        </div>
        <Button size="sm" variant="ghost" onClick={() => setup.update({ dismissed: true })}>
          Skip setup
        </Button>
      </div>
      <ol className="px-5 pt-5">
        <Step n={1} title="Get a speech model" done={steps.model} current={current === 1}>
          <ModelDownload settings={settings} compact />
        </Step>
        <Step n={2} title="Choose your shortcut" done={steps.shortcut} current={current === 2}>
          {caps.recordsShortcut ? (
            <div className="flex flex-wrap items-center justify-between gap-3">
              <p className="max-w-sm text-[12.5px] leading-relaxed text-muted">
                {sameKeys(d.shortcut, ctrlWin)
                  ? `${describeShortcut(ctrlWin, platform)} is easy to hold with one hand and rarely used by other apps.`
                  : 'Keep the one you have, or record another.'}
              </p>
              <div className="flex items-center gap-2">
                <ShortcutField
                  label="dictation shortcut during setup"
                  value={d.shortcut}
                  fallback={ctrlWin}
                  validate={validateShortcut}
                  onChange={(s) => {
                    onShortcut(s)
                    setup.update({ shortcut: true })
                  }}
                />
                <Button size="sm" variant="primary" onClick={() => setup.update({ shortcut: true })}>
                  Use this
                </Button>
              </div>
            </div>
          ) : (
            <div className="flex flex-wrap items-center justify-between gap-3">
              <p className="max-w-md text-[12.5px] leading-relaxed text-muted">
                Your desktop chooses the shortcut. The first time dictation turns on it asks you to pick one; you can
                change it later in its keyboard settings.
              </p>
              <Button size="sm" variant="primary" onClick={() => setup.update({ shortcut: true })}>
                Got it
              </Button>
            </div>
          )}
        </Step>
        <Step n={3} title="Try it once" done={steps.tried} current={current === 3}>
          {d.enabled ? (
            <div className="flex flex-wrap items-center justify-between gap-3">
              <p className="text-[12.5px] leading-relaxed text-muted">
                Click the practice box below, {caps.holdToTalk && d.mode !== 'toggle' ? 'hold' : 'press'}{' '}
                {caps.recordsShortcut ? <Keys shortcut={d.shortcut} size="sm" /> : 'your shortcut'} and say a sentence.
              </p>
              <Button
                size="sm"
                onClick={() => {
                  const box = document.getElementById('dictation-practice')
                  box?.scrollIntoView({ behavior: 'smooth', block: 'center' })
                  box?.focus({ preventScroll: true })
                }}
              >
                Go to the practice box
              </Button>
            </div>
          ) : (
            <div className="flex flex-wrap items-center justify-between gap-3">
              <p className="text-[12.5px] text-muted">Dictation is off. Turn it on to try it.</p>
              <Button size="sm" variant="primary" icon={<Power className="size-3.5" strokeWidth={2} />} onClick={onEnable}>
                Turn on dictation
              </Button>
            </div>
          )}
        </Step>
      </ol>
    </Card>
  )
}
