import { useEffect, useRef, useState } from 'react'
import { Pause, Play } from 'lucide-react'
import type { OverlayPosition } from '@/lib/types'
import { cx } from '@/lib/cx'
import { useUi } from '@/stores/ui'
import { resolvePalette } from '@/lib/themes'
import { PillView } from '@/overlay/PillView'
import type { PillState } from '@/overlay/state'
import { pillSamples, speechLevel } from '@/overlay/samples'
import type { PillSample } from '@/overlay/samples'
import { pasteKeys } from '@/features/dictation/shortcut'
import { useKeyPlatform } from '@/features/dictation/platform'
import '@/overlay/pill.css'

const choices: { id: PillSample; label: string }[] = [
  { id: 'listening', label: 'Listening' },
  { id: 'handsFree', label: 'Hands-free' },
  { id: 'working', label: 'Transcribing' },
  { id: 'done', label: 'Inserted' },
  { id: 'copied', label: 'Copied' },
  { id: 'micError', label: 'Mic error' },
  { id: 'cancelled', label: 'Cancelled' },
]

/** One dictation, start to finish, for the preview to play on a loop. */
const story: { id: PillSample | 'hidden'; ms: number }[] = [
  { id: 'listening', ms: 3200 },
  { id: 'working', ms: 1300 },
  { id: 'done', ms: 2400 },
  { id: 'hidden', ms: 1100 },
]

const calm = () => typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches

/**
 * The real pill on a small stand-in desktop, in every state it can show. It plays one
 * dictation on a loop while it is on screen (unless motion is reduced); clicking a state stops
 * the loop and shows that state. The pill sits where the position setting puts it.
 */
