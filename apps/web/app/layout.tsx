import type { Metadata, Viewport } from 'next'
import localFont from 'next/font/local'
import { SITE_URL } from '@/lib/site'
import './globals.css'

// Served from the @fontsource packages rather than next/font/google, so a build never depends on
// reaching Google Fonts (a failed fetch there fails the whole build).
const sans = localFont({
  src: '../node_modules/@fontsource-variable/geist/files/geist-latin-wght-normal.woff2',
  weight: '100 900',
  variable: '--font-geist-sans',
  display: 'swap',
})

const mono = localFont({
  src: '../node_modules/@fontsource-variable/geist-mono/files/geist-mono-latin-wght-normal.woff2',
  weight: '100 900',
  variable: '--font-geist-mono',
  display: 'swap',
})

export const metadata: Metadata = {
  metadataBase: new URL(SITE_URL),
  alternates: { canonical: '/' },
  title: {
    default: 'Talkr: speech tools that never phone home',
    template: '%s · Talkr',
  },
  description:
    'Turn text into speech and speech into text on your own computer. Whisper, Kokoro and Piper run locally on your computer. Free, open source, no account.',
  openGraph: {
    title: 'Talkr: speech tools that never phone home',
    description: 'Text to speech and speech to text, running entirely on your own machine.',
    type: 'website',
    url: '/',
    locale: 'en_US',
    siteName: 'Talkr',
  },
}

export const viewport: Viewport = {
  themeColor: '#0b0b0c',
  colorScheme: 'dark',
}

export default function RootLayout({ children }: LayoutProps<'/'>) {
  return (
    <html lang="en" className={`${sans.variable} ${mono.variable}`}>
      <body className="min-h-dvh overflow-x-clip bg-bg text-fg">{children}</body>
    </html>
  )
}
