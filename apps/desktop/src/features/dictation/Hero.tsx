import { useEffect, useState } from 'react'
import { AudioLines, Download, LoaderCircle, TriangleAlert } from 'lucide-react'
import type { DictationSettings, DictationStatus, Settings } from '@/lib/types'
import { Button, Card, Progress } from '@/components/ui'
import { Switch } from '@/features/settings/controls'
import { cx } from '@/lib/cx'
import { useModels, initModels } from '@/stores/models'
import { onDictationDone, isTauri } from '@/lib/api'
import { Keys } from '@/features/dictation/ShortcutField'
import { formatBytes } from '@/features/history/utils'
import { recommendedModel } from '@/features/dictation/utils'

function StatusLine({ status, dictation }: { status: DictationStatus; dictation: DictationSettings }) {
  if (!status.supported)
    return <span className="text-[12.5px] text-muted">Dictation is available on Windows for now. macOS and Linux are next.</span>
  if (status.error)
    return (
      <span role="alert" className="inline-flex items-center gap-1.5 text-[12.5px] text-accent">
        <TriangleAlert className="size-3.5" strokeWidth={2} />
        {status.error}
      </span>
    )
  if (!dictation.enabled) return <span className="text-[12.5px] text-muted">Off. Turn it on to use your voice in any app.</span>
  return (
    <span className="inline-flex items-center gap-2 text-[12.5px] text-fg">
      <span className="relative flex size-2">
        <span className="absolute inline-flex size-full animate-ping rounded-full bg-accent opacity-60" />
        <span className="relative inline-flex size-2 rounded-full bg-accent" />
      </span>
      {status.active ? 'Ready everywhere' : 'Starting…'}
    </span>
  )
}

function ModelLine({ status, settings }: { status: DictationStatus; settings: Settings }) {
  const catalog = useModels((s) => s.catalog)
  const installed = useModels((s) => s.installed)
  const hardware = useModels((s) => s.hardware)
  const downloads = useModels((s) => s.downloads)
  const download = useModels((s) => s.download)
  const refresh = useModels((s) => s.refresh)

  useEffect(() => {
    initModels()
  }, [])

  const nameOf = (id: string) => catalog.find((m) => m.id === id)?.name ?? installed.find((m) => m.id === id)?.name ?? id
  const modelId = status.modelId

  // A finished download should make the model appear here without a reload.
  const anyDownloading = Object.keys(downloads).length > 0
  useEffect(() => {
    if (!anyDownloading && isTauri()) void refresh()
  }, [anyDownloading, refresh])

  if (modelId) {
    return (
      <p className="text-[12.5px] text-muted">
        Speech model <span className="font-medium text-fg">{nameOf(modelId)}</span>
        <span className="mx-1.5 text-subtle">·</span>
        {status.warm ? (
          <span className="text-fg">Loaded and ready</span>
        ) : settings.dictation.enabled && settings.dictation.keepWarm ? (
          <span className="inline-flex items-center gap-1">
            <LoaderCircle className="size-3 animate-spin" strokeWidth={2} /> Loading
          </span>
        ) : (
          'Loads on first use'
        )}
      </p>
    )
  }

  const gpu = !!hardware && hardware.recommendedBackend !== 'cpu'
  const suggestion = recommendedModel(gpu, settings.dictation.language ?? settings.sttLanguage)
  const model = catalog.find((m) => m.id === suggestion)
  const active = downloads[suggestion]
  return (
    <div className="flex flex-wrap items-center gap-3 rounded-xl border border-line bg-bg/60 px-3.5 py-2.5">
      <div className="min-w-0 flex-1">
        <p className="text-[13px] font-medium">Dictation needs a speech model</p>
        <p className="text-[12px] text-muted">
          {model
            ? `${model.name}${model.sizeBytes ? ` · ${formatBytes(model.sizeBytes)}` : ''} is a good fit for this computer.`
            : 'Download one in Models.'}
        </p>
        {active ? <Progress className="mt-2" value={active.progress} label={`Downloading ${model?.name ?? suggestion}`} /> : null}
      </div>
      {model ? (
        <Button
          size="sm"
          variant="primary"
          loading={!!active}
          icon={<Download className="size-3.5" strokeWidth={2} />}
          onClick={() => void download(suggestion)}
        >
          {active ? 'Downloading' : 'Download'}
        </Button>
      ) : null}
    </div>
  )
}

