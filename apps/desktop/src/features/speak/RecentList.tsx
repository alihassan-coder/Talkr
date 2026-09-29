import { Play } from 'lucide-react'
import { Kicker } from '@/components/ui'
import { cx } from '@/lib/cx'
import type { HistoryItem } from '@/lib/types'
import { formatDuration, relativeTime } from './utils'

export function RecentList({
  items,
  activeId,
  onSelect,
}: {
  items: HistoryItem[]
  activeId: string | null
  onSelect: (item: HistoryItem) => void
}) {
  if (items.length === 0) return null
  return (
    <section>
      <Kicker className="mb-3 px-1">Recent</Kicker>
      <ul className="divide-y divide-fg/[0.06] rounded-2xl border border-fg/10">
        {items.map((item) => (
          <li key={item.id}>
            <button
              type="button"
              onClick={() => onSelect(item)}
              className={cx(
                'group flex w-full items-center gap-4 px-4 py-3 text-left transition-colors duration-200 first:rounded-t-2xl hover:bg-fg/[0.04]',
                item.id === activeId && 'bg-fg/[0.04]',
              )}
            >
              <span className="grid size-7 shrink-0 place-items-center rounded-full border border-fg/12 text-fg/50 transition-colors group-hover:border-fg/25 group-hover:text-fg">
                <Play className="size-3 translate-x-px" fill="currentColor" strokeWidth={0} />
              </span>
              <span className="min-w-0 flex-1 truncate text-[13.5px] text-fg/85">{item.title}</span>
              {item.voiceId ? (
                <span className="hidden max-w-32 truncate font-mono text-[11px] text-fg/40 sm:block">{item.voiceId}</span>
              ) : null}
              <span className="w-10 text-right font-mono text-[11px] tabular-nums text-fg/40">
                {item.durationMs === null ? '–' : formatDuration(item.durationMs)}
              </span>
              <span className="w-20 text-right text-[12px] text-fg/35">{relativeTime(item.createdAt)}</span>
            </button>
          </li>
        ))}
      </ul>
    </section>
  )
}
