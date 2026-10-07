import { useEffect } from 'react'
import { useNavigate } from 'react-router'
import { isTauri } from '@/lib/api'
import { Progress } from '@/components/ui'
import { cx } from '@/lib/cx'
import { useModels, initModels } from '@/stores/models'
import { backendLabel, formatRam, isAccelerated, primaryGpu, shortCpuName } from '@/features/models/format'

export function SystemStatus() {
  const navigate = useNavigate()
  const hardware = useModels((s) => s.hardware)
  const downloads = useModels((s) => s.downloads)
  const hardwareError = useModels((s) => s.hardwareError)
  const loaded = useModels((s) => s.loaded)

  useEffect(() => {
    initModels()
  }, [])

  const active = Object.values(downloads)
  const measurable = active.filter((d) => d.progress !== null)
  const overall =
    measurable.length === 0 ? null : measurable.reduce((sum, d) => sum + (d.progress ?? 0), 0) / measurable.length

  let title = 'Preview mode'
  let detail = 'Running in a browser'
  let accelerated = false
  if (hardware) {
    accelerated = isAccelerated(hardware)
    const gpu = primaryGpu(hardware)
    title = accelerated ? `GPU · ${backendLabel(hardware.recommendedBackend)}` : 'CPU only'
    const chip = accelerated && gpu ? gpu.name : shortCpuName(hardware.cpuName)
    detail = `${chip} · ${formatRam(hardware.ramBytes)}`
  } else if (isTauri()) {
    if (hardwareError || loaded) {
      title = 'Hardware unknown'
      detail = hardwareError ?? 'Could not read this computer'
    } else {
      title = 'Checking hardware'
      detail = '…'
    }
  }

  // Spans rather than paragraphs: a button holds phrasing content.
  return (
    <button
      type="button"
      onClick={() => navigate('/models')}
      title={`${title} · ${detail}`}
      className="block w-full rounded-lg border border-line p-3 text-left transition-colors duration-200 hover:border-line-strong hover:bg-fg/[0.03]"
    >
      <span className="flex items-center gap-2 text-xs leading-4 text-muted">
        <span aria-hidden="true" className={cx('size-1.5 shrink-0 rounded-full', accelerated ? 'bg-accent' : 'bg-subtle')} />
        <span className="min-w-0 truncate">{title}</span>
      </span>
      <span className="mt-1 block truncate font-mono text-[11px] leading-4 text-subtle">{detail}</span>
      {active.length > 0 ? (
        <span className="mt-3 block space-y-1.5">
          <Progress value={overall} label="Model downloads" />
          <span className="block font-mono text-[11px] text-subtle">
            Downloading {active.length} {active.length === 1 ? 'model' : 'models'}
          </span>
        </span>
      ) : null}
    </button>
  )
}
