import { act, cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'
import { SpeakPage } from '@/pages/SpeakPage'
import { useJobs } from '@/stores/jobs'
import {
  emitEvent,
  flush,
  historyFixture,
  installedFixture,
  mockBackend,
  pathsFixture,
  reject,
  settingsFixture,
} from '@/test/tauri'
import { Route, Routes, useLocation } from 'react-router'

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
  read_history_audio: new ArrayBuffer(44),
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

  it('keeps the job across navigation: progress and Cancel on return, then the result', async () => {
    const done = historyFixture({ id: 'job-7', kind: 'tts', title: 'Hello there', text: 'Hello there', audioPath: 'audio/job-7.wav' })
    const backend = mockBackend(handlers({ cancel_job: null }))
    await generate()
    expect(screen.getByText('Generating speech…')).toBeInTheDocument()
    await emitEvent('job://progress', { jobId: 'job-7', progress: 0.3 })

    // Leave the page: the job keeps going.
    cleanup()
    expect(screen.queryByText('Generating speech…')).not.toBeInTheDocument()
    await emitEvent('job://progress', { jobId: 'job-7', progress: 0.6 })

    renderPage()
    expect(await screen.findByText('Generating speech…')).toBeInTheDocument()
    expect(screen.getByText('60%')).toBeInTheDocument()
    expect(screen.getByRole('progressbar', { name: 'Generating speech' })).toHaveAttribute('aria-valuetext', '60%')
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled()

    await emitEvent('tts://done', { jobId: 'job-7', historyItem: done })
    expect(await screen.findByRole('button', { name: 'Save as WAV' })).toBeInTheDocument()
    expect(backend.argsOf('read_history_audio')).toEqual([{ id: 'job-7' }])
  })

  it('shows a finished result that arrived while away', async () => {
    const done = historyFixture({ id: 'job-7', kind: 'tts', title: 'While away', text: 'While away', audioPath: 'audio/job-7.wav' })
    mockBackend(handlers())
    const { unmount } = render(
      <MemoryRouter>
        <SpeakPage />
      </MemoryRouter>,
    )
    await screen.findByRole('combobox', { name: /Voice/ })
    await act(() => useJobs.getState().start('tts', () => Promise.resolve('job-7')))
    unmount()
    await emitEvent('tts://done', { jobId: 'job-7', historyItem: done })
    renderPage()
    expect(await screen.findByText('While away')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Save as WAV' })).toBeInTheDocument()
  })

  it('does not start while a transcription runs, and says why', async () => {
    mockBackend(handlers())
    const user = userEvent.setup()
    renderPage()
    const box = await screen.findByRole('textbox', { name: 'Text to speak' })
    await screen.findByRole('combobox', { name: /Voice/ })
    await act(() => useJobs.getState().start('stt', () => Promise.resolve('job-stt')))
    await user.type(box, 'Hello')
    expect(screen.getByRole('button', { name: /Generate/ })).toBeDisabled()
    expect(screen.getByText('Wait for the transcription to finish.')).toBeInTheDocument()
  })

  it('lays out typed text by its own direction', async () => {
    mockBackend(handlers())
    renderPage()
    expect(await screen.findByRole('textbox', { name: 'Text to speak' })).toHaveAttribute('dir', 'auto')
  })

  it('shows when recent items cannot be loaded, with a retry', async () => {
    let fail = true
    const backend = mockBackend(
      handlers({
        history_list: () => (fail ? Promise.reject('database is locked') : { items: [], nextCursor: null }),
      }),
    )
    const user = userEvent.setup()
    renderPage()
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Could not load recent speech')
    expect(alert).toHaveTextContent('database is locked')
    fail = false
    await user.click(screen.getByRole('button', { name: 'Retry' }))
    await flushUntil(() => backend.count('history_list') === 2)
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  it('shows an inline message when the audio cannot be loaded', async () => {
    const item = historyFixture({ id: 'old', kind: 'tts', title: 'Old clip', audioPath: 'audio/old.wav' })
    mockBackend(
      handlers({
        history_list: { items: [item], nextCursor: null },
        read_history_audio: reject('The audio file is missing.'),
      }),
    )
    const user = userEvent.setup()
    renderPage()
    await user.click(await screen.findByRole('button', { name: /Old clip/ }))
    expect(await screen.findByText(/Audio not available/)).toBeInTheDocument()
    expect(screen.queryByRole('group', { name: /Audio player/ })).not.toBeInTheDocument()
  })

  it('links to the text-to-speech models when no voice is installed', async () => {
    mockBackend(handlers({ list_installed_models: [] }))
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/speak']}>
        <Routes>
          <Route path="/speak" element={<SpeakPage />} />
          <Route path="/models" element={<LocationProbe />} />
        </Routes>
      </MemoryRouter>,
    )
    await user.click(await screen.findByRole('button', { name: 'Browse models' }))
    expect(screen.getByTestId('location')).toHaveTextContent('/models?kind=tts')
  })
})

function LocationProbe() {
  const location = useLocation()
  return <p data-testid="location">{location.pathname + location.search}</p>
}

async function flushUntil(done: () => boolean) {
  for (let i = 0; i < 20 && !done(); i++) await flush()
  await flush()
}
