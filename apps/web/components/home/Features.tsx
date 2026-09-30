import type { CSSProperties, ReactNode } from 'react'
import { Search } from 'lucide-react'
import { Card, Container, Dim, SectionIntro } from '@/components/ui'
import { SpotlightGroup } from '@/components/SpotlightGroup'
import { Waveform, speechBars } from '@/components/Waveform'

const SPEAK_TEXT = 'The quarterly numbers came in this morning, and they are better than we hoped.'

function Tile({ title, body, className = '', children }: { title: string; body: string; className?: string; children: ReactNode }) {
  return (
    <Card className={`spotlight flex flex-col p-6 transition-colors duration-300 hover:border-fg/20 md:p-8 ${className}`}>
      <div aria-hidden="true" className="flex flex-1 flex-col justify-center">
        {children}
      </div>
      <h3 className="mt-8 text-lg font-medium tracking-[-0.02em]">{title}</h3>
      <p className="mt-1.5 max-w-md text-[15px] leading-relaxed text-fg/50">{body}</p>
    </Card>
  )
}

function SpeakDemo() {
  return (
    <div className="space-y-2">
      <div className="rounded-xl border border-fg/10 bg-bg p-4">
        <p className="text-[15px] leading-relaxed text-fg/85 md:text-base">
          {SPEAK_TEXT}
          <span className="ml-0.5 inline-block h-[1.1em] w-px translate-y-[0.2em] animate-caret bg-fg motion-reduce:animate-none" />
        </p>
        <div className="mt-4 flex flex-wrap items-center gap-2 border-t border-fg/[0.08] pt-3 text-xs">
          <span className="rounded-full border border-fg/10 px-2.5 py-1 text-fg/55">
            Voice <span className="text-fg">af_heart</span>
          </span>
          <span className="rounded-full border border-fg/10 px-2.5 py-1 text-fg/55">
            Speed <span className="text-fg">1.0×</span>
          </span>
          <span className="ml-auto font-mono text-[11px] text-fg/35">{SPEAK_TEXT.length} / 5,000</span>
        </div>
      </div>
      <div className="flex items-center gap-4 rounded-xl border border-fg/10 bg-bg px-4 py-3">
        <span className="grid size-8 shrink-0 place-items-center rounded-full bg-fg">
          <svg viewBox="0 0 10 10" className="ml-0.5 size-2.5 fill-bg">
            <path d="M1 0.5v9l8-4.5z" />
          </svg>
        </span>
        <Waveform bars={speechBars(70, { seed: 5, phrases: 2 })} className="h-7 min-w-0 flex-1 text-fg/70" />
        <span className="font-mono text-[11px] text-fg/45">0:04.2</span>
      </div>
    </div>
  )
}

const levels: CSSProperties[] = [
  { animationDelay: '-0.1s', animationDuration: '0.8s' },
  { animationDelay: '-0.5s', animationDuration: '1.1s' },
  { animationDelay: '-0.3s', animationDuration: '0.7s' },
  { animationDelay: '-0.8s', animationDuration: '1.2s' },
  { animationDelay: '-0.2s', animationDuration: '0.9s' },
  { animationDelay: '-0.6s', animationDuration: '1s' },
  { animationDelay: '-0.4s', animationDuration: '0.75s' },
  { animationDelay: '-0.7s', animationDuration: '1.05s' },
  { animationDelay: '-0.15s', animationDuration: '0.85s' },
]

function RecordDemo() {
  return (
    <div className="flex flex-col items-center py-4">
      <span className="relative grid size-20 place-items-center rounded-full border border-fg/15">
        <span className="absolute inset-0 animate-ping rounded-full border border-fg/10 [animation-duration:2.4s] motion-reduce:hidden" />
        <span className="size-6 rounded-full bg-fg" />
      </span>
      <div className="mt-7 flex h-8 items-center gap-[3px]">
        {levels.map((style, i) => (
          <span key={i} style={style} className="h-full w-[3px] origin-center animate-level rounded-full bg-fg/70 motion-reduce:animate-none" />
        ))}
      </div>
      <p className="mt-4 font-mono text-xs text-fg/45">00:12:08 · Built-in microphone</p>
    </div>
  )
}

