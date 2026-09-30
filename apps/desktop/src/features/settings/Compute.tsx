import type { ReactNode } from 'react'
import { Cpu, Gpu } from 'lucide-react'
import type { DevicePreference } from '@/lib/types'
import { useEngineStatus } from '@/features/settings/useEngineStatus'
import { Segmented } from '@/components/ui'
import { Notice } from '@/components/Notice'
import { cx } from '@/lib/cx'
import { Row, Section } from '@/features/settings/controls'
import { GPU_FAILED_NOTICE, describeDevice, deviceKindLabel, sortDevices, sttTarget } from '@/features/settings/computeFormat'

const deviceOptions: { value: DevicePreference; label: string }[] = [
  { value: 'auto', label: 'Auto' },
  { value: 'gpu', label: 'GPU' },
  { value: 'cpu', label: 'CPU' },
]

const accelerationHelp: Record<DevicePreference, string> = {
  auto: 'Uses the GPU when one works, and the CPU otherwise.',
  gpu: 'Always tries the GPU first. Falls back to the CPU if it fails.',
  cpu: 'Runs everything on the processor. Slower, but always works.',
}

/**
 * Settings > Compute: which devices the speech engine found, where speech to text
 * runs right now, and the device preference that controls it.
 */
export function ComputeSection({
  device,
  onDeviceChange,
  children,
}: {
  device: DevicePreference
  /** Persist the new preference; the status is refetched once it resolves. */
  onDeviceChange: (device: DevicePreference) => Promise<unknown>
  /** Extra rows, such as CPU threads. */
  children?: ReactNode
}) {
  const { load, refresh } = useEngineStatus()
  const status = load.state === 'ready' ? load.status : null
  const target = status ? sttTarget(status) : null

  const change = async (next: DevicePreference) => {
    await onDeviceChange(next)
    await refresh()
  }

  return (
    <Section title="Compute">
      {status?.gpuFailed ? (
        <div className="border-b border-line px-5 py-4">
          <Notice>{GPU_FAILED_NOTICE}</Notice>
        </div>
      ) : null}

      <Row
        label="Speech to text runs on"
        description={
          load.state === 'preview'
            ? 'Shown in the Talkr desktop app.'
            : load.state === 'error'
              ? 'Could not reach the speech engine. It starts with your first transcription.'
              : 'Text to speech always runs on the CPU. It is fast enough there.'
        }
      >
        <span
          aria-live="polite"
          className="inline-flex items-center gap-2 rounded-full border border-line px-3 py-1 font-mono text-[11.5px] text-muted"
        >
          <span
            aria-hidden="true"
            className={cx('size-1.5 shrink-0 rounded-full', target?.onGpu ? 'bg-accent' : 'bg-fg/30')}
          />
          {load.state === 'loading' ? 'Checking…' : (target?.label ?? '—')}
        </span>
      </Row>

      <Row label="Acceleration" description={accelerationHelp[device]}>
        <Segmented label="Acceleration" value={device} options={deviceOptions} onChange={(d) => void change(d)} />
      </Row>

      {status ? (
        <div className="border-b border-line px-5 py-4 last:border-0">
          <p className="text-[13.5px] font-medium tracking-[-0.005em]">Devices found</p>
          {status.devices.length === 0 ? (
            <p className="mt-0.5 text-[13px] text-muted">None reported yet. They appear once the speech engine has started.</p>
          ) : (
            <ul aria-label="Devices found" className="mt-3 space-y-2">
              {sortDevices(status.devices).map((d, i) => {
                const Icon = d.kind === 'cpu' ? Cpu : Gpu
                return (
                  <li key={`${d.name}-${i}`} className="flex items-center gap-3">
                    <span className="grid size-8 shrink-0 place-items-center rounded-lg border border-line text-muted">
                      <Icon aria-hidden="true" className="size-3.5" strokeWidth={1.75} />
                    </span>
                    <span className="min-w-0">
                      <span className="block truncate text-[13px] text-fg" title={describeDevice(d)}>
                        {describeDevice(d)}
                      </span>
                      <span className="block font-mono text-[11px] text-subtle">{deviceKindLabel(d.kind)}</span>
                    </span>
                  </li>
                )
              })}
            </ul>
          )}
          {!status.gpuAvailable && !status.gpuFailed ? (
            <p className="mt-3 text-[12.5px] text-muted">No GPU the engine can use was found, so Talkr uses the CPU.</p>
          ) : null}
        </div>
      ) : null}

      {children}
    </Section>
  )
}
