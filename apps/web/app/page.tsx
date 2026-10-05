import { Navbar } from '@/components/Navbar'
import { Footer } from '@/components/Footer'
import { Hero } from '@/components/home/Hero'
import { Stack } from '@/components/home/Stack'
import { Features } from '@/components/home/Features'
import { Models } from '@/components/home/Models'
import { Compare } from '@/components/home/Compare'
import { Privacy } from '@/components/home/Privacy'
import { OpenSource } from '@/components/home/OpenSource'
import { FAQ } from '@/components/home/FAQ'
import { FinalCta } from '@/components/home/FinalCta'
import { REPO_URL, VERSION } from '@/lib/releases'
import { SITE_URL } from '@/lib/site'

// Tells search engines what Talkr is (a free desktop app), for richer results.
const structuredData = {
  '@context': 'https://schema.org',
  '@type': 'SoftwareApplication',
  name: 'Talkr',
  description: 'Text to speech and speech to text that run on your own computer, with Whisper, Kokoro and Piper.',
  url: SITE_URL,
  downloadUrl: `${SITE_URL}/download`,
  applicationCategory: 'MultimediaApplication',
  operatingSystem: 'Windows 10, macOS 11, Linux',
  softwareVersion: VERSION,
  license: 'https://opensource.org/licenses/MIT',
  isAccessibleForFree: true,
  offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
  sameAs: [REPO_URL],
}

export default function HomePage() {
  return (
    <>
      <script
        type="application/ld+json"
        // JSON.stringify output with "<" escaped cannot close the script tag.
        dangerouslySetInnerHTML={{ __html: JSON.stringify(structuredData).replace(/</g, '\\u003c') }}
      />
      <Navbar />
      <main>
        <Hero />
        <Stack />
        <Features />
        <Models />
        <Compare />
        <Privacy />
        <OpenSource />
        <FAQ />
        <FinalCta />
      </main>
      <Footer />
    </>
  )
}
