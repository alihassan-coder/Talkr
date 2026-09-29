import type { Metadata } from 'next'
import { Navbar } from '@/components/Navbar'
import { Footer } from '@/components/Footer'
import { DownloadButton } from '@/components/DownloadButton'
import { Card, Container, Dim, SectionIntro } from '@/components/ui'
import { RELEASES_URL, VERSION, downloadUrl, platforms } from '@/lib/releases'

export const metadata: Metadata = {
  title: 'Download',
  description:
    'Download Talkr for Windows, macOS or Linux. Free, open-source, offline text-to-speech and speech-to-text. Checksums and signatures on every release.',
}

const checks = [
  { os: 'Windows', command: 'Get-FileHash .\\Talkr_0.1.0_x64-setup.exe -Algorithm SHA256' },
  { os: 'macOS', command: 'shasum -a 256 Talkr_0.1.0_aarch64.dmg' },
  { os: 'Linux', command: 'sha256sum Talkr_0.1.0_amd64.AppImage' },
]

const requirements = [
  { term: 'Windows', detail: '10 or later, x64' },
  { term: 'macOS', detail: '11 Big Sur or later. Apple Silicon for GPU acceleration' },
  { term: 'Linux', detail: 'glibc 2.31 or later. NVIDIA (CUDA 12+) or any Vulkan 1.2 GPU for acceleration' },
  { term: 'Memory', detail: '4 GB for the small models, 8 GB or more for Whisper Medium and Large' },
  { term: 'Disk', detail: 'About 5 to 15 MB for the app, plus 50 MB to 1.6 GB per model' },
]

export default function DownloadPage() {
  return (
    <>
      <Navbar />
      <main className="pt-32 pb-24 md:pt-40 md:pb-32">
        <Container>
          <div className="mx-auto max-w-3xl text-center">
            <p className="font-mono text-xs text-fg/45">{`v${VERSION} · free · MIT`}</p>
            <h1 className="mt-6 animate-rise text-balance text-5xl font-semibold leading-[0.95] tracking-[-0.045em] motion-reduce:animate-none md:text-7xl">
              Download Talkr. <Dim>Keep it forever.</Dim>
            </h1>
            <p className="mx-auto mt-6 max-w-xl text-pretty text-lg leading-relaxed text-fg/55">
              Pick your system. Every file is also on the GitHub release page, next to its checksum.
            </p>
            <div className="mt-8">
              <DownloadButton />
            </div>
          </div>

          <div className="mt-20 grid gap-4 md:grid-cols-2">
            {platforms.map((p) => (
              <section key={p.id} id={p.id} aria-labelledby={`${p.id}-title`} className="scroll-mt-24">
                <Card className="h-full p-6 transition-colors duration-300 ease-out-quint hover:border-fg/20 hover:bg-fg/[0.04] md:p-7">
                  <h2 id={`${p.id}-title`} className="text-xl font-semibold tracking-[-0.02em]">
                    {p.name} <span className="font-normal text-fg/45">{p.detail}</span>
                  </h2>
                  <p className="mt-1 font-mono text-xs text-fg/45">{p.requirement}</p>
                  <ul className="mt-6 divide-y divide-fg/[0.08] border-t border-fg/[0.08]">
                    {p.files.map((f) => (
                      <li key={f.name} className="flex items-center justify-between gap-4 py-4">
                        <div className="min-w-0">
                          <p className="break-all font-mono text-sm">{f.name}</p>
                          <p className="mt-1 text-xs text-fg/50">{`${f.format} · ${f.arch} · ${f.size}`}</p>
                        </div>
                        <a
                          href={downloadUrl(f.name)}
                          aria-label={`Download ${f.name}`}
                          className="inline-flex h-9 shrink-0 items-center rounded-full border border-fg/15 px-4 text-sm transition-colors duration-300 ease-out-quint hover:bg-fg/[0.06]"
                        >
                          Download
                        </a>
                      </li>
                    ))}
                  </ul>
                </Card>
              </section>
            ))}
          </div>

          <section aria-label="Verify your download" className="mt-24">
            <SectionIntro
              kicker="Verify"
              title={
                <>
                  Check the file. <Dim>Trust, but hash.</Dim>
                </>
              }
              lede={
                <>
                  Compare the output with the SHA-256 on the release page. If they don&apos;t match, don&apos;t open it.
                </>
              }
            />
            <div className="mt-12 grid gap-4 md:grid-cols-3">
              {checks.map((c) => (
                <Card key={c.os} className="p-5">
                  <p className="font-mono text-xs text-fg/50">{c.os}</p>
                  <pre className="mt-3 whitespace-pre-wrap break-all font-mono text-[13px] leading-relaxed text-fg/80">
                    {c.command}
                  </pre>
                </Card>
              ))}
            </div>
            <p className="mx-auto mt-8 max-w-2xl text-pretty text-center text-sm leading-relaxed text-fg/50">
              Installers are signed where the platform supports it: Authenticode on Windows, notarized on macOS, GPG
              signatures for Linux packages. Signature files are on the{' '}
              <a
                href={RELEASES_URL}
                target="_blank"
                rel="noopener noreferrer"
                className="text-fg underline decoration-fg/30 underline-offset-4 transition-colors hover:decoration-fg"
              >
                release page
              </a>
              .
            </p>
          </section>

          <section aria-label="System requirements" className="mt-24">
            <SectionIntro
              kicker="Requirements"
              title={
                <>
                  What it needs. <Dim>Not much.</Dim>
                </>
              }
            />
            <Card className="mx-auto mt-12 max-w-3xl">
              <dl>
                {requirements.map((r) => (
                  <div
                    key={r.term}
                    className="grid gap-1 border-b border-fg/[0.08] px-6 py-4 last:border-0 sm:grid-cols-[9rem_1fr] sm:gap-6"
                  >
                    <dt className="self-center font-mono text-xs uppercase tracking-[0.14em] text-fg/50">{r.term}</dt>
                    <dd className="text-fg/80">{r.detail}</dd>
                  </div>
                ))}
              </dl>
            </Card>
          </section>
        </Container>
      </main>
      <Footer />
    </>
  )
}
