import { useEffect, useRef, useState } from 'react'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { Download, FolderOpen, Import, RotateCw, Trash2, TriangleAlert, X } from 'lucide-react'
import { isTauri, openDataFolder } from '@/lib/api'
import type { CatalogModel, HardwareInfo, InstalledModel, ModelKind } from '@/lib/types'
import { Badge, Button, Card, EmptyState, IconButton, PageHeader, Progress, Segmented } from '@/components/ui'
import { cx } from '@/lib/cx'
import { type ActiveDownload, initModels, useModels } from '@/stores/models'
import { toast, toastError } from '@/stores/toast'
import {
  backendLabel,
  describeLanguages,
  formatBytes,
  formatRam,
  formatSpeed,
  isAccelerated,
  isRecommended,
  primaryGpu,
  shortCpuName,
  shortLicense,
  sortModels,
  splitName,
  tagLabels,
} from '@/features/models/format'

const KIND_OPTIONS: { value: ModelKind; label: string }[] = [
  { value: 'stt', label: 'Speech to text' },
  { value: 'tts', label: 'Text to speech' },
]

const KIND_META: Record<ModelKind, string> = {
  stt: 'Whisper · GGML',
  tts: 'Kokoro, Piper · ONNX',
}

export function ModelsPage() {
  const catalog = useModels((s) => s.catalog)
  const installed = useModels((s) => s.installed)
  const installedIds = useModels((s) => s.installedIds)
  const downloads = useModels((s) => s.downloads)
  const hardware = useModels((s) => s.hardware)
  const modelsBytes = useModels((s) => s.modelsBytes)
  const loading = useModels((s) => s.loading)
  const loaded = useModels((s) => s.loaded)
  const error = useModels((s) => s.error)
  const refresh = useModels((s) => s.refresh)
  const [kind, setKind] = useState<ModelKind>('stt')

  useEffect(() => {
    initModels()
  }, [])

  const catalogIds = new Set(catalog.map((m) => m.id))
  const models = sortModels(catalog.filter((m) => m.kind === kind))
  const imported = installed.filter((m) => m.kind === kind && !catalogIds.has(m.id))
  const isInstalled = (m: CatalogModel) => m.installed || installedIds.includes(m.id)
  const installedCount = models.filter(isInstalled).length + imported.length

  return (
    <div className="space-y-8">
      <PageHeader
        title="Models"
        description="Open models that run on this computer. Download once, use offline forever."
        actions={
          <>
            <ImportMenu />
            <IconButton label="Refresh" onClick={() => void refresh()} disabled={loading}>
              <RotateCw className={cx('size-4', loading && 'animate-spin')} strokeWidth={1.75} />
            </IconButton>
          </>
        }
      />

      <HardwareStrip hardware={hardware} modelsBytes={modelsBytes} installedCount={installedIds.length} />

      <section className="space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <Segmented label="Model type" value={kind} options={KIND_OPTIONS} onChange={setKind} />
          <p className="font-mono text-[11px] text-subtle">
            {KIND_META[kind]}
            {loaded && !error ? ` · ${installedCount} of ${models.length + imported.length} installed` : ''}
          </p>
        </div>

        {error ? (
          <EmptyState
            icon={<TriangleAlert className="size-4" strokeWidth={1.75} />}
            title="Could not load models"
            description={error}
            action={
              <Button onClick={() => void refresh()} icon={<RotateCw className="size-3.5" strokeWidth={2} />}>
                Try again
              </Button>
            }
          />
        ) : !loaded ? (
          <ListSkeleton />
        ) : (
          <Card className="overflow-hidden">
            <ul className="divide-y divide-line">
              {models.map((model) => (
                <ModelRow
                  key={model.id}
                  model={model}
                  installed={isInstalled(model)}
                  download={downloads[model.id]}
                  hardware={hardware}
                />
              ))}
              {imported.map((model) => (
                <ImportedRow key={model.id} model={model} />
              ))}
            </ul>
          </Card>
        )}
      </section>

      <footer className="flex flex-wrap items-center justify-between gap-3 border-t border-line pt-5">
        <p className="font-mono text-[11px] text-subtle">Models are stored in ~/.talkr/models</p>
        <Button
          variant="ghost"
          size="sm"
          icon={<FolderOpen className="size-3.5" strokeWidth={1.75} />}
          onClick={() => {
            if (!isTauri()) return toast('Available in the Talkr desktop app')
            openDataFolder().catch(toastError)
          }}
        >
          Open folder
        </Button>
      </footer>
    </div>
  )
}

