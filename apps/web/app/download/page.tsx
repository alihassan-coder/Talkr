import { Metadata } from 'next'
import { Download, Github, FileText, Shield, CheckCircle } from 'lucide-react'
import Link from 'next/link'

export const metadata: Metadata = {
  title: 'Download Talkr - Windows, macOS, Linux',
  description: 'Download Talkr for Windows, macOS, or Linux. Free, private, offline text-to-speech and speech-to-text.',
}

const releases = [
  {
    platform: 'Windows',
    icon: '🪟',
    files: [
      { name: 'talkr-x64.msi', arch: 'x64 (Installer)', size: '~120 MB', sha256: 'SHA256 will be populated on release' },
      { name: 'talkr-x64.exe', arch: 'x64 (Portable)', size: '~115 MB', sha256: 'SHA256 will be populated on release' },
    ],
    recommended: true,
  },
  {
    platform: 'macOS (Apple Silicon)',
    icon: '🍎',
    files: [
      { name: 'talkr-aarch64.dmg', arch: 'Apple Silicon (M1/M2/M3)', size: '~110 MB', sha256: 'SHA256 will be populated on release' },
    ],
    recommended: true,
  },
  {
    platform: 'macOS (Intel)',
    icon: '🍎',
    files: [
      { name: 'talkr-x64.dmg', arch: 'Intel', size: '~110 MB', sha256: 'SHA256 will be populated on release' },
    ],
    recommended: false,
  },
  {
    platform: 'Linux',
    icon: '🐧',
    files: [
      { name: 'talkr-x86_64.AppImage', arch: 'x64 (AppImage)', size: '~115 MB', sha256: 'SHA256 will be populated on release' },
      { name: 'talkr-x86_64.deb', arch: 'x64 (.deb)', size: '~115 MB', sha256: 'SHA256 will be populated on release' },
      { name: 'talkr-x86_64.rpm', arch: 'x64 (.rpm)', size: '~115 MB', sha256: 'SHA256 will be populated on release' },
    ],
    recommended: true,
  },
]

export default function DownloadPage() {
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
            <Link href="/download" className="text-sm font-medium text-accent">Download</Link>
          </nav>
        </div>
      </header>

      <main className="max-w-7xl mx-auto px-6 py-16 md:py-24 lg:px-12">
        <header className="text-center mb-16">
          <h1 className="text-4xl md:text-5xl font-semibold tracking-tight text-text-primary mb-4">
            Download Talkr
          </h1>
          <p className="text-lg text-text-secondary max-w-2xl mx-auto">
            Choose the installer for your platform. All builds are signed and reproducible. Verify with SHA256 checksums below.
          </p>
        </header>

        <div className="space-y-12">
          {releases.map((release) => (
            <section key={release.platform}>
              <div className="flex items-center gap-3 mb-6">
                <span className="text-3xl">{release.icon}</span>
                <h2 className="text-2xl font-semibold text-text-primary">{release.platform}</h2>
                {release.recommended && <span className="px-2 py-0.5 text-xs font-medium bg-success-soft text-success rounded-[var(--radius-pill)]">Recommended</span>}
              </div>

              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                {release.files.map((file) => (
                  <article key={file.name} className="bg-card border border-border rounded-[var(--radius-card)] p-6 hover:shadow-[var(--shadow-soft)] transition-shadow">
                    <div className="flex items-start justify-between mb-4">
                      <div>
                        <h3 className="font-semibold text-text-primary">{file.name}</h3>
                        <p className="text-sm text-text-muted">{file.arch}</p>
                      </div>
                      <span className="text-sm text-text-muted">{file.size}</span>
                    </div>

                    <div className="mb-4">
                      <label className="block text-xs font-medium text-text-muted mb-1">SHA256 Checksum</label>
                      <code className="text-xs font-mono text-text-secondary bg-app-canvas px-3 py-2 rounded-[var(--radius-input)] block break-all">{file.sha256}</code>
                    </div>

                    <a
                      href={`https://github.com/talkr/talkr/releases/latest/download/${file.name}`}
                      className="block w-full text-center px-4 py-3 bg-accent text-white rounded-[var(--radius-input)] font-medium hover:bg-accent-hover transition-colors"
                      download
                    >
                      Download {file.name}
                    </a>
                  </article>
                ))}
              </div>
            </section>
          ))}

          <section className="pt-8 border-t border-border">
            <h2 className="text-xl font-semibold text-text-primary mb-6">Verification</h2>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
              <Card variant="outlined">
                <h3 className="font-semibold text-text-primary mb-3 flex items-center gap-2"><Shield className="w-5 h-5" /> Verify SHA256</h3>
                <div className="space-y-3 text-sm">
                  <div>
                    <code className="font-mono text-text-secondary bg-app-canvas px-2 py-1 rounded">sha256sum talkr-*.msi</code>
                    <p className="text-text-muted mt-1">Windows (PowerShell): <code className="font-mono">Get-FileHash talkr-*.msi -Algorithm SHA256</code></p>
                  </div>
                  <div>
                    <code className="font-mono text-text-secondary bg-app-canvas px-2 py-1 rounded">sha256sum talkr-*.dmg</code>
                    <p className="text-text-muted mt-1">macOS: <code className="font-mono">shasum -a 256 talkr-*.dmg</code></p>
                  </div>
                  <div>
                    <code className="font-mono text-text-secondary bg-app-canvas px-2 py-1 rounded">sha256sum talkr-*.AppImage</code>
                    <p className="text-text-muted mt-1">Linux: <code className="font-mono">sha256sum talkr-*.AppImage</code></p>
                  </div>
                </div>
              </Card>

              <Card variant="outlined">
                <h3 className="font-semibold text-text-primary mb-3 flex items-center gap-2"><FileText className="w-5 h-5" /> Verify Signature</h3>
                <p className="text-text-secondary text-sm mb-3">All releases are signed. Check the <a href="https://github.com/talkr/talkr/releases" target="_blank" rel="noopener noreferrer" className="text-accent hover:underline">GitHub Releases page</a> for signature files and verification instructions.</p>
                <div className="text-sm text-text-muted">
                  <p>Windows: Authenticode signature</p>
                  <p>macOS: Notarized by Apple</p>
                  <p>Linux: GPG signed</p>
                </div>
              </Card>
            </div>
          </section>
        </div>
      </main>

      <footer className="border-t border-border bg-white">
        <div className="max-w-7xl mx-auto px-6 py-8 text-center text-sm text-text-muted">
          © {new Date().getFullYear()} Talkr. Open source under MIT license.
        </div>
      </footer>
    </>
  )
}