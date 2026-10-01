import { useEffect, useEffectEvent, useRef, useState } from 'react'
import { History, LoaderCircle, Mic, RotateCw, Search, SearchX, Star, TriangleAlert, Volume2, X } from 'lucide-react'
import { historyList, historyToggleFavorite, isTauri } from '@/lib/api'
import type { HistoryItem, HistoryKind } from '@/lib/types'
import { Button, Card, EmptyState, IconButton, Kbd, Kicker, PageHeader, Segmented } from '@/components/ui'
import { cx } from '@/lib/cx'
import { errorText } from '@/lib/errors'
import { toastError } from '@/stores/toast'
import { useJobs } from '@/stores/jobs'
import { modKey } from '@/lib/platform'
import { HistoryDetail } from '@/features/history/HistoryDetail'
import { filterSample } from '@/features/history/sample'
import { formatDuration, groupByDay, relativeTime } from '@/features/history/utils'

type KindFilter = 'all' | HistoryKind

/** The loaded list and the filters (`key`) it belongs to. */
type Page = { key: string; items: HistoryItem[]; cursor: number | null; error: string | null }

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
  const [reload, setReload] = useState(0)
  const [page, setPage] = useState<Page | null>(null)
  const [loadingMoreKey, setLoadingMoreKey] = useState<string | null>(null)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  // Bumped for every first-page request, so a slow "Load more" for older filters is dropped.
  const generation = useRef(0)
  const detailRef = useRef<HTMLElement>(null)
  const rowRefs = useRef(new Map<string, HTMLButtonElement>())

  const filterKey = JSON.stringify([query, kind, favoritesOnly, reload])
  // Until the first page for the current filters arrives, nothing older is shown.
  const loading = page?.key !== filterKey
  // With "favorites only" on, an item un-starred here leaves the list (it stays in state, so a
  // failed toggle can put it back).
  const items = page && !loading ? page.items.filter((i) => !favoritesOnly || i.favorite) : []
  const cursor = page && !loading ? page.cursor : null
  const loadError = page && !loading ? page.error : null
  const loadingMore = loadingMoreKey === filterKey

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
    generation.current += 1
    const key = filterKey
    const kindParam = kind === 'all' ? null : kind
    const load = isTauri()
      ? historyList({ query, kind: kindParam, favoritesOnly })
      : Promise.resolve({ items: filterSample(query, kindParam, favoritesOnly), nextCursor: null })
    load
      .then((res) => {
        if (!cancelled) setPage({ key, items: res.items, cursor: res.nextCursor, error: null })
      })
      .catch((e: unknown) => {
        if (!cancelled) setPage({ key, items: [], cursor: null, error: errorText(e) })
      })
    return () => {
      cancelled = true
    }
  }, [query, kind, favoritesOnly, filterKey])

  // A job that finishes while History is open shows up without a reload or a spinner.
  const finished = useJobs((s) => `${s.tts.result?.seq ?? 0}:${s.stt.result?.seq ?? 0}`)
  const refreshQuietly = useEffectEvent(() => {
    if (!isTauri() || !page || page.key !== filterKey) return
    const key = filterKey
    historyList({ query, kind: kind === 'all' ? null : kind, favoritesOnly })
      .then((res) =>
        setPage((prev) => {
          if (!prev || prev.key !== key) return prev
          const fresh = new Set(res.items.map((i) => i.id))
          const older = prev.items.filter((i) => !fresh.has(i.id))
          // Keep pages loaded with "Load more" (and their cursor).
          const keptMore = prev.items.length > res.items.length
          return { ...prev, items: [...res.items, ...older], cursor: keptMore ? prev.cursor : res.nextCursor }
        }),
      )
      .catch(() => {})
  })
  const seenFinished = useRef(finished)
  useEffect(() => {
    if (finished === seenFinished.current) return
    seenFinished.current = finished
    refreshQuietly()
  }, [finished])

  // A newly selected item: move focus to its details and make sure they are on screen
  // (below the lg breakpoint the panel sits under the list).
  useEffect(() => {
    if (!selectedId) return
    const panel = detailRef.current
    if (!panel) return
    panel.focus({ preventScroll: true })
    panel.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
  }, [selectedId])

  /** Close the details and give focus back to the row that opened them. */
  const closeDetail = () => {
    if (selectedId) rowRefs.current.get(selectedId)?.focus()
    setSelectedId(null)
  }

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
          setSelectedId((id) => {
            if (id) rowRefs.current.get(id)?.focus()
            return null
          })
        }
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const loadMore = async () => {
    if (cursor === null || loadingMore) return
    const key = filterKey
    const gen = generation.current
    setLoadingMoreKey(key)
    try {
      const res = await historyList({ cursor, query, kind: kind === 'all' ? null : kind, favoritesOnly })
      // The filters changed meanwhile: this page belongs to another list.
      if (gen !== generation.current) return
      setPage((prev) =>
        prev && prev.key === key
          ? {
              ...prev,
              items: [...prev.items, ...res.items.filter((n) => !prev.items.some((p) => p.id === n.id))],
              cursor: res.nextCursor,
            }
          : prev,
      )
    } catch (e) {
      if (gen === generation.current) toastError(e)
    } finally {
      setLoadingMoreKey((k) => (k === key ? null : k))
    }
  }

  const setItems = (update: (prev: HistoryItem[]) => HistoryItem[]) =>
    setPage((prev) => (prev ? { ...prev, items: update(prev.items) } : prev))

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
    searchRef.current?.focus()
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
              <Kbd>{modKey()}</Kbd>
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
        <div role="status" aria-label="Loading history" className="grid place-items-center py-20 text-subtle">
          <LoaderCircle className="size-4 animate-spin" strokeWidth={2} />
        </div>
      ) : loadError ? (
        <EmptyState
          icon={<TriangleAlert className="size-4.5" strokeWidth={1.75} />}
          title="Could not load history"
          description={loadError}
          action={
            <Button
              size="sm"
              icon={<RotateCw className="size-3.5" strokeWidth={2} />}
              onClick={() => setReload((n) => n + 1)}
            >
              Try again
            </Button>
          }
        />
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
                        buttonRef={(el) => {
                          if (el) rowRefs.current.set(item.id, el)
                          else rowRefs.current.delete(item.id)
                        }}
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
            <aside
              ref={detailRef}
              tabIndex={-1}
              aria-label="Item details"
              className="min-w-0 scroll-mt-6 rounded-2xl outline-none lg:sticky lg:top-0"
            >
              <HistoryDetail
                key={selected.id}
                item={selected}
                onClose={closeDetail}
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
  buttonRef,
  onSelect,
  onToggleFavorite,
}: {
  item: HistoryItem
  selected: boolean
  buttonRef: (el: HTMLButtonElement | null) => void
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
        ref={buttonRef}
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
          <span dir="auto" className="line-clamp-1 text-[13.5px] font-medium tracking-[-0.005em] text-fg">
            {item.title}
          </span>
          {snippet && snippet !== item.title ? (
            <span dir="auto" className="line-clamp-1 text-[13px] text-muted">
              {snippet}
            </span>
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
