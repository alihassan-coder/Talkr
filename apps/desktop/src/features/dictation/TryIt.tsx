import { useEffect, useState } from 'react'
import { ClipboardCopy, Eraser } from 'lucide-react'
import type { DictationCapabilities, DictationDone, DictationSettings } from '@/lib/types'
import { Button } from '@/components/ui'
import { cx } from '@/lib/cx'
import { dictationCopyLast, isTauri, onDictationDone } from '@/lib/api'
import { toastError } from '@/stores/toast'
import { Keys } from '@/features/dictation/Keycaps'
import { pasteKeys } from '@/features/dictation/shortcut'
import { useKeyPlatform } from '@/features/dictation/platform'

const how = { direct: 'Written straight into the field', paste: 'Pasted', type: 'Typed key by key' } as const

/** A practice box: click into it, dictate, and see what arrives and how. */
export function TryIt({
  dictation,
  capabilities,
  hasLast,
}: {
  dictation: DictationSettings
  capabilities: DictationCapabilities
  hasLast: boolean
}) {
  const platform = useKeyPlatform()
  const [text, setText] = useState('')
  const [last, setLast] = useState<DictationDone | null>(null)
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (!isTauri()) return
    const p = onDictationDone(setLast)
    return () => void p.then((f) => f()).catch(() => {})
  }, [])
  useEffect(() => {
    if (!copied) return
    const t = setTimeout(() => setCopied(false), 1600)
    return () => clearTimeout(t)
  }, [copied])

  const enabled = dictation.enabled
  const hold = capabilities.holdToTalk && dictation.mode !== 'toggle'
  const words = last?.text.trim().split(/\s+/).filter(Boolean).length ?? 0

  return (
    <div className="space-y-3 px-5 py-4">
      <div
        className={cx(
          'dict-practice group relative rounded-xl border bg-bg/70 transition-[border-color,box-shadow] duration-300',
          enabled ? 'border-line focus-within:border-accent/60' : 'border-dashed border-line-strong',
        )}
      >
        <textarea
          id="dictation-practice"
          aria-label="Practice box"
          rows={3}
          disabled={!enabled}
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder={enabled ? 'Click here, then speak…' : 'Turn dictation on above, then try it here.'}
          className="block w-full resize-none bg-transparent px-4 pb-11 pt-3.5 text-[14.5px] leading-relaxed text-fg placeholder:text-subtle focus:outline-none disabled:cursor-not-allowed"
        />
        <div className="pointer-events-none absolute inset-x-3 bottom-2.5 flex items-center justify-between gap-3">
          <span className="pointer-events-auto inline-flex items-center gap-1.5 text-[12px] text-muted">
            {capabilities.recordsShortcut ? (
              <>
                {hold ? 'Hold' : 'Press'} <Keys shortcut={dictation.shortcut} size="sm" /> {hold ? 'and speak' : 'to start and to finish'}
              </>
            ) : (
              'Press your Talkr shortcut and speak'
            )}
          </span>
          <span className="pointer-events-auto flex items-center gap-1">
            {text ? (
              <Button size="sm" variant="ghost" icon={<Eraser className="size-3.5" strokeWidth={2} />} onClick={() => setText('')}>
                Clear
              </Button>
            ) : null}
            {hasLast || last ? (
              <Button
                size="sm"
                variant="ghost"
                icon={<ClipboardCopy className="size-3.5" strokeWidth={2} />}
                onClick={() =>
                  dictationCopyLast()
                    .then(() => setCopied(true))
                    .catch(toastError)
                }
              >
                {copied ? 'Copied' : 'Copy last'}
              </Button>
            ) : null}
          </span>
        </div>
      </div>
      <p aria-live="polite" className="min-h-4 text-[12px] text-subtle">
        {last ? (
          last.inserted ? (
            <>
              <span className="font-medium text-fg">{how[last.method ?? 'paste'] ?? 'Inserted'}</span>
              {last.app ? ` into ${last.app}` : ''} · {words} {words === 1 ? 'word' : 'words'}
            </>
          ) : (
            <>
              <span className="font-medium text-fg">Copied to the clipboard</span> instead. Press {pasteKeys(platform)} to paste it.
            </>
          )
        ) : !capabilities.insertsText ? (
          `On this system the words are copied: press ${pasteKeys(platform)} to paste them.`
        ) : dictation.mode === 'auto' && capabilities.holdToTalk ? (
          'Tip: tap the shortcut quickly to keep listening hands-free; press it again to finish.'
        ) : (
          'Everything stays on this computer.'
        )}
      </p>
    </div>
  )
}
