import { useEffect, useState } from 'react'
import { Check, Copy, Download, Mic, Star, Trash2, Volume2, X } from 'lucide-react'
import { audioSrc, historyDelete, historyExport, historyToggleFavorite, isTauri, parseSegments } from '@/lib/api'
import type { ExportFormat, HistoryItem } from '@/lib/types'
import { Button, Card, IconButton, Kicker } from '@/components/ui'
import { cx } from '@/lib/cx'
import { toast, toastError } from '@/stores/toast'
import { AudioPlayer } from './AudioPlayer'
import { formatClock, formatDuration } from './utils'

export function HistoryDetail({
  item,
  home,
  onClose,
  onChange,
  onDeleted,
}: {
  item: HistoryItem
  home: string | null
  onClose: () => void
  onChange: (item: HistoryItem) => void
  onDeleted: (id: string) => void
}) {
  const [confirmDelete, setConfirmDelete] = useState(false)
  const [copied, setCopied] = useState(false)
  const [exporting, setExporting] = useState<ExportFormat | null>(null)
  const segments = parseSegments(item)
  const formats: ExportFormat[] = item.kind === 'stt' ? (segments.length ? ['txt', 'srt'] : ['txt']) : ['wav', 'txt']
  const created = new Date(item.createdAt)

  // Second click within 3s deletes.
  useEffect(() => {
    if (!confirmDelete) return
    const t = setTimeout(() => setConfirmDelete(false), 3000)
    return () => clearTimeout(t)
  }, [confirmDelete])

  useEffect(() => {
    if (!copied) return
    const t = setTimeout(() => setCopied(false), 1600)
    return () => clearTimeout(t)
  }, [copied])

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(item.text)
      setCopied(true)
    } catch (e) {
      toastError(e)
    }
  }

  const exportAs = async (format: ExportFormat) => {
    if (!isTauri()) return toast('Export is available in the desktop app')
    setExporting(format)
    try {
      const saved = await historyExport({ id: item.id, format })
      if (saved) toast(`Saved ${format.toUpperCase()}`)
    } catch (e) {
      toastError(e)
    } finally {
      setExporting(null)
    }
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
          <h2 className="line-clamp-2 text-[14px] font-medium tracking-[-0.01em]">{item.title}</h2>
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
        {item.audioPath && home ? (
          <AudioPlayer
            key={item.id}
            src={audioSrc(home, item.audioPath)}
            seed={item.id}
            fallbackDurationMs={item.durationMs}
          />
        ) : null}

        {segments.length ? (
          <div data-selectable className="max-h-80 space-y-2.5 overflow-y-auto pr-1 text-[13px] leading-relaxed">
            {segments.map((seg, i) => (
              <p key={i} className="grid grid-cols-[3.25rem_1fr] gap-2">
                <span className="pt-px font-mono text-[11px] tabular-nums text-subtle">{formatClock(seg.startMs)}</span>
                <span className="text-fg">{seg.text.trim()}</span>
              </p>
            ))}
          </div>
        ) : (
          <p
            data-selectable
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
          {formats.map((f) => (
            <Button
              key={f}
              size="sm"
              loading={exporting === f}
              disabled={exporting !== null && exporting !== f}
              icon={<Download className="size-3.5" strokeWidth={2} />}
              onClick={() => void exportAs(f)}
            >
              <span className="font-mono">.{f}</span>
            </Button>
          ))}
        </div>
        <div className="flex items-center gap-1">
          <IconButton label={copied ? 'Copied' : 'Copy text'} onClick={() => void copy()}>
            {copied ? <Check className="size-4" strokeWidth={2} /> : <Copy className="size-4" strokeWidth={2} />}
          </IconButton>
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
