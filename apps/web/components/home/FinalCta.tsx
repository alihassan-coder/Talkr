import { LogoMark } from '@/components/Logo'
import { DownloadButton } from '@/components/DownloadButton'
import { Container } from '@/components/ui'
import { REPO_URL } from '@/lib/releases'

export function FinalCta() {
  return (
    <section className="pb-28 pt-12 md:pb-40">
      <Container className="text-center">
        <LogoMark className="mx-auto size-12" />
        <h2 className="mx-auto mt-8 max-w-2xl text-balance text-4xl font-semibold leading-[1.02] tracking-[-0.045em] md:text-6xl">
          Your voice stays yours. <span className="text-fg/40">Starting now.</span>
        </h2>
        <p className="mx-auto mt-5 max-w-md text-lg text-fg/55">Free for everyone. A 5 MB download. No sign-up screen, ever.</p>
        <div className="mt-10 flex flex-col items-center justify-center gap-3 sm:flex-row">
          <DownloadButton />
          <a
            href={REPO_URL}
            target="_blank"
            rel="noreferrer"
            className="inline-flex h-12 items-center rounded-full border border-fg/15 px-6 text-[15px] text-fg/80 transition-colors hover:bg-fg/[0.05] hover:text-fg"
          >
            Fork it on GitHub
          </a>
        </div>
      </Container>
    </section>
  )
}
