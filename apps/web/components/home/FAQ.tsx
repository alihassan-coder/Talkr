import type { ReactNode } from 'react'
import { Container, Dim } from '@/components/ui'
import { ISSUES_URL } from '@/lib/releases'

function Code({ children }: { children: ReactNode }) {
  return <code className="rounded bg-fg/[0.07] px-1.5 py-0.5 font-mono text-[0.88em] text-fg/80">{children}</code>
}

const faqs: { q: string; a: ReactNode }[] = [
  {
    q: 'Is it really free?',
    a: (
      <>
        Yes. Talkr is MIT licensed and has no paid tier. The models are open source too: Whisper is MIT, Kokoro is
        Apache 2.0, and Piper voices are MIT with espeak-ng data under GPL 3.0.
      </>
    ),
  },
  {
    q: 'What does my computer need?',
    a: (
      <>
        Windows 10+, macOS 11+, or Linux with glibc 2.31+, and 4 GB of RAM for the smaller models. A GPU makes the big
        Whisper models much faster: Apple Silicon, an NVIDIA card with CUDA 12+, or any Vulkan 1.2 GPU.
      </>
    ),
  },
  {
    q: 'Does anything get uploaded?',
    a: (
      <>
        No. Audio and text are processed on your machine. Talkr only goes online when you download a model you picked,
        or check for updates, which is off by default.
      </>
    ),
  },
  {
    q: 'Can it translate?',
    a: (
      <>
        One way. Whisper can take speech in any language it supports and write it out in English. Turn on Translate
        before you transcribe.
      </>
    ),
  },
  {
    q: 'Where are my files?',
    a: (
      <>
        In <Code>~/.talkr</Code> on macOS and Linux, and <Code>{'C:\\Users\\You\\.talkr'}</Code> on Windows. Set{' '}
        <Code>TALKR_HOME</Code> to move it. Delete the folder and everything is gone.
      </>
    ),
  },
  {
    q: 'Can I use my own models?',
    a: (
      <>
        Yes. Use Import local model on the Models page. Speech to text takes a Whisper GGML <Code>.bin</Code> file.
        Text to speech takes a folder with <Code>model.onnx</Code>, <Code>tokens.txt</Code> and{' '}
        <Code>espeak-ng-data</Code>.
      </>
    ),
  },
]

export function FAQ() {
  return (
    <section id="faq" className="py-24 md:py-32">
      <Container>
        <div className="reveal grid gap-12 lg:grid-cols-[1fr_1.6fr] lg:gap-20">
          <div className="self-start lg:sticky lg:top-28">
            <p className="font-mono text-xs uppercase tracking-[0.18em] text-fg/45">FAQ</p>
            <h2 className="mt-5 text-4xl font-semibold leading-[1.02] tracking-[-0.04em] md:text-5xl">
              Questions.<br /> <Dim>Short answers.</Dim>
            </h2>
            <p className="mt-5 max-w-xs text-fg/55">
              Something missing?{' '}
              <a
                href={ISSUES_URL}
                target="_blank"
                rel="noreferrer"
                className="text-fg underline decoration-fg/30 underline-offset-4 transition-colors hover:decoration-fg"
              >
                Ask on GitHub
              </a>
            </p>
          </div>

          <div className="divide-y divide-fg/[0.08] border-y border-fg/[0.08]">
            {faqs.map((item, i) => (
              <details key={item.q} className="group" name="faq" open={i === 0}>
                <summary className="flex cursor-pointer list-none items-center justify-between gap-6 py-5 text-left text-[17px] font-medium tracking-[-0.015em] text-fg/85 transition-colors hover:text-fg [&::-webkit-details-marker]:hidden">
                  {item.q}
                  <span aria-hidden className="relative size-3.5 shrink-0">
                    <span className="absolute top-1/2 left-0 h-px w-full bg-fg/60" />
                    <span className="absolute top-1/2 left-0 h-px w-full rotate-90 bg-fg/60 transition-[transform,opacity] duration-300 ease-out-quint group-open:rotate-0 group-open:opacity-0" />
                  </span>
                </summary>
                <div className="pb-6 pr-10 text-[15px] leading-relaxed text-fg/55 motion-safe:animate-rise">{item.a}</div>
              </details>
            ))}
          </div>
        </div>
      </Container>
    </section>
  )
}
