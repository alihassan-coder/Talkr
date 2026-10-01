import { useEffect, useRef, useState } from 'react'
import { useNavigate } from 'react-router'
import { Check, Copy, ListOrdered, Mic, PenLine, Star, Trash2, Volume2, X } from 'lucide-react'
import { historyDelete, historyToggleFavorite, isTauri, parseSegments } from '@/lib/api'
import type { HistoryItem } from '@/lib/types'
import { Button, Card, IconButton, Kicker } from '@/components/ui'
import { cx } from '@/lib/cx'
import { toastError } from '@/stores/toast'
import { ExportMenu } from '@/features/export/ExportMenu'
import { useDrafts } from '@/stores/drafts'
import { AudioPlayer, type PlayerHandle } from './AudioPlayer'
import { formatClock, formatDuration } from './utils'

export function HistoryDetail({
  item,
  onClose,
  onChange,
  onDeleted,
}: {
  item: HistoryItem
  onClose: () => void
  onChange: (item: HistoryItem) => void
  onDeleted: (id: string) => void
}) {
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [copied, setCopied] = useState<'text' | 'timed' | null>(null)
  const navigate = useNavigate()
  const player = useRef<PlayerHandle>(null)
  const segments = parseSegments(item)
  const created = new Date(item.createdAt)

  // Second click within 3s deletes.
  useEffect(() => {
    if (!confirmDelete) return
    const t = setTimeout(() => setConfirmDelete(false), 3000)
    return () => clearTimeout(t)
  }, [confirmDelete])

  useEffect(() => {
    if (!copied) return
    const t = setTimeout(() => setCopied(null), 1600)
    return () => clearTimeout(t)
  }, [copied])

  const copy = async (what: 'text' | 'timed') => {
    const text =
      what === 'timed' ? segments.map((seg) => `[${formatClock(seg.startMs)}] ${seg.text.trim()}`).join('\n') : item.text
    try {
      await navigator.clipboard.writeText(text)
      setCopied(what)
    } catch (e) {
      toastError(e)
    }
  }

  /** Put this item's text in the Speak box, to hear it again with another voice or speed. */
  const openInSpeak = () => {
    useDrafts.getState().setSpeakText(item.text)
    navigate('/speak')
  }

  const toggleFavorite = async () => {
    const optimistic = { ...item, favorite: !item.favorite }
    onChange(optimistic)
    if (!isTauri()) return
    try {
      const favorite = await historyToggleFavorite({ id: item.id })
      onChange({ ...item, favorite })
    } catch (e) {
      onChange(item)
      toastError(e)
    }
  }

  const remove = async () => {
    if (!confirmDelete) return setConfirmDelete(true)
    try {
      if (isTauri()) await historyDelete({ id: item.id })
      onDeleted(item.id)
    } catch (e) {
      toastError(e)
    }
  }

  const meta = [
    item.kind === 'tts' ? 'Speech' : 'Transcript',
    item.modelId,
    item.voiceId,
    item.language && item.language !== 'auto' ? item.language : null,
    formatDuration(item.durationMs),
    item.device.toUpperCase(),
  ].filter(Boolean)

  return (
    <Card className="animate-rise overflow-hidden">
      <div className="flex items-start gap-3 border-b border-line px-4 py-3.5">
        <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg border border-line text-muted">
          {item.kind === 'tts' ? <Volume2 className="size-3.5" strokeWidth={2} /> : <Mic className="size-3.5" strokeWidth={2} />}
        </span>
        <div className="min-w-0 flex-1">
          <h2 dir="auto" className="line-clamp-2 text-[14px] font-medium tracking-[-0.01em]">
            {item.title}
          </h2>
          <p className="mt-0.5 font-mono text-[11px] text-subtle">
            {created.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })} ·{' '}
            {created.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })}
          </p>
        </div>
        <IconButton label="Close" onClick={onClose} className="-mr-1.5 -mt-0.5">
          <X className="size-4" strokeWidth={2} />
        </IconButton>
      </div>

      <div className="space-y-4 px-4 py-4">
        {item.audioPath ? (
          <AudioPlayer
            key={item.id}
            ref={player}
            id={item.id}
            path={item.audioPath}
            seed={item.id}
            fallbackDurationMs={item.durationMs}
          />
        ) : null}

        {segments.length ? (
          <div data-selectable className="max-h-80 space-y-2.5 overflow-y-auto pr-1 text-[13px] leading-relaxed">
            {segments.map((seg, i) => (
              <p key={i} className="grid grid-cols-[3.25rem_1fr] gap-2">
                {item.audioPath ? (
                  <button
                    type="button"
                    onClick={() => player.current?.playFrom(seg.startMs / 1000)}
                    aria-label={`Play from ${formatClock(seg.startMs)}`}
                    title="Play from here"
                    className="self-start rounded pt-px text-left font-mono text-[11px] tabular-nums text-subtle underline-offset-2 transition-colors hover:text-accent hover:underline"
                  >
                    {formatClock(seg.startMs)}
                  </button>
                ) : (
                  <span className="pt-px font-mono text-[11px] tabular-nums text-subtle">{formatClock(seg.startMs)}</span>
                )}
                <span dir="auto" className="text-fg">
                  {seg.text.trim()}
                </span>
              </p>
            ))}
          </div>
        ) : (
          <p
            data-selectable
            dir="auto"
            className="max-h-80 overflow-y-auto whitespace-pre-wrap pr-1 text-[13px] leading-relaxed text-fg"
          >
            {item.text}
          </p>
        )}

        <p className="flex flex-wrap gap-x-2 gap-y-1 font-mono text-[11px] text-subtle">
          {meta.map((m, i) => (
            <span key={i}>
              {i > 0 ? <span className="mr-2 text-line-strong">/</span> : null}
              {m}
            </span>
          ))}
        </p>
      </div>

      <div className="space-y-3 border-t border-line px-4 py-3.5">
        <div className="flex flex-wrap items-center gap-2">
          <Kicker className="mr-1">Export</Kicker>
          <ExportMenu item={item} prefer={item.kind === 'tts' ? 'audio' : 'text'} />
        </div>
        <div className="flex items-center gap-1">
          <IconButton label={copied === 'text' ? 'Copied' : 'Copy text'} onClick={() => void copy('text')}>
            {copied === 'text' ? <Check className="size-4" strokeWidth={2} /> : <Copy className="size-4" strokeWidth={2} />}
          </IconButton>
          {segments.length ? (
            <IconButton
              label={copied === 'timed' ? 'Copied' : 'Copy with timestamps'}
              onClick={() => void copy('timed')}
            >
              {copied === 'timed' ? (
                <Check className="size-4" strokeWidth={2} />
              ) : (
                <ListOrdered className="size-4" strokeWidth={2} />
              )}
            </IconButton>
          ) : null}
          {item.kind === 'tts' ? (
            <IconButton label="Open in Speak" onClick={openInSpeak}>
              <PenLine className="size-4" strokeWidth={2} />
            </IconButton>
          ) : null}
          <IconButton
            label={item.favorite ? 'Remove from favorites' : 'Add to favorites'}
            active={item.favorite}
            onClick={() => void toggleFavorite()}
          >
            <Star className="size-4" strokeWidth={2} fill={item.favorite ? 'currentColor' : 'none'} />
          </IconButton>
          <Button
            size="sm"
            variant={confirmDelete ? 'primary' : 'ghost'}
            className={cx('ml-auto', !confirmDelete && 'text-subtle')}
            icon={<Trash2 className="size-3.5" strokeWidth={2} />}
            onClick={() => void remove()}
          >
            {confirmDelete ? 'Click again to delete' : 'Delete'}
          </Button>
        </div>
      </div>
    </Card>
  )
}
