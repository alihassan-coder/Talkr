import type { Metadata } from 'next'
import type { ReactNode } from 'react'
import { Navbar } from '@/components/Navbar'
import { Footer } from '@/components/Footer'
import { Card, Container, Dim } from '@/components/ui'
import { ISSUES_URL, REPO_URL } from '@/lib/releases'

export const metadata: Metadata = {
  title: 'Privacy policy',
  description:
    'Talkr runs on your computer and keeps your data there. No servers, no account, no telemetry. Here is what it stores, where, and how to remove it.',
  alternates: { canonical: '/privacy' },
}

const toc = [
  { id: 'commitment', title: 'Our commitment' },
  { id: 'processing', title: 'What Talkr handles' },
  { id: 'location', title: 'Where your data lives' },
  { id: 'rights', title: 'Your rights' },
  { id: 'network', title: 'Network connections' },
  { id: 'components', title: 'Third-party components' },
  { id: 'contact', title: 'Contact' },
]

const link = 'text-fg underline decoration-fg/30 underline-offset-4 transition-colors hover:decoration-fg'

function Code({ children }: { children: ReactNode }) {
  return <code className="rounded bg-fg/[0.06] px-1.5 py-0.5 font-mono text-[0.88em] text-fg">{children}</code>
}

function Section({ index, children }: { index: number; children: ReactNode }) {
  const entry = toc[index]
  if (!entry) return null
  const { id, title } = entry
  return (
    <section id={id} aria-labelledby={`${id}-title`} className="scroll-mt-24">
      <p aria-hidden className="font-mono text-xs text-fg/35">
        {String(index + 1).padStart(2, '0')}
      </p>
      <h2 id={`${id}-title`} className="mt-2 text-2xl font-semibold tracking-[-0.03em] md:text-3xl">
        {title}
      </h2>
      <div className="mt-4 space-y-4 text-[17px] leading-relaxed text-fg/65">{children}</div>
    </section>
  )
}

/** Hairline-divided definition list inside a Card. */
function Terms({ items }: { items: { term: string; detail: ReactNode }[] }) {
  return (
    <Card>
      <dl>
        {items.map((h) => (
          <div
            key={h.term}
            className="grid gap-1 border-b border-fg/[0.08] px-5 py-4 last:border-0 sm:grid-cols-[10rem_1fr] sm:gap-6 md:px-6"
          >
            <dt className="font-medium text-fg">{h.term}</dt>
            <dd>{h.detail}</dd>
          </div>
        ))}
      </dl>
    </Card>
  )
}

const handles = [
  {
    term: 'Processing',
    detail:
      'All audio and text is processed on your computer. Nothing is sent to us or to anyone else. Once a model is downloaded, Talkr works fully offline.',
  },
  {
    term: 'Storage',
    detail: (
      <>
        Models, history and settings live in <Code>~/.talkr</Code> on your machine. Delete them whenever you like. There
        is no cloud sync and no account.
      </>
    ),
  },
  {
    term: 'Telemetry',
    detail:
      'None. No analytics, no crash reports, no usage statistics. Talkr makes no network connections unless you ask it to download a model.',
  },
  {
    term: 'Source',
    detail: (
      <>
        The code is MIT licensed. You can audit it, build it yourself or contribute at{' '}
        <a href={REPO_URL} target="_blank" rel="noopener noreferrer" className={link}>
          github.com/alihassan-coder/Talkr
        </a>
        .
      </>
    ),
  },
  {
    term: 'Model licenses',
    detail:
      'Whisper (MIT), Kokoro (Apache 2.0), Piper (MIT, with espeak-ng data under GPL 3.0). Each license is shown in the app before you download the model.',
  },
]

const components = [
  {
    term: 'whisper.cpp',
    detail: (
      <>
        MIT. Speech recognition, from{' '}
        <a href="https://github.com/ggerganov/whisper.cpp" target="_blank" rel="noopener noreferrer" className={link}>
          ggerganov/whisper.cpp
        </a>
        .
      </>
    ),
  },
  {
    term: 'sherpa-onnx',
    detail: (
      <>
        Apache 2.0. Text-to-speech, from{' '}
        <a href="https://github.com/k2-fsa/sherpa-onnx" target="_blank" rel="noopener noreferrer" className={link}>
          k2-fsa/sherpa-onnx
        </a>
        .
      </>
    ),
  },
  { term: 'Tauri', detail: 'MIT or Apache 2.0. The desktop framework.' },
  { term: 'React and Rust', detail: 'The interface and the core runtime.' },
]

