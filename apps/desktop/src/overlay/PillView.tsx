import { useEffect, useId, useRef, useState } from 'react'
import type { RefObject } from 'react'
import { Ban, CircleAlert, ClipboardCheck, Info, Lock, TriangleAlert, X } from 'lucide-react'
import { WAVE, WaveMotion, wavePath, waveWidth } from '@/overlay/wave'
import { announce } from '@/overlay/state'
import type { PillState } from '@/overlay/state'

const prefersCalm = () =>
  typeof window !== 'undefined' && typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches

/**
 * The live waveform, drawn as one SVG path in an animation frame loop: React renders it once,
 * then only the path's `d` and the stage's `--level` change. Round caps keep every bar crisp.
 */
function Wave({ level }: { level: RefObject<number> }) {
  const path = useRef<SVGPathElement>(null)
  const id = `pill-wave-${useId().replace(/[^a-zA-Z0-9]/g, '')}`
  useEffect(() => {
    const motion = new WaveMotion(prefersCalm())
    const stage = path.current?.closest<HTMLElement>('.pill-stage') ?? null
    let frame = 0
    const draw = (now: number) => {
      const { bars, level: smooth } = motion.step(now, level.current)
      path.current?.setAttribute('d', wavePath(bars))
      stage?.style.setProperty('--level', smooth.toFixed(3))
      frame = requestAnimationFrame(draw)
    }
    frame = requestAnimationFrame(draw)
    return () => {
      cancelAnimationFrame(frame)
      stage?.style.setProperty('--level', '0')
    }
  }, [level])
  return (
    <svg className="pill-wave" width={waveWidth} height={WAVE.height} viewBox={`0 0 ${waveWidth} ${WAVE.height}`} aria-hidden="true">
      <defs>
        <linearGradient id={id} x1="0" y1="0" x2={waveWidth} y2="0" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="var(--pill-accent)" stopOpacity="0.55" />
          <stop offset="0.5" stopColor="var(--pill-accent-hi)" />
          <stop offset="1" stopColor="var(--pill-accent)" stopOpacity="0.55" />
        </linearGradient>
      </defs>
      <path ref={path} d={wavePath(new Array<number>(WAVE.bars).fill(0.06))} stroke={`url(#${id})`} strokeWidth={WAVE.stroke} strokeLinecap="round" fill="none" />
    </svg>
  )
}