export function Hero({
  settings,
  status,
  onToggle,
}: {
  settings: Settings
  status: DictationStatus
  onToggle: (on: boolean) => void
}) {
  const d = settings.dictation
  return (
    <Card className="relative overflow-hidden">
      {/* A soft wash of the accent, strongest while dictation is on. */}
      <div
        aria-hidden="true"
        className={cx(
          'pointer-events-none absolute -right-24 -top-24 size-72 rounded-full bg-accent blur-3xl transition-opacity duration-700',
          d.enabled ? 'opacity-[0.13]' : 'opacity-[0.05]',
        )}
      />
      <div className="relative space-y-5 p-6">
        <div className="flex items-start justify-between gap-6">
          <div className="space-y-2">
            <div className="flex items-center gap-2.5">
              <span className="grid size-8 place-items-center rounded-xl bg-accent/15 text-accent">
                <AudioLines className="size-4" strokeWidth={2} />
              </span>
              <h2 className="text-[19px] font-semibold tracking-[-0.025em]">Dictate anywhere</h2>
            </div>
            <p className="max-w-lg text-[13.5px] leading-relaxed text-muted">
              Hold <Keys shortcut={d.shortcut} /> in any app, speak, and let go. Your words are typed where your cursor
              is. It all happens on this computer.
            </p>
          </div>
          <Switch
            label="Dictation"
            checked={d.enabled}
            disabled={!status.supported}
            onChange={onToggle}
          />
        </div>

        <div className="flex flex-wrap gap-2">
          {[
            ['Hold', 'to talk'],
            ['Tap', 'for hands-free'],
            ['Esc', 'to cancel'],
            ...(d.pasteLastEnabled ? [['Paste again', '']] : []),
          ].map(([a, b]) => (
            <span key={a} className="inline-flex h-7 items-center gap-1.5 rounded-full border border-line bg-bg/60 px-3 text-[12px] text-muted">
              <span className="font-medium text-fg">{a}</span>
              {b}
              {a === 'Paste again' ? <Keys shortcut={d.pasteLastShortcut} /> : null}
            </span>
          ))}
        </div>

        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-line pt-4">
          <StatusLine status={status} dictation={d} />
        </div>
        {status.supported ? <ModelLine status={status} settings={settings} /> : null}
      </div>
    </Card>
  )
}

/** A practice box: click into it, dictate, and see what arrives. */
export function TryIt({ dictation, enabled }: { dictation: DictationSettings; enabled: boolean }) {
  const [last, setLast] = useState<{ inserted: boolean; method: string | null } | null>(null)
  useEffect(() => {
    if (!isTauri()) return
    const p = onDictationDone((d) => setLast({ inserted: d.inserted, method: d.method }))
    return () => void p.then((f) => f())
  }, [])
  const how = { direct: 'written straight into the field', paste: 'pasted', type: 'typed key by key' } as const
  return (
    <div className="space-y-2 px-5 py-4">
      <textarea
        aria-label="Practice box"
        rows={3}
        disabled={!enabled}
        placeholder={
          enabled
            ? 'Click here, hold the shortcut, and say something…'
            : 'Turn dictation on above, then try it here.'
        }
        className="w-full resize-none rounded-xl border border-line bg-bg px-3.5 py-3 text-[14px] leading-relaxed text-fg placeholder:text-subtle focus:border-accent/60 focus:outline-none disabled:opacity-60"
      />
      <p aria-live="polite" className="min-h-4 text-[12px] text-subtle">
        {last
          ? last.inserted
            ? `Last dictation was ${how[last.method as keyof typeof how] ?? 'inserted'}.`
            : 'Last dictation was copied to the clipboard instead.'
          : dictation.mode === 'toggle'
            ? 'Press the shortcut to start, press it again to finish.'
            : 'Tip: tap the shortcut quickly to keep listening hands-free; press it again to finish.'}
      </p>
    </div>
  )
}
