import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { ExportMenu } from '../ExportMenu'
import { formatsFor } from '../formats'
import { historyFixture, mockBackend } from '@/test/tauri'
import { useUi } from '@/stores/ui'
import { useToasts } from '@/stores/toast'
import { makeWav } from '@/test/wav'

const segments = JSON.stringify([{ startMs: 0, endMs: 1000, text: 'Hi' }])
const speech = historyFixture({ id: 'tts-1', kind: 'tts', audioPath: 'audio/2026/10/tts-1.wav', title: 'Hello' })
const transcript = historyFixture({ id: 'stt-1', kind: 'stt', segmentsJson: segments })
const fileTranscript = historyFixture({ id: 'stt-2', kind: 'stt', audioPath: 'D:\\Music\\talk.mp3' })

const labels = (items: { label: string }[]) => items.map((i) => i.label)

describe('formatsFor', () => {
  it('offers audio formats only for WAV audio, and timed formats only with segments', () => {
    expect(labels(formatsFor(speech).audio)).toEqual(['MP3', 'WAV', 'FLAC'])
    expect(labels(formatsFor(speech).text)).toEqual(['Plain text', 'Markdown', 'JSON'])
    expect(formatsFor(transcript).audio).toEqual([])
    expect(labels(formatsFor(transcript).text)).toEqual(['Plain text', 'Markdown', 'Subtitles', 'Web subtitles', 'Spreadsheet', 'JSON'])
    expect(formatsFor(fileTranscript).audio).toEqual([])
  })
})

describe('ExportMenu', () => {
  it('saves in the remembered format with one click', async () => {
    const backend = mockBackend({ history_export: 'C:\\out\\Hello.wav' })
    const user = userEvent.setup()
    render(<ExportMenu item={speech} />)
    await user.click(screen.getByRole('button', { name: 'Save WAV' }))
    await waitFor(() => expect(backend.argsOf('history_export')).toEqual([{ id: 'tts-1', format: 'wav' }]))
    await waitFor(() => expect(useToasts.getState().toasts.at(-1)?.message).toBe('Saved as WAV'))
  })

  it('opens every format with the keyboard and remembers the choice', async () => {
    const backend = mockBackend({ history_export: 'C:\\out\\Hello.flac' })
    const user = userEvent.setup()
    render(<ExportMenu item={speech} />)
    const more = screen.getByRole('button', { name: 'More formats' })
    more.focus()
    await user.keyboard('{ArrowDown}')
    const menu = screen.getByRole('menu', { name: 'Save as' })
    expect(menu).toBeInTheDocument()
    expect(screen.getAllByRole('menuitem').map((m) => m.textContent)).toEqual([
      'MP3Small, plays everywhere',
      'WAVOriginal, uncompressed',
      'FLACLossless, about half the size',
      'Plain text.txt',
      'Markdown.md, with timestamps',
      'JSONText, timings and details',
    ])
    expect(screen.getAllByRole('menuitem')[0]).toHaveFocus()
    await user.keyboard('{ArrowDown}{ArrowDown}{Enter}')
    await waitFor(() => expect(backend.argsOf('history_export')).toEqual([{ id: 'tts-1', format: 'flac' }]))
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
    expect(useUi.getState().audioFormat).toBe('flac')
    expect(await screen.findByRole('button', { name: 'Save FLAC' })).toBeInTheDocument()
  })

  it('closes on Escape and gives focus back', async () => {
    mockBackend({})
    const user = userEvent.setup()
    render(<ExportMenu item={transcript} prefer="text" />)
    await user.click(screen.getByRole('button', { name: 'More formats' }))
    expect(screen.getByRole('menu')).toBeInTheDocument()
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'More formats' })).toHaveFocus()
  })

  it('encodes MP3 in the app and hands the bytes to the backend', async () => {
    const wav = makeWav([Array.from({ length: 24_000 }, (_, i) => 0.3 * Math.sin(i / 10))])
    const backend = mockBackend({ read_history_audio: wav, save_export_bytes: 'C:\\out\\Hello.mp3' })
    useUi.setState({ audioFormat: 'mp3' })
    const user = userEvent.setup()
    render(<ExportMenu item={speech} />)
    await user.click(screen.getByRole('button', { name: 'Save MP3' }))
    await waitFor(() => expect(backend.count('save_export_bytes')).toBe(1))
    const body = backend.argsOf('save_export_bytes')[0] as unknown as Uint8Array
    expect(body).toBeInstanceOf(Uint8Array)
    expect(body[0]).toBe(0xff)
    expect(backend.argsOf('read_history_audio')).toEqual([{ id: 'tts-1' }])
    await waitFor(() => expect(useToasts.getState().toasts.at(-1)?.message).toBe('Saved as MP3'))
  })

  it('reports a failed save and stays usable', async () => {
    mockBackend({ history_export: () => Promise.reject('Disk is full') })
    const user = userEvent.setup()
    render(<ExportMenu item={transcript} prefer="text" />)
    await user.click(screen.getByRole('button', { name: 'Save Plain text' }))
    await waitFor(() => expect(useToasts.getState().toasts.at(-1)?.message).toContain('Disk is full'))
    expect(screen.getByRole('button', { name: 'Save Plain text' })).toBeEnabled()
  })
})
