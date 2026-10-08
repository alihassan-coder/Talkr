import { useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { Ban, CircleAlert, Download, LoaderCircle, ShieldAlert } from 'lucide-react'
import type { DictationCapabilities, DictationDone, DictationStatus, Settings } from '@/lib/types'
import { Button, Card, Progress } from '@/components/ui'
import { Switch } from '@/features/settings/controls'
import { cx } from '@/lib/cx'
import { isTauri, onDictationDone } from '@/lib/api'
import { useModels, initModels } from '@/stores/models'
import { formatBytes } from '@/features/history/utils'
import { recommendedModel } from '@/features/dictation/utils'
import { Keys } from '@/features/dictation/Keycaps'
import { liveCopy, liveState } from '@/features/dictation/state'
import type { LiveState } from '@/features/dictation/state'
import { osName } from '@/features/dictation/platform'

/** Five bars that idle like a voice at rest; the hero's emblem. */
function Bars() {
  return (
    <span className="dict-orb-bars" aria-hidden="true">
      {[0, 1, 2, 3, 4].map((i) => (
        <i key={i} style={{ animationDelay: `${i * -0.23}s` }} />
      ))}
    </span>
  )
}

/** The live emblem: a breathing accent orb when ready, a turning ring while it gets ready. */
function Orb({ state }: { state: LiveState }) {
  const icon =
    state === 'permission' ? (
      <ShieldAlert className="size-5" strokeWidth={1.9} />
    ) : state === 'error' || state === 'noModel' ? (
      <CircleAlert className="size-5" strokeWidth={1.9} />
    ) : state === 'unsupported' ? (
      <Ban className="size-5" strokeWidth={1.9} />
    ) : (
      <Bars />
    )
  return (
    <span className="dict-orb" data-state={state} aria-hidden="true">
      <span className="dict-orb-ripple" />
      <span className="dict-orb-ripple" style={{ animationDelay: '-1.6s' }} />
      <span className="dict-orb-ring" />
      <span className="dict-orb-core">{icon}</span>
    </span>
  )
}

/** Download the model that suits this computer, with progress. */
export function ModelDownload({ settings, compact = false }: { settings: Settings; compact?: boolean }) {
  const catalog = useModels((s) => s.catalog)
  const hardware = useModels((s) => s.hardware)
  const downloads = useModels((s) => s.downloads)
  const failures = useModels((s) => s.failures)
  const download = useModels((s) => s.download)
  const refresh = useModels((s) => s.refresh)

  useEffect(() => {
    initModels()
  }, [])

  // A finished download should make the model appear without a reload.
  const anyDownloading = Object.keys(downloads).length > 0
  useEffect(() => {
    if (!anyDownloading && isTauri()) void refresh()
  }, [anyDownloading, refresh])

  const gpu = !!hardware && hardware.recommendedBackend !== 'cpu'
  const suggestion = recommendedModel(gpu, settings.dictation.language ?? settings.sttLanguage)
  const model = catalog.find((m) => m.id === suggestion)
  const active = downloads[suggestion]
  const failed = failures[suggestion]
  return (
    <div className={cx('flex flex-wrap items-center gap-3', !compact && 'rounded-xl border border-line bg-bg/60 px-3.5 py-3')}>
      <div className="min-w-0 flex-1">
        <p className="text-[13px] font-medium">Dictation needs a speech model</p>
        <p className="text-[12px] text-muted">
          {failed
            ? failed
            : model
              ? `${model.name}${model.sizeBytes ? ` · ${formatBytes(model.sizeBytes)}` : ''} is a good fit for this computer. It stays on this computer.`
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
          {active ? 'Downloading' : failed ? 'Try again' : 'Download'}
        </Button>
      ) : null}
    </div>
  )
}

function ModelStatus({ status, settings }: { status: DictationStatus; settings: Settings }) {
  const catalog = useModels((s) => s.catalog)
  const installed = useModels((s) => s.installed)
  const id = status.modelId
  if (!id) return null
  const name = catalog.find((m) => m.id === id)?.name ?? installed.find((m) => m.id === id)?.name ?? id
  const d = settings.dictation
  return (
    <span className="inline-flex items-center gap-1.5">
      <span className="text-subtle">Model</span>
      <span className="font-medium text-fg">{name}</span>
      <span className="text-subtle">·</span>
      {status.warm ? (
        <span>loaded</span>
      ) : d.enabled && d.keepWarm ? (
        <span className="inline-flex items-center gap-1">
          <LoaderCircle className="size-3 animate-spin" strokeWidth={2} /> loading
        </span>
      ) : (
        <span>loads on first use</span>
      )}
    </span>
  )
}

/** The last dictation, while it is fresh. */
function useLastDictation() {
  const [last, setLast] = useState<DictationDone | null>(null)
  useEffect(() => {
    if (!isTauri()) return
    const p = onDictationDone(setLast)
    return () => void p.then((f) => f()).catch(() => {})
  }, [])
  useEffect(() => {
    if (!last) return
    const t = setTimeout(() => setLast(null), 6000)
    return () => clearTimeout(t)
  }, [last])
  return last
}

