'use client'

import type { PointerEvent, ReactNode } from 'react'

/**
 * Feeds pointer coordinates to every `.spotlight` child so each card lights up
 * under the cursor. Touch input is ignored; there is no hover on touch screens.
 */
export function SpotlightGroup({ className = '', children }: { className?: string; children: ReactNode }) {
  const onMove = (e: PointerEvent<HTMLDivElement>) => {
    if (e.pointerType !== 'mouse') return
    for (const card of e.currentTarget.querySelectorAll<HTMLElement>('.spotlight')) {
      const rect = card.getBoundingClientRect()
      card.style.setProperty('--mx', `${e.clientX - rect.left}px`)
      card.style.setProperty('--my', `${e.clientY - rect.top}px`)
      card.style.setProperty('--spot', '1')
    }
  }

  const onLeave = (e: PointerEvent<HTMLDivElement>) => {
    for (const card of e.currentTarget.querySelectorAll<HTMLElement>('.spotlight')) {
      card.style.setProperty('--spot', '0')
    }
  }

  return (
    <div className={className} onPointerMove={onMove} onPointerLeave={onLeave}>
      {children}
    </div>
  )
}
