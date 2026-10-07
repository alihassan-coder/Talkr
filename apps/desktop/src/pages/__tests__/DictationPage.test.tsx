import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'
import { DictationPage } from '@/pages/DictationPage'
import { emitEvent, flush, hardwareFixture, mockBackend, reject, settingsFixture } from '@/test/tauri'
import { defaultDictation } from '@/features/dictation/shortcut'
import type { DictationSettings, DictationStatus, PartialSettings, Settings } from '@/lib/types'

const status = (patch: Partial<DictationStatus> = {}): DictationStatus => ({
  supported: true,
  active: false,
  error: null,
  modelId: 'whisper-small-en-q5',
  warm: false,
  hasLast: false,
  ...patch,
})

function backend(opts: { dictation?: Partial<DictationSettings>; status?: DictationStatus; failSave?: boolean } = {}) {
  let settings: Settings = settingsFixture({ dictation: { ...defaultDictation, ...opts.dictation } })
  return mockBackend({
    get_settings: () => settings,
    update_settings: opts.failSave
      ? reject('Could not save the settings')
      : (args: Record<string, unknown>) => {
          settings = { ...settings, ...(args.settings as PartialSettings) } as Settings
          return settings
        },
    dictation_status: () => opts.status ?? status({ active: !!settings.dictation.enabled }),
    dictation_capture_shortcut: null,
    dictation_warm_up: null,
    list_installed_models: [],
    list_microphones: [
      { id: 'wasapi:1', name: 'Headset Microphone', isDefault: true },
      { id: 'wasapi:2', name: 'USB Microphone', isDefault: false },
    ],
    list_catalog: [],
    get_hardware_info: hardwareFixture(),
    get_storage_usage: { modelsBytes: 0, audioBytes: 0, dbBytes: 0, totalBytes: 0 },
  })
}

const renderPage = async () => {
  render(
    <MemoryRouter>
      <DictationPage />
    </MemoryRouter>,
  )
  await screen.findByRole('heading', { name: 'Dictate anywhere' })
}

const lastDictation = (b: ReturnType<typeof backend>) =>
  (b.argsOf('update_settings').at(-1)!.settings as PartialSettings).dictation!

