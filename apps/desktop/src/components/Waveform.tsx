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
