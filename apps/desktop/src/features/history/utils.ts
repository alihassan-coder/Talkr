import type { HistoryItem } from '@/lib/types'

const DAY = 86_400_000

export function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const exp = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)))
  const value = bytes / 1024 ** exp
  return `${value >= 100 || exp === 0 ? Math.round(value) : value.toFixed(1)} ${units[exp]}`
}

/** 42_000 -> "0:42", 3_723_000 -> "1:02:03". */
export function formatClock(ms: number) {
  const total = Math.max(0, Math.floor(ms / 1000))
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = String(total % 60).padStart(2, '0')
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`
}

export function formatDuration(ms: number | null) {
  return ms === null ? null : formatClock(ms)
}

const startOfDay = (ts: number) => {
  const d = new Date(ts)
  d.setHours(0, 0, 0, 0)
  return d.getTime()
}

export function relativeTime(ts: number, now = Date.now()) {
  const diff = now - ts
  if (diff < 60_000) return 'just now'
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`
  if (diff < DAY && startOfDay(ts) === startOfDay(now)) return `${Math.floor(diff / 3_600_000)}h ago`
  return new Date(ts).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
}

export function dayLabel(ts: number, now = Date.now()) {
  const days = Math.round((startOfDay(now) - startOfDay(ts)) / DAY)
  if (days === 0) return 'Today'
  if (days === 1) return 'Yesterday'
  const sameYear = new Date(ts).getFullYear() === new Date(now).getFullYear()
  return new Date(ts).toLocaleDateString(undefined, {
    weekday: days < 7 ? 'long' : undefined,
    month: 'short',
    day: 'numeric',
    year: sameYear ? undefined : 'numeric',
  })
}

export type DayGroup = { key: number; label: string; items: HistoryItem[] }

/** Items are already newest-first, so consecutive runs share a day. */
export function groupByDay(items: HistoryItem[], now = Date.now()): DayGroup[] {
  const groups: DayGroup[] = []
  for (const item of items) {
    const key = startOfDay(item.createdAt)
    const last = groups.at(-1)
    if (last && last.key === key) last.items.push(item)
    else groups.push({ key, label: dayLabel(item.createdAt, now), items: [item] })
  }
  return groups
}

/** Stable small integer from a string, for deterministic waveforms. */
export function hashSeed(value: string) {
  let h = 0
  for (let i = 0; i < value.length; i++) h = (h * 31 + value.charCodeAt(i)) | 0
  return (Math.abs(h) % 1000) / 100
}