function headline(state: LiveState, caps: DictationCapabilities, status: DictationStatus): string {
  switch (state) {
    case 'unsupported':
      return caps.note ?? `Dictation is not available on ${osName[caps.os]} yet. It is on the way.`
    case 'permission':
      return status.permission.state === 'missing' ? status.permission.detail : ''
    case 'error':
      return status.error ?? ''
    case 'off':
      return 'Turn it on to use your voice in any app.'
    case 'noModel':
      return 'Download a speech model to start. It runs on this computer.'
    case 'starting':
      return 'Getting the shortcut ready…'
    case 'loading':
      return 'Loading the speech model so your first words come back fast.'
    case 'ready':
      return ''
  }
}

/** How to use it, in one line: "Hold Ctrl Win, speak, let go." */
function Instruction({ settings, caps }: { settings: Settings; caps: DictationCapabilities }) {
  const d = settings.dictation
  const keys = caps.recordsShortcut ? <Keys shortcut={d.shortcut} /> : <span className="font-medium text-fg">your Talkr shortcut</span>
  const where = caps.insertsText ? 'Your words appear where your cursor is.' : 'Your words are copied, ready to paste.'
  if (!caps.holdToTalk || d.mode === 'toggle')
    return (
      <>
        Press {keys} in any app, speak, and press it again. {where}
      </>
    )
  return (
    <>
      Hold {keys} in any app, speak, and let go. {where}
    </>
  )
}

export function Hero({
  settings,
  status,
  onToggle,
  children,
}: {
  settings: Settings
  status: DictationStatus
  onToggle: (on: boolean) => void
  /** Shown at the bottom of the hero (the model download when setup is closed). */
  children?: ReactNode
}) {
  const d = settings.dictation
  const caps = status.capabilities
  const state = liveState(status, d)
  const copy = liveCopy[state]
  const detail = headline(state, caps, status)
  const last = useLastDictation()

  const tips: [string, string][] = []
  if (caps.holdToTalk && d.mode !== 'toggle') tips.push(['Hold', 'to talk'])
  if (caps.holdToTalk && d.mode === 'auto') tips.push(['Tap', 'for hands-free'])
  if (!caps.holdToTalk || d.mode === 'toggle') tips.push(['Press twice', 'start and finish'])
  // Escape is heard only where Talkr watches the keyboard itself.
  if (caps.recordsShortcut) tips.push(['Esc', 'to cancel'])

  return (
    <Card className="dict-hero relative overflow-hidden">
      <div aria-hidden="true" className="dict-hero-wash" data-state={state} />
      <div className="relative p-6">
        <div className="flex items-start gap-5">
          <Orb state={state} />
          <div className="min-w-0 flex-1 space-y-1.5">
            <p
              aria-live="polite"
              data-tone={copy.tone}
              className="dict-live inline-flex items-center gap-2 font-mono text-[10.5px] uppercase tracking-[0.16em]"
            >
              <span className="dict-live-dot" aria-hidden="true" />
              {copy.label}
            </p>
            <h2 className="text-[22px] font-semibold leading-tight tracking-[-0.03em]">Dictate anywhere</h2>
            <p className="max-w-xl text-[13.5px] leading-relaxed text-muted">
              {state === 'unsupported' ? detail : <Instruction settings={settings} caps={caps} />}
            </p>
          </div>
          <div className="flex flex-col items-end gap-1 pt-1">
            <Switch label="Dictation" checked={d.enabled} disabled={!status.supported} onChange={onToggle} />
            <span className="text-[11px] text-subtle">{d.enabled ? 'On' : 'Off'}</span>
          </div>
        </div>

        {status.supported ? (
          <div className="mt-5 flex flex-wrap gap-1.5">
            {tips.map(([a, b]) => (
              <span key={a} className="inline-flex h-7 items-center gap-1.5 rounded-full border border-line bg-bg/50 px-3 text-[12px] text-muted">
                <span className="font-medium text-fg">{a}</span>
                {b}
              </span>
            ))}
            {d.pasteLastEnabled && caps.recordsShortcut ? (
              <span className="inline-flex h-7 items-center gap-1.5 rounded-full border border-line bg-bg/50 pl-3 pr-1 text-[12px] text-muted">
                <span className="font-medium text-fg">Paste again</span>
                <Keys shortcut={d.pasteLastShortcut} size="sm" />
              </span>
            ) : null}
          </div>
        ) : null}

        {status.supported && (detail || status.modelId || last) ? (
          <div className="mt-5 flex flex-wrap items-center justify-between gap-x-4 gap-y-2 border-t border-line pt-4 text-[12.5px] text-muted">
            {last ? (
              <span aria-live="polite" className="min-w-0 animate-rise truncate">
                <span className="font-medium text-fg">{last.inserted ? 'Inserted' : 'Copied'}</span>
                {last.app ? ` into ${last.app}` : ''} · “{last.text}”
              </span>
            ) : detail && state !== 'permission' ? (
              <span role={state === 'error' ? 'alert' : undefined} className={cx(state === 'error' && 'dict-warn-text')}>
                {detail}
              </span>
            ) : (
              <span />
            )}
            <ModelStatus status={status} settings={settings} />
          </div>
        ) : null}
        {children ? <div className="mt-4">{children}</div> : null}
      </div>
    </Card>
  )
}
