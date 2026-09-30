import type { Metadata, Viewport } from 'next'
import { Geist, Geist_Mono } from 'next/font/google'
import './globals.css'

const sans = Geist({
  subsets: ['latin'],
  variable: '--font-geist-sans',
  display: 'swap',
})

const mono = Geist_Mono({
  subsets: ['latin'],
  variable: '--font-geist-mono',
  display: 'swap',
})

export const metadata: Metadata = {
  metadataBase: new URL('https://talkr.app'),
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
