import { useState } from 'react'
import { Check, Copy } from 'lucide-react'
import { Button, Card, Segmented } from '@/components/ui'
import { historyExport, parseSegments } from '@/lib/api'
import type { ExportFormat, HistoryItem } from '@/lib/types'
import { toast, toastError } from '@/stores/toast'
import { formatDuration, formatSeconds } from '@/features/speak/utils'
import { formatTimestamp, languageName } from './utils'

type View = 'segments' | 'text'

export function TranscriptResult({ item, name }: { item: HistoryItem; name: string }) {
  const segments = parseSegments(item)
  const [view, setView] = useState<View>(segments.length > 0 ? 'segments' : 'text')
  const [copied, setCopied] = useState(false)
  const empty = item.text.trim().length === 0

  const meta = [
    item.durationMs === null ? null : formatDuration(item.durationMs),
    languageName(item.language),
    item.modelId,
  ].filter(Boolean)

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(item.text.trim())
      setCopied(true)
      setTimeout(() => setCopied(false), 1600)
    } catch (err) {
      toastError(err)
    }
  }

  const exportAs = async (format: ExportFormat) => {
    try {
      const saved = await historyExport({ id: item.id, format })
      if (saved) toast(`Saved as ${format.toUpperCase()}`)
    } catch (err) {
      toastError(err)
    }
  }

  return (
    <Card className="animate-rise overflow-hidden">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-fg/[0.08] px-6 py-4">
        <div className="min-w-0">
          <p className="truncate text-[15px] font-medium tracking-[-0.01em]">{name}</p>
          <p className="mt-0.5 font-mono text-[11px] text-fg/40">{meta.join(' · ')}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            variant="ghost"
            disabled={empty}
            icon={copied ? <Check className="size-3.5" strokeWidth={2} /> : <Copy className="size-3.5" strokeWidth={2} />}
            onClick={() => void copy()}
          >
            {copied ? 'Copied' : 'Copy text'}
          </Button>
          <Button size="sm" disabled={empty} onClick={() => void exportAs('txt')}>
            TXT
          </Button>
          <Button size="sm" disabled={segments.length === 0} onClick={() => void exportAs('srt')}>
            SRT
          </Button>
        </div>
      </div>

      {empty ? (
        <p className="px-6 py-10 text-center text-[13.5px] text-fg/45">No speech was detected in this audio.</p>
      ) : (
        <>
          {segments.length > 0 ? (
            <div className="px-6 pt-4">
              <Segmented<View>
                label="Transcript view"
                value={view}
                onChange={setView}
                options={[
                  { value: 'segments', label: 'Segments' },
                  { value: 'text', label: 'Plain text' },
                ]}
              />
            </div>
          ) : null}

          {view === 'segments' && segments.length > 0 ? (
            <ol className="space-y-0.5 px-3 py-4">
              {segments.map((s, i) => (
                <li
                  key={`${s.startMs}-${i}`}
                  className="flex gap-5 rounded-lg px-3 py-2.5 transition-colors duration-200 hover:bg-fg/[0.04]"
                >
                  <span className="w-11 shrink-0 pt-[3px] font-mono text-[11px] tabular-nums text-fg/40">
                    {formatTimestamp(s.startMs)}
                  </span>
                  <span data-selectable className="text-[15px] leading-snug text-fg/90">
                    {s.text.trim()}
                  </span>
                </li>
              ))}
            </ol>
          ) : (
            <p data-selectable className="whitespace-pre-wrap px-6 py-5 text-[15px] leading-relaxed text-fg/90">
              {item.text.trim()}
            </p>
          )}
        </>
      )}

      <div className="flex items-center justify-between gap-3 border-t border-fg/[0.08] px-6 py-3 font-mono text-[11px] text-fg/40">
        <span>
          Done in {formatSeconds(item.processingMs)} · {item.device.toUpperCase()}
        </span>
        <span>0 bytes uploaded</span>
      </div>
    </Card>
  )
}
