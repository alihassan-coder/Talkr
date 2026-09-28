import { FileAudio, Mic, Globe, BadgeCheck, Volume2 } from 'lucide-react'
import { Card, Badge } from '@talkr/ui'
import catalog from '@talkr/model-catalog/catalog.json'

const models = catalog.models || []

const sttModels = models.filter((m: any) => m.kind === 'stt')
const ttsModels = models.filter((m: any) => m.kind === 'tts')

function ModelCard({ model }: { model: any }) {
  const isStt = model.kind === 'stt'
  return (
    <Card variant="outlined" className="flex flex-col h-full">
      <div className="flex items-start justify-between mb-3">
        <div className="flex items-center gap-2">
          <div className={`w-10 h-10 rounded-[var(--radius-input)] flex items-center justify-center ${
            isStt ? 'bg-accent-soft text-accent' : 'bg-success-soft text-success'
          }`}>
            {isStt ? <Mic className="w-5 h-5" strokeWidth={1.75} /> : <Volume2 className="w-5 h-5" strokeWidth={1.75} />}
          </div>
          <div>
            <h4 className="font-semibold text-text-primary">{model.name}</h4>
            <p className="text-sm text-text-muted">{model.languages.slice(0, 3).join(', ')}{model.languages.length > 3 ? '…' : ''}</p>
          </div>
        </div>
        {model.tags.includes('recommended') && (
          <Badge variant="success" dot size="sm">Recommended</Badge>
        )}
      </div>
      <p className="text-text-secondary text-sm mb-4 flex-1">{model.description}</p>
      <div className="flex flex-wrap gap-2 mb-4">
        <Badge variant="default" size="sm">{(model.sizeBytes / 1e6).toFixed(0)} MB</Badge>
        <Badge variant="default" size="sm">{model.license}</Badge>
      </div>
      <div className="pt-4 border-t border-border flex items-center justify-between">
        <a href={model.homepage} target="_blank" rel="noopener noreferrer" className="text-sm text-accent hover:underline flex items-center gap-1">
          <Globe className="w-4 h-4" />
          Details
        </a>
      </div>
    </Card>
  )
}

export function ModelsShowcase() {
  return (
    <section id="models" className="py-24 px-6 md:py-32 md:px-12 lg:px-24 bg-app-canvas">
      <div className="max-w-7xl mx-auto">
        <header className="text-center mb-16">
          <h2 className="text-3xl md:text-4xl font-semibold tracking-tight text-text-primary mb-4">
            Model Library
          </h2>
          <p className="text-lg text-text-secondary max-w-2xl mx-auto">
            Curated open-source models for transcription and speech synthesis. All downloadable with one click inside the app.
          </p>
        </header>

        <div className="mb-16">
          <h3 className="text-xl font-semibold text-text-primary mb-6 flex items-center gap-2">
            <Mic className="w-6 h-6 text-accent" strokeWidth={1.75} />
            Transcription (Speech → Text)
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            {sttModels.slice(0, 6).map((model: any) => (
              <ModelCard key={model.id} model={model} />
            ))}
          </div>
        </div>

        <div>
          <h3 className="text-xl font-semibold text-text-primary mb-6 flex items-center gap-2">
            <Volume2 className="w-6 h-6 text-success" strokeWidth={1.75} />
            Speech Synthesis (Text → Speech)
          </h3>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            {ttsModels.slice(0, 6).map((model: any) => (
              <ModelCard key={model.id} model={model} />
            ))}
          </div>
        </div>
      </div>
    </section>
  )
}