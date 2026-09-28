import { Navbar } from '@/components/Navbar'
import { Hero } from '@/components/Hero'
import { Features } from '@/components/Features'
import { HowItWorks } from '@/components/HowItWorks'
import { ModelsShowcase } from '@/components/ModelsShowcase'
import { PrivacySection } from '@/components/PrivacySection'
import { FAQ } from '@/components/FAQ'
import { Footer } from '@/components/Footer'

export default function HomePage() {
  return (
    <>
      <Navbar />
      <main className="min-h-screen">
        <Hero />
        <Features />
        <HowItWorks />
        <ModelsShowcase />
        <PrivacySection />
        <FAQ />
      </main>
      <Footer />
    </>
  )
}