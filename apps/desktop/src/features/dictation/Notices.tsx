import { useEffect, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import { Check, Copy, Info, ShieldAlert } from 'lucide-react'
import type { DictationPermission } from '@/lib/types'
import { Button } from '@/components/ui'
import { cx } from '@/lib/cx'
import { dictationRequestPermission } from '@/lib/api'
import { errorText } from '@/lib/errors'

/** How long the page keeps checking after sending the user to the system's settings. */
const POLL_MS = 1500
const POLL_FOR_MS = 60_000

/**
 * What the system must still allow, with the one button that asks for it. After asking, the
 * page keeps checking (and checks again whenever the window regains focus), so the banner
 * clears by itself the moment the user flips the switch in the system's settings.
 */
export function PermissionBanner({
  permission,
  onCheck,
}: {
  permission: Extract<DictationPermission, { state: 'missing' }>
  onCheck: () => void
}) {
  const [asking, setAsking] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [pollUntil, setPollUntil] = useState(0)
  const check = useRef(onCheck)
  useEffect(() => {
    check.current = onCheck
  })

  useEffect(() => {
    const onFocus = () => check.current()
    window.addEventListener('focus', onFocus)
    return () => window.removeEventListener('focus', onFocus)
  }, [])

  useEffect(() => {
    if (!pollUntil) return
    const t = setInterval(() => {
      if (Date.now() > pollUntil) setPollUntil(0)
      else check.current()
    }, POLL_MS)
    return () => clearInterval(t)
  }, [pollUntil])

  const request = async () => {
    setAsking(true)
    setError(null)
    try {
      await dictationRequestPermission()
      setPollUntil(Date.now() + POLL_FOR_MS)
    } catch (e) {
      setError(errorText(e))
    } finally {
      setAsking(false)
      check.current()
    }
  }

  return (
    <section
      role="alert"
      aria-label={`Permission needed: ${permission.title}`}
      className="dict-permission relative flex animate-rise items-start gap-4 overflow-hidden rounded-2xl border px-5 py-4"
    >
      <span className="dict-permission-icon grid size-9 shrink-0 place-items-center rounded-xl">
        <ShieldAlert className="size-[18px]" strokeWidth={1.9} />
      </span>
      <div className="min-w-0 flex-1">
        <p className="text-[14px] font-semibold tracking-[-0.01em]">Allow {permission.title}</p>
        <p data-selectable className="mt-0.5 text-[13px] leading-relaxed text-muted">
          {permission.detail}
        </p>
        {pollUntil ? (
          <p role="status" className="mt-2 inline-flex items-center gap-2 text-[12px] text-muted">
            <span className="dict-rec-dot size-1.5 rounded-full bg-accent" aria-hidden="true" />
            Waiting for you to allow it. This updates by itself.
          </p>
        ) : null}
        {error ? <p className="dict-warn-text mt-2 text-[12px]">{error}</p> : null}
      </div>
      <div className="flex shrink-0 items-center gap-2 self-center">
        {permission.canRequest ? (
          <Button variant="primary" size="sm" loading={asking} onClick={() => void request()}>
            Allow {permission.title}
          </Button>
        ) : null}
        <Button size="sm" variant={permission.canRequest ? 'ghost' : 'secondary'} onClick={onCheck}>
          Check again
        </Button>
      </div>
    </section>
  )
}

/** A command with a copy button: `talkr --dictate`. */
export function CopyCommand({ command }: { command: string }) {
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (!copied) return
    const t = setTimeout(() => setCopied(false), 1600)
    return () => clearTimeout(t)
  }, [copied])
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(command)
      setCopied(true)
    } catch {
      // Older webviews without the async clipboard: select the text so Ctrl+C works.
      const el = document.getElementById(`cmd-${command}`)
      if (el) window.getSelection()?.selectAllChildren(el)
    }
  }
  return (
    <span className="inline-flex h-8 items-center gap-1 rounded-lg border border-line bg-bg pl-3 pr-1 font-mono text-[12.5px]">
      <span id={`cmd-${command}`} data-selectable>
        {command}
      </span>
      <button
        type="button"
        onClick={() => void copy()}
        aria-label={copied ? 'Copied' : `Copy ${command}`}
        className="ml-1 grid size-6 place-items-center rounded-md text-subtle transition-colors hover:bg-fg/[0.07] hover:text-fg"
      >
        {copied ? <Check className="size-3.5" strokeWidth={2.25} /> : <Copy className="size-3.5" strokeWidth={2} />}
      </button>
    </span>
  )
}

/** A quiet explanation inside a section: how something works differently on this system. */
export function Explainer({ title, children, className }: { title?: string; children: ReactNode; className?: string }) {
  return (
    <div className={cx('flex items-start gap-3 border-b border-line px-5 py-4 last:border-0', className)}>
      <span className="mt-0.5 grid size-6 shrink-0 place-items-center rounded-lg bg-fg/[0.06] text-muted">
        <Info className="size-3.5" strokeWidth={2} />
      </span>
      <div className="min-w-0 text-[13px] leading-relaxed text-muted">
        {title ? <p className="text-[13.5px] font-medium tracking-[-0.005em] text-fg">{title}</p> : null}
        {children}
      </div>
    </div>
  )
}
