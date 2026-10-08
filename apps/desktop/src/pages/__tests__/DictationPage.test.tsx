import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'
import { emit } from '@tauri-apps/api/event'
import { DictationPage } from '@/pages/DictationPage'
import { emitEvent, flush, hardwareFixture, mockBackend, reject, settingsFixture } from '@/test/tauri'
import { defaultDictation } from '@/features/dictation/shortcut'
import type { DictationCapabilities, DictationSettings, DictationStatus, PartialSettings, Settings } from '@/lib/types'

const status = (patch: Partial<DictationStatus> = {}): DictationStatus => ({
  supported: true,
  capabilities: {
    os: 'windows',
    supported: true,
    holdToTalk: true,
    modifierOnly: true,
    recordsShortcut: true,
    verifiesInsertion: true,
    insertsText: true,
    metaKey: 'Win',
    note: null,
  },
  permission: { state: 'notNeeded' },
  active: false,
  error: null,
  modelId: 'whisper-small-en-q5',
  warm: false,
  hasLast: false,
  recording: false,
  ...patch,
})

const caps = (patch: Partial<DictationCapabilities>): DictationCapabilities => ({ ...status().capabilities, ...patch })

const wayland = caps({
  os: 'linux',
  metaKey: 'Super',
  holdToTalk: false,
  modifierOnly: false,
  recordsShortcut: false,
  verifiesInsertion: false,
  insertsText: false,
  note: 'Wayland session',
})