/** Time since mount as m:ss, written straight to the DOM (no React render per tick). */
function Timer() {
  const el = useRef<HTMLSpanElement>(null)
  useEffect(() => {
    const started = performance.now()
    let shown = 0
    const t = setInterval(() => {
      const s = Math.floor((performance.now() - started) / 1000)
      if (s === shown || !el.current) return
      shown = s
      el.current.textContent = `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
    }, 250)
    return () => clearInterval(t)
  }, [])
  return (
    <span ref={el} className="pill-timer">
      0:00
    </span>
  )
}

function Listening({
  state,
  level,
  onStop,
  onCancel,
}: {
  state: Extract<PillState, { kind: 'listening' }>
  level: RefObject<number>
  onStop: () => void
  onCancel: () => void
}) {
  return (
    <>
      <span className="pill-mic" data-locked={state.locked}>
        {state.locked ? <Lock className="size-3" strokeWidth={2.5} aria-label="Hands-free" /> : <i />}
      </span>
      <Wave level={level} />
      <Timer />
      {state.app ? <span className="pill-target">→ {state.app}</span> : null}
      {state.locked ? (
        <span className="pill-actions">
          <button type="button" className="pill-btn" aria-label="Cancel" title="Cancel (Esc)" onClick={onCancel}>
            <X className="size-3.5" strokeWidth={2.25} />
          </button>
          <button type="button" className="pill-btn pill-primary" onClick={onStop}>
            <span className="pill-stop" aria-hidden="true" />
            Done
          </button>
        </span>
      ) : null}
    </>
  )
}

function Working({ label }: { label: string }) {
  return (
    <>
      <svg className="pill-spinner" width="18" height="18" viewBox="0 0 18 18" aria-hidden="true">
        <circle cx="9" cy="9" r="7" fill="none" stroke="currentColor" strokeOpacity="0.16" strokeWidth="2" />
        <circle cx="9" cy="9" r="7" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeDasharray="12 32" />
      </svg>
      <span className="pill-label pill-shimmer">{label}</span>
    </>
  )
}

function Done({ preview, app }: { preview: string; app: string | null }) {
  return (
    <>
      <span className="pill-badge" data-tone="ok">
        <svg className="pill-check" width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true">
          <path d="M2.4 6.4 4.9 8.8 9.7 3.5" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </span>
      <span className="pill-preview">{preview}</span>
      {app ? <span className="pill-target">→ {app}</span> : null}
    </>
  )
}

function Notice({ tone, title, detail }: { tone: 'info' | 'warn' | 'error'; title: string; detail: string | null }) {
  const Icon = /^copied/i.test(title) ? ClipboardCheck : tone === 'info' ? Info : tone === 'warn' ? TriangleAlert : CircleAlert
  return (
    <>
      <span className="pill-badge" data-tone={tone}>
        <Icon className="size-3" strokeWidth={2.25} />
      </span>
      <span className="pill-label">{title}</span>
      {detail ? <span className="pill-detail">{detail}</span> : null}
    </>
  )
}

function Body(props: { state: PillState; level: RefObject<number>; onStop: () => void; onCancel: () => void }) {
  const { state } = props
  switch (state.kind) {
    case 'listening':
      return <Listening state={state} level={props.level} onStop={props.onStop} onCancel={props.onCancel} />
    case 'working':
      return <Working label={state.label} />
    case 'done':
      return <Done preview={state.preview} app={state.app} />
    case 'notice':
      return <Notice tone={state.tone} title={state.title} detail={state.detail} />
    case 'cancelled':
      return (
        <>
          <span className="pill-badge" data-tone="muted">
            <Ban className="size-3" strokeWidth={2.25} />
          </span>
          <span className="pill-label pill-muted">Cancelled</span>
        </>
      )
    case 'hidden':
      return null
  }
}

/**
 * The pill itself, without any window plumbing: one glass capsule that morphs between
 * listening, working, done and notices. Its width follows its content with a spring, measured
 * (not `width: auto`), so the morph is smooth in every webview. Used by the overlay window and
 * by the preview on the Dictation page.
 */
export function PillView({
  state,
  level,
  onStop,
  onCancel,
  entrance = 0,
  className,
}: {
  state: PillState
  level: RefObject<number>
  onStop: () => void
  onCancel: () => void
  /** Bump to replay the entrance. */
  entrance?: number
  className?: string
}) {
  // The last visible state, kept while the pill fades out so its content does not vanish first.
  const [shown, setShown] = useState<PillState>(state)
  if (state.kind !== 'hidden' && state !== shown) setShown(state)

  const pill = useRef<HTMLDivElement>(null)
  const measure = (body: HTMLDivElement | null) => {
    if (!body || typeof ResizeObserver === 'undefined') return
    const apply = () => {
      const el = pill.current
      if (!el) return
      const w = body.offsetWidth
      el.style.width = `${w}px`
      el.parentElement?.style.setProperty('--pill-w', `${w}px`)
    }
    apply()
    const observer = new ResizeObserver(apply)
    observer.observe(body)
    return () => observer.disconnect()
  }

  const visible = state.kind !== 'hidden'
  const session = 'session' in shown ? shown.session : 0
  return (
    <div className={['pill-stage', className].filter(Boolean).join(' ')} data-kind={shown.kind}>
      <div className="pill-glow" data-on={visible && shown.kind === 'listening'} aria-hidden="true" />
      <div
        key={`${entrance}-${session}`}
        ref={pill}
        className="pill"
        data-visible={visible}
        data-kind={shown.kind}
        role="status"
        aria-live="polite"
        aria-label={announce(shown)}
      >
        <div key={shown.kind} ref={measure} className="pill-body">
          <Body state={shown} level={level} onStop={onStop} onCancel={onCancel} />
        </div>
      </div>
    </div>
  )
}
