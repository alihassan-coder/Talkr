import { Container } from '@/components/ui'
import { ISSUES_URL, REPO_URL } from '@/lib/releases'
import { CopyButton } from './CopyButton'

type Line = { kind: 'command' | 'comment' | 'output'; text: string }

const lines: Line[] = [
  { kind: 'comment', text: '# grab the source' },
  { kind: 'command', text: `git clone ${REPO_URL}` },
  { kind: 'command', text: 'cd talkr && pnpm install' },
  { kind: 'comment', text: '# run the desktop app with hot reload' },
  { kind: 'command', text: 'pnpm dev:desktop' },
  { kind: 'output', text: '✓ Rust core compiled · GPU backend: Metal' },
  { kind: 'output', text: '✓ Talkr is running. No servers were contacted.' },
  { kind: 'comment', text: '# or just this website' },
  { kind: 'command', text: 'pnpm dev:web' },
]

const commands = lines
  .filter((l) => l.kind === 'command')
  .map((l) => l.text)
  .join('\n')

const facts = [
  { value: 'MIT', label: 'License' },
  { value: 'Rust', label: 'Core' },
  { value: 'Tauri 2', label: 'Shell' },
]

export function OpenSource() {
  return (
    <section id="source" className="py-24 md:py-32">
      <Container>
        <div className="reveal grid items-center gap-12 lg:grid-cols-[1fr_1.15fr] lg:gap-16">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.18em] text-fg/45">Open source</p>
            <h2 className="mt-5 text-balance text-4xl font-semibold leading-[1.02] tracking-[-0.04em] md:text-[3.4rem]">
              Don&apos;t trust us. <span className="text-fg/40">Build it yourself.</span>
            </h2>
            <p className="mt-5 max-w-md text-pretty text-lg leading-relaxed text-fg/55">
              The whole app is on GitHub under the MIT license: the Rust core, the interface, this website. Clone it,
              read it, change it, ship your own.
            </p>

            <ul className="mt-8 grid grid-cols-3 gap-px overflow-hidden rounded-xl border border-fg/10 bg-fg/10">
              {facts.map((f) => (
                <li key={f.label} className="bg-bg px-4 py-4">
                  <p className="text-xl font-semibold tracking-[-0.03em]">{f.value}</p>
                  <p className="mt-1 font-mono text-[11px] uppercase tracking-[0.14em] text-fg/45">{f.label}</p>
                </li>
              ))}
            </ul>

            <div className="mt-8 flex flex-wrap items-center gap-3">
              <a
                href={REPO_URL}
                target="_blank"
                rel="noreferrer"
                className="inline-flex h-11 items-center rounded-full bg-fg px-5 text-sm font-medium text-bg transition-transform duration-200 ease-out-quint active:scale-[0.97]"
              >
                View on GitHub
              </a>
              <a
                href={ISSUES_URL}
                target="_blank"
                rel="noreferrer"
                className="inline-flex h-11 items-center rounded-full border border-fg/15 px-5 text-sm font-medium text-fg/80 transition-[background-color,transform] duration-200 ease-out-quint hover:bg-fg/[0.05] active:scale-[0.97]"
              >
                Report an issue
              </a>
            </div>
          </div>

          <figure className="min-w-0">
            <figcaption className="sr-only">Commands to build Talkr from source</figcaption>
            <div className="overflow-hidden rounded-2xl border border-fg/10 bg-bg shadow-[0_0_0_1px_color-mix(in_oklab,var(--color-bg)_80%,transparent),0_40px_120px_-20px_color-mix(in_oklab,var(--color-fg)_9%,transparent)]">
              <div className="flex h-10 items-center border-b border-fg/[0.08] bg-fg/[0.02] px-4">
                <div aria-hidden="true" className="flex flex-1 items-center gap-2">
                  <span className="size-2.5 rounded-full bg-fg/15" />
                  <span className="size-2.5 rounded-full bg-fg/15" />
                  <span className="size-2.5 rounded-full bg-fg/15" />
                </div>
                <span className="text-xs text-fg/40">~/code</span>
                <div className="flex flex-1 justify-end">
                  <CopyButton text={commands} />
                </div>
              </div>

              <pre className="overflow-x-auto p-5 font-mono text-[13px] leading-[1.9] md:p-6">
                {lines.map((line, i) => {
                  const last = i === lines.length - 1
                  return (
                    <div key={i}>
                      {line.kind === 'command' ? (
                        <>
                          <span className="select-none text-fg/30">$ </span>
                          <span className="text-fg/90">{line.text}</span>
                        </>
                      ) : (
                        <span className={line.kind === 'comment' ? 'text-fg/35' : 'text-fg/45'}>{line.text}</span>
                      )}
                      {last ? (
                        <span
                          aria-hidden="true"
                          className="ml-1 inline-block h-[1.1em] w-2 animate-caret bg-fg/80 align-middle motion-reduce:animate-none"
                        />
                      ) : null}
                    </div>
                  )
                })}
              </pre>
            </div>
          </figure>
        </div>
      </Container>
    </section>
  )
}
