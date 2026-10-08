import { useEffect, useState } from 'react'
import { cx } from '@/lib/cx'

/**
 * The page's index: one chip per section, sticky under the top edge while scrolling, with the
 * section in view highlighted.
 */
export function SectionNav({ sections }: { sections: readonly (readonly [string, string])[] }) {
  const [active, setActive] = useState<string | null>(null)
  const ids = sections.map(([id]) => id).join()

  useEffect(() => {
    if (typeof IntersectionObserver === 'undefined') return
    const visible = new Map<string, number>()
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) visible.set(e.target.id.replace('dictation-', ''), e.isIntersecting ? e.boundingClientRect.top : Infinity)
        const top = [...visible.entries()].filter(([, y]) => y !== Infinity).sort((a, b) => a[1] - b[1])[0]
        if (top) setActive(top[0])
      },
      { rootMargin: '-80px 0px -55% 0px' },
    )
    for (const id of ids.split(',')) {
      const el = document.getElementById(`dictation-${id}`)
      if (el) io.observe(el)
    }
    return () => io.disconnect()
  }, [ids])

  return (
    <nav aria-label="Dictation sections" className="dict-nav sticky top-0 z-20 -mx-2 px-2 py-2">
      <ul className="flex flex-wrap gap-1">
        {sections.map(([id, label]) => (
          <li key={id}>
            <button
              type="button"
              aria-current={active === id ? 'location' : undefined}
              onClick={() => {
                setActive(id)
                document.getElementById(`dictation-${id}`)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
              }}
              className={cx(
                'inline-flex h-7 items-center rounded-full px-3 text-[12px] font-medium transition-colors duration-200',
                active === id ? 'bg-fg/[0.08] text-fg' : 'text-muted hover:text-fg',
              )}
            >
              {label}
            </button>
          </li>
        ))}
      </ul>
    </nav>
  )
}
