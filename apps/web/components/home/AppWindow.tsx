'use client'

import type { CSSProperties } from 'react'
import { useState } from 'react'
import { Boxes, History, Mic, Settings, Volume2 } from 'lucide-react'
import { LogoMark } from '@/components/Logo'
import { Waveform, speechBars } from '@/components/Waveform'

type View = 'transcribe' | 'speak'

const views: { id: View; label: string; description: string }[] = [
  {
    id: 'transcribe',
    label: 'Transcribe',
    description: 'The Transcribe screen turning a 12 second interview into timestamped text with Whisper, offline, on the GPU.',
  },
  {
    id: 'speak',
    label: 'Speak',
    description: 'The Speak screen turning a paragraph into audio with the Kokoro voice af_heart, offline.',
  },
]

const nav: { icon: typeof Mic; label: string; view?: View }[] = [
  { icon: Volume2, label: 'Speak', view: 'speak' },
  { icon: Mic, label: 'Transcribe', view: 'transcribe' },
  { icon: Boxes, label: 'Models' },
  { icon: History, label: 'History' },
  { icon: Settings, label: 'Settings' },
]

const segments = [
  { at: '00:00', text: 'So where does the recording actually go?' },
  { at: '00:04', text: 'Nowhere. It gets saved to a folder on this laptop.' },
  { at: '00:08', text: 'And the transcript is made here too, with the wifi off.' },
]

const transcribeBars = speechBars(110, { seed: 2, phrases: 3 })
const speakBars = speechBars(120, { seed: 7, phrases: 4 })

const SPEAK_TEXT =
  'Good morning, everyone. The quarterly numbers came in overnight, and they are better than we hoped. Here is the short version before the long one.'

function Playback({ bars, ticks }: { bars: number[]; ticks: string[] }) {
  return (
    <>
      <div className="relative h-16 md:h-20">
        <Waveform bars={bars} className="absolute inset-0 size-full text-fg/15" />
        <Waveform
          bars={bars}
          className="absolute inset-0 size-full animate-reveal text-fg motion-reduce:animate-none motion-reduce:[clip-path:inset(0_60%_0_0)]"
        />
        <div className="pointer-events-none absolute inset-0 animate-playhead motion-reduce:hidden">
          <span className="absolute -inset-y-2 left-0 w-px bg-fg" />
        </div>
      </div>
      <div className="mt-2 flex justify-between font-mono text-[10px] text-fg/30">
        {ticks.map((t) => (
          <span key={t}>{t}</span>
        ))}
      </div>
    </>
  )
}

function TranscribeView() {
  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-fg/[0.08] px-5 py-4 md:px-7">
        <div>
          <p className="text-[15px] font-medium tracking-[-0.01em]">interview-03.m4a</p>
          <p className="mt-0.5 font-mono text-[11px] text-fg/40">0:12 · 44.1 kHz · stereo</p>
        </div>
        <div className="flex items-center gap-2">
          <span className="rounded-md border border-fg/10 px-2 py-1 font-mono text-[11px] text-fg/55">whisper-base.en</span>
          <span className="rounded-md bg-fg px-2.5 py-1 text-[11px] font-medium text-bg">Transcribe</span>
        </div>
      </div>

      <div className="px-5 pt-7 md:px-7">
        <Playback bars={transcribeBars} ticks={['0:00', '0:04', '0:08', '0:12']} />
      </div>

      <ol className="flex-1 space-y-1 px-3 py-5 md:px-5">
        {segments.map((s, i) => (
          <li
            key={s.at}
            style={{ animationDelay: `${i * 4}s` } as CSSProperties}
            className={`flex gap-4 rounded-lg px-2 py-2.5 opacity-35 animate-segment motion-reduce:animate-none md:gap-6 ${
              i === 0 ? 'motion-reduce:opacity-100' : ''
            }`}
          >
            <span className="pt-0.5 font-mono text-[11px] tabular-nums text-fg/45">{s.at}</span>
            <span className="text-[15px] leading-snug md:text-base">{s.text}</span>
          </li>
        ))}
      </ol>

      <div className="flex items-center justify-between gap-3 border-t border-fg/[0.08] px-5 py-3 md:px-7">
        <span className="font-mono text-[11px] text-fg/40">Done in 0.41 s · 0 bytes uploaded</span>
        <span className="flex gap-1.5">
          {['TXT', 'SRT', 'WAV'].map((f) => (
            <span key={f} className="rounded border border-fg/10 px-1.5 py-0.5 font-mono text-[10px] text-fg/55">
              {f}
            </span>
          ))}
        </span>
      </div>
    </>
  )
}