function GpuDemo() {
  const lines = [
    ['Vulkan', 'RTX 3060 · 12 GB', 'in use'],
    ['CPU', '8 cores', 'fallback'],
  ]
  return (
    <ul className="divide-y divide-fg/[0.08] rounded-xl border border-fg/10 bg-bg font-mono text-xs">
      {lines.map(([name, detail, state]) => (
        <li key={name} className={`flex items-center gap-3 px-4 py-2.5 ${state === 'in use' ? 'text-fg' : 'text-fg/40'}`}>
          <span className={`size-1.5 rounded-full ${state === 'in use' ? 'bg-fg' : 'bg-fg/20'}`} />
          <span className="w-14">{name}</span>
          <span className="flex-1">{detail}</span>
          <span>{state}</span>
        </li>
      ))}
    </ul>
  )
}

function HistoryDemo() {
  const rows = [
    ['Q3 planning call', 'Transcript · 42 min'],
    ['Quarterly update intro', 'Speech · af_heart'],
  ]
  return (
    <div className="rounded-xl border border-fg/10 bg-bg">
      <div className="flex items-center gap-2 border-b border-fg/[0.08] px-4 py-3 text-sm">
        <Search className="size-3.5 text-fg/40" strokeWidth={2} />
        <span>quarterly</span>
        <span className="ml-auto font-mono text-[11px] text-fg/35">2 results</span>
      </div>
      <ul className="p-1.5">
        {rows.map(([title, meta], i) => (
          <li key={title} className={`rounded-lg px-3 py-2.5 ${i === 0 ? 'bg-fg/[0.05]' : ''}`}>
            <p className="text-sm">{title}</p>
            <p className="mt-0.5 font-mono text-[11px] text-fg/40">{meta}</p>
          </li>
        ))}
      </ul>
    </div>
  )
}

function ExportDemo() {
  return (
    <div className="flex items-center justify-center gap-3">
      {['.txt', '.srt', '.wav'].map((ext, i) => (
        <span
          key={ext}
          className={`grid h-24 w-20 place-items-end rounded-lg border border-fg/10 bg-bg p-2.5 font-mono text-xs text-fg/60 ${
            i === 1 ? '-translate-y-2 border-fg/25 text-fg' : ''
          }`}
        >
          {ext}
        </span>
      ))}
    </div>
  )
}

export function Features() {
  return (
    <section id="features" className="py-24 md:py-32">
      <Container>
        <SectionIntro
          kicker="Features"
          title={
            <>
              Two tools. <Dim>Zero servers.</Dim>
            </>
          }
          lede="One screen turns text into a voice. The other turns a voice into text. Everything in between stays on your disk."
        />

        <SpotlightGroup className="reveal mt-16 grid gap-4 md:grid-cols-6">
          <Tile
            className="md:col-span-4"
            title="Speak"
            body="Paste a paragraph, a script or a whole chapter. Pick a voice, set the speed, and save the result as a WAV."
          >
            <SpeakDemo />
          </Tile>
          <Tile
            className="md:col-span-2"
            title="Transcribe"
            body="Record from the mic or drop in an MP3, WAV, FLAC, OGG or M4A. Get text back with timestamps."
          >
            <RecordDemo />
          </Tile>
          <Tile
            className="md:col-span-2"
            title="Uses your GPU"
            body="Transcription runs on Vulkan or Metal, in its own process. No GPU, or it stops? Talkr quietly switches to the CPU."
          >
            <GpuDemo />
          </Tile>
          <Tile
            className="md:col-span-2"
            title="History that keeps"
            body="Everything you make is saved locally with full-text search and favorites."
          >
            <HistoryDemo />
          </Tile>
          <Tile
            className="md:col-span-2"
            title="Take it anywhere"
            body="Export transcripts as TXT or SRT subtitles, and speech as WAV."
          >
            <ExportDemo />
          </Tile>
        </SpotlightGroup>
      </Container>
    </section>
  )
}
