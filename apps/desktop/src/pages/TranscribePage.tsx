import { EmptyState } from '../components/EmptyState'

export function TranscribePage() {
  return (
    <div className="max-w-3xl mx-auto space-y-6">
      <header className="space-y-2">
        <h1 className="text-4xl font-semibold tracking-tight text-text-primary">Transcribe</h1>
        <p className="text-text-secondary">Convert speech to text from microphone or audio files</p>
      </header>

      <EmptyState
        title="No transcription models installed"
        description="Download an STT model from the Models page to get started."
        actionLabel="Browse Models"
        actionHref="/models"
      />
    </div>
  )
}