export function PillPreview({ position, showTarget }: { position: OverlayPosition; showTarget: boolean }) {
  const platform = useKeyPlatform()
  const theme = useUi((s) => s.theme)
  const customAccent = useUi((s) => s.customAccent)
  const palette = resolvePalette(theme, customAccent, 'dark')

  const [current, setCurrent] = useState<PillSample | 'hidden'>('listening')
  // Plays by itself unless motion is reduced; picking a state pauses it.
  const [paused, setPaused] = useState(calm)
  const playing = !paused
  const [inView, setInView] = useState(false)
  const [run, setRun] = useState(0)
  const stage = useRef<HTMLDivElement>(null)
  const level = useRef(0)

  // Play only while visible: no work for a preview nobody is looking at.
  useEffect(() => {
    const el = stage.current
    if (!el || typeof IntersectionObserver === 'undefined') return
    const io = new IntersectionObserver(([e]) => setInView(!!e?.isIntersecting), { threshold: 0.3 })
    io.observe(el)
    return () => io.disconnect()
  }, [])

  useEffect(() => {
    if (!playing || !inView) return
    let step = Math.max(0, story.findIndex((s) => s.id === current))
    let t: ReturnType<typeof setTimeout>
    const next = () => {
      t = setTimeout(() => {
        step = (step + 1) % story.length
        setCurrent(story[step]!.id)
        next()
      }, story[step]!.ms)
    }
    next()
    return () => clearTimeout(t)
    // `current` only seeds where the loop resumes; changing it must not restart the timer.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playing, inView])

  const listening = current === 'listening' || current === 'handsFree'
  useEffect(() => {
    if (!listening) {
      level.current = 0
      return
    }
    let frame = 0
    const start = performance.now()
    const tick = (now: number) => {
      level.current = speechLevel((now - start) / 1000 + 0.6)
      frame = requestAnimationFrame(tick)
    }
    frame = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(frame)
  }, [listening])

  const sample = (id: PillSample | 'hidden'): PillState => {
    if (id === 'hidden') return { kind: 'hidden' }
    const s = pillSamples[id]
    const session = run * 10 + ('session' in s ? s.session : 0)
    if (id === 'copied') return { ...pillSamples.copied, session, title: `Copied, press ${pasteKeys(platform)} to paste` }
    if (s.kind === 'listening' || s.kind === 'done') return { ...s, session, app: showTarget ? s.app : null }
    return { ...s, session }
  }

  const choose = (id: PillSample) => {
    setPaused(true)
    if (id === 'listening' || id === 'handsFree') setRun((n) => n + 1)
    setCurrent(id)
  }

  const typed = current === 'done' || current === 'hidden'
  return (
    <div className="space-y-3 px-5 py-4">
      <div
        ref={stage}
        className="dict-desk relative h-[232px] overflow-hidden rounded-xl border border-line"
        style={{ ['--pill-theme-accent' as string]: palette.accent, ['--pill-theme-on-accent' as string]: palette.onAccent }}
      >
        {/* A stand-in app with a message box, where the words land. */}
        <div
          aria-hidden="true"
          className={cx(
            'dict-desk-window absolute inset-x-[12%] rounded-lg border border-line bg-surface shadow-[var(--shadow-pop)] transition-[top] duration-500 ease-out-quint',
            position === 'top' ? 'top-[34%]' : 'top-[12%]',
          )}
        >
          <div className="flex items-center gap-1.5 border-b border-line px-3 py-2">
            <span className="size-2 rounded-full bg-fg/15" />
            <span className="size-2 rounded-full bg-fg/15" />
            <span className="size-2 rounded-full bg-fg/15" />
            <span className="ml-2 text-[10.5px] font-medium text-subtle">#launch · Slack</span>
          </div>
          <div className="space-y-2 px-3.5 py-3">
            <div className="flex gap-2">
              <span className="size-5 shrink-0 rounded-md bg-fg/10" />
              <div className="space-y-1 pt-0.5">
                <span className="block h-1.5 w-24 rounded-full bg-fg/12" />
                <span className="block h-1.5 w-44 rounded-full bg-fg/8" />
              </div>
            </div>
            <div className="flex h-8 items-center rounded-md border border-line bg-bg/70 px-2.5 text-[12px]">
              {typed ? (
                <span key={run} className="dict-typed truncate text-fg">
                  {pillSamples.done.preview}
                </span>
              ) : (
                <span className="text-subtle">Message #launch</span>
              )}
              <span className="dict-caret ml-px h-3.5 w-px bg-accent" />
            </div>
          </div>
        </div>

        <div
          className={cx(
            'absolute inset-x-0 flex justify-center transition-[top] duration-500 ease-out-quint',
            position === 'top' ? 'top-3' : 'top-[calc(100%-60px)]',
          )}
        >
          <PillView
            state={sample(current)}
            level={level}
            entrance={run}
            onStop={() => {
              setCurrent('working')
              setTimeout(() => setCurrent((c) => (c === 'working' ? 'done' : c)), 1200)
            }}
            onCancel={() => setCurrent('cancelled')}
            className="dict-preview-pill"
          />
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        <button
          type="button"
          onClick={() => {
            if (!playing && !story.some((s) => s.id === current)) setCurrent('listening')
            setPaused((p) => !p)
          }}
          aria-label={playing ? 'Pause the preview' : 'Play the preview'}
          className="mr-1 grid size-7 place-items-center rounded-full border border-line text-muted transition-colors hover:border-line-strong hover:text-fg"
        >
          {playing ? <Pause className="size-3" strokeWidth={2.25} fill="currentColor" /> : <Play className="size-3 translate-x-px" strokeWidth={2.25} fill="currentColor" />}
        </button>
        <span role="group" aria-label="Pill states" className="contents">
          {choices.map((c) => (
            <button
              key={c.id}
              type="button"
              aria-pressed={current === c.id}
              onClick={() => choose(c.id)}
              className={cx(
                'h-7 rounded-full px-2.5 text-[12px] font-medium transition-colors duration-200',
                current === c.id ? 'bg-fg text-bg' : 'text-muted hover:bg-fg/[0.06] hover:text-fg',
              )}
            >
              {c.label}
            </button>
          ))}
        </span>
      </div>
    </div>
  )
}
