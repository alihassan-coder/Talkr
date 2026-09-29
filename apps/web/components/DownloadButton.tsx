'use client'

import { Download } from 'lucide-react'
import { useSyncExternalStore } from 'react'
import { detectDownload, downloadUrl } from '@/lib/releases'

const subscribe = () => () => {}

/**
 * Primary call to action. Renders a neutral label on the server, then swaps in
 * the visitor's platform after hydration.
 */
export function DownloadButton({ size = 'lg' }: { size?: 'md' | 'lg' }) {
  const detected = useSyncExternalStore(subscribe, detectDownload, () => null)

  const href = detected ? downloadUrl(detected.file.name) : '/download'
  const label = detected ? `Download for ${detected.label}` : 'Download Talkr'
  const sizing = size === 'lg' ? 'h-12 px-6 text-[15px]' : 'h-10 px-4 text-sm'

  return (
    <a
      href={href}
      className={`group inline-flex items-center justify-center gap-2.5 rounded-full bg-fg font-medium text-bg transition-[transform,background-color] duration-300 ease-out-quint hover:bg-fg/90 active:scale-[0.97] ${sizing}`}
    >
      <Download className="size-4 transition-transform duration-500 ease-out-quint group-hover:translate-y-0.5" strokeWidth={2} />
      {label}
    </a>
  )
}
