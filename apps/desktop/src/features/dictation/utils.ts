/** Normalise what the user typed into an executable name: "Slack" -> "slack.exe". */
export function toExe(name: string): string {
  const base = name.trim().toLowerCase().split(/[\\/]/).pop() ?? ''
  if (!base) return ''
  return base.endsWith('.exe') ? base : `${base}.exe`
}

/** The model to suggest when none is installed: Turbo on a GPU, small otherwise. */
export function recommendedModel(gpu: boolean, language: string): string {
  if (gpu) return 'whisper-large-v3-turbo-q5'
  return language === 'en' ? 'whisper-small-en-q5' : 'whisper-small-q5'
}
