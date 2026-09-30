import { useEffect, useRef, useState } from 'react'
import { History, LoaderCircle, Mic, Search, SearchX, Star, Volume2, X } from 'lucide-react'
import { historyList, historyToggleFavorite, isTauri } from '@/lib/api'
import type { HistoryItem, HistoryKind } from '@/lib/types'
import { Button, Card, EmptyState, IconButton, Kbd, Kicker, PageHeader, Segmented } from '@/components/ui'
import { cx } from '@/lib/cx'
import { toastError } from '@/stores/toast'
import { HistoryDetail } from '@/features/history/HistoryDetail'
import { filterSample } from '@/features/history/sample'
import { formatDuration, groupByDay, relativeTime } from '@/features/history/utils'

type KindFilter = 'all' | HistoryKind

const kindOptions: { value: KindFilter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'tts', label: 'Speech' },
  { value: 'stt', label: 'Transcripts' },
]

export function HistoryPage() {
  const searchRef = useRef<HTMLInputElement>(null)
  const [input, setInput] = useState('')
  const [query, setQuery] = useState('')
  const [kind, setKind] = useState<KindFilter>('all')
  const [favoritesOnly, setFavoritesOnly] = useState(false)
  const [items, setItems] = useState<HistoryItem[]>([])
  const [cursor, setCursor] = useState<number | null>(null)
  const [loading, setLoading] = useState(true)
  const [loadingMore, setLoadingMore] = useState(false)
  const [selectedId, setSelectedId] = useState<string | null>(null)

  const filtered = query.trim() !== '' || kind !== 'all' || favoritesOnly
  const selected = items.find((i) => i.id === selectedId) ?? null

  // Debounce the search box.
  useEffect(() => {
    const t = setTimeout(() => setQuery(input.trim()), 200)
    return () => clearTimeout(t)
  }, [input])

  // First page whenever the filters change.
  useEffect(() => {
    let cancelled = false
    const kindParam = kind === 'all' ? null : kind
    const load = isTauri()
      ? historyList({ query, kind: kindParam, favoritesOnly })
      : Promise.resolve({ items: filterSample(query, kindParam, favoritesOnly), nextCursor: null })
    load
      .then((res) => {
        if (cancelled) return
        setItems(res.items)
        setCursor(res.nextCursor)
      })
      .catch((e: unknown) => {
        if (!cancelled) toastError(e)
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
    }
  }, [query, kind, favoritesOnly])

  // Ctrl+F focuses search, Esc clears it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 'f') {
        e.preventDefault()
        searchRef.current?.focus()
        searchRef.current?.select()
      } else if (e.key === 'Escape') {
        if (document.activeElement === searchRef.current && searchRef.current?.value) {
          setInput('')
          setQuery('')
        } else if (document.activeElement === searchRef.current) {
          searchRef.current?.blur()
        } else {
          setSelectedId(null)
        }
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const loadMore = async () => {
    if (cursor === null) return
    setLoadingMore(true)
    try {
      const res = await historyList({ cursor, query, kind: kind === 'all' ? null : kind, favoritesOnly })
      setItems((prev) => [...prev, ...res.items.filter((n) => !prev.some((p) => p.id === n.id))])
      setCursor(res.nextCursor)
    } catch (e) {
      toastError(e)
    } finally {
      setLoadingMore(false)
    }
  }

  const replaceItem = (next: HistoryItem) => setItems((prev) => prev.map((i) => (i.id === next.id ? next : i)))

  const toggleFavorite = async (item: HistoryItem) => {
    replaceItem({ ...item, favorite: !item.favorite })
    if (!isTauri()) return
    try {
      const favorite = await historyToggleFavorite({ id: item.id })
      replaceItem({ ...item, favorite })
    } catch (e) {
      replaceItem(item)
      toastError(e)
    }
  }

  const removeItem = (id: string) => {
    setItems((prev) => prev.filter((i) => i.id !== id))
    setSelectedId(null)
  }

  const clearFilters = () => {
    setInput('')
    setQuery('')
    setKind('all')
    setFavoritesOnly(false)
  }

  const groups = groupByDay(items)

  return (
    <div className="space-y-7">
      <PageHeader title="History" description="Everything you have made, searchable. Stored only on this computer." />

      <div className="flex flex-wrap items-center gap-2.5">
        <label className="group relative flex h-9 min-w-[14rem] flex-1 items-center rounded-full border border-line bg-surface pl-9 pr-3 transition-colors hover:border-line-strong focus-within:border-accent">
          <Search className="pointer-events-none absolute left-3.5 size-3.5 text-subtle" strokeWidth={2} />
          <input
            ref={searchRef}
            type="search"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Search history"
            aria-label="Search history"
            spellCheck={false}
            className="h-full min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-subtle [&::-webkit-search-cancel-button]:hidden"
          />
          {input ? (
            <button
              type="button"
              aria-label="Clear search"
              onClick={() => {
                setInput('')
                setQuery('')
                searchRef.current?.focus()
              }}
              className="grid size-5 place-items-center rounded-full text-subtle hover:bg-fg/[0.08] hover:text-fg"
            >
              <X className="size-3" strokeWidth={2.25} />
            </button>
          ) : (
            <span className="pointer-events-none hidden items-center gap-1 sm:flex">
              <Kbd>Ctrl</Kbd>
              <Kbd>F</Kbd>
            </span>
          )}
        </label>
        <Segmented label="Filter by kind" value={kind} options={kindOptions} onChange={setKind} />
        <IconButton
          label={favoritesOnly ? 'Show all items' : 'Show favorites only'}
          active={favoritesOnly}
          aria-pressed={favoritesOnly}
          onClick={() => setFavoritesOnly((v) => !v)}
          className="size-9 rounded-full"
        >
          <Star className="size-4" strokeWidth={2} fill={favoritesOnly ? 'currentColor' : 'none'} />
        </IconButton>
      </div>

      {!isTauri() ? (
        <p className="-mt-3 font-mono text-[11px] text-subtle">Preview data. Open the desktop app to see your history.</p>
      ) : null}

      {loading ? (
        <div className="grid place-items-center py-20 text-subtle">
          <LoaderCircle className="size-4 animate-spin" strokeWidth={2} />
        </div>
      ) : items.length === 0 ? (
        filtered ? (
          <EmptyState
            icon={<SearchX className="size-4.5" strokeWidth={1.75} />}
            title="No results"
            description={query ? `Nothing matches "${query}" with the current filters.` : 'Nothing matches the current filters.'}
            action={
              <Button size="sm" onClick={clearFilters}>
                Clear filters
              </Button>
            }
          />
        ) : (
          <EmptyState
            icon={<History className="size-4.5" strokeWidth={1.75} />}
            title="Your history is empty"
            description="Generated speech and transcripts will show up here."
          />
        )
      ) : (
        <div className={cx('grid items-start gap-6', selected && 'lg:grid-cols-[1fr_22rem]')}>
          <div className="min-w-0 space-y-6">
            {groups.map((group) => (
              <section key={group.key} className="space-y-2.5">
                <Kicker className="px-1">{group.label}</Kicker>
                <Card>
                  <ul className="divide-y divide-line">
                    {group.items.map((item) => (
                      <HistoryRow
                        key={item.id}
                        item={item}
                        selected={item.id === selectedId}
                        onSelect={() => setSelectedId(item.id === selectedId ? null : item.id)}
                        onToggleFavorite={() => void toggleFavorite(item)}
                      />
                    ))}
                  </ul>
                </Card>
              </section>
            ))}
            {cursor !== null ? (
              <div className="flex justify-center">
                <Button size="sm" variant="ghost" loading={loadingMore} onClick={() => void loadMore()}>
                  Load more
                </Button>
              </div>
            ) : null}
          </div>

          {selected ? (
            <aside className="min-w-0 lg:sticky lg:top-0">
              <HistoryDetail
                key={selected.id}
                item={selected}
                onClose={() => setSelectedId(null)}
                onChange={replaceItem}
                onDeleted={removeItem}
              />
            </aside>
          ) : null}
        </div>
      )}
    </div>
  )
}

function HistoryRow({
  item,
  selected,
  onSelect,
  onToggleFavorite,
}: {
  item: HistoryItem
  selected: boolean
  onSelect: () => void
  onToggleFavorite: () => void
}) {
  const snippet = item.text.replace(/\s+/g, ' ').trim()
  const meta = [item.modelId, formatDuration(item.durationMs), relativeTime(item.createdAt)].filter(Boolean)
  return (
    <li
      className={cx(
        'group flex items-center gap-2 pr-2.5 transition-colors duration-200 first:rounded-t-2xl last:rounded-b-2xl',
        selected ? 'bg-fg/[0.05]' : 'hover:bg-fg/[0.03]',
      )}
    >
      <button
        type="button"
        onClick={onSelect}
        aria-current={selected || undefined}
        className="flex min-w-0 flex-1 items-center gap-3.5 py-3 pl-4 text-left outline-offset-[-2px]"
      >
        <span
          className={cx(
            'grid size-8 shrink-0 place-items-center rounded-lg border transition-colors',
            selected ? 'border-line-strong text-fg' : 'border-line text-muted',
          )}
        >
          {item.kind === 'tts' ? <Volume2 className="size-3.5" strokeWidth={2} /> : <Mic className="size-3.5" strokeWidth={2} />}
        </span>
        <span className="min-w-0 flex-1">
          <span className="line-clamp-1 text-[13.5px] font-medium tracking-[-0.005em] text-fg">{item.title}</span>
          {snippet && snippet !== item.title ? (
            <span className="line-clamp-1 text-[13px] text-muted">{snippet}</span>
          ) : null}
          <span className="mt-0.5 line-clamp-1 font-mono text-[11px] text-subtle">{meta.join(' · ')}</span>
        </span>
      </button>
      <IconButton
        label={item.favorite ? 'Remove from favorites' : 'Add to favorites'}
        onClick={onToggleFavorite}
        className={cx(item.favorite ? 'text-fg!' :'opacity-0 group-hover:opacity-100 focus-visible:opacity-100')}
      >
        <Star className="size-3.5" strokeWidth={2} fill={item.favorite ? 'currentColor' : 'none'} />
      </IconButton>
    </li>
  )
}
