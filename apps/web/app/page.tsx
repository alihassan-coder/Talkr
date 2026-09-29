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

export default function HomePage() {
  return (
    <>
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
