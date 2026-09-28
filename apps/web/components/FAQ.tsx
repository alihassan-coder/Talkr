"use client"

import { ChevronDown, ChevronUp } from 'lucide-react'
import { useState } from 'react'
import { Card } from '@talkr/ui/Card'

const faqs = [
  {
    q: 'What are the system requirements?',
    a: 'Talkr runs on Windows 10+, macOS 11+, and Linux (glibc 2.31+). For GPU acceleration: macOS needs Apple Silicon; Windows/Linux need NVIDIA GPU with CUDA 12+ or any Vulkan 1.2+ GPU. CPU-only works on any x64/ARM64 machine with 4GB+ RAM.',
  },
  {
    q: 'Which GPU backends are supported?',
    a: 'Metal on macOS (Apple Silicon), CUDA on NVIDIA (Windows/Linux), and Vulkan on NVIDIA/AMD/Intel (Windows/Linux). The app auto-detects the best available backend and falls back to CPU if needed.',
  },
  {
    q: 'Where are my models and history stored?',
    a: 'Everything lives in ~/.talkr/ (Linux/macOS) or C:\\Users\\You\\.talkr\\ (Windows). You can override with the TALKR_HOME environment variable. The folder contains models/, audio/, history/, cache/, and logs/.',
  },
  {
    q: 'How do I uninstall Talkr and delete my data?',
    a: 'Uninstall the app normally (Windows: Settings → Apps; macOS: drag to Trash; Linux: remove AppImage/package). To delete all data, remove the ~/.talkr/ folder. There\'s also a "Delete all history" button in Settings.',
  },
  {
    q: 'Are the models really free and open source?',
    a: 'Yes. Whisper models are MIT licensed. Kokoro is Apache-2.0. Piper voices are MIT but bundle espeak-ng-data (GPL-3.0). All licenses are shown in the app before download.',
  },
  {
    q: 'Can I use my own models?',
    a: 'Yes. Use "Import local model" in the Models page. For STT: a GGML .bin file. For TTS: a folder with model.onnx, tokens.txt, and espeak-ng-data/.',
  },
]

export function FAQ() {
  const [openIndex, setOpenIndex] = useState<number | null>(0)

  return (
    <section id="faq" className="py-24 px-6 md:py-32 md:px-12 lg:px-24 bg-app-canvas">
      <div className="max-w-3xl mx-auto">
        <header className="text-center mb-16">
          <h2 className="text-3xl md:text-4xl font-semibold tracking-tight text-text-primary mb-4">
            Frequently Asked Questions
          </h2>
        </header>

        <div className="space-y-4">
          {faqs.map((faq, index) => (
            <Card key={index} variant="outlined" className="overflow-hidden">
              <button
                onClick={() => setOpenIndex(openIndex === index ? null : index)}
                className="w-full px-6 py-5 text-left flex items-center justify-between gap-4 focus:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 rounded-[var(--radius-card)]"
                aria-expanded={openIndex === index}
              >
                <span className="font-medium text-text-primary pr-10">{faq.q}</span>
                {openIndex === index ? (
                  <ChevronUp className="w-5 h-5 text-text-secondary flex-shrink-0" strokeWidth={2} />
                ) : (
                  <ChevronDown className="w-5 h-5 text-text-secondary flex-shrink-0" strokeWidth={2} />
                )}
              </button>
              {openIndex === index && (
                <div className="px-6 pb-6 border-t border-border animate-slide-down">
                  <p className="text-text-secondary">{faq.a}</p>
                </div>
              )}
            </Card>
          ))}
        </div>
      </div>
    </section>
  )
}