import Link from 'next/link'
import { DownloadButton } from '@/components/DownloadButton'
import { Container } from '@/components/ui'
import { AppWindow } from '@/components/home/AppWindow'
import { FloatingChips } from '@/components/home/FloatingChips'
import { REPO_URL, VERSION } from '@/lib/releases'

export function Hero() {
  return (
    <section className="relative pt-32 md:pt-40">
      <div
        aria-hidden="true"
        className="pointer-events-none absolute inset-x-0 top-0 -z-10 h-[720px] bg-[radial-gradient(60%_50%_at_50%_0%,color-mix(in_oklab,var(--color-fg)_9%,transparent),transparent)]"
      />

      <Container>
        <div className="relative">
          <FloatingChips />

          <div className="relative mx-auto max-w-3xl text-center">
            <a
              href={`${REPO_URL}/releases`}
              target="_blank"
              rel="noreferrer"
              className="inline-flex animate-rise items-center gap-2 rounded-full border border-fg/10 bg-fg/[0.03] py-1 pl-1 pr-3.5 text-[13px] text-fg/60 transition-colors hover:border-fg/20 hover:text-fg motion-reduce:animate-none"
            >
              <span className="rounded-full bg-fg px-2 py-0.5 text-[11px] font-medium text-bg">v{VERSION}</span>
              Free and open source, forever
            </a>

            <h1 className="mt-7 animate-rise text-balance text-[2.9rem] font-semibold leading-[0.95] tracking-[-0.05em] [animation-delay:80ms] motion-reduce:animate-none sm:text-6xl md:text-[5.5rem]">
              Speech tools that
              <br />
              <span className="text-fg/40">never phone home.</span>
            </h1>

            <p className="mx-auto mt-7 max-w-xl animate-rise text-pretty text-lg leading-relaxed text-fg/55 [animation-delay:160ms] motion-reduce:animate-none md:text-xl">
              Turn text into speech and speech into text, right on your computer. Whisper, Kokoro and Piper run on your own
              GPU. No account, no API key, no monthly bill.
            </p>

            <div className="mt-10 flex animate-rise flex-col items-center justify-center gap-3 [animation-delay:240ms] motion-reduce:animate-none sm:flex-row">
              <DownloadButton />
              <a
                href={REPO_URL}
                target="_blank"
                rel="noreferrer"
                className="inline-flex h-12 items-center rounded-full border border-fg/15 px-6 text-[15px] text-fg/80 transition-colors hover:bg-fg/[0.05] hover:text-fg"
              >
                Read every line of it
              </a>
            </div>

            <p className="mt-5 text-sm text-fg/40">
              On another system?{' '}
              <Link href="/download" className="text-fg/65 underline decoration-fg/25 underline-offset-4 hover:text-fg">
                See every download
              </Link>
            </p>
          </div>
        </div>

        <div className="relative mx-auto mt-20 max-w-5xl animate-rise [animation-delay:320ms] motion-reduce:animate-none md:mt-24">
          <AppWindow />
        </div>
      </Container>
    </section>
  )
}
