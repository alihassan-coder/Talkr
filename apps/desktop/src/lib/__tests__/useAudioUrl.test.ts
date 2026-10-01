import { renderHook, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { audioMimeType, useAudioUrl } from '@/lib/useAudioUrl'
import { mockBackend, reject } from '@/test/tauri'

describe('audioMimeType', () => {
  it.each([
    ['audio/a.wav', 'audio/wav'],
    ['C:\\music\\song.MP3', 'audio/mpeg'],
    ['a.m4a', 'audio/mp4'],
    ['a.mp4', 'audio/mp4'],
    ['a.aac', 'audio/mp4'],
    ['a.flac', 'audio/flac'],
    ['a.ogg', 'audio/ogg'],
    ['a.oga', 'audio/ogg'],
    ['a.opus', 'audio/ogg'],
    ['a.webm', 'audio/webm'],
    ['a.xyz', ''],
    ['no-extension', ''],
    [null, ''],
  ])('%s -> %s', (path, mime) => {
    expect(audioMimeType(path)).toBe(mime)
  })
})

describe('useAudioUrl', () => {
  it('reads the audio by history id and labels the blob with the type from the path', async () => {
    const backend = mockBackend({ read_history_audio: new ArrayBuffer(8) })
    const { result } = renderHook(() => useAudioUrl('job-1', 'C:\\audio\\talk.flac'))
    await waitFor(() => expect(result.current.url).toBe('blob:talkr-audio'))
    expect(backend.argsOf('read_history_audio')).toEqual([{ id: 'job-1' }])
    const blob = vi.mocked(URL.createObjectURL).mock.calls[0]![0] as Blob
    expect(blob.type).toBe('audio/flac')
  })

  it('leaves the type out when the extension is unknown', async () => {
    mockBackend({ read_history_audio: new ArrayBuffer(8) })
    const { result } = renderHook(() => useAudioUrl('job-2', 'audio/clip'))
    await waitFor(() => expect(result.current.url).not.toBeNull())
    expect((vi.mocked(URL.createObjectURL).mock.calls[0]![0] as Blob).type).toBe('')
  })

  it('reports a read error', async () => {
    mockBackend({ read_history_audio: reject('missing') })
    const { result } = renderHook(() => useAudioUrl('job-3', 'a.wav'))
    await waitFor(() => expect(result.current.error).toBe('missing'))
    expect(result.current.url).toBeNull()
  })
})
