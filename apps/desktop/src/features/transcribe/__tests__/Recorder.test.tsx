import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { Recorder } from '@/features/transcribe/Recorder'
import { emitEvent, flush, mockBackend, reject } from '@/test/tauri'

describe('Recorder', () => {
  it('releases the microphone when the screen is left while it is still opening', async () => {
    let opened!: () => void
    const backend = mockBackend({
      start_recording: () => new Promise<void>((r) => (opened = () => r())),
      stop_recording: { tempAudioPath: 'cache/rec.wav', durationMs: 10 },
    })
    const user = userEvent.setup()
    const { unmount } = render(<Recorder onRecorded={vi.fn()} />)
    await user.click(screen.getByRole('button', { name: 'Start recording' }))
    expect(screen.getByText('Opening microphone…')).toBeInTheDocument()
    unmount()
    expect(backend.count('stop_recording')).toBe(0)
    await act(async () => opened())
    await flush()
    expect(backend.count('stop_recording')).toBe(1)
  })

  it('keeps what was said when the screen is left mid-recording', async () => {
    const backend = mockBackend({ start_recording: null, stop_recording: { tempAudioPath: 'x.wav', durationMs: 1 } })
    const user = userEvent.setup()
    const onRecorded = vi.fn()
    const { unmount } = render(<Recorder onRecorded={onRecorded} />)
    await user.click(screen.getByRole('button', { name: 'Start recording' }))
    await screen.findByRole('button', { name: 'Stop recording' })
    unmount()
    await flush()
    expect(backend.count('stop_recording')).toBe(1)
    // Saved and handed on for transcription instead of thrown away.
    expect(onRecorded).toHaveBeenCalledWith('x.wav', 1)
  })

  it('tells the page while it records', async () => {
    mockBackend({ start_recording: null, stop_recording: { tempAudioPath: 'x.wav', durationMs: 1 } })
    const user = userEvent.setup()
    const onRecordingChange = vi.fn()
    render(<Recorder onRecorded={vi.fn()} onRecordingChange={onRecordingChange} />)
    await user.click(screen.getByRole('button', { name: 'Start recording' }))
    await screen.findByRole('button', { name: 'Stop recording' })
    expect(onRecordingChange).toHaveBeenLastCalledWith(true)
    await user.click(screen.getByRole('button', { name: 'Stop recording' }))
    await screen.findByRole('button', { name: 'Start recording' })
    expect(onRecordingChange).toHaveBeenLastCalledWith(false)
  })

  it('does not pretend to record when the microphone fails while opening', async () => {
    let opened!: () => void
    const backend = mockBackend({
      start_recording: () => new Promise<void>((r) => (opened = () => r())),
      stop_recording: reject('Not recording'),
    })
    const user = userEvent.setup()
    render(<Recorder onRecorded={vi.fn()} />)
    await flush()
    await user.click(screen.getByRole('button', { name: 'Start recording' }))
    await emitEvent('mic://error', { message: 'No microphone found.' })
    await act(async () => opened())
    await flush()
    expect(screen.getByRole('alert')).toHaveTextContent('No microphone found.')
    expect(screen.getByRole('button', { name: 'Start recording' })).toBeEnabled()
    expect(screen.queryByText(/Listening/)).not.toBeInTheDocument()
    expect(backend.count('stop_recording')).toBeGreaterThanOrEqual(1)
  })

  it('leaves the recording state and shows why when the microphone fails', async () => {
    const backend = mockBackend({ start_recording: null, stop_recording: reject('Not recording') })
    const user = userEvent.setup()
    const onRecorded = vi.fn()
    render(<Recorder onRecorded={onRecorded} />)
    await flush()
    await user.click(screen.getByRole('button', { name: 'Start recording' }))
    await screen.findByRole('button', { name: 'Stop recording' })

    await emitEvent('mic://error', { message: 'The microphone was disconnected.' })
    expect(screen.getByRole('alert')).toHaveTextContent('The microphone was disconnected.')
    expect(screen.getByRole('button', { name: 'Start recording' })).toBeEnabled()
    await flush()
    expect(backend.count('stop_recording')).toBe(1)
    expect(onRecorded).not.toHaveBeenCalled()

    // Starting again clears the message.
    await user.click(screen.getByRole('button', { name: 'Start recording' }))
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  it('shows a microphone that cannot be opened inline', async () => {
    mockBackend({ start_recording: reject('No microphone found.') })
    const user = userEvent.setup()
    render(<Recorder onRecorded={vi.fn()} />)
    await user.click(screen.getByRole('button', { name: 'Start recording' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('No microphone found.')
    expect(screen.getByRole('button', { name: 'Start recording' })).toBeEnabled()
  })

  it('says why it is disabled', () => {
    render(<Recorder onRecorded={vi.fn()} disabled disabledReason="Wait for speech generation to finish." />)
    expect(screen.getByRole('button', { name: 'Start recording' })).toBeDisabled()
    expect(screen.getByText('Wait for speech generation to finish.')).toBeInTheDocument()
  })
})