describe('DictationPage', () => {
  it('turns dictation on and saves the whole dictation object', async () => {
    const user = userEvent.setup()
    const b = backend()
    await renderPage()
    expect(screen.getByText(/Off\. Turn it on/)).toBeInTheDocument()
    await user.click(screen.getByRole('switch', { name: 'Dictation' }))
    await waitFor(() => expect(b.count('update_settings')).toBe(1))
    expect(lastDictation(b)).toEqual({ ...defaultDictation, enabled: true })
    expect(await screen.findByText('Ready everywhere')).toBeInTheDocument()
  })

  it('shows the shortcut and how to use it', async () => {
    backend()
    await renderPage()
    expect(screen.getAllByLabelText('Ctrl plus Win').length).toBeGreaterThan(0)
    expect(screen.getByText('to talk')).toBeInTheDocument()
    expect(screen.getByText('for hands-free')).toBeInTheDocument()
  })

  it('records a new shortcut from the keyboard hook', async () => {
    const user = userEvent.setup()
    const b = backend({ dictation: { enabled: true } })
    await renderPage()
    await user.click(screen.getByRole('button', { name: 'Change dictation shortcut' }))
    expect(screen.getByText('Press the new shortcut…')).toBeInTheDocument()
    await waitFor(() => expect(b.argsOf('dictation_capture_shortcut')).toEqual([{ active: true }]))
    await flush()
    await emitEvent('dictation://captured', { ctrl: true, shift: false, alt: true, win: false, key: 32, keyLabel: 'Space' })
    await waitFor(() => expect(b.count('update_settings')).toBe(1))
    expect(lastDictation(b).shortcut).toEqual({ ctrl: true, shift: false, alt: true, win: false, key: 32, keyLabel: 'Space' })
  })

  it('refuses a shortcut that would fire while typing', async () => {
    const user = userEvent.setup()
    const b = backend({ dictation: { enabled: true } })
    await renderPage()
    await user.click(screen.getByRole('button', { name: 'Change dictation shortcut' }))
    await flush()
    await emitEvent('dictation://captured', { ctrl: false, shift: false, alt: false, win: false, key: 65, keyLabel: 'A' })
    expect(await screen.findByRole('alert')).toHaveTextContent(/Add Ctrl/)
    expect(b.count('update_settings')).toBe(0)
  })

  it('refuses the paste-again shortcut as the dictation shortcut', async () => {
    const user = userEvent.setup()
    const b = backend({ dictation: { enabled: true } })
    await renderPage()
    await user.click(screen.getByRole('button', { name: 'Change dictation shortcut' }))
    await flush()
    await emitEvent('dictation://captured', { ctrl: false, shift: true, alt: true, win: false, key: 0x56, keyLabel: 'V' })
    expect(await screen.findByRole('alert')).toHaveTextContent(/already the paste-again shortcut/)
    expect(b.count('update_settings')).toBe(0)
  })

  it('adds vocabulary words and replacements', async () => {
    const user = userEvent.setup()
    const b = backend()
    await renderPage()
    await user.type(screen.getByLabelText('New vocabulary words'), 'Talkr, Kokoro{Enter}')
    await waitFor(() => expect(lastDictation(b).vocabulary).toEqual(['Talkr', 'Kokoro']))
    await user.type(screen.getByLabelText('Words to replace'), 'talker')
    await user.type(screen.getByLabelText('Replacement'), 'Talkr{Enter}')
    await waitFor(() => expect(lastDictation(b).replacements).toEqual([{ from: 'talker', to: 'Talkr' }]))
    await user.click(screen.getByRole('button', { name: 'Remove Kokoro' }))
    await waitFor(() => expect(lastDictation(b).vocabulary).toEqual(['Talkr']))
  })

  it('adds an app rule from a plain app name', async () => {
    const user = userEvent.setup()
    const b = backend()
    await renderPage()
    await user.type(screen.getByLabelText('App to add a rule for'), 'MSTSC{Enter}')
    await waitFor(() => expect(lastDictation(b).appRules).toEqual([{ app: 'mstsc.exe', method: 'type', learned: false }]))
  })

  it('shows rules Talkr learned', async () => {
    backend({ dictation: { appRules: [{ app: 'oldapp.exe', method: 'type', learned: true }] } })
    await renderPage()
    expect(screen.getByText('oldapp.exe')).toBeInTheDocument()
    expect(screen.getByText('Learned')).toBeInTheDocument()
  })

  it('rolls back a failed save', async () => {
    const user = userEvent.setup()
    backend({ failSave: true })
    await renderPage()
    const sounds = screen.getByRole('switch', { name: 'Sounds' })
    expect(sounds).toHaveAttribute('aria-checked', 'true')
    await user.click(sounds)
    await waitFor(() => expect(sounds).toHaveAttribute('aria-checked', 'true'))
  })

  it('offers a model download when none is installed', async () => {
    backend({ status: status({ modelId: null }) })
    await renderPage()
    expect(await screen.findByText('Dictation needs a speech model')).toBeInTheDocument()
  })

  it('explains when the system is not supported', async () => {
    backend({ status: status({ supported: false }) })
    await renderPage()
    expect(await screen.findByText(/available on Windows for now/)).toBeInTheDocument()
    expect(screen.getByRole('switch', { name: 'Dictation' })).toBeDisabled()
  })

  it('follows settings changed elsewhere (the tray)', async () => {
    backend()
    await renderPage()
    await flush()
    await emitEvent('dictation://settings', settingsFixture({ dictation: { ...defaultDictation, enabled: true } }))
    await waitFor(() => expect(screen.getByRole('switch', { name: 'Dictation' })).toHaveAttribute('aria-checked', 'true'))
  })

  it('lists microphones with the default marked', async () => {
    const user = userEvent.setup()
    const b = backend()
    await renderPage()
    await user.click(screen.getByRole('combobox', { name: 'Microphone' }))
    await user.click(await screen.findByRole('option', { name: 'USB Microphone' }))
    await waitFor(() => expect(lastDictation(b).microphone).toBe('wasapi:2'))
  })
})
