/**
 * Deterministic speech-like bar heights (0..1). Pure maths, no randomness, so
 * the same text always draws the same shape. `phrases` sets how many bursts appear.
 */
export function speechBars(count: number, { seed = 1, phrases = 3 } = {}) {
  return Array.from({ length: count }, (_, i) => {
    const t = i / count
    const syllable = Math.abs(Math.sin(i * 0.9 + seed) * Math.cos(i * 0.31 + seed * 2))
    const phrase = Math.abs(Math.sin(t * Math.PI * phrases + 0.08)) ** 0.6
    return Math.round((0.06 + 0.94 * syllable * phrase) * 100) / 100
  })
}
