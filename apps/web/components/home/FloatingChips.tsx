'use client'

import type { CSSProperties } from 'react'
import { useEffect, useRef } from 'react'

/** Little pieces of the product drifting around the headline. `depth` sets how far each follows the pointer. */
const chips = [
  { text: 'whisper-base.en', className: 'left-0 top-[12%]', r: '-6deg', delay: '0s', depth: 1.4 },
  { text: '.srt', className: 'left-[7%] top-[44%]', r: '5deg', delay: '-2.4s', depth: 0.6 },
  { text: '0 bytes uploaded', className: 'left-[1%] top-[76%]', r: '3deg', delay: '-4.8s', depth: 1 },
  { text: 'Kokoro · af_heart', className: 'right-0 top-[10%]', r: '6deg', delay: '-1.2s', depth: 1.2 },
  { text: '.wav', className: 'right-[8%] top-[42%]', r: '-5deg', delay: '-5.6s', depth: 0.5 },
  { text: 'Metal · Vulkan', className: 'right-[1%] top-[74%]', r: '-4deg', delay: '-3.6s', depth: 0.9 },
]

export function FloatingChips() {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const el = ref.current
    if (!el) return
    if (!window.matchMedia('(pointer: fine) and (prefers-reduced-motion: no-preference)').matches) return

    let frame = 0
    const onMove = (e: PointerEvent) => {
      cancelAnimationFrame(frame)
      frame = requestAnimationFrame(() => {
        el.style.setProperty('--px', `${(e.clientX / window.innerWidth - 0.5) * 2}`)
        el.style.setProperty('--py', `${(e.clientY / window.innerHeight - 0.5) * 2}`)
      })
    }
    window.addEventListener('pointermove', onMove, { passive: true })
    return () => {
      cancelAnimationFrame(frame)
      window.removeEventListener('pointermove', onMove)
    }
  }, [])

  return (
    <div ref={ref} aria-hidden="true" className="pointer-events-none absolute inset-0 hidden lg:block">
      {chips.map((chip) => (
        <span
          key={chip.text}
          style={
            {
              '--r': chip.r,
              '--d': chip.depth,
              animationDelay: chip.delay,
              translate: 'calc(var(--px, 0) * var(--d) * -14px) calc(var(--py, 0) * var(--d) * -10px)',
            } as CSSProperties
          }
          className={`absolute animate-float rounded-xl border border-fg/10 bg-bg/80 px-3 py-1.5 font-mono text-xs text-fg/55 backdrop-blur-sm transition-[translate] duration-700 ease-out-quint motion-reduce:animate-none ${chip.className}`}
        >
          {chip.text}
        </span>
      ))}
    </div>
  )
}
