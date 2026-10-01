import { act, cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router'
import { describe, expect, it } from 'vitest'
import { TranscribePage } from '@/pages/TranscribePage'
import { useJobs } from '@/stores/jobs'
import { emitEvent, flush, historyFixture, installedFixture, mockBackend, reject, settingsFixture } from '@/test/tauri'

const handlers = (extra: Record<string, unknown> = {}) => ({
  list_installed_models: [
    installedFixture({ id: 'whisper-base', kind: 'stt', name: 'Whisper Base' }),
    installedFixture({ id: 'kokoro', kind: 'tts', name: 'Kokoro' }),
  ],
  get_settings: settingsFixture({ defaultSttModel: 'whisper-base', sttLanguage: 'de' }),
  'plugin:dialog|open': 'C:\\audio\\interview.mp3',
  transcribe_file: 'job-1',
  cancel_job: null,
  ...extra,
})

const renderPage = () =>
  render(
    <MemoryRouter>
      <TranscribePage />
    </MemoryRouter>,
  )

async function startFileJob() {
  const user = userEvent.setup()
  renderPage()
  await user.click(await screen.findByRole('radio', { name: 'File' }))
  await user.click(screen.getByRole('button', { name: 'Choose file' }))
  await screen.findByText('interview.mp3')
  return user
}

describe('TranscribePage', () => {
  it('offers the Models page when no Whisper model is installed', async () => {
    mockBackend(handlers({ list_installed_models: [] }))
    renderPage()
    expect(await screen.findByText('No Whisper model installed')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Browse models' })).toBeInTheDocument()
  })

  it('starts a job with the chosen file, model and language, and shows the result on stt://done', async () => {
    const backend = mockBackend(handlers())
    await startFileJob()
    expect(backend.argsOf('transcribe_file')).toEqual([
      { path: 'C:\\audio\\interview.mp3', modelId: 'whisper-base', language: 'de', translate: false },
    ])
    expect(screen.getByText(/Transcribing/)).toBeInTheDocument()

    await emitEvent('job://progress', { jobId: 'job-1', progress: 0.4 })
    expect(screen.getByText('Transcribing · 40%')).toBeInTheDocument()

    // Events for other jobs are ignored.
    await emitEvent('stt://done', { jobId: 'other', historyItem: historyFixture({ id: 'other', text: 'Wrong one' }) })
    expect(screen.queryByText('Wrong one')).not.toBeInTheDocument()

    await emitEvent('stt://done', {
      jobId: 'job-1',
      historyItem: historyFixture({ id: 'job-1', text: 'Hello from the interview.' }),
    })
    expect(await screen.findByText('Hello from the interview.')).toBeInTheDocument()
    expect(screen.queryByText(/Transcribing/)).not.toBeInTheDocument()
  })

  it('shows the job://error message clearly and lets the user dismiss it', async () => {
    mockBackend(handlers())
    const user = await startFileJob()
    const message = 'The speech engine ran out of memory. Close other apps, or pick a smaller or compressed model in Models.'
    await emitEvent('job://error', { jobId: 'job-1', error: message, cancelled: false })
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Transcription failed')
    expect(alert).toHaveTextContent(message)
    expect(screen.getByRole('button', { name: 'Choose file' })).toBeEnabled()
    await user.click(screen.getByRole('button', { name: 'Dismiss' }))
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  it('never shows raw JSON for an error', async () => {
    mockBackend(handlers())
    await startFileJob()
    await emitEvent('job://error', {
      jobId: 'job-1',
      error: '{"message":"The speech engine stopped unexpectedly. It has been restarted, so please try again."}',
      cancelled: false,
    })
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('The speech engine stopped unexpectedly. It has been restarted, so please try again.')
    expect(alert.textContent).not.toContain('{')
  })

  it('shows an error when the command itself is rejected', async () => {
    mockBackend(handlers({ transcribe_file: reject('Not enough free memory to run whisper-base: it needs about 1.2 GB.') }))
    const user = userEvent.setup()
    renderPage()
    await user.click(await screen.findByRole('radio', { name: 'File' }))
    await user.click(screen.getByRole('button', { name: 'Choose file' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Not enough free memory to run whisper-base')
  })

  it('cancels a running job without showing an error', async () => {
    const backend = mockBackend(handlers())
    const user = await startFileJob()
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(backend.argsOf('cancel_job')).toEqual([{ jobId: 'job-1' }])
    await emitEvent('job://error', { jobId: 'job-1', error: 'cancelled', cancelled: true })
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    expect(screen.queryByText(/Transcribing/)).not.toBeInTheDocument()
  })

  it('sends a Cancel pressed before the job id arrives once it does', async () => {
    let resolveJob!: (id: string) => void
    const backend = mockBackend(handlers({ transcribe_file: () => new Promise<string>((r) => (resolveJob = r)) }))
    const user = userEvent.setup()
    renderPage()
    await user.click(await screen.findByRole('radio', { name: 'File' }))
    await user.click(screen.getByRole('button', { name: 'Choose file' }))
    await user.click(await screen.findByRole('button', { name: 'Cancel' }))
    expect(screen.getByRole('button', { name: 'Cancelling…' })).toBeDisabled()
    expect(backend.count('cancel_job')).toBe(0)
    await act(async () => resolveJob('job-1'))
    await flush()
    expect(backend.argsOf('cancel_job')).toEqual([{ jobId: 'job-1' }])
  })

  it('shows the running job and then its result after leaving and coming back', async () => {
    mockBackend(handlers())
    await startFileJob()
    cleanup()
    await emitEvent('job://progress', { jobId: 'job-1', progress: 0.25 })
    renderPage()
    expect(await screen.findByText('interview.mp3')).toBeInTheDocument()
    expect(screen.getByText('Transcribing · 25%')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeInTheDocument()
    cleanup()
    await emitEvent('stt://done', {
      jobId: 'job-1',
      historyItem: historyFixture({ id: 'job-1', text: 'مرحبا بالعالم' }),
    })
    renderPage()
    const text = await screen.findByText('مرحبا بالعالم')
    expect(text).toHaveAttribute('dir', 'auto')
    expect(screen.getByText('interview.mp3')).toBeInTheDocument()
  })

  it('does not start a transcription while speech is being generated', async () => {
    const backend = mockBackend(handlers())
    const user = userEvent.setup()
    renderPage()
    await user.click(await screen.findByRole('radio', { name: 'File' }))
    await act(() => useJobs.getState().start('tts', () => Promise.resolve('job-tts')))
    expect(screen.getByRole('button', { name: 'Choose file' })).toBeDisabled()
    expect(screen.getByText('Wait for speech generation to finish.')).toBeInTheDocument()
    await user.click(screen.getByRole('radio', { name: 'Record' }))
    expect(screen.getByRole('button', { name: 'Start recording' })).toBeDisabled()
    expect(backend.count('transcribe_file')).toBe(0)
  })

  it('links to the speech-to-text models when none is installed', async () => {
    mockBackend(handlers({ list_installed_models: [] }))
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/transcribe']}>
        <Routes>
          <Route path="/transcribe" element={<TranscribePage />} />
          <Route path="/models" element={<LocationProbe />} />
        </Routes>
      </MemoryRouter>,
    )
    await user.click(await screen.findByRole('button', { name: 'Browse models' }))
    expect(screen.getByTestId('location')).toHaveTextContent('/models?kind=stt')
  })
})

function LocationProbe() {
  const location = useLocation()
  return <p data-testid="location">{location.pathname + location.search}</p>
}

describe('TranscribePage while recording', () => {
  it('locks the source switch', async () => {
    mockBackend(handlers({ start_recording: null, stop_recording: { tempAudioPath: 'cache/rec.wav', durationMs: 900 } }))
    const user = userEvent.setup()
    renderPage()
    await user.click(await screen.findByRole('button', { name: 'Start recording' }))
    await screen.findByRole('button', { name: 'Stop recording' })
    expect(screen.getByRole('radio', { name: 'File' })).toBeDisabled()
    await user.click(screen.getByRole('button', { name: 'Stop recording' }))
    expect(await screen.findByRole('radio', { name: 'File' })).toBeEnabled()
  })
})
