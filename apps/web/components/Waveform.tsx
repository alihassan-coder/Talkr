/**
 * Deterministic speech-like bar heights (0..1). Pure maths, no randomness, so
 * the server and client always agree. `phrases` controls how many bursts of
 * speech appear, separated by near-silent gaps.
 */
export function speechBars(count: number, { seed = 1, phrases = 3 } = {}) {
  return Array.from({ length: count }, (_, i) => {
    const t = i / count
    const syllable = Math.abs(Math.sin(i * 0.9 + seed) * Math.cos(i * 0.31 + seed * 2))
    const phrase = Math.abs(Math.sin(t * Math.PI * phrases + 0.08)) ** 0.6
    return Math.round((0.06 + 0.94 * syllable * phrase) * 100) / 100
  })
}

export function Waveform({ bars, className = '' }: { bars: number[]; className?: string }) {
  const step = 4
  return (
    <svg
      viewBox={`0 0 ${bars.length * step} 100`}
      preserveAspectRatio="none"
      className={className}
      aria-hidden="true"
      focusable="false"
    >
      {bars.map((h, i) => {
        const height = Math.max(h * 100, 3)
        return <rect key={i} x={i * step} y={(100 - height) / 2} width={2} height={height} rx={1} fill="currentColor" />
      })}
    </svg>
  )
}
