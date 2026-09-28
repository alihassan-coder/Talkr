import { EmptyState } from '../components/EmptyState'

export function SpeakPage() {
  return (
    <div className="max-w-3xl mx-auto space-y-6">
      <header className="space-y-2">
        <h1 className="text-4xl font-semibold tracking-tight text-text-primary">Speak</h1>
        <p className="text-text-secondary">Turn text into natural speech</p>
      </header>

      <EmptyState
        title="No voice models installed"
        description="Download a TTS model from the Models page to get started."
        actionLabel="Browse Models"
        actionHref="/models"
      />
    </div>
  )
}