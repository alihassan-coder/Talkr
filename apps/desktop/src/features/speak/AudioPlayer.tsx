import type { KeyboardEvent, MouseEvent, ReactNode } from 'react'
import { useEffect, useRef, useState } from 'react'
import { Pause, Play } from 'lucide-react'
import { Card } from '@/components/ui'
import { Waveform } from '@/components/Waveform'
import { speechBars } from '@/lib/waveform'
import { toastError } from '@/stores/toast'
import { useAudioUrl } from '@/lib/useAudioUrl'
import { formatDuration } from './utils'

/** Card with a custom player: a waveform that fills in as the audio plays. */
export function AudioPlayer({
  path,
  seed,
  title,
  meta,
  footer,
  actions,
  fallbackDurationMs = 0,
  autoPlay = false,
}: {
  path: string
  seed: number
  title: string
  meta: ReactNode
  footer: ReactNode
  actions: ReactNode
  fallbackDurationMs?: number
  autoPlay?: boolean
}) {
  const { url, error } = useAudioUrl(path)
  const audioRef = useRef<HTMLAudioElement>(null)
  const [playing, setPlaying] = useState(false)
  const [time, setTime] = useState(0)
  const [duration, setDuration] = useState(fallbackDurationMs / 1000)

  // Smooth playhead while playing; timeupdate alone only fires ~4 times a second.
  useEffect(() => {
    if (!playing) return
    let frame = 0
    const tick = () => {
      if (audioRef.current) setTime(audioRef.current.currentTime)
      frame = requestAnimationFrame(tick)
    }
    frame = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(frame)
  }, [playing])

  const bars = speechBars(140, { seed, phrases: 4 })
  const ratio = duration > 0 ? Math.min(1, time / duration) : 0

  useEffect(() => {
    if (error) toastError(error)
  }, [error])

  const toggle = () => {
    const audio = audioRef.current
    if (!audio) return
    if (audio.paused) audio.play().catch(toastError)
    else audio.pause()
  }

  const seekTo = (seconds: number) => {
    const audio = audioRef.current
    if (!audio || !duration) return
    audio.currentTime = Math.min(duration, Math.max(0, seconds))
    setTime(audio.currentTime)
  }

  const onWaveClick = (e: MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect()
    seekTo(((e.clientX - rect.left) / rect.width) * duration)
  }

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return
    if (e.key === ' ') {
      e.preventDefault()
      toggle()
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault()
      seekTo(time + (e.key === 'ArrowLeft' ? -5 : 5))
    }
  }

  return (
    <Card className="animate-rise overflow-hidden">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line px-6 py-4">
        <div className="min-w-0">
          <p className="truncate text-[15px] font-medium tracking-[-0.01em]">{title}</p>
          <p className="mt-0.5 font-mono text-[11px] text-subtle">{meta}</p>
        </div>
        <div className="flex items-center gap-2">{actions}</div>
      </div>

      <div
        tabIndex={0}
        role="group"
        aria-label="Audio player. Space to play or pause, arrows to seek."
        onKeyDown={onKeyDown}
        className="flex items-center gap-5 rounded-b-2xl px-6 pb-5 pt-6"
      >
        <button
          type="button"
          onClick={toggle}
          disabled={!url}
          aria-label={playing ? 'Pause' : 'Play'}
          className="grid size-11 shrink-0 place-items-center rounded-full bg-accent text-on-accent transition-transform duration-200 ease-out-quint hover:scale-[1.04] active:scale-95"
        >
          {playing ? (
            <Pause className="size-4" fill="currentColor" strokeWidth={0} />
          ) : (
            <Play className="size-4 translate-x-px" fill="currentColor" strokeWidth={0} />
          )}
        </button>

        <div className="min-w-0 flex-1">
          <div className="relative h-16 cursor-pointer" onClick={onWaveClick}>
            <Waveform bars={bars} className="absolute inset-0 size-full text-line-strong" />
            {/* clip-path keeps both layers aligned bar for bar */}
            <div className="absolute inset-0" style={{ clipPath: `inset(0 ${100 - ratio * 100}% 0 0)` }}>
              <Waveform bars={bars} className="size-full text-accent" />
            </div>
            {time > 0 ? (
              <span className="pointer-events-none absolute -inset-y-1.5 w-px bg-accent" style={{ left: `${ratio * 100}%` }} />
            ) : null}
          </div>
          <div className="mt-2 flex justify-between font-mono text-[10.5px] tabular-nums text-subtle">
            <span>{formatDuration(time * 1000)}</span>
            <span>{formatDuration(duration * 1000)}</span>
          </div>
        </div>
      </div>

      <div className="border-t border-line px-6 py-3 font-mono text-[11px] text-subtle">{footer}</div>

      <audio
        ref={audioRef}
        src={url ?? undefined}
        autoPlay={autoPlay}
        preload="metadata"
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={(e) => {
          setPlaying(false)
          setTime(e.currentTarget.duration)
        }}
        onTimeUpdate={(e) => setTime(e.currentTarget.currentTime)}
        onLoadedMetadata={(e) => {
          if (Number.isFinite(e.currentTarget.duration)) setDuration(e.currentTarget.duration)
        }}
        onError={(event) => {
          const mediaError = event.currentTarget.error
          console.error('Could not decode the audio file', { code: mediaError?.code, message: mediaError?.message })
          toastError('Could not decode the audio file')
        }}
      />
    </Card>
  )
}
