import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react'
import { useSearchParams } from 'react-router'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { Download, FolderOpen, Import, RotateCw, Trash2, TriangleAlert, X } from 'lucide-react'
import { isTauri, openDataFolder } from '@/lib/api'
import type { CatalogModel, HardwareInfo, InstalledModel, ModelKind } from '@/lib/types'
import { Badge, Button, Card, EmptyState, IconButton, PageHeader, Progress, Segmented } from '@/components/ui'
import { cx } from '@/lib/cx'
import { type ActiveDownload, initModels, useModels } from '@/stores/models'
import { toast, toastError } from '@/stores/toast'
import {
  COMPRESSED_HINT,
  backendLabel,
  compressedAlternative,
  describeLanguages,
  formatBytes,
  formatRam,
  formatSpeed,
  isAccelerated,
  isCompressed,
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
  const failures = useModels((s) => s.failures)
  const hardware = useModels((s) => s.hardware)
  const modelsBytes = useModels((s) => s.modelsBytes)
  const loading = useModels((s) => s.loading)
  const loaded = useModels((s) => s.loaded)
  const error = useModels((s) => s.error)
  const refresh = useModels((s) => s.refresh)
  // Other screens link here with ?kind=tts or ?kind=stt to open the right list.
  const [searchParams] = useSearchParams()
  const [kind, setKind] = useState<ModelKind>(() => (searchParams.get('kind') === 'tts' ? 'tts' : 'stt'))

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
                  failure={failures[model.id]}
                  alternative={compressedAlternative(model, catalog)}
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
  failure,
  alternative,
  hardware,
}: {
  model: CatalogModel
  installed: boolean
  download: ActiveDownload | undefined
  failure: string | undefined
  alternative: CatalogModel | null
  hardware: HardwareInfo | null
}) {
  const startDownload = useModels((s) => s.download)
  const dismissFailure = useModels((s) => s.dismissFailure)
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
          {isCompressed(model) ? <Badge>{COMPRESSED_HINT}</Badge> : null}
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
            {alternative && hardware && alternative.ramRecommendedBytes <= hardware.ramBytes ? (
              <span>· the compressed version fits</span>
            ) : null}
          </p>
        ) : null}
        {failure && !download ? (
          <div role="alert" className="mt-2 flex items-start gap-1.5 text-[12.5px] leading-relaxed text-fg">
            <TriangleAlert aria-hidden="true" className="mt-0.5 size-3.5 shrink-0 text-accent" strokeWidth={1.75} />
            <span data-selectable className="min-w-0 flex-1">
              {failure}
            </span>
            <button
              type="button"
              onClick={() => dismissFailure(model.id)}
              aria-label="Dismiss error"
              className="grid size-5 shrink-0 place-items-center rounded text-subtle transition-colors hover:bg-fg/[0.06] hover:text-fg"
            >
              <X className="size-3" strokeWidth={2} />
            </button>
          </div>
        ) : null}
      </div>

      <div className="flex shrink-0 items-center justify-end">
        {download ? (
          <DownloadStatus name={model.name} download={download} onCancel={() => void cancel(model.id)} />
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
            {failure ? 'Try again' : 'Download'}
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

function DownloadStatus({
  name,
  download,
  onCancel,
}: {
  name: string
  download: ActiveDownload
  onCancel: () => void
}) {
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
        <Progress value={download.progress} label={`Downloading ${name}`} />
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

const menuItems = (menu: HTMLElement | null) =>
  Array.from(menu?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])

function ImportMenu() {
  const importModel = useModels((s) => s.importModel)
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  const triggerRef = useRef<HTMLButtonElement>(null)
  const menuRef = useRef<HTMLDivElement>(null)

  const items = () => menuItems(menuRef.current)

  const close = (restoreFocus: boolean) => {
    setOpen(false)
    if (restoreFocus) triggerRef.current?.focus()
  }

  useEffect(() => {
    if (!open) return
    // Focus moves into the menu when it opens.
    menuItems(menuRef.current)[0]?.focus()
    const onPointer = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false)
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        setOpen(false)
        triggerRef.current?.focus()
      }
    }
    window.addEventListener('pointerdown', onPointer)
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('pointerdown', onPointer)
      window.removeEventListener('keydown', onKey)
    }
  }, [open])

  const onMenuKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    const list = items()
    if (list.length === 0) return
    const index = list.indexOf(document.activeElement as HTMLElement)
    let next: number | null = null
    if (e.key === 'ArrowDown') next = (index + 1) % list.length
    else if (e.key === 'ArrowUp') next = (index - 1 + list.length) % list.length
    else if (e.key === 'Home') next = 0
    else if (e.key === 'End') next = list.length - 1
    else if (e.key === 'Tab') close(false)
    if (next !== null) {
      e.preventDefault()
      list[next]?.focus()
    }
  }

  const pick = async (kind: ModelKind) => {
    close(true)
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
        ref={triggerRef}
        loading={busy}
        icon={<Import className="size-3.5" strokeWidth={1.75} />}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? 'import-menu' : undefined}
        onClick={() => setOpen((v) => !v)}
        onKeyDown={(e) => {
          if (e.key === 'ArrowDown' && !open) {
            e.preventDefault()
            setOpen(true)
          }
        }}
      >
        Import model
      </Button>
      {open ? (
        <div
          ref={menuRef}
          id="import-menu"
          role="menu"
          aria-label="Import model"
          onKeyDown={onMenuKeyDown}
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
      tabIndex={-1}
      onClick={onClick}
      className="block w-full rounded-lg px-3 py-2.5 text-left transition-colors duration-200 hover:bg-fg/[0.06] focus-visible:bg-fg/[0.06]"
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
