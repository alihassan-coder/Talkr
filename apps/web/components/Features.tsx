import { Shield, Zap, Database, Globe, History, User } from 'lucide-react'
import { Card } from '@talkr/ui/Card'

const features = [
  {
    icon: Shield,
    title: '100% Offline',
    description: 'Everything runs on your machine. Your voice, your text, your models — never leave your computer.',
  },
  {
    icon: Zap,
    title: 'GPU Accelerated',
    description: 'Automatically uses Metal (macOS), CUDA/Vulkan (Windows/Linux) for fast transcription. Falls back to CPU seamlessly.',
  },
  {
    icon: Database,
    title: 'Open-Source Models',
    description: 'Whisper for transcription, Kokoro & Piper for speech. All models are open-source and freely licensed.',
  },
  {
    icon: History,
    title: 'Unlimited History',
    description: 'Every generation and transcription saved forever. Full-text search, favorites, export to TXT/SRT/WAV.',
  },
  {
    icon: Globe,
    title: 'Speak & Transcribe',
    description: 'Two tools in one: Text-to-Speech with multiple voices, and Speech-to-Text from mic or audio files.',
  },
  {
    icon: User,
    title: 'No Account Required',
    description: 'Download, install, and use immediately. No sign-up, no login, no tracking, no telemetry.',
  },
]

export function Features() {
  return (
    <section id="features" className="py-24 px-6 md:py-32 md:px-12 lg:px-24 bg-app-canvas">
      <div className="max-w-7xl mx-auto">
        <header className="text-center mb-16">
          <h2 className="text-3xl md:text-4xl font-semibold tracking-tight text-text-primary mb-4">
            Built for Privacy & Performance
          </h2>
          <p className="text-lg text-text-secondary max-w-2xl mx-auto">
            Talkr combines state-of-the-art open models with a beautiful, native desktop experience.
          </p>
        </header>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {features.map((feature, index) => (
            <Card key={feature.title} className="group hover:shadow-[var(--shadow-soft)] transition-shadow duration-normal">
              <div className="w-12 h-12 rounded-[var(--radius-input)] bg-accent-soft flex items-center justify-center mb-4 group-hover:bg-accent/20 transition-colors">
                <feature.icon className="w-6 h-6 text-accent" strokeWidth={1.75} />
              </div>
              <h3 className="text-lg font-semibold text-text-primary mb-2">{feature.title}</h3>
              <p className="text-text-secondary">{feature.description}</p>
            </Card>
          ))}
        </div>
      </div>
    </section>
  )
}