const locations = [
  { path: '~/.talkr/', where: 'Linux and macOS' },
  { path: 'C:\\Users\\You\\.talkr\\', where: 'Windows' },
  { path: '$TALKR_HOME', where: 'Custom location' },
]

const rights = [
  { term: 'Access.', detail: 'Everything is in ~/.talkr. Inspect it, copy it or move it at any time.' },
  {
    term: 'Deletion.',
    detail: 'Delete the folder and every trace is gone. Or use “Delete all history” in Settings.',
  },
  {
    term: 'Portability.',
    detail: 'Export any history item as TXT, SRT or WAV. Models are standard GGML and ONNX files.',
  },
  {
    term: 'No profiling.',
    detail: 'We do not build profiles, show ads or sell data. There is no account to delete.',
  },
]

export default function PrivacyPage() {
  return (
    <>
      <Navbar />
      <main className="pt-32 pb-24 md:pt-40 md:pb-32">
        <Container>
          <div className="mx-auto max-w-3xl">
            <div className="text-center">
              <p className="font-mono text-xs text-fg/45">Privacy policy · Updated 29 September 2026</p>
              <h1 className="mt-6 animate-rise text-balance text-5xl font-semibold leading-[0.95] tracking-[-0.045em] motion-reduce:animate-none md:text-7xl">
                Your voice, your data. <Dim>Your computer.</Dim>
              </h1>
              <p className="mx-auto mt-6 max-w-xl text-pretty text-lg leading-relaxed text-fg/55">
                Talkr has no servers, so this is a short one.
              </p>
            </div>
  
            <Card className="mt-12 p-4 md:p-5">
              <nav aria-label="On this page">
                <ul className="flex flex-wrap gap-2">
                  {toc.map((item) => (
                    <li key={item.id}>
                      <a
                        href={`#${item.id}`}
                        className="inline-block rounded-full border border-fg/10 px-3 py-1 text-sm text-fg/60 transition-colors duration-300 ease-out-quint hover:border-fg/20 hover:text-fg"
                      >
                        {item.title}
                      </a>
                    </li>
                  ))}
                </ul>
              </nav>
            </Card>
  
            <div className="mt-16 space-y-16">
              <Section index={0}>
                <p>
                  Talkr is built on one rule: your voice, your data, your computer. We do not collect, transmit or store
                  your personal data on any server. We don&apos;t run one.
                </p>
                <p>This page explains what Talkr handles, where it lives, and what you can do with it.</p>
              </Section>
  
              <Section index={1}>
                <Terms items={handles} />
              </Section>
  
              <Section index={2}>
                <p>Everything Talkr keeps sits in a single folder on your computer.</p>
                <div className="grid gap-3 sm:grid-cols-3">
                  {locations.map((l) => (
                    <Card key={l.path} className="p-5">
                      <p className="break-all font-mono text-sm text-fg">{l.path}</p>
                      <p className="mt-2 text-sm text-fg/50">{l.where}</p>
                    </Card>
                  ))}
                </div>
                <p>
                  Inside it you will find <Code>models/</Code> for downloaded models, <Code>audio/</Code> for generated
                  speech, <Code>history/</Code> for the SQLite database, <Code>cache/</Code> for temporary downloads and{' '}
                  <Code>logs/</Code> for app logs.
                </p>
              </Section>
  
              <Section index={3}>
                <Card>
                  <ul>
                    {rights.map((r) => (
                      <li key={r.term} className="border-b border-fg/[0.08] px-5 py-4 last:border-0 md:px-6">
                        <span className="font-medium text-fg">{r.term}</span> {r.detail}
                      </li>
                    ))}
                  </ul>
                </Card>
              </Section>
  
              <Section index={4}>
                <p>
                  Talkr only connects to the internet in two cases: when you click Download on a model, which fetches it
                  from Hugging Face or GitHub Releases, and when you check for updates, which is off by default.
                </p>
                <p>
                  There is no analytics, no crash reporting and nothing running in the background. Any network monitor
                  will show you the same.
                </p>
              </Section>
  
              <Section index={5}>
                <p>Talkr bundles a few open-source projects. All of them run locally.</p>
                <Terms items={components} />
              </Section>
  
              <Section index={6}>
                <p>
                  Questions about this policy? Open an issue on{' '}
                  <a href={ISSUES_URL} target="_blank" rel="noopener noreferrer" className={link}>
                    GitHub
                  </a>
                  .
                </p>
              </Section>
            </div>
          </div>
        </Container>
      </main>
      <Footer />
    </>
  )
}
