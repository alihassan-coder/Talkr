import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'
import { SettingsPage } from '@/pages/SettingsPage'
import { hardwareFixture, installedFixture, mockBackend, pathsFixture, reject, settingsFixture } from '@/test/tauri'
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

describe('Settings > loading and saving', () => {
  const base = (extra: Record<string, unknown> = {}) => ({
    get_settings: settingsFixture(),
    get_engine_status: gpuStatus,
    list_installed_models: [],
    get_storage_usage: { modelsBytes: 0, audioBytes: 0, dbBytes: 0, totalBytes: 0 },
    get_app_paths: pathsFixture,
    get_hardware_info: hardwareFixture(),
    'plugin:app|version': '9.8.7',
    ...extra,
  })

  const render_ = () =>
    render(
      <MemoryRouter>
        <SettingsPage />
      </MemoryRouter>,
    )

  it('shows the app version from Tauri', async () => {
    mockBackend(base())
    render_()
    expect(await screen.findByText('v9.8.7')).toBeInTheDocument()
  })

  it('shows "dev" when the version cannot be read', async () => {
    mockBackend(base({ 'plugin:app|version': reject('no app plugin') }))
    render_()
    expect(await screen.findByText('dev')).toBeInTheDocument()
  })

  it('offers a retry when the settings cannot be loaded', async () => {
    let fail = true
    mockBackend(base({ get_settings: () => (fail ? Promise.reject('config.json is broken') : settingsFixture()) }))
    const user = userEvent.setup()
    render_()
    expect(await screen.findByText('Could not load settings')).toBeInTheDocument()
    expect(screen.getByText('config.json is broken')).toBeInTheDocument()
    fail = false
    await user.click(screen.getByRole('button', { name: 'Retry' }))
    expect(await screen.findByText('Compute')).toBeInTheDocument()
  })

  it('rolls back only the failed field, keeping a newer change', async () => {
    let failRetention = true
    const api = mockBackend(
      base({
        update_settings: (args: Record<string, unknown>) => {
          const patch = args.settings as PartialSettings
          if ('historyRetentionDays' in patch && failRetention) {
            failRetention = false
            return new Promise((_, rejectWith) => setTimeout(() => rejectWith('disk is read-only'), 30))
          }
          return settingsFixture({ ...patch, historyRetentionDays: 0 } as Partial<Settings>)
        },
      }),
    )
    const user = userEvent.setup()
    render_()
    await screen.findByText('Compute')
    await user.click(screen.getByRole('combobox', { name: 'Keep history' }))
    await user.click(await screen.findByRole('option', { name: '30 days' }))
    await user.click(screen.getByRole('switch', { name: 'Keep recordings' }))
    expect(screen.getByRole('switch', { name: 'Keep recordings' })).toHaveAttribute('aria-checked', 'false')
    await waitFor(() => expect(screen.getByRole('combobox', { name: 'Keep history' })).toHaveTextContent('Forever'))
    // The newer, successful change survives the rollback of the failed one.
    expect(screen.getByRole('switch', { name: 'Keep recordings' })).toHaveAttribute('aria-checked', 'false')
    expect(api.count('update_settings')).toBe(2)
  })

  it('picks a voice of the new speech model when the default voice is not in it', async () => {
    const api = mockBackend(
      base({
        get_settings: settingsFixture({ defaultTtsModel: 'kokoro', defaultVoice: 'af_heart' }),
        list_installed_models: [
          installedFixture({ id: 'kokoro', kind: 'tts', name: 'Kokoro' }),
          installedFixture({ id: 'piper-amy', kind: 'tts', name: 'Piper Amy' }),
        ],
        list_voices: (args: Record<string, unknown>) =>
          args.modelId === 'kokoro'
            ? [{ id: 'af_heart', name: 'Heart', language: 'en', gender: null }]
            : [{ id: 'amy', name: 'Amy', language: 'en', gender: null }],
        update_settings: (args: Record<string, unknown>) => settingsFixture(args.settings as Partial<Settings>),
      }),
    )
    const user = userEvent.setup()
    render_()
    await screen.findByText('Compute')
    await user.click(screen.getByRole('combobox', { name: 'Speech model' }))
    await user.click(await screen.findByRole('option', { name: 'Piper Amy' }))
    await waitFor(() =>
      expect(api.argsOf('update_settings')).toEqual([
        { settings: { defaultTtsModel: 'piper-amy' } },
        { settings: { defaultVoice: 'amy' } },
      ]),
    )
  })

  it('says when models, voices, disk usage or hardware cannot be read', async () => {
    mockBackend(
      base({
        get_settings: settingsFixture({ defaultTtsModel: 'kokoro' }),
        list_installed_models: reject('models folder is unreadable'),
        list_voices: reject('kokoro failed to load'),
        get_storage_usage: reject('access denied'),
        get_hardware_info: reject('wmi unavailable'),
      }),
    )
    render_()
    expect(await screen.findByText('Could not load installed models')).toBeInTheDocument()
    expect(await screen.findByText(/Could not load the voices: kokoro failed to load/)).toBeInTheDocument()
    expect(await screen.findByText(/Could not measure disk usage: access denied/)).toBeInTheDocument()
    expect(await screen.findByText(/Could not read the hardware: wmi unavailable/)).toBeInTheDocument()
  })
})
