import { levelToHeight } from '@/overlay/level'

/** Geometry of the waveform, in SVG user units (CSS pixels). */
export const WAVE = { bars: 21, pitch: 4.6, height: 22, stroke: 2.6 } as const

export const waveWidth = (WAVE.bars - 1) * WAVE.pitch + WAVE.stroke

/**
 * One SVG path for every bar: a vertical stroke per bar, with round caps, so short bars stay
 * perfect dots and tall ones perfect capsules at any height (scaling a box would squash its
 * rounded ends). `values` are 0..1.
 */
export function wavePath(values: readonly number[]): string {
  const { pitch, height, stroke } = WAVE
  const mid = height / 2
  const reach = (height - stroke) / 2
  let d = ''
  for (let i = 0; i < values.length; i++) {
    const x = stroke / 2 + i * pitch
    const amp = Math.max(0.01, Math.min(1, values[i] ?? 0) * reach)
    d += `M${x.toFixed(2)} ${(mid - amp).toFixed(2)}V${(mid + amp).toFixed(2)}`
  }
  return d
}

/**
 * The waveform's motion: the newest level enters in the middle and flows outwards, shaped by
 * a soft bell so the edges stay calmer, with each bar easing towards its target. In silence it
 * breathes a gentle ripple so the pill never looks frozen (unless motion is reduced).
 */
export class WaveMotion {
  private history: number[]
  private shown: number[]
  private smooth = 0
  private shift = 0
  private last = -1

  constructor(private readonly calm = false) {
    const half = Math.ceil(WAVE.bars / 2)
    this.history = new Array<number>(half).fill(0)
    this.shown = new Array<number>(WAVE.bars).fill(0)
  }

  /** Advance to `now` (ms) with the latest microphone RMS; returns the bar heights and the level. */
  step(now: number, rms: number): { bars: number[]; level: number } {
    const dt = this.last < 0 ? 16 : Math.min(64, now - this.last)
    this.last = now
    const target = levelToHeight(rms)
    // Fast attack, slower release, like a VU meter.
    this.smooth += (target - this.smooth) * (target > this.smooth ? 0.5 : 0.1)
    this.shift += dt
    if (this.shift >= 42) {
      this.shift = 0
      this.history.pop()
      this.history.unshift(this.smooth)
    } else {
      this.history[0] = Math.max(this.history[0] ?? 0, this.smooth)
    }
    const half = (WAVE.bars - 1) / 2
    for (let i = 0; i < WAVE.bars; i++) {
      const fromCentre = Math.abs(i - half)
      const h = this.history[Math.min(this.history.length - 1, Math.round(fromCentre))] ?? 0
      const bell = 1 - 0.6 * (fromCentre / half) ** 2
      const idle = this.calm ? 0.1 : 0.08 + 0.14 * (0.5 + 0.5 * Math.sin(now / 340 - fromCentre * 0.7))
      const goal = Math.max(idle * bell + 0.02, h * bell)
      const cur = this.shown[i] ?? 0
      this.shown[i] = cur + (goal - cur) * (goal > cur ? 0.45 : 0.2)
    }
    return { bars: this.shown, level: this.smooth }
  }
}
