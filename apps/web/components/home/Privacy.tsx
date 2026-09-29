import Link from 'next/link'
import { Container } from '@/components/ui'

const zeros = [
  { value: '0', label: 'servers', note: 'Talkr has no backend to send anything to.' },
  { value: '0', label: 'accounts', note: 'Nothing to sign up for, nothing to leak.' },
  { value: '0', label: 'analytics', note: 'No tracking, no crash reporter, no pings.' },
]

export function Privacy() {
  return (
    <section id="privacy" className="py-24 md:py-32">
      <Container>
        <div className="reveal relative overflow-hidden rounded-3xl border border-fg/10 px-6 py-16 md:px-16 md:py-24">
          <div
            aria-hidden="true"
            className="pointer-events-none absolute inset-0 bg-[radial-gradient(50%_60%_at_50%_0%,color-mix(in_oklab,var(--color-fg)_6%,transparent),transparent)]"
          />
          <div className="relative mx-auto max-w-3xl text-center">
            <p className="font-mono text-xs uppercase tracking-[0.18em] text-fg/45">Privacy</p>
            <h2 className="mt-5 text-balance text-4xl font-semibold leading-[1.02] tracking-[-0.04em] md:text-6xl">
              We couldn&apos;t read your files <span className="text-fg/40">if we wanted to.</span>
            </h2>
            <p className="mx-auto mt-6 max-w-xl text-lg leading-relaxed text-fg/55">
              Audio, text and history live in <code className="font-mono text-[0.9em] text-fg">~/.talkr</code>. Talkr only
              goes online when you download a model you picked. Delete the folder and it&apos;s all gone.
            </p>
          </div>

          <dl className="relative mx-auto mt-14 grid max-w-4xl gap-px overflow-hidden rounded-2xl border border-fg/10 bg-fg/10 sm:grid-cols-3">
            {zeros.map((z) => (
              <div key={z.label} className="bg-bg px-6 py-7 text-center">
                <dt className="sr-only">{z.label}</dt>
                <dd>
                  <span className="block text-5xl font-semibold tracking-[-0.05em]">{z.value}</span>
                  <span className="mt-1 block font-mono text-xs uppercase tracking-[0.16em] text-fg/60">{z.label}</span>
                  <span className="mt-3 block text-sm text-fg/45">{z.note}</span>
                </dd>
              </div>
            ))}
          </dl>

          <p className="relative mt-10 text-center">
            <Link href="/privacy" className="text-sm text-fg/60 underline decoration-fg/25 underline-offset-4 hover:text-fg">
              Read the privacy policy
            </Link>
          </p>
        </div>
      </Container>
    </section>
  )
}
