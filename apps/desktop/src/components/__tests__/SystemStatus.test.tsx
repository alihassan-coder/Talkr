import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router'
import { describe, expect, it, vi } from 'vitest'
import { catalogFixture, hardwareFixture, mockBackend, reject } from '@/test/tauri'

// The models store keeps module state, so each test gets a fresh copy.
const load = async () => {
  vi.resetModules()
  return (await import('@/components/SystemStatus')).SystemStatus
}

const handlers = (extra: Record<string, unknown> = {}) => ({
  list_catalog: [catalogFixture({ id: 'whisper-base' })],
  list_installed_models: [],
  get_hardware_info: hardwareFixture(),
  get_storage_usage: { modelsBytes: 0, audioBytes: 0, dbBytes: 0, totalBytes: 0 },
  ...extra,
})

const renderStatus = (SystemStatus: Awaited<ReturnType<typeof load>>) =>
  render(
    <MemoryRouter>
      <SystemStatus />
    </MemoryRouter>,
  )

describe('SystemStatus', () => {
  it('shows the GPU once the hardware is known', async () => {
    mockBackend(handlers())
    renderStatus(await load())
    expect(await screen.findByText('GPU · Vulkan')).toBeInTheDocument()
  })

  it('says the hardware is unknown instead of checking forever when it cannot be read', async () => {
    mockBackend(handlers({ get_hardware_info: reject('wmi unavailable') }))
    renderStatus(await load())
    expect(await screen.findByText('Hardware unknown')).toBeInTheDocument()
    expect(screen.getByText('wmi unavailable')).toBeInTheDocument()
    expect(screen.queryByText('Checking hardware')).not.toBeInTheDocument()
  })

  it('says the hardware is unknown when the whole load fails', async () => {
    mockBackend(handlers({ list_catalog: reject('catalog missing') }))
    renderStatus(await load())
    expect(await screen.findByText('Hardware unknown')).toBeInTheDocument()
  })
})
