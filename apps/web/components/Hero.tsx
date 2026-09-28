"use client"

import { Download, ArrowRight, CheckCircle, Github } from 'lucide-react'
import { Button } from '@talkr/ui/Button'
import { useEffect, useState } from 'react'

interface OSInfo {
  name: string
  icon: string
  downloadUrl: string
}

export function Hero() {
  const [os, setOs] = useState<OSInfo>({ name: 'your platform', icon: '💻', downloadUrl: '/download' })

  useEffect(() => {
    const ua = navigator.userAgent.toLowerCase()
    const platform = navigator.userAgentData?.platform?.toLowerCase() || navigator.platform.toLowerCase()

    if (ua.includes('win') || platform.includes('win')) {
      setOs({ name: 'Windows', icon: '🪟', downloadUrl: 'https://github.com/talkr/talkr/releases/latest/download/talkr-x64.msi' })
    } else if (ua.includes('mac') || platform.includes('mac')) {
      const isArm = ua.includes('arm64') || platform.includes('arm')
      setOs({ name: isArm ? 'macOS (Apple Silicon)' : 'macOS (Intel)', icon: '🍎', downloadUrl: isArm ? 'https://github.com/talkr/talkr/releases/latest/download/talkr-aarch64.dmg' : 'https://github.com/talkr/talkr/releases/latest/download/talkr-x64.dmg' })
    } else if (ua.includes('linux') || platform.includes('linux')) {
      setOs({ name: 'Linux', icon: '🐧', downloadUrl: 'https://github.com/talkr/talkr/releases/latest/download/talkr-x86_64.AppImage' })
    }
  }, [])

  return (
    <section className="relative pt-32 pb-24 px-6 md:pt-48 md:pb-32 md:px-12 lg:px-24">
      <div className="max-w-7xl mx-auto text-center">
        <div className="inline-flex items-center gap-2 px-4 py-2 rounded-[var(--radius-pill)] bg-accent-soft text-accent text-sm font-medium mb-8 animate-fade-in">
          <span className="w-2 h-2 rounded-full bg-success animate-pulse" />
          <span>Version 0.1.0 — Free, Private, Offline</span>
        </div>

        <h1 className="text-4xl md:text-6xl lg:text-7xl font-semibold tracking-tight text-text-primary mb-6 animate-slide-up">
          Your voice tools.<br />
          <span className="text-accent">On your computer.</span>
        </h1>
        <p className="text-lg md:text-xl text-text-secondary max-w-3xl mx-auto mb-10 animate-slide-up delay-100">
          Turn text into natural speech and speech into text — completely offline, GPU-accelerated, and free forever. No account. No cloud. No telemetry.
        </p>

        <div className="flex flex-col sm:flex-row items-center justify-center gap-4 mb-16 animate-slide-up delay-200">
          <a
            href={os.downloadUrl}
            className="group inline-flex items-center gap-3 px-8 py-4 bg-accent text-white rounded-[var(--radius-pill)] font-medium text-lg hover:bg-accent-hover transition-all duration-fast shadow-[var(--shadow-card)]"
            download
          >
            <Download className="w-6 h-6 group-hover:translate-x-1 transition-transform" strokeWidth={2} />
            <span>Download for {os.name} {os.icon}</span>
            <ArrowRight className="w-5 h-5 group-hover:translate-x-1 transition-transform" strokeWidth={2} />
          </a>
          <a
            href="/download"
            className="inline-flex items-center gap-2 px-6 py-3 text-text-secondary font-medium rounded-[var(--radius-pill)] hover:text-text-primary hover:bg-app-canvas transition-colors"
          >
            Other platforms
          </a>
        </div>

        <div className="flex flex-wrap items-center justify-center gap-8 text-sm text-text-muted animate-fade-in delay-300">
          <span className="flex items-center gap-2">
            <CheckCircle className="w-4 h-4 text-success" />
            100% Offline
          </span>
          <span className="flex items-center gap-2">
            <CheckCircle className="w-4 h-4 text-success" />
            GPU Accelerated
          </span>
          <span className="flex items-center gap-2">
            <CheckCircle className="w-4 h-4 text-success" />
            Open Source Models
          </span>
          <span className="flex items-center gap-2">
            <CheckCircle className="w-4 h-4 text-success" />
            No Account Required
          </span>
        </div>

        <div className="mt-16 relative animate-fade-in delay-400">
          <div className="bg-app-canvas rounded-[var(--radius-card)] p-2 shadow-[var(--shadow-soft)] border border-border inline-block">
            <div className="bg-card rounded-[calc(var(--radius-card)-4px)] overflow-hidden shadow-[var(--shadow-card)] border border-border" style={{ aspectRatio: '16/10' }}>
              <div className="flex items-center justify-center h-full bg-gradient-to-br from-accent-soft to-success-soft">
                <span className="text-text-muted text-lg">App Screenshot</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}