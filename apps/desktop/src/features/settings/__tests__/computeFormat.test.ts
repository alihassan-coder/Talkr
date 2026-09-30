import { describe, expect, it } from 'vitest'
import {
  describeDevice,
  deviceBackend,
  deviceKindLabel,
  formatMemory,
  primaryAccelerator,
  sortDevices,
  sttTarget,
} from '@/features/settings/computeFormat'
import type { EngineDevice, EngineStatus } from '@/lib/types'

const GIB = 1024 ** 3
const cpu: EngineDevice = { kind: 'cpu', name: 'CPU', description: 'AMD Ryzen 7', memoryFree: 8 * GIB, memoryTotal: 16 * GIB }
const rtx: EngineDevice = {
  kind: 'gpu',
  name: 'Vulkan0',
  description: 'NVIDIA GeForce RTX 3060',
  memoryFree: 11 * GIB,
  memoryTotal: 12 * GIB,
}
const igpu: EngineDevice = { kind: 'igpu', name: 'Vulkan1', description: 'Intel UHD 630', memoryFree: 0, memoryTotal: 1.5 * GIB }

const status = (patch: Partial<EngineStatus> = {}): EngineStatus => ({
  devices: [cpu, igpu, rtx],
  gpuAvailable: true,
  gpuFailed: false,
  sttUsesGpu: true,
  ...patch,
})

describe('compute formatting', () => {
  it('formats device memory', () => {
    expect(formatMemory(12 * GIB)).toBe('12 GB')
    expect(formatMemory(1.5 * GIB)).toBe('1.5 GB')
    expect(formatMemory(4 * GIB)).toBe('4 GB')
    expect(formatMemory(512 * 1024 ** 2)).toBe('512 MB')
    expect(formatMemory(0)).toBeNull()
  })

  it('derives the backend from the device name', () => {
    expect(deviceBackend(rtx)).toBe('Vulkan')
    expect(deviceBackend({ ...rtx, name: 'Metal' })).toBe('Metal')
    expect(deviceBackend(cpu)).toBe('')
  })

  it('describes devices', () => {
    expect(describeDevice(rtx)).toBe('NVIDIA GeForce RTX 3060, 12 GB (Vulkan)')
    expect(describeDevice(cpu)).toBe('AMD Ryzen 7, 16 GB')
    expect(describeDevice({ ...rtx, description: '', memoryTotal: 0 })).toBe('Vulkan0 (Vulkan)')
  })

  it('labels kinds', () => {
    expect(deviceKindLabel('gpu')).toBe('Graphics card')
    expect(deviceKindLabel('igpu')).toBe('Integrated graphics')
    expect(deviceKindLabel('accelerator')).toBe('Accelerator')
    expect(deviceKindLabel('cpu')).toBe('Processor')
  })

  it('sorts graphics before the CPU without mutating', () => {
    const devices = [cpu, igpu, rtx]
    expect(sortDevices(devices).map((d) => d.kind)).toEqual(['gpu', 'igpu', 'cpu'])
    expect(devices[0]).toBe(cpu)
    expect(primaryAccelerator(status())).toBe(rtx)
    expect(primaryAccelerator(status({ devices: [cpu] }))).toBeNull()
  })

  it('says where speech to text runs', () => {
    expect(sttTarget(status())).toEqual({ onGpu: true, label: 'GPU · NVIDIA GeForce RTX 3060' })
    expect(sttTarget(status({ sttUsesGpu: false }))).toEqual({ onGpu: false, label: 'CPU' })
    expect(sttTarget(status({ devices: [] }))).toEqual({ onGpu: true, label: 'GPU' })
  })
})
