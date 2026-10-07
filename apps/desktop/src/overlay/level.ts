/** RMS (0..1) to a 0..1 bar height on a log scale: quiet rooms rest low, speech fills the bars. */
export function levelToHeight(rms: number): number {
  if (!(rms > 0)) return 0
  const db = 20 * Math.log10(rms)
  return Math.min(1, Math.max(0, (db + 58) / 42))
}
