"use client"

import Link from 'next/link'
import { Download, Github, Menu, X } from 'lucide-react'
import { useState, useEffect } from 'react'
import { Button } from '@talkr/ui/Button'

export function Navbar() {
  const [mobileOpen, setMobileOpen] = useState(false)
  const [scrolled, setScrolled] = useState(false)

  useEffect(() => {
    const handleScroll = () => setScrolled(window.scrollY > 20)
    window.addEventListener('scroll', handleScroll)
    return () => window.removeEventListener('scroll', handleScroll)
  }, [])

  const navLinks = [
    { href: '#features', label: 'Features' },
    { href: '#models', label: 'Models' },
    { href: '#privacy', label: 'Privacy' },
    { href: '#faq', label: 'FAQ' },
  ]

  return (
    <header className={`fixed top-0 left-0 right-0 z-40 transition-all duration-normal ${
      scrolled ? 'bg-white/80 backdrop-blur-md border-b border-border' : 'bg-transparent'
    }`}>
      <nav className="max-w-7xl mx-auto px-6 py-4" aria-label="Main navigation">
        <div className="flex items-center justify-between">
          <Link href="/" className="flex items-center gap-2 text-text-primary" aria-label="Talkr home">
            <Download className="w-8 h-8 text-accent" strokeWidth={2} />
            <span className="text-xl font-semibold tracking-tight">Talkr</span>
          </Link>

          <div className="hidden md:flex items-center gap-8">
            {navLinks.map((link) => (
              <Link
                key={link.href}
                href={link.href}
                className="text-sm font-medium text-text-secondary hover:text-text-primary transition-colors"
              >
                {link.label}
              </Link>
            ))}
            <Link href="/download">
              <Button size="sm">Download</Button>
            </Link>
          </div>

          <div className="md:hidden flex items-center gap-4">
            <button
              onClick={() => setMobileOpen(!mobileOpen)}
              className="p-2 rounded-[var(--radius-input)] text-text-secondary hover:text-text-primary hover:bg-app-canvas transition-colors"
              aria-label={mobileOpen ? 'Close menu' : 'Open menu'}
              aria-expanded={mobileOpen}
            >
              {mobileOpen ? <X className="w-6 h-6" strokeWidth={2} /> : <Menu className="w-6 h-6" strokeWidth={2} />}
            </button>
          </div>
        </div>

        {mobileOpen && (
          <div className="md:hidden mt-4 py-4 border-t border-border animate-slide-down">
            <div className="flex flex-col gap-4">
              {navLinks.map((link) => (
                <Link
                  key={link.href}
                  href={link.href}
                  className="text-base font-medium text-text-secondary hover:text-text-primary transition-colors"
                  onClick={() => setMobileOpen(false)}
                >
                  {link.label}
                </Link>
              ))}
              <Link href="/download" onClick={() => setMobileOpen(false)}>
                <Button className="w-full justify-center">Download</Button>
              </Link>
            </div>
          </div>
        )}
      </nav>
    </header>
  )
}