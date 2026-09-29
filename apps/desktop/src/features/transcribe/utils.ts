export const AUDIO_EXTENSIONS = ['mp3', 'wav', 'flac', 'ogg', 'm4a']

export const isAudioPath = (path: string) => AUDIO_EXTENSIONS.includes(path.split('.').pop()?.toLowerCase() ?? '')

export const baseName = (path: string) => path.split(/[/\\]/).pop() || path

/** Segment timestamp: mm:ss, or h:mm:ss past an hour. */
export function formatTimestamp(ms: number) {
  const total = Math.max(0, Math.floor(ms / 1000))
  const h = Math.floor(total / 3600)
  const mm = String(Math.floor((total % 3600) / 60)).padStart(2, '0')
  const ss = String(total % 60).padStart(2, '0')
  return h > 0 ? `${h}:${mm}:${ss}` : `${mm}:${ss}`
}

export const LANGUAGES = [
  { code: 'auto', label: 'Auto-detect' },
  { code: 'en', label: 'English' },
  { code: 'es', label: 'Spanish' },
  { code: 'fr', label: 'French' },
  { code: 'de', label: 'German' },
  { code: 'it', label: 'Italian' },
  { code: 'pt', label: 'Portuguese' },
  { code: 'ru', label: 'Russian' },
  { code: 'ja', label: 'Japanese' },
  { code: 'ko', label: 'Korean' },
  { code: 'zh', label: 'Chinese' },
]

export const languageName = (code: string | null) =>
  code ? (LANGUAGES.find((l) => l.code === code)?.label ?? code.toUpperCase()) : null
