import type { EngineDevice, EngineDeviceKind, EngineStatus } from '@/lib/types'

const GIB = 1024 ** 3

/** Device memory as the box says it: "12 GB", "1.5 GB", "512 MB". */
export function formatMemory(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return null
  if (bytes < GIB) return `${Math.round(bytes / 1024 ** 2)} MB`
  const gb = bytes / GIB
  return `${gb >= 10 ? Math.round(gb) : Number(gb.toFixed(1))} GB`
}

/** "Vulkan0" -> "Vulkan", "CUDA1" -> "CUDA", "Metal" -> "Metal". Empty for the CPU. */
export function deviceBackend(device: EngineDevice) {
  if (device.kind === 'cpu') return ''
  return device.name.replace(/\d+$/, '').trim()
}

/** "NVIDIA GeForce RTX 3060, 12 GB (Vulkan)" */
export function describeDevice(device: EngineDevice) {
  const name = device.description.trim() || device.name
  const memory = formatMemory(device.memoryTotal)
  const backend = deviceBackend(device)
  return `${name}${memory ? `, ${memory}` : ''}${backend && backend !== name ? ` (${backend})` : ''}`
}

const KIND_LABELS: Record<EngineDeviceKind, string> = {
  gpu: 'Graphics card',
  igpu: 'Integrated graphics',
  accelerator: 'Accelerator',
  cpu: 'Processor',
}

export const deviceKindLabel = (kind: EngineDeviceKind) => KIND_LABELS[kind]

const RANK: Record<EngineDeviceKind, number> = { gpu: 0, igpu: 1, accelerator: 2, cpu: 3 }

/** Graphics first, the CPU last; the engine's order otherwise. */
export const sortDevices = (devices: EngineDevice[]) => [...devices].sort((a, b) => RANK[a.kind] - RANK[b.kind])

/** The GPU speech to text would run on: the first non-CPU device. */
export const primaryAccelerator = (status: EngineStatus) => sortDevices(status.devices).find((d) => d.kind !== 'cpu') ?? null

/** One line for "Speech to text runs on". */
export function sttTarget(status: EngineStatus) {
  if (!status.sttUsesGpu) return { onGpu: false, label: 'CPU' }
  const gpu = primaryAccelerator(status)
  return { onGpu: true, label: gpu ? `GPU · ${gpu.description.trim() || gpu.name}` : 'GPU' }
}

export const GPU_FAILED_NOTICE =
  'The GPU engine stopped, so Talkr switched to the CPU. Changing the setting below tries the GPU again.'