function HardwareStrip({
  hardware,
  modelsBytes,
  installedCount,
}: {
  hardware: HardwareInfo | null
  modelsBytes: number | null
  installedCount: number
}) {
  const gpu = hardware ? primaryGpu(hardware) : null
  const accelerated = hardware ? isAccelerated(hardware) : false
  const cells = [
    {
      label: 'Acceleration',
      value: hardware ? (accelerated ? backendLabel(hardware.recommendedBackend) : 'CPU only') : '—',
      detail: hardware ? (gpu?.name ?? 'No GPU detected') : 'Preview mode',
      dot: true,
    },
    {
      label: 'Processor',
      value: hardware ? `${hardware.cpuCores} cores` : '—',
      detail: hardware ? shortCpuName(hardware.cpuName) : '—',
    },
    {
      label: 'Memory',
      value: hardware ? formatRam(hardware.ramBytes) : '—',
      detail: hardware ? `${hardware.os} · ${hardware.arch}` : '—',
    },
    {
      label: 'Models on disk',
      value: modelsBytes === null ? '—' : formatBytes(modelsBytes),
      detail: `${installedCount} installed`,
    },
  ]

  return (
    <Card className="grid grid-cols-4 divide-x divide-line">
      {cells.map((cell) => (
        <div key={cell.label} className="min-w-0 px-5 py-4">
          <p className="font-mono text-[10.5px] uppercase tracking-[0.16em] text-subtle">{cell.label}</p>
          <p className="mt-2 flex items-center gap-2 text-[15px] font-medium tracking-[-0.01em]">
            {cell.dot ? (
              <span className={cx('size-1.5 shrink-0 rounded-full', accelerated ? 'bg-accent' : 'bg-fg/30')} />
            ) : null}
            {cell.value}
          </p>
          <p className="mt-1 truncate font-mono text-[11px] text-subtle" title={cell.detail}>
            {cell.detail}
          </p>
        </div>
      ))}
    </Card>
  )
}

function ModelRow({
  model,
  installed,
  download,
  hardware,
}: {
  model: CatalogModel
  installed: boolean
  download: ActiveDownload | undefined
  hardware: HardwareInfo | null
}) {
  const startDownload = useModels((s) => s.download)
  const cancel = useModels((s) => s.cancel)
  const remove = useModels((s) => s.remove)
  const { base, variant } = splitName(model.name)
  const tags = tagLabels(model)
  const tooBig = hardware !== null && model.ramRecommendedBytes > hardware.ramBytes

  return (
    <li className="flex items-center gap-6 px-5 py-4 transition-colors duration-200 hover:bg-fg/[0.03]">
      <div className="min-w-0 flex-1">
        <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[14px]">
          <span className="font-medium tracking-[-0.01em]">{base}</span>
          {variant ? <span className="text-muted">{variant}</span> : null}
          {isRecommended(model) ? <Badge solid>Recommended</Badge> : null}
          {tags.map((tag) => (
            <Badge key={tag}>{tag}</Badge>
          ))}
        </p>
        <p className="mt-1 line-clamp-1 text-[13px] text-muted">{model.description}</p>
        <p className="mt-1.5 font-mono text-[11px] text-subtle" title={model.license}>
          {describeLanguages(model.languages)} · {shortLicense(model.license)} · {formatBytes(model.sizeBytes)}
        </p>
        {tooBig ? (
          <p className="mt-2 flex items-center gap-1.5 text-[12px] text-muted">
            <TriangleAlert className="size-3.5 shrink-0" strokeWidth={1.75} />
            Needs more memory than this computer has
            <span className="font-mono text-[11px] text-subtle">{formatBytes(model.ramRecommendedBytes)} recommended</span>
          </p>
        ) : null}
      </div>

      <div className="flex shrink-0 items-center justify-end">
        {download ? (
          <DownloadStatus download={download} onCancel={() => void cancel(model.id)} />
        ) : installed ? (
          <div className="flex items-center gap-1.5">
            <Badge>Installed</Badge>
            <DeleteButton onConfirm={() => void remove(model.id)} />
          </div>
        ) : (
          <Button
            size="sm"
            icon={<Download className="size-3.5" strokeWidth={1.75} />}
            onClick={() => void startDownload(model.id)}
          >
            Download
            <span className="font-mono text-[11px] text-subtle">{formatBytes(model.sizeBytes)}</span>
          </Button>
        )}
      </div>
    </li>
  )
}

function ImportedRow({ model }: { model: InstalledModel }) {
  const remove = useModels((s) => s.remove)
  return (
    <li className="flex items-center gap-6 px-5 py-4 transition-colors duration-200 hover:bg-fg/[0.03]">
      <div className="min-w-0 flex-1">
        <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[14px]">
          <span className="font-medium tracking-[-0.01em]">{model.name}</span>
          <span className="text-muted">Imported</span>
        </p>
        <p className="mt-1 line-clamp-1 text-[13px] text-muted">Added from a file on this computer.</p>
        <p className="mt-1.5 truncate font-mono text-[11px] text-subtle" title={model.path}>
          {model.engine} · {formatBytes(model.manifest.sizeBytes)}
        </p>
      </div>
      <div className="flex shrink-0 items-center gap-1.5">
        <Badge>Installed</Badge>
        <DeleteButton onConfirm={() => void remove(model.id)} />
      </div>
    </li>
  )
}

