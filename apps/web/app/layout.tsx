import type { Metadata } from 'next'
import './globals.css'

export const metadata: Metadata = {
  title: 'Talkr - Your Voice Tools, Private & Offline',
  description: 'Free, private, offline desktop app for text-to-speech and speech-to-text. Run Whisper and Kokoro locally on your GPU or CPU. No account, no cloud, no telemetry.',
  openGraph: {
    title: 'Talkr - Your Voice Tools, Private & Offline',
    description: 'Free, private, offline desktop app for text-to-speech and speech-to-text.',
    type: 'website',
    locale: 'en_US',
    siteName: 'Talkr',
  },
}

export default function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <html lang="en">
      <head>
        <link rel="preconnect" href="https://fonts.googleapis.com" />
        <link rel="preconnect" href="https://fonts.gstatic.com" crossOrigin="anonymous" />
      </head>
      <body className="min-h-screen bg-bg-primary">
        {children}
      </body>
    </html>
  )
}