function SpeakView() {
  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-fg/[0.08] px-5 py-4 md:px-7">
        <div>
          <p className="text-[15px] font-medium tracking-[-0.01em]">Morning update</p>
          <p className="mt-0.5 font-mono text-[11px] text-fg/40">{SPEAK_TEXT.length} characters · English</p>
        </div>
        <div className="flex items-center gap-2">
          <span className="rounded-md border border-fg/10 px-2 py-1 font-mono text-[11px] text-fg/55">kokoro-multi-lang</span>
          <span className="rounded-md bg-fg px-2.5 py-1 text-[11px] font-medium text-bg">Generate</span>
        </div>
      </div>

      <div className="px-5 pt-6 md:px-7">
        <div className="rounded-xl border border-fg/10 bg-fg/[0.02] p-4">
          <p className="text-[15px] leading-relaxed text-fg/85 md:text-base">
            {SPEAK_TEXT}
            <span className="ml-0.5 inline-block h-[1.1em] w-px translate-y-[0.2em] animate-caret bg-fg motion-reduce:animate-none" />
          </p>
          <div className="mt-4 flex flex-wrap gap-2 border-t border-fg/[0.08] pt-3 text-xs">
            {[
              ['Voice', 'af_heart'],
              ['Speed', '1.0×'],
              ['Language', 'English (US)'],
            ].map(([k, v]) => (
              <span key={k} className="rounded-full border border-fg/10 px-2.5 py-1 text-fg/50">
                {k} <span className="text-fg">{v}</span>
              </span>
            ))}
          </div>
        </div>
      </div>

      <div className="flex-1 px-5 pb-5 pt-6 md:px-7">
        <div className="flex items-center gap-4">
          <span className="grid size-9 shrink-0 place-items-center rounded-full bg-fg">
            <span className="flex gap-[3px]">
              <span className="h-3 w-[3px] rounded-sm bg-bg" />
              <span className="h-3 w-[3px] rounded-sm bg-bg" />
            </span>
          </span>
          <div className="min-w-0 flex-1">
            <Playback bars={speakBars} ticks={['0:00', '0:03', '0:06', '0:09']} />
          </div>
        </div>
      </div>

      <div className="flex items-center justify-between gap-3 border-t border-fg/[0.08] px-5 py-3 md:px-7">
        <span className="font-mono text-[11px] text-fg/40">Rendered in 0.9 s · 24 kHz WAV</span>
        <span className="rounded border border-fg/10 px-1.5 py-0.5 font-mono text-[10px] text-fg/55">Save WAV</span>
      </div>
    </>
  )
}

/** A faithful, animated recreation of the desktop app, switchable between its two tools. */
export function AppWindow() {
  const [view, setView] = useState<View>('transcribe')
  const active = views.find((v) => v.id === view) ?? views[0]!

  return (
    <div>
      <div className="flex justify-center">
        <div role="tablist" aria-label="App screens" className="relative inline-grid grid-cols-2 rounded-full border border-fg/10 bg-fg/[0.03] p-1">
          <span
            aria-hidden="true"
            className={`absolute inset-y-1 left-1 w-[calc(50%-4px)] rounded-full bg-fg transition-transform duration-500 ease-out-quint ${
              view === 'speak' ? 'translate-x-full' : ''
            }`}
          />
          {views.map((v) => (
            <button
              key={v.id}
              type="button"
              role="tab"
              id={`tab-${v.id}`}
              aria-selected={view === v.id}
              aria-controls="app-window-panel"
              onClick={() => setView(v.id)}
              className={`relative z-10 rounded-full px-5 py-1.5 text-sm font-medium transition-colors duration-300 ${
                view === v.id ? 'text-bg' : 'text-fg/55 hover:text-fg'
              }`}
            >
              {v.label}
            </button>
          ))}
        </div>
      </div>

      <figure id="app-window-panel" role="tabpanel" aria-labelledby={`tab-${view}`} className="relative mt-6">
        <figcaption className="sr-only">{active.description}</figcaption>

        <div
          aria-hidden="true"
          className="overflow-hidden rounded-2xl border border-fg/10 bg-bg shadow-[0_0_0_1px_color-mix(in_oklab,var(--color-bg)_80%,transparent),0_40px_120px_-20px_color-mix(in_oklab,var(--color-fg)_9%,transparent)]"
        >
          <div className="flex h-10 items-center gap-2 border-b border-fg/[0.08] bg-fg/[0.02] px-4">
            <span className="size-2.5 rounded-full bg-fg/15" />
            <span className="size-2.5 rounded-full bg-fg/15" />
            <span className="size-2.5 rounded-full bg-fg/15" />
            <span className="mx-auto pr-12 text-xs text-fg/40">Talkr · {active.label}</span>
          </div>

          <div className="flex min-h-[440px] text-left md:min-h-[480px]">
            <aside className="hidden w-52 shrink-0 flex-col border-r border-fg/[0.08] bg-fg/[0.015] p-3 md:flex">
              <div className="flex items-center gap-2 px-2 py-2">
                <LogoMark className="size-5" />
                <span className="text-sm font-semibold tracking-[-0.02em]">Talkr</span>
              </div>
              <ul className="mt-4 space-y-0.5">
                {nav.map(({ icon: Icon, label, view: target }) => (
                  <li key={label}>
                    <button
                      type="button"
                      tabIndex={-1}
                      disabled={!target}
                      onClick={() => target && setView(target)}
                      className={`flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-[13px] transition-colors ${
                        target === view ? 'bg-fg/[0.07] text-fg' : 'text-fg/45'
                      } ${target && target !== view ? 'hover:bg-fg/[0.04] hover:text-fg/80' : ''}`}
                    >
                      <Icon className="size-4" strokeWidth={1.75} />
                      {label}
                    </button>
                  </li>
                ))}
              </ul>
              <div className="mt-auto rounded-lg border border-fg/[0.08] p-3">
                <p className="flex items-center gap-2 text-xs text-fg/70">
                  <span className="size-1.5 rounded-full bg-fg" />
                  GPU · Metal
                </p>
                <p className="mt-1 font-mono text-[11px] text-fg/35">Apple M2 · 16 GB</p>
              </div>
            </aside>

            <div key={view} className="flex min-w-0 flex-1 animate-rise flex-col motion-reduce:animate-none">
              {view === 'transcribe' ? <TranscribeView /> : <SpeakView />}
            </div>
          </div>
        </div>
      </figure>
    </div>
  )
}
