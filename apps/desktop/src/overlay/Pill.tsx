import { useEffect, useRef, useState } from 'react'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { Ban, Info, Lock, Square, TriangleAlert, X } from 'lucide-react'
import { dictationCancel, dictationStop, isTauri } from '@/lib/api'
import { levelToHeight } from '@/overlay/level'

/** Mirrors `overlay::State` in src-tauri/src/dictation/overlay.rs. */
export type PillState =
  | { kind: 'hidden' }
  | { kind: 'listening'; session: number; locked: boolean; app: string | null }
  | { kind: 'working'; session: number; label: string }
  | { kind: 'done'; session: number; preview: string; app: string | null }
  | { kind: 'notice'; session: number; tone: 'info' | 'warn' | 'error'; title: string; detail: string | null }
  | { kind: 'cancelled'; session: number }

const BARS = 28

/** Time since mount as m:ss. The pill remounts for every dictation, which restarts it. */
function useElapsed() {
  const [seconds, setSeconds] = useState(0)
  useEffect(() => {
    const started = performance.now()
    const t = setInterval(() => setSeconds(Math.floor((performance.now() - started) / 1000)), 250)
    return () => clearInterval(t)
  }, [])
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
}

/**
 * The live waveform: bars mirrored around the centre, newest level in the middle, flowing out.
 * Drawn straight to the DOM in an animation frame loop; React only renders the bars once.
 */
function Wave({ level }: { level: React.RefObject<number> }) {
  const bars = useRef<(HTMLSpanElement | null)[]>([])
  useEffect(() => {
    const half = BARS / 2
    const history = new Array<number>(half).fill(0)
    let smooth = 0
    let frame = 0
    let last = performance.now()
    let shift = 0
    const draw = (now: number) => {
      const dt = Math.min(64, now - last)
      last = now
      const target = levelToHeight(level.current)
      // Fast attack, slower release, like a VU meter.
      smooth += (target - smooth) * (target > smooth ? 0.55 : 0.12)
      shift += dt
      if (shift >= 45) {
        shift = 0
        history.pop()
        history.unshift(smooth)
      } else {
        history[0] = Math.max(history[0] ?? 0, smooth)
      }
      for (let i = 0; i < BARS; i++) {
        const fromCentre = Math.floor(Math.abs(i + 0.5 - half))
        const h = history[Math.min(half - 1, fromCentre)] ?? 0
        // Taper to the edges and breathe a little in silence, so it never looks frozen.
        const taper = 1 - (fromCentre / half) * 0.45
        const idle = 0.1 + 0.05 * Math.sin(now / 260 + i * 0.7)
        const scale = Math.max(idle, h * taper)
        const bar = bars.current[i]
        if (bar) bar.style.transform = `scaleY(${scale.toFixed(3)})`
      }
      document.documentElement.style.setProperty('--level', smooth.toFixed(3))
      frame = requestAnimationFrame(draw)
    }
    frame = requestAnimationFrame(draw)
    return () => {
      cancelAnimationFrame(frame)
      document.documentElement.style.setProperty('--level', '0')
    }
  }, [level])
  return (
    <div className="wave" aria-hidden="true">
      {Array.from({ length: BARS }, (_, i) => (
        <span
          key={i}
          ref={(el) => {
            bars.current[i] = el
          }}
        />
      ))}
    </div>
  )
}

