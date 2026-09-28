import { Github, Download, Mail } from 'lucide-react'
import Link from 'next/link'

export function Footer() {
  return (
    <footer className="border-t border-border bg-white">
      <div className="max-w-7xl mx-auto px-6 py-12 md:py-16 lg:px-12">
        <div className="grid grid-cols-1 md:grid-cols-4 gap-8 mb-12">
          <div className="md:col-span-2">
            <Link href="/" className="flex items-center gap-2 text-text-primary mb-4">
              <Download className="w-8 h-8 text-accent" strokeWidth={2} />
              <span className="text-xl font-semibold tracking-tight">Talkr</span>
            </Link>
            <p className="text-text-secondary max-w-sm">
              Free, private, offline voice tools for everyone. Text-to-speech and speech-to-text running locally on your GPU or CPU.
            </p>
          </div>

          <div>
            <h4 className="font-semibold text-text-primary mb-4">Product</h4>
            <ul className="space-y-2 text-sm text-text-secondary">
              <li><Link href="/download" className="hover:text-accent transition-colors">Download</Link></li>
              <li><Link href="#features" className="hover:text-accent transition-colors">Features</Link></li>
              <li><Link href="#models" className="hover:text-accent transition-colors">Models</Link></li>
              <li><Link href="#privacy" className="hover:text-accent transition-colors">Privacy</Link></li>
            </ul>
          </div>

          <div>
            <h4 className="font-semibold text-text-primary mb-4">Resources</h4>
            <ul className="space-y-2 text-sm text-text-secondary">
              <li><Link href="#faq" className="hover:text-accent transition-colors">FAQ</Link></li>
              <li><a href="https://github.com/talkr/talkr" target="_blank" rel="noopener noreferrer" className="flex items-center gap-2 hover:text-accent transition-colors"><Github className="w-4 h-4" /> GitHub</a></li>
              <li><a href="https://github.com/talkr/talkr/releases" target="_blank" rel="noopener noreferrer" className="flex items-center gap-2 hover:text-accent transition-colors"><Download className="w-4 h-4" /> Releases</a></li>
            </ul>
          </div>
        </div>

        <div className="pt-8 border-t border-border flex flex-col md:flex-row items-center justify-between gap-4">
          <p className="text-sm text-text-muted">
            © {new Date().getFullYear()} Talkr. Open source under MIT license.
          </p>
          <div className="flex items-center gap-6">
            <a href="https://github.com/talkr/talkr" target="_blank" rel="noopener noreferrer" className="text-text-muted hover:text-accent transition-colors" aria-label="GitHub">
              <Github className="w-5 h-5" />
            </a>
            <a href="mailto:hello@talkr.app" className="text-sm text-text-muted hover:text-accent transition-colors flex items-center gap-1">
              <Mail className="w-4 h-4" />
              Contact
            </a>
          </div>
        </div>
      </div>
    </footer>
  )
}