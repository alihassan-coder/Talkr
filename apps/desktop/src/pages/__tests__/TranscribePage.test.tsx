import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'
import { TranscribePage } from '@/pages/TranscribePage'
import { emitEvent, historyFixture, installedFixture, mockBackend, reject, settingsFixture } from '@/test/tauri'

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
  await user.click(await screen.findByRole('tab', { name: 'File' }))
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
    await user.click(await screen.findByRole('tab', { name: 'File' }))
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
})
