import { LogoMark } from '@/components/Logo'
import { Container } from '@/components/ui'
import { ISSUES_URL, RELEASES_URL, REPO_URL, VERSION } from '@/lib/releases'

const links = [
  { href: '/download', label: 'Download' },
  { href: '/privacy', label: 'Privacy' },
  { href: REPO_URL, label: 'GitHub', external: true },
  { href: RELEASES_URL, label: 'Releases', external: true },
  { href: ISSUES_URL, label: 'Contact', external: true },
]

export function Footer() {
  return (
    <footer className="border-t border-fg/[0.08]">
      <Container className="flex flex-col gap-8 py-10 md:flex-row md:items-center md:justify-between">
        <div className="flex items-center gap-3 text-sm text-fg/50">
          <LogoMark className="size-5" />
          <span>
            © {new Date().getFullYear()} Talkr · MIT · v{VERSION}
          </span>
        </div>
        <nav aria-label="Footer">
          <ul className="flex flex-wrap gap-x-6 gap-y-2 text-sm text-fg/50">
            {links.map((link) => (
              <li key={link.label}>
                <a
                  href={link.href}
                  {...(link.external ? { target: '_blank', rel: 'noreferrer' } : {})}
                  className="transition-colors hover:text-fg"
                >
                  {link.label}
                </a>
              </li>
            ))}
          </ul>
        </nav>
      </Container>
    </footer>
  )
}
