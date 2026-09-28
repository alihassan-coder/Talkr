import { Download, Brain, Mic, Volume2 } from 'lucide-react'

const steps = [
  {
    number: '01',
    icon: Download,
    title: 'Download Talkr',
    description: 'Get the installer for Windows, macOS, or Linux. No account needed — just download and run.',
  },
  {
    number: '02',
    icon: Brain,
    title: 'Pick a Model',
    description: 'Browse the built-in model library. One-click download Whisper for transcription or Kokoro/Piper for speech.',
  },
  {
    number: '03',
    icon: Mic,
    title: 'Speak or Transcribe',
    description: 'Type text and generate natural audio, or record from your microphone and get instant transcripts.',
  },
]

export function HowItWorks() {
  return (
    <section className="py-24 px-6 md:py-32 md:px-12 lg:px-24">
      <div className="max-w-7xl mx-auto">
        <header className="text-center mb-16">
          <h2 className="text-3xl md:text-4xl font-semibold tracking-tight text-text-primary mb-4">
            How It Works
          </h2>
          <p className="text-lg text-text-secondary max-w-2xl mx-auto">
            Three simple steps to private, offline voice tools.
          </p>
        </header>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-8">
          {steps.map((step, index) => (
            <div key={step.number} className="relative">
              <div className="absolute left-1/2 -translate-x-1/2 top-0 w-16 h-16 rounded-full bg-accent-soft flex items-center justify-center text-2xl font-bold text-accent mb-6">
                {step.number}
              </div>
              <div className="text-center pt-8">
                <div className="w-14 h-14 rounded-[var(--radius-input)] bg-accent-soft flex items-center justify-center mx-auto mb-4">
                  <step.icon className="w-7 h-7 text-accent" strokeWidth={1.75} />
                </div>
                <h3 className="text-xl font-semibold text-text-primary mb-2">{step.title}</h3>
                <p className="text-text-secondary">{step.description}</p>
              </div>
              {index < steps.length - 1 && (
                <div className="hidden md:block absolute top-8 left-[calc(50%+8px)] w-full h-0.5 bg-gradient-to-r from-accent-soft to-transparent" />
              )}
            </div>
          ))}
        </div>
      </div>
    </section>
  )
}