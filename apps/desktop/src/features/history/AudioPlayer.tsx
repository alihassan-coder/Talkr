import { useRef, useState } from 'react'
import type { PointerEvent } from 'react'
import { Pause, Play } from 'lucide-react'
import { Waveform } from '@/components/Waveform'
import { speechBars } from '@/lib/waveform'
import { formatClock, hashSeed } from './utils'

/** Minimal player: play/pause, a seekable waveform and mono time. Key it by item id to reset. */
export function AudioPlayer({ src, seed, fallbackDurationMs }: { src: string; seed: string; fallbackDurationMs: number | null }) {
  const audioRef = useRef<HTMLAudioElement>(null)
  const [playing, setPlaying] = useState(false)
  const [current, setCurrent] = useState(0)
  const [duration, setDuration] = useState((fallbackDurationMs ?? 0) / 1000)
  const [failed, setFailed] = useState(false)
  const bars = speechBars(56, { seed: hashSeed(seed), phrases: 4 })

  if (failed) {
    return <p className="font-mono text-[11px] text-subtle">Audio file is not available.</p>
  }

  const toggle = () => {
    const audio = audioRef.current
    if (!audio) return
    if (audio.paused) void audio.play().catch(() => setFailed(true))
    else audio.pause()
  }

  const seek = (e: PointerEvent<HTMLDivElement>) => {
    const audio = audioRef.current
    if (!audio || !duration) return
    const rect = e.currentTarget.getBoundingClientRect()
    const ratio = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width))
    audio.currentTime = ratio * duration
    setCurrent(audio.currentTime)
  }

  const progress = duration ? Math.min(1, current / duration) : 0

  return (
    <div className="flex items-center gap-3 rounded-xl border border-line bg-surface px-2.5 py-2">
      <audio
        ref={audioRef}
        src={src}
        preload="metadata"
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={() => setPlaying(false)}
        onTimeUpdate={(e) => setCurrent(e.currentTarget.currentTime)}
        onLoadedMetadata={(e) => {
          if (Number.isFinite(e.currentTarget.duration)) setDuration(e.currentTarget.duration)
        }}
        onError={() => setFailed(true)}
      />
      <button
        type="button"
        onClick={toggle}
        aria-label={playing ? 'Pause' : 'Play'}
        className="grid size-8 shrink-0 place-items-center rounded-full bg-accent text-on-accent transition-transform duration-200 ease-out-quint active:scale-95"
      >
        {playing ? (
          <Pause className="size-3.5" fill="currentColor" strokeWidth={0} />
        ) : (
          <Play className="ml-0.5 size-3.5" fill="currentColor" strokeWidth={0} />
        )}
      </button>
      <div
        role="slider"
        tabIndex={0}
        aria-label="Seek"
        aria-valuemin={0}
        aria-valuemax={Math.round(duration)}
        aria-valuenow={Math.round(current)}
        onPointerDown={seek}
        onKeyDown={(e) => {
          const audio = audioRef.current
          if (!audio) return
          if (e.key === 'ArrowRight') audio.currentTime = Math.min(duration, audio.currentTime + 5)
          else if (e.key === 'ArrowLeft') audio.currentTime = Math.max(0, audio.currentTime - 5)
          else if (e.key === ' ') {
            e.preventDefault()
            toggle()
          }
        }}
        className="relative h-7 min-w-0 flex-1 cursor-pointer"
      >
        <Waveform bars={bars} className="absolute inset-0 size-full text-line-strong" />
        <div className="absolute inset-0 overflow-hidden" style={{ clipPath: `inset(0 ${100 - progress * 100}% 0 0)` }}>
          <Waveform bars={bars} className="size-full text-accent" />
        </div>
      </div>
      <span className="shrink-0 font-mono text-[11px] tabular-nums text-subtle">
        {formatClock(current * 1000)} / {formatClock(duration * 1000)}
      </span>
    </div>
  )
}
