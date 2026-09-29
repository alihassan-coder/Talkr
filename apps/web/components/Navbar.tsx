'use client'

import Link from 'next/link'
import { useEffect, useState } from 'react'
import { Logo } from '@/components/Logo'
import { REPO_URL } from '@/lib/releases'

const links = [
  { href: '/#features', label: 'Features' },
  { href: '/#models', label: 'Models' },
  { href: '/#privacy', label: 'Privacy' },
  { href: '/#source', label: 'Source' },
  { href: '/#faq', label: 'FAQ' },
]

const sectionIds = links.map((l) => l.href.slice(2))

/** Tracks which homepage section sits in the middle band of the viewport. */
function useActiveSection() {
  const [active, setActive] = useState<string | null>(null)

  useEffect(() => {
    const sections = sectionIds
      .map((id) => document.getElementById(id))
      .filter((el): el is HTMLElement => el !== null)
    if (sections.length === 0) return

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) setActive(entry.target.id)
        }
      },
      { rootMargin: '-45% 0px -50% 0px' },
    )
    sections.forEach((s) => observer.observe(s))
    return () => observer.disconnect()
  }, [])

  return active
}

function GitHubMark({ className = 'size-4' }: { className?: string }) {
  return (
    <svg viewBox="0 0 16 16" className={className} fill="currentColor" aria-hidden="true">
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z" />
    </svg>
  )
}

export function Navbar() {
  const [open, setOpen] = useState(false)
  const [scrolled, setScrolled] = useState(false)
  const active = useActiveSection()

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 8)
    onScroll()
    window.addEventListener('scroll', onScroll, { passive: true })
    return () => window.removeEventListener('scroll', onScroll)
  }, [])

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false)
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [open])

  return (
    <header
      className={`fixed inset-x-0 top-0 z-50 border-b transition-colors duration-300 ${
        scrolled || open ? 'border-fg/[0.08] bg-bg/75 backdrop-blur-xl' : 'border-transparent'
      }`}
    >
      <nav aria-label="Main" className="mx-auto flex h-16 w-full max-w-6xl items-center justify-between px-5 md:px-8">
        <Logo />

        <ul className="absolute left-1/2 hidden -translate-x-1/2 items-center gap-1 lg:flex">
          {links.map((link) => (
            <li key={link.href}>
              <a
                href={link.href}
                aria-current={active === link.href.slice(2) ? 'location' : undefined}
                className="rounded-full px-3.5 py-1.5 text-sm text-fg/60 transition-colors duration-300 hover:bg-fg/[0.06] hover:text-fg aria-[current]:bg-fg/[0.07] aria-[current]:text-fg"
              >
                {link.label}
              </a>
            </li>
          ))}
        </ul>

        <div className="flex items-center gap-2">
          <a
            href={REPO_URL}
            target="_blank"
            rel="noreferrer"
            className="hidden h-9 items-center gap-2 rounded-full border border-fg/10 px-3.5 text-sm text-fg/70 transition-colors hover:border-fg/20 hover:text-fg sm:inline-flex"
          >
            <GitHubMark />
            Star on GitHub
          </a>
          <Link
            href="/download"
            className="hidden h-9 items-center rounded-full bg-fg px-4 text-sm font-medium text-bg transition-transform duration-300 ease-out-quint active:scale-[0.97] md:inline-flex"
          >
            Download
          </Link>
          <button
            type="button"
            onClick={() => setOpen((v) => !v)}
            aria-expanded={open}
            aria-controls="mobile-menu"
            aria-label={open ? 'Close menu' : 'Open menu'}
            className="grid size-9 place-items-center rounded-full border border-fg/10 lg:hidden"
          >
            <span className="relative block h-2.5 w-3.5" aria-hidden="true">
              <span
                className={`absolute left-0 h-px w-full bg-fg transition-transform duration-300 ${open ? 'top-1/2 rotate-45' : 'top-0'}`}
              />
              <span
                className={`absolute left-0 h-px w-full bg-fg transition-transform duration-300 ${open ? 'top-1/2 -rotate-45' : 'bottom-0'}`}
              />
            </span>
          </button>
        </div>
      </nav>

      {open ? (
        <div id="mobile-menu" className="border-t border-fg/[0.08] px-5 pb-6 pt-2 lg:hidden">
          <ul>
            {links.map((link) => (
              <li key={link.href} className="border-b border-fg/[0.08]">
                <a href={link.href} onClick={() => setOpen(false)} className="block py-4 text-lg text-fg/80">
                  {link.label}
                </a>
              </li>
            ))}
          </ul>
          <div className="mt-6 grid grid-cols-2 gap-3">
            <a
              href={REPO_URL}
              target="_blank"
              rel="noreferrer"
              className="inline-flex h-11 items-center justify-center gap-2 rounded-full border border-fg/15 text-sm"
            >
              <GitHubMark /> GitHub
            </a>
            <Link href="/download" className="inline-flex h-11 items-center justify-center rounded-full bg-fg text-sm font-medium text-bg">
              Download
            </Link>
          </div>
        </div>
      ) : null}
    </header>
  )
}
