/** m:ss, or h:mm:ss past an hour. */
export function formatDuration(ms: number) {
  const total = Math.max(0, Math.round(ms / 1000))
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = String(total % 60).padStart(2, '0')
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`
}

export function formatSeconds(ms: number) {
  return `${(ms / 1000).toFixed(ms < 10_000 ? 2 : 1)} s`
}

const units: [Intl.RelativeTimeFormatUnit, number][] = [
  ['minute', 60_000],
  ['hour', 3_600_000],
  ['day', 86_400_000],
]

const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto', style: 'short' })

/** "just now", "5 min. ago", "yesterday", then a short date. */
export function relativeTime(timestamp: number, now = Date.now()) {
  const diff = now - timestamp
  if (diff < 45_000) return 'just now'
  if (diff < 7 * 86_400_000) {
    const [unit, size] = diff < 3_600_000 ? units[0]! : diff < 86_400_000 ? units[1]! : units[2]!
    return rtf.format(-Math.round(diff / size), unit)
  }
  return new Date(timestamp).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}

export const MAX_CHARS = 5000

/** Typical narration pace: about 15 characters (two and a half words) per second at 1×. */
const CHARS_PER_SECOND = 15

export function countWords(text: string) {
  const words = text.trim().match(/\S+/g)
  return words ? words.length : 0
}

/** Rough length of the speech for `text` at `speed`, in ms. */
export function estimateSpeechMs(text: string, speed = 1) {
  const chars = text.replace(/\s+/g, ' ').trim().length
  return Math.round((chars / CHARS_PER_SECOND / Math.max(0.25, speed)) * 1000)
}
