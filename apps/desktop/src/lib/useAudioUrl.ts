import { useEffect, useState } from 'react'
import { readAudioFile } from './api'

/** Load audio through Tauri's binary IPC and expose it as a webview-native Blob URL. */
export function useAudioUrl(path: string | null) {
  const [result, setResult] = useState<{ path: string | null; url: string | null; error: unknown }>({
    path,
    url: null,
    error: null,
  })

  useEffect(() => {
    if (!path) return

    let active = true
    let objectUrl: string | null = null

    readAudioFile({ path })
      .then((bytes) => {
        if (!active) return
        objectUrl = URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }))
        setResult({ path, url: objectUrl, error: null })
      })
      .catch((reason: unknown) => {
        if (active) setResult({ path, url: null, error: reason })
      })

    return () => {
      active = false
      if (objectUrl) URL.revokeObjectURL(objectUrl)
    }
  }, [path])

  return result.path === path ? result : { path, url: null, error: null }
}
