import { useEffect, useState } from 'react'
import { readHistoryAudio } from './api'

const MIME_TYPES: Record<string, string> = {
  wav: 'audio/wav',
  mp3: 'audio/mpeg',
  m4a: 'audio/mp4',
  mp4: 'audio/mp4',
  aac: 'audio/mp4',
  flac: 'audio/flac',
  ogg: 'audio/ogg',
  oga: 'audio/ogg',
  opus: 'audio/ogg',
  webm: 'audio/webm',
}

/** MIME type for an audio path, from its extension; '' when unknown (the webview then sniffs it). */
export function audioMimeType(path: string | null): string {
  const ext = path?.match(/\.([a-z0-9]+)$/i)?.[1]?.toLowerCase()
  return (ext && MIME_TYPES[ext]) || ''
}

/**
 * Load a history item's audio through Tauri's binary IPC and expose it as a webview-native Blob URL.
 * `audioPath` is only used to pick the MIME type; the backend resolves the file from the id.
 */
export function useAudioUrl(id: string | null, audioPath: string | null) {
  const key = id ? `${id}\n${audioPath ?? ''}` : null
  const [result, setResult] = useState<{ key: string | null; url: string | null; error: unknown }>({
    key,
    url: null,
    error: null,
  })

  useEffect(() => {
    if (!id) return

    let active = true
    let objectUrl: string | null = null
    const type = audioMimeType(audioPath)

    readHistoryAudio({ id })
      .then((bytes) => {
        if (!active) return
        objectUrl = URL.createObjectURL(new Blob([bytes], type ? { type } : undefined))
        setResult({ key, url: objectUrl, error: null })
      })
      .catch((reason: unknown) => {
        if (active) setResult({ key, url: null, error: reason })
      })

    return () => {
      active = false
      if (objectUrl) URL.revokeObjectURL(objectUrl)
    }
  }, [id, audioPath, key])

  return result.key === key ? result : { key, url: null, error: null }
}
