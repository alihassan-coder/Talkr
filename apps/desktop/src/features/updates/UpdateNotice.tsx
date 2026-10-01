import { useEffect, useState } from 'react'
import { ArrowUpCircle, ChevronDown } from 'lucide-react'
import { Button } from '@/components/ui'
import { cx } from '@/lib/cx'
import { canCheckForUpdates, shouldOffer, startUpdateChecks, useUpdates } from '@/stores/updates'

/**
 * Starts the background update checks and, when a newer Talkr is out, shows a banner at the top
 * of the page (in the flow, so it never covers a page's own controls). The release notes fold
 * out; "Later" hides it until the next launch, "Skip this version" for good.
 */
export function UpdateNotice({ enabled = canCheckForUpdates() }: { enabled?: boolean }) {
  const state = useUpdates()
  const [notesOpen, setNotesOpen] = useState(false)

  useEffect(() => {
    if (!enabled) return
    return startUpdateChecks()
  }, [enabled])

  if (!shouldOffer(state) || !state.update) return null
  const { update, phase } = state
  const busy = phase === 'downloading' || phase === 'installing'
  const percent = state.total ? Math.min(100, Math.round((state.downloaded / state.total) * 100)) : undefined
  const notes = update.body?.trim()

  return (
    <section
      role="status"
      aria-label="Update available"
      className="animate-rise overflow-hidden rounded-2xl border border-line bg-surface shadow-[var(--shadow-card)]"
    >
      <div className="flex flex-wrap items-center gap-x-4 gap-y-3 px-5 py-3.5">
        <ArrowUpCircle aria-hidden="true" className="size-5 shrink-0 text-accent" strokeWidth={1.75} />
        <div className="min-w-0 flex-1">
          <p className="text-[13.5px] font-medium tracking-[-0.005em]">Talkr {update.version} is available</p>
          <p className="text-[12.5px] text-muted">
            {phase === 'failed'
              ? state.error
              : phase === 'installing'
                ? 'Installing. Talkr will restart in a moment.'
                : `You have ${update.currentVersion}. Talkr restarts to finish the update; your history and models stay.`}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {notes ? (
            <Button
              size="sm"
              variant="ghost"
              aria-expanded={notesOpen}
              icon={<ChevronDown className={cx('size-3.5 transition-transform', notesOpen && 'rotate-180')} strokeWidth={2} />}
              onClick={() => setNotesOpen((o) => !o)}
            >
              What&apos;s new
            </Button>
          ) : null}
          {busy ? null : (
            <>
              <Button size="sm" variant="ghost" onClick={state.dismiss}>
                Later
              </Button>
              <Button size="sm" variant="ghost" onClick={state.skip}>
                Skip this version
              </Button>
            </>
          )}
          <Button size="sm" variant="primary" loading={busy} onClick={() => void state.install()}>
            {phase === 'downloading'
              ? percent === undefined
                ? 'Downloading…'
                : `Downloading… ${percent}%`
              : phase === 'installing'
                ? 'Installing…'
                : phase === 'failed'
                  ? 'Try again'
                  : 'Install and restart'}
          </Button>
        </div>
      </div>
      {notes && notesOpen ? (
        <div
          data-selectable
          className="max-h-56 overflow-y-auto whitespace-pre-wrap border-t border-line px-5 py-3.5 text-[12.5px] leading-relaxed text-muted"
        >
          {notes}
        </div>
      ) : null}
    </section>
  )
}