function backend(
  opts: {
    dictation?: Partial<DictationSettings>
    status?: DictationStatus | (() => DictationStatus)
    failSave?: boolean
    extra?: Record<string, unknown>
  } = {},
) {
  let settings: Settings = settingsFixture({ dictation: { ...defaultDictation, ...opts.dictation } })
  return mockBackend({
    get_settings: () => settings,
    update_settings: opts.failSave
      ? reject('Could not save the settings')
      : (args: Record<string, unknown>) => {
          settings = { ...settings, ...(args.settings as PartialSettings) } as Settings
          return settings
        },
    dictation_status: () =>
      typeof opts.status === 'function' ? opts.status() : (opts.status ?? status({ active: !!settings.dictation.enabled })),
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
    ...opts.extra,
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
    expect(screen.getByText('Turn it on to use your voice in any app.')).toBeInTheDocument()
    await user.click(screen.getByRole('switch', { name: 'Dictation' }))
    await waitFor(() => expect(b.count('update_settings')).toBe(1))
    expect(lastDictation(b)).toEqual({ ...defaultDictation, enabled: true })
    // On, with the model still loading into memory.
    expect(await screen.findByText('Loading the model')).toBeInTheDocument()
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
    expect(await screen.findByText(/not available on Windows yet/)).toBeInTheDocument()
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

  it('writes keys the way macOS does', async () => {
    backend({ status: status({ capabilities: caps({ os: 'macos', metaKey: '⌘' }) }) })
    await renderPage()
    expect(screen.getAllByLabelText('Control plus Command').length).toBeGreaterThan(0)
    expect(screen.getAllByText('⌃').length).toBeGreaterThan(0)
    expect(screen.getAllByText('⌘').length).toBeGreaterThan(0)
    expect(screen.queryByText('Win')).toBeNull()
    expect(screen.getByRole('switch', { name: 'Open at login' })).toBeInTheDocument()
  })

  it('calls the Win key Super on Linux', async () => {
    backend({ status: status({ capabilities: caps({ os: 'linux', metaKey: 'Super' }) }) })
    await renderPage()
    expect(screen.getAllByLabelText('Ctrl plus Super').length).toBeGreaterThan(0)
  })

  it('asks for a missing permission and clears once it is granted', async () => {
    const user = userEvent.setup()
    let granted = false
    const b = backend({
      status: () =>
        status({
          permission: granted
            ? { state: 'granted' }
            : { state: 'missing', title: 'Accessibility', detail: 'Talkr needs Accessibility to type for you.', canRequest: true },
        }),
      extra: {
        dictation_request_permission: () => {
          granted = true
          return null
        },
      },
    })
    await renderPage()
    expect(await screen.findByRole('alert', { name: 'Permission needed: Accessibility' })).toHaveTextContent(/type for you/)
    expect(screen.getByText('Needs your permission')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Allow Accessibility' }))
    expect(b.count('dictation_request_permission')).toBe(1)
    await waitFor(() => expect(screen.queryByRole('alert', { name: /Permission needed/ })).toBeNull())
  })

  it('explains a desktop that chooses the shortcut and only copies text (Wayland)', async () => {
    backend({ status: status({ capabilities: wayland }) })
    await renderPage()
    expect(screen.queryByRole('button', { name: 'Change dictation shortcut' })).toBeNull()
    expect(screen.getByText('Your desktop chooses the shortcut')).toBeInTheDocument()
    expect(screen.getByText('talkr --dictate')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Copy talkr --dictate' })).toBeInTheDocument()
    expect(screen.getByText('Your words are copied, ready to paste')).toBeInTheDocument()
    // No insertion settings or per-app rules where nothing is inserted.
    expect(screen.queryByRole('radiogroup', { name: 'How text goes in' })).toBeNull()
    expect(screen.queryByLabelText('App to add a rule for')).toBeNull()
    // Press to start, press to stop: holding is not offered.
    expect(screen.queryByRole('radiogroup', { name: 'How dictation starts' })).toBeNull()
    expect(screen.getAllByText('Press twice').length).toBeGreaterThan(0)
  })

  it('offers only press to start and stop when the release is not reported', async () => {
    backend({ status: status({ capabilities: caps({ holdToTalk: false }) }), dictation: { mode: 'auto' } })
    await renderPage()
    expect(screen.queryByRole('radiogroup', { name: 'How dictation starts' })).toBeNull()
    expect(screen.getByText(/does not report when a shortcut is let go/)).toBeInTheDocument()
    expect(screen.getByText(/speak, and press it again/)).toBeInTheDocument()
    // The recorder is still there: only holding is unavailable.
    expect(screen.getByRole('button', { name: 'Change dictation shortcut' })).toBeInTheDocument()
  })

  it('shows the system note when dictation is not supported', async () => {
    backend({ status: status({ supported: false, capabilities: caps({ supported: false, note: 'Coming to this desktop soon' }) }) })
    await renderPage()
    expect(await screen.findByText('Coming to this desktop soon')).toBeInTheDocument()
  })

  it('shows the keys as they are pressed while recording', async () => {
    const user = userEvent.setup()
    backend({
      dictation: { enabled: true },
      extra: {
        dictation_capture_shortcut: (args: Record<string, unknown>) => {
          if (args.active) void emit('dictation://capturing', { ctrl: true, shift: false, alt: true, win: false, key: null, keyLabel: null })
          return null
        },
      },
    })
    await renderPage()
    await user.click(screen.getByRole('button', { name: 'Change dictation shortcut' }))
    expect(await screen.findByLabelText(/Ctrl plus Alt/)).toBeInTheDocument()
  })

  it('says when the keyboard cannot be listened to', async () => {
    const user = userEvent.setup()
    let failed = false
    backend({
      dictation: { enabled: true },
      status: () => status({ active: !failed, error: failed ? 'The keyboard hook could not start' : null }),
      extra: {
        // The backend answers at once when it cannot listen.
        dictation_capture_shortcut: (args: Record<string, unknown>) => {
          if (args.active) {
            failed = true
            void emit('dictation://capture-failed', { message: 'The keyboard hook could not start' })
          }
          return null
        },
      },
    })
    await renderPage()
    await user.click(screen.getByRole('button', { name: 'Change dictation shortcut' }))
    expect(await screen.findByText(/Talkr could not listen to the keyboard: The keyboard hook could not start/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Change dictation shortcut' })).toBeInTheDocument()
  })

  it('cancels recording and lets go of the keyboard', async () => {
    const user = userEvent.setup()
    const b = backend({ dictation: { enabled: true } })
    await renderPage()
    await user.click(screen.getByRole('button', { name: 'Change dictation shortcut' }))
    await waitFor(() => expect(b.argsOf('dictation_capture_shortcut')).toEqual([{ active: true }]))
    await user.click(screen.getByRole('button', { name: 'Stop recording dictation shortcut' }))
    expect(b.argsOf('dictation_capture_shortcut')).toEqual([{ active: true }, { active: false }])
    expect(screen.queryByText('Press the new shortcut…')).toBeNull()
    // Escape in the hook arrives as null: back to the shortcut, no error.
    await user.click(screen.getByRole('button', { name: 'Change dictation shortcut' }))
    await waitFor(() => expect(b.count('dictation_capture_shortcut')).toBe(3))
    await flush()
    await emitEvent('dictation://captured', null)
    await waitFor(() => expect(screen.queryByText('Press the new shortcut…')).toBeNull())
    expect(screen.queryByText(/could not listen/)).toBeNull()
  })

  it('refuses a modifier-only shortcut where the system needs a key', async () => {
    const user = userEvent.setup()
    const b = backend({
      dictation: { enabled: true },
      status: status({ capabilities: caps({ os: 'macos', metaKey: '⌘', modifierOnly: false }) }),
    })
    await renderPage()
    await user.click(screen.getByRole('button', { name: 'Change dictation shortcut' }))
    await flush()
    await emitEvent('dictation://captured', { ctrl: true, shift: false, alt: false, win: true, key: null, keyLabel: null })
    expect(await screen.findByText('This system needs a key with the modifiers, like ⌃⌘Space.')).toBeInTheDocument()
    expect(b.count('update_settings')).toBe(0)
  })

  it('resets a changed shortcut to the default', async () => {
    const user = userEvent.setup()
    const b = backend({ dictation: { shortcut: { ctrl: true, shift: false, alt: true, win: false, key: 32, keyLabel: 'Space' } } })
    await renderPage()
    await user.click(screen.getByRole('button', { name: 'Reset dictation shortcut' }))
    await waitFor(() => expect(lastDictation(b).shortcut).toEqual(defaultDictation.shortcut))
  })

  it('guides first-run setup and folds it away when skipped', async () => {
    const user = userEvent.setup()
    backend()
    await renderPage()
    expect(screen.getByText('Get set up')).toBeInTheDocument()
    expect(screen.getByText('1 of 3')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Use this' }))
    expect(screen.getByText('2 of 3')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Turn on dictation' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Skip setup' }))
    expect(screen.queryByText('Get set up')).toBeNull()
  })

  it('previews every state of the pill', async () => {
    const user = userEvent.setup()
    backend()
    await renderPage()
    const pill = () => document.querySelector('.dict-preview-pill .pill')!
    expect(pill()).toHaveAccessibleName('Listening')
    await user.click(screen.getByRole('button', { name: 'Inserted' }))
    expect(pill()).toHaveAccessibleName('Inserted: Let’s ship the new onboarding on Friday.')
    await user.click(screen.getByRole('button', { name: 'Hands-free' }))
    expect(pill()).toHaveAccessibleName('Dictating hands-free')
    await user.click(screen.getByRole('button', { name: 'Done' }))
    expect(pill()).toHaveAccessibleName('Transcribing')
    await user.click(screen.getByRole('button', { name: 'Copied' }))
    expect(pill()).toHaveAccessibleName(/Copied, press Ctrl \+ V to paste/)
  })
})