function Listening({ state, level }: { state: Extract<PillState, { kind: 'listening' }>; level: React.RefObject<number> }) {
  const elapsed = useElapsed()
  return (
    <>
      {state.locked ? <Lock className="lock size-3.5" strokeWidth={2.25} aria-label="Hands-free" /> : <span className="rec" />}
      <Wave level={level} />
      <span className="timer">{elapsed}</span>
      {state.app ? <span className="target">→ {state.app}</span> : null}
      {state.locked ? (
        <span className="fade-in flex items-center gap-1.5 pl-1">
          <button type="button" className="btn" aria-label="Cancel" title="Cancel (Esc)" onClick={() => void dictationCancel()}>
            <X className="size-3.5" strokeWidth={2.25} />
          </button>
          <button type="button" className="btn primary" onClick={() => void dictationStop()}>
            <Square className="size-2.5" strokeWidth={0} fill="currentColor" />
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
      <span className="dots" aria-hidden="true">
        <i />
        <i />
        <i />
      </span>
      <span className="label shimmer">{label}</span>
    </>
  )
}

function Done({ preview, app }: { preview: string; app: string | null }) {
  return (
    <>
      <span className="badge">
        <svg className="check" width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true">
          <path d="M2.2 6.3 4.8 8.8 9.8 3.4" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </span>
      <span className="preview fade-in">{preview}</span>
      {app ? <span className="target fade-in">→ {app}</span> : null}
    </>
  )
}

function Notice({ tone, title, detail }: { tone: 'info' | 'warn' | 'error'; title: string; detail: string | null }) {
  const Icon = tone === 'info' ? Info : TriangleAlert
  return (
    <>
      <span className="badge" data-tone={tone}>
        <Icon className="size-3" strokeWidth={2.25} />
      </span>
      <span className="label">{title}</span>
      {detail ? <span className="detail fade-in">{detail}</span> : null}
    </>
  )
}

function Cancelled() {
  return (
    <>
      <span className="badge" data-tone="muted">
        <Ban className="size-3" strokeWidth={2.25} />
      </span>
      <span className="label" style={{ color: 'var(--pill-muted)' }}>
        Cancelled
      </span>
    </>
  )
}

function Body({ state, level }: { state: PillState; level: React.RefObject<number> }) {
  switch (state.kind) {
    case 'listening':
      return <Listening state={state} level={level} />
    case 'working':
      return <Working label={state.label} />
    case 'done':
      return <Done preview={state.preview} app={state.app} />
    case 'notice':
      return <Notice tone={state.tone} title={state.title} detail={state.detail} />
    case 'cancelled':
      return <Cancelled />
    case 'hidden':
      return null
  }
}

/** Short, spoken summary of the state for screen readers. */
function announce(state: PillState): string {
  switch (state.kind) {
    case 'listening':
      return state.locked ? 'Dictating hands-free' : 'Listening'
    case 'working':
      return state.label
    case 'done':
      return `Inserted: ${state.preview}`
    case 'notice':
      return [state.title, state.detail].filter(Boolean).join('. ')
    case 'cancelled':
      return 'Cancelled'
    case 'hidden':
      return ''
  }
}

/** The pill: one shape that morphs between listening, working, done and notices. */
export function Pill({ initial = { kind: 'hidden' } }: { initial?: PillState }) {
  const [state, setState] = useState<PillState>(initial)
  // The last visible state, kept while the pill fades out so its content does not vanish first.
  const [shown, setShown] = useState<PillState>(initial)
  // Bumped whenever the window becomes visible, to replay the entrance.
  const [entrance, setEntrance] = useState(0)
  const level = useRef(0)

  useEffect(() => {
    if (!isTauri()) return
    const win = getCurrentWebviewWindow()
    const unlisten = [
      win.listen<PillState>('dictation://overlay', (e) => {
        setState(e.payload)
        if (e.payload.kind !== 'hidden') setShown(e.payload)
        if (e.payload.kind !== 'listening') level.current = 0
      }),
      win.listen<{ session: number; level: number }>('dictation://level', (e) => {
        level.current = e.payload.level
      }),
    ]
    const onVisible = () => {
      if (document.visibilityState === 'visible') setEntrance((n) => n + 1)
    }
    document.addEventListener('visibilitychange', onVisible)
    return () => {
      document.removeEventListener('visibilitychange', onVisible)
      for (const p of unlisten) void p.then((f) => f())
    }
  }, [])

  const visible = state.kind !== 'hidden'
  const listening = shown.kind === 'listening'
  return (
    <div className="stage">
      <div className="glow" data-on={visible && listening} />
      <div
        key={`${entrance}-${'session' in shown ? shown.session : 0}`}
        className="pill"
        data-visible={visible}
        data-kind={shown.kind}
        role="status"
        aria-live="polite"
        aria-label={announce(shown)}
      >
        <Body state={shown} level={level} />
      </div>
    </div>
  )
}

