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
    title = 'Checking hardware'
    detail = '…'
  }

  return (
    <button
      type="button"
      onClick={() => navigate('/models')}
      className="block w-full rounded-lg border border-line p-3 text-left transition-colors duration-200 hover:border-line-strong hover:bg-fg/[0.03]"
    >
      <p className="flex items-center gap-2 text-xs text-muted">
        <span className={cx('size-1.5 shrink-0 rounded-full', accelerated ? 'bg-accent' : 'bg-fg/30')} />
        {title}
      </p>
      <p className="mt-1 truncate font-mono text-[11px] text-subtle" title={detail}>
        {detail}
      </p>
      {active.length > 0 ? (
        <div className="mt-3 space-y-1.5">
          <Progress value={overall} />
          <p className="font-mono text-[11px] text-subtle">
            Downloading {active.length} {active.length === 1 ? 'model' : 'models'}
          </p>
        </div>
      ) : null}
    </button>
  )
}
