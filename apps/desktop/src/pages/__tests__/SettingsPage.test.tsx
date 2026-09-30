import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'
import { SettingsPage } from '@/pages/SettingsPage'
import { hardwareFixture, mockBackend, pathsFixture, reject, settingsFixture } from '@/test/tauri'
import type { EngineStatus, PartialSettings, Settings } from '@/lib/types'

const GIB = 1024 ** 3
const gpuStatus: EngineStatus = {
  devices: [
    { kind: 'cpu', name: 'CPU', description: 'Intel Core i7-9750H', memoryFree: 8 * GIB, memoryTotal: 16 * GIB },
    { kind: 'gpu', name: 'Vulkan0', description: 'NVIDIA GeForce RTX 3060', memoryFree: 11 * GIB, memoryTotal: 12 * GIB },
  ],
  gpuAvailable: true,
  gpuFailed: false,
  sttUsesGpu: true,
}

function backend(statuses: (EngineStatus | (() => Promise<never>))[], initial: Partial<Settings> = {}) {
  let settings = settingsFixture(initial)
  let n = 0
  return mockBackend({
    get_settings: () => settings,
    update_settings: (args: Record<string, unknown>) => {
      settings = { ...settings, ...(args.settings as PartialSettings) } as Settings
      return settings
    },
    get_engine_status: () => {
      const next = statuses[Math.min(n++, statuses.length - 1)]!
      return typeof next === 'function' ? next() : next
    },
    list_installed_models: [],
    get_storage_usage: { modelsBytes: 0, audioBytes: 0, dbBytes: 0, totalBytes: 0 },
    get_app_paths: pathsFixture,
    get_hardware_info: hardwareFixture(),
  })
}

const renderPage = async () => {
  render(
    <MemoryRouter>
      <SettingsPage />
    </MemoryRouter>,
  )
  await screen.findByText('Compute')
}

const compute = () => screen.getByText('Compute').closest('section')!

describe('Settings > Compute', () => {
  it('lists the devices and says speech to text runs on the GPU', async () => {
    backend([gpuStatus])
    await renderPage()
    const devices = await within(compute()).findByRole('list', { name: 'Devices found' })
    const rows = within(devices).getAllByRole('listitem')
    expect(rows[0]).toHaveTextContent('NVIDIA GeForce RTX 3060, 12 GB (Vulkan)')
    expect(rows[0]).toHaveTextContent('Graphics card')
    expect(rows[1]).toHaveTextContent('Intel Core i7-9750H, 16 GB')
    expect(within(compute()).getByText('GPU · NVIDIA GeForce RTX 3060')).toBeInTheDocument()
    expect(within(compute()).queryByRole('status', { name: /GPU engine stopped/ })).not.toBeInTheDocument()
    expect(screen.queryByText(/The GPU engine stopped/)).not.toBeInTheDocument()
  })

  it('shows the calm notice when the GPU engine failed', async () => {
    backend([{ ...gpuStatus, gpuFailed: true, gpuAvailable: false, sttUsesGpu: false }])
    await renderPage()
    expect(
      await screen.findByText(
        'The GPU engine stopped, so Talkr switched to the CPU. Changing the setting below tries the GPU again.',
      ),
    ).toBeInTheDocument()
    expect(within(compute()).getByText('CPU', { selector: 'span[aria-live]' })).toBeInTheDocument()
  })

  it('saves the device setting and refetches the status afterwards', async () => {
    const api = backend([{ ...gpuStatus, gpuFailed: true, sttUsesGpu: false }, { ...gpuStatus, sttUsesGpu: false }], {
      device: 'auto',
    })
    const user = userEvent.setup()
    await renderPage()
    await screen.findByText(/The GPU engine stopped/)
    const group = within(compute()).getByRole('tablist', { name: 'Acceleration' })
    expect(within(group).getByRole('tab', { name: 'Auto' })).toHaveAttribute('aria-selected', 'true')

    await user.click(within(group).getByRole('tab', { name: 'CPU' }))
    expect(api.argsOf('update_settings')).toEqual([{ settings: { device: 'cpu' } }])
    await within(compute()).findByText('CPU', { selector: 'span[aria-live]' })
    expect(api.count('get_engine_status')).toBe(2)
    // update_settings finished before the status was refetched.
    const order = api.calls.map((c) => c.cmd).filter((c) => c === 'update_settings' || c === 'get_engine_status')
    expect(order).toEqual(['get_engine_status', 'update_settings', 'get_engine_status'])
    expect(screen.queryByText(/The GPU engine stopped/)).not.toBeInTheDocument()
    expect(within(group).getByRole('tab', { name: 'CPU' })).toHaveAttribute('aria-selected', 'true')
  })

  it('explains when the engine status cannot be read', async () => {
    backend([reject('engine not running')])
    await renderPage()
    expect(await screen.findByText(/Could not reach the speech engine/)).toBeInTheDocument()
  })

  it('shows a preview outside the desktop app', async () => {
    render(
      <MemoryRouter>
        <SettingsPage />
      </MemoryRouter>,
    )
    expect(await screen.findByText('Shown in the Talkr desktop app.')).toBeInTheDocument()
  })
})
