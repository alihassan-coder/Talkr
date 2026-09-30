import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'
import { SpeakPage } from '@/pages/SpeakPage'
import {
  emitEvent,
  historyFixture,
  installedFixture,
  mockBackend,
  pathsFixture,
  reject,
  settingsFixture,
} from '@/test/tauri'

const handlers = (extra: Record<string, unknown> = {}) => ({
  list_installed_models: [installedFixture({ id: 'kokoro', kind: 'tts', name: 'Kokoro' })],
  get_settings: settingsFixture({ defaultTtsModel: 'kokoro', defaultVoice: 'am_adam', speechRate: 1.25 }),
  get_app_paths: pathsFixture,
  history_list: { items: [], nextCursor: null },
  list_voices: [
    { id: 'af_heart', name: 'Heart', language: 'en-US', gender: 'female' },
    { id: 'am_adam', name: 'Adam', language: 'en-US', gender: 'male' },
  ],
  synthesize: 'job-7',
  read_audio_file: new ArrayBuffer(44),
  ...extra,
})

const renderPage = () =>
  render(
    <MemoryRouter>
      <SpeakPage />
    </MemoryRouter>,
  )

async function generate(text = 'Hello there') {
  const user = userEvent.setup()
  renderPage()
  const box = await screen.findByRole('textbox', { name: 'Text to speak' })
  await screen.findByRole('combobox', { name: /Voice/ })
  await user.type(box, text)
  await user.click(await screen.findByRole('button', { name: /Generate/ }))
  return user
}

describe('SpeakPage', () => {
  it('asks for a voice model when none is installed', async () => {
    mockBackend(handlers({ list_installed_models: [] }))
    renderPage()
    expect(await screen.findByText('No voice installed yet')).toBeInTheDocument()
  })

  it('synthesizes with the default voice and speed, and plays the result on tts://done', async () => {
    const done = historyFixture({
      id: 'job-7',
      kind: 'tts',
      title: 'Hello there',
      text: 'Hello there',
      audioPath: 'audio/job-7.wav',
      voiceId: 'am_adam',
      modelId: 'kokoro',
      durationMs: 1500,
    })
    const backend = mockBackend(handlers({ history_list: () => ({ items: backend.count('synthesize') ? [done] : [], nextCursor: null }) }))
    await generate()
    expect(backend.argsOf('synthesize')).toEqual([{ text: 'Hello there', modelId: 'kokoro', voiceId: 'am_adam', speed: 1.25 }])
    expect(backend.argsOf('list_voices')).toEqual([{ modelId: 'kokoro' }])
    expect(screen.getByText('Generating speech…')).toBeInTheDocument()

    await emitEvent('tts://done', { jobId: 'job-7', historyItem: done })
    expect(await screen.findByRole('button', { name: 'Save as WAV' })).toBeInTheDocument()
    expect(screen.getByRole('group', { name: /Audio player/ })).toBeInTheDocument()
    expect(screen.queryByText('Generating speech…')).not.toBeInTheDocument()
  })

  it('shows the job://error message on the page', async () => {
    mockBackend(handlers())
    const user = await generate()
    await emitEvent('job://error', {
      jobId: 'job-7',
      error: 'The speech engine stopped unexpectedly (exit code 3). It has been restarted, so please try again.',
      cancelled: false,
    })
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Could not generate speech')
    expect(alert).toHaveTextContent('stopped unexpectedly (exit code 3). It has been restarted')
    expect(screen.getByRole('button', { name: /Generate/ })).toBeEnabled()

    // A new attempt clears the old error.
    await user.click(screen.getByRole('button', { name: /Generate/ }))
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  it('shows a rejected synthesize command as an error', async () => {
    mockBackend(handlers({ synthesize: reject('Not enough free memory to run kokoro.') }))
    await generate()
    expect(await screen.findByRole('alert')).toHaveTextContent('Not enough free memory to run kokoro.')
  })

  it('generates with Ctrl+Enter', async () => {
    const backend = mockBackend(handlers())
    const user = userEvent.setup()
    renderPage()
    const box = await screen.findByRole('textbox', { name: 'Text to speak' })
    await screen.findByRole('combobox', { name: /Voice/ })
    await user.type(box, 'Quick')
    await user.keyboard('{Control>}{Enter}{/Control}')
    expect(backend.count('synthesize')).toBe(1)
  })
})