const STATE_LABELS: Partial<Record<ActiveDownload['state'], string>> = {
  queued: 'Starting',
  verifying: 'Verifying',
  extracting: 'Unpacking',
}

function DownloadStatus({ download, onCancel }: { download: ActiveDownload; onCancel: () => void }) {
  const label =
    download.progress === null
      ? (STATE_LABELS[download.state] ?? 'Starting')
      : `${Math.floor(download.progress * 100)}%`
  const bytes =
    download.bytesTotal > 0
      ? `${formatBytes(download.bytesDone)} / ${formatBytes(download.bytesTotal)}`
      : formatBytes(download.bytesDone)

  return (
    <div className="flex w-56 items-center gap-2">
      <div className="min-w-0 flex-1 space-y-1.5">
        <div className="flex items-baseline justify-between gap-2 font-mono text-[11px] tabular-nums">
          <span className="text-fg">{label}</span>
          {download.state === 'downloading' ? (
            <span className="truncate text-subtle">
              {bytes}
              {download.speed > 0 ? ` · ${formatSpeed(download.speed)}` : ''}
            </span>
          ) : null}
        </div>
        <Progress value={download.progress} />
      </div>
      <IconButton label="Cancel download" onClick={onCancel}>
        <X className="size-4" strokeWidth={1.75} />
      </IconButton>
    </div>
  )
}

/** Two-step delete: the first click arms it for 3 seconds, the second confirms. */
function DeleteButton({ onConfirm }: { onConfirm: () => void }) {
  const [armed, setArmed] = useState(false)

  useEffect(() => {
    if (!armed) return
    const timer = setTimeout(() => setArmed(false), 3000)
    return () => clearTimeout(timer)
  }, [armed])

  if (armed) {
    return (
      <Button
        variant="danger"
        size="sm"
        autoFocus
        onClick={() => {
          setArmed(false)
          onConfirm()
        }}
      >
        Delete?
      </Button>
    )
  }
  return (
    <IconButton label="Delete model" onClick={() => setArmed(true)}>
      <Trash2 className="size-4" strokeWidth={1.75} />
    </IconButton>
  )
}

function ImportMenu() {
  const importModel = useModels((s) => s.importModel)
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const onPointer = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false)
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false)
    }
    window.addEventListener('pointerdown', onPointer)
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('pointerdown', onPointer)
      window.removeEventListener('keydown', onKey)
    }
  }, [open])

  const pick = async (kind: ModelKind) => {
    setOpen(false)
    if (!isTauri()) {
      toast('Importing works in the Talkr desktop app')
      return
    }
    try {
      const path = await openDialog(
        kind === 'stt'
          ? { title: 'Import a Whisper model', multiple: false, filters: [{ name: 'Whisper GGML model', extensions: ['bin'] }] }
          : { title: 'Import a voice model folder', multiple: false, directory: true },
      )
      if (typeof path !== 'string') return
      setBusy(true)
      await importModel(path, kind)
    } catch (e) {
      toastError(e)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div ref={ref} className="relative">
      <Button
        loading={busy}
        icon={<Import className="size-3.5" strokeWidth={1.75} />}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        Import model
      </Button>
      {open ? (
        <div
          role="menu"
          className="absolute right-0 top-full z-20 mt-2 w-64 animate-rise rounded-xl border border-line bg-surface p-1 shadow-[var(--shadow-pop)]"
        >
          <MenuItem title="Speech to text" detail="Whisper GGML file · .bin" onClick={() => void pick('stt')} />
          <MenuItem title="Text to speech" detail="sherpa-onnx model folder" onClick={() => void pick('tts')} />
        </div>
      ) : null}
    </div>
  )
}

function MenuItem({ title, detail, onClick }: { title: string; detail: string; onClick: () => void }) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={onClick}
      className="block w-full rounded-lg px-3 py-2.5 text-left transition-colors duration-200 hover:bg-fg/[0.06]"
    >
      <span className="block text-[13px] font-medium">{title}</span>
      <span className="mt-0.5 block font-mono text-[11px] text-subtle">{detail}</span>
    </button>
  )
}

function ListSkeleton() {
  return (
    <Card className="divide-y divide-line overflow-hidden">
      {[0, 1, 2, 3].map((i) => (
        <div key={i} className="flex items-center gap-6 px-5 py-5">
          <div className="flex-1 space-y-2.5">
            <div className="h-3 w-40 rounded-full bg-fg/[0.07]" />
            <div className="h-2.5 w-72 rounded-full bg-fg/[0.05]" />
          </div>
          <div className="h-8 w-28 rounded-full bg-fg/[0.05]" />
        </div>
      ))}
    </Card>
  )
}
