import { Shield, Lock, Database, Eye } from 'lucide-react'
import { Card } from '@talkr/ui/Card'

const privacyPoints = [
  {
    icon: Lock,
    title: 'Your Data Never Leaves',
    description: 'All processing happens locally on your device. No audio, text, or usage data is ever sent to our servers or any third party.',
  },
  {
    icon: Database,
    title: 'Local Storage Only',
    description: 'Models, history, and settings live in ~/.talkr/ on your machine. You own your data completely — delete it anytime.',
  },
  {
    icon: Shield,
    title: 'No Telemetry',
    description: 'We don\'t collect analytics, crash reports, or usage statistics. Talkr has zero network connections unless you download a model.',
  },
  {
    icon: Eye,
    title: 'Open Source',
    description: 'The entire codebase is open source. You can audit every line, build it yourself, or contribute improvements.',
  },
]

export function PrivacySection() {
  return (
    <section id="privacy" className="py-24 px-6 md:py-32 md:px-12 lg:px-24">
      <div className="max-w-7xl mx-auto">
        <header className="text-center mb-16">
          <h2 className="text-3xl md:text-4xl font-semibold tracking-tight text-text-primary mb-4">
            Privacy by Default
          </h2>
          <p className="text-lg text-text-secondary max-w-2xl mx-auto">
            Talkr is designed from the ground up to respect your privacy. Here's what that means in practice:
          </p>
        </header>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6">
          {privacyPoints.map((point) => (
            <Card key={point.title} className="text-center">
              <div className="w-14 h-14 rounded-[var(--radius-input)] bg-accent-soft flex items-center justify-center mx-auto mb-4">
                <point.icon className="w-7 h-7 text-accent" strokeWidth={1.75} />
              </div>
              <h3 className="text-lg font-semibold text-text-primary mb-2">{point.title}</h3>
              <p className="text-text-secondary">{point.description}</p>
            </Card>
          ))}
        </div>

        <div className="mt-16 p-8 bg-app-canvas rounded-[var(--radius-card)] border border-border">
          <h3 className="text-xl font-semibold text-text-primary mb-4 text-center">Data Location</h3>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-6 text-center">
            <div>
              <code className="bg-white px-3 py-2 rounded-[var(--radius-input)] border border-border text-sm font-mono block mb-2">~/.talkr/</code>
              <p className="text-text-secondary text-sm">Linux / macOS</p>
            </div>
            <div>
              <code className="bg-white px-3 py-2 rounded-[var(--radius-input)] border border-border text-sm font-mono block mb-2">C:\Users\You\.talkr\</code>
              <p className="text-text-secondary text-sm">Windows</p>
            </div>
            <div>
              <code className="bg-white px-3 py-2 rounded-[var(--radius-input)] border border-border text-sm font-mono block mb-2">TALKR_HOME</code>
              <p className="text-text-secondary text-sm">Custom (env var)</p>
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}