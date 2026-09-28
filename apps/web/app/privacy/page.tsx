import { Metadata } from 'next'
import Link from 'next/link'
import { Shield, Lock, Database, Eye, Download } from 'lucide-react'

export const metadata: Metadata = {
  title: 'Privacy Policy - Talkr',
  description: 'Talkr privacy policy: your data stays on your device. No telemetry, no cloud, no tracking.',
}

const privacyPoints = [
  {
    icon: Lock,
    title: 'Your Data Never Leaves',
    description: 'All processing happens locally on your device. No audio, text, or usage data is ever sent to our servers or any third party. Talkr works 100% offline after initial model download.',
  },
  {
    icon: Database,
    title: 'Local Storage Only',
    description: 'Models, history, and settings live in ~/.talkr/ on your machine. You own your data completely — delete it anytime. No cloud sync, no account required.',
  },
  {
    icon: Shield,
    title: 'No Telemetry',
    description: 'We don\'t collect analytics, crash reports, or usage statistics. Talkr has zero network connections unless you explicitly download a model from the built-in catalog.',
  },
  {
    icon: Eye,
    title: 'Open Source',
    description: 'The entire codebase is open source under MIT license. You can audit every line, build it yourself, or contribute improvements at github.com/talkr/talkr.',
  },
  {
    icon: Download,
    title: 'Model Licenses',
    description: 'All models are open source: Whisper (MIT), Kokoro (Apache-2.0), Piper (MIT + GPL espeak-ng-data). Licenses are displayed in-app before download.',
  },
]

export default function PrivacyPage() {
  return (
    <>
      <header className="border-b border-border bg-white">
        <div className="max-w-7xl mx-auto px-6 py-4 flex items-center justify-between">
          <Link href="/" className="flex items-center gap-2 text-text-primary">
            <Download className="w-8 h-8 text-accent" strokeWidth={2} />
            <span className="text-xl font-semibold tracking-tight">Talkr</span>
          </Link>
          <nav className="hidden md:flex items-center gap-8">
            <Link href="/" className="text-sm font-medium text-text-secondary hover:text-text-primary">Home</Link>
            <Link href="/privacy" className="text-sm font-medium text-accent">Privacy</Link>
          </nav>
        </div>
      </header>

      <main className="max-w-3xl mx-auto px-6 py-16 md:py-24 lg:px-12">
        <header className="mb-16">
          <h1 className="text-4xl md:text-5xl font-semibold tracking-tight text-text-primary mb-4">
            Privacy Policy
          </h1>
          <p className="text-lg text-text-secondary">
            Last updated: {new Date().toLocaleDateString('en-US', { year: 'numeric', month: 'long', day: 'numeric' })}
          </p>
        </header>

        <section className="mb-16">
          <h2 className="text-2xl font-semibold text-text-primary mb-6">Our Commitment</h2>
          <p className="text-text-secondary mb-4">
            Talkr is built on a simple principle: <strong className="text-text-primary">your voice, your data, your computer.</strong> We don't collect, transmit, or store any of your personal data on our servers. Ever.
          </p>
          <p className="text-text-secondary">
            This policy explains what data Talkr handles, where it lives, and your rights over it.
          </p>
        </section>

        <section className="mb-16">
          <h2 className="text-2xl font-semibold text-text-primary mb-6">What Data Talkr Processes</h2>
          <div className="space-y-6">
            {privacyPoints.map((point) => (
              <article key={point.title} className="flex gap-4 p-6 bg-app-canvas rounded-[var(--radius-card)] border border-border">
                <div className="w-12 h-12 rounded-[var(--radius-input)] bg-accent-soft flex items-center justify-center flex-shrink-0">
                  <point.icon className="w-6 h-6 text-accent" strokeWidth={1.75} />
                </div>
                <div>
                  <h3 className="font-semibold text-text-primary mb-1">{point.title}</h3>
                  <p className="text-text-secondary">{point.description}</p>
                </div>
              </article>
            ))}
          </div>
        </section>

        <section className="mb-16">
          <h2 className="text-2xl font-semibold text-text-primary mb-6">Data Location</h2>
          <p className="text-text-secondary mb-4">
            All Talkr data is stored in a single folder on your computer:
          </p>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-6">
            <div className="bg-card border border-border rounded-[var(--radius-card)] p-4">
              <code className="font-mono text-sm text-text-primary block mb-1">~/.talkr/</code>
              <span className="text-sm text-text-muted">Linux / macOS</span>
            </div>
            <div className="bg-card border border-border rounded-[var(--radius-card)] p-4">
              <code className="font-mono text-sm text-text-primary block mb-1">C:\Users\You\.talkr\</code>
              <span className="text-sm text-text-muted">Windows</span>
            </div>
            <div className="bg-card border border-border rounded-[var(--radius-card)] p-4">
              <code className="font-mono text-sm text-text-primary block mb-1">$TALKR_HOME</code>
              <span className="text-sm text-text-muted">Custom (optional)</span>
            </div>
          </div>
          <p className="text-text-secondary">
            Inside this folder you'll find: <code className="font-mono bg-app-canvas px-1.5 py-0.5 rounded">models/</code> (downloaded models), <code className="font-mono bg-app-canvas px-1.5 py-0.5 rounded">audio/</code> (generated speech), <code className="font-mono bg-app-canvas px-1.5 py-0.5 rounded">history/</code> (SQLite database), <code className="font-mono bg-app-canvas px-1.5 py-0.5 rounded">cache/</code> (temporary downloads), and <code className="font-mono bg-app-canvas px-1.5 py-0.5 rounded">logs/</code> (app logs).
          </p>
        </section>

        <section className="mb-16">
          <h2 className="text-2xl font-semibold text-text-primary mb-6">Your Rights</h2>
          <ul className="space-y-4 text-text-secondary">
            <li><strong>Access:</strong> All your data is in ~/.talkr/ — you can inspect, copy, or move it anytime.</li>
            <li><strong>Deletion:</strong> Delete the ~/.talkr/ folder to remove all data. Or use "Delete all history" in Settings.</li>
            <li><strong>Portability:</strong> Export any history item as TXT, SRT, or WAV. Models are standard GGML/ONNX files.</li>
            <li><strong>No profiling:</strong> We don't build profiles, show ads, or sell data. There's no account to delete.</li>
          </ul>
        </section>

        <section className="mb-16">
          <h2 className="text-2xl font-semibold text-text-primary mb-6">Network Connections</h2>
          <p className="text-text-secondary mb-4">
            Talkr only makes network connections when you:
          </p>
          <ul className="space-y-2 text-text-secondary pl-6 list-disc">
            <li>Click "Download" on a model in the Models page (connects to Hugging Face or GitHub Releases)</li>
            <li>Optionally check for updates (disabled by default)</li>
          </ul>
          <p className="text-text-secondary mt-4">
            No analytics, crash reporting, or background connections. You can verify this with any network monitor.
          </p>
        </section>

        <section className="mb-16">
          <h2 className="text-2xl font-semibold text-text-primary mb-6">Third-Party Components</h2>
          <p className="text-text-secondary mb-4">
            Talkr bundles these open-source libraries (all run locally):
          </p>
          <ul className="space-y-2 text-text-secondary pl-6 list-disc">
            <li><strong>whisper.cpp</strong> (MIT) — Speech recognition via <a href="https://github.com/ggerganov/whisper.cpp" target="_blank" rel="noopener noreferrer" className="text-accent hover:underline">ggerganov/whisper.cpp</a></li>
            <li><strong>sherpa-onnx</strong> (Apache-2.0) — TTS via <a href="https://github.com/k2-fsa/sherpa-onnx" target="_blank" rel="noopener noreferrer" className="text-accent hover:underline">k2-fsa/sherpa-onnx</a></li>
            <li><strong>Tauri</strong> (MIT/Apache-2.0) — Desktop framework</li>
            <li><strong>Rust std</strong> / <strong>Node.js</strong> / <strong>React</strong> — Core runtime</li>
          </ul>
        </section>

        <section>
          <h2 className="text-2xl font-semibold text-text-primary mb-6">Contact</h2>
          <p className="text-text-secondary">
            Questions about this policy? Open an issue on <a href="https://github.com/talkr/talkr" target="_blank" rel="noopener noreferrer" className="text-accent hover:underline">GitHub</a> or email <a href="mailto:privacy@talkr.app" className="text-accent hover:underline">privacy@talkr.app</a>.
          </p>
        </section>
      </main>

      <footer className="border-t border-border bg-white">
        <div className="max-w-7xl mx-auto px-6 py-8 text-center text-sm text-text-muted">
          © {new Date().getFullYear()} Talkr. Open source under MIT license.
        </div>
      </footer>
    </>
  )
}