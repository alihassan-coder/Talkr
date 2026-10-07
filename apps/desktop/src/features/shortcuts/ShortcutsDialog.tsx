import { useEffect, useRef } from 'react'
import { X } from 'lucide-react'
import { Kbd } from '@/components/ui'
import { modKey } from '@/lib/platform'

const groups = (mod: string) => [
  {
    title: 'Go to',
    items: [
      { keys: [mod, '1'], label: 'Speak' },
      { keys: [mod, '2'], label: 'Transcribe' },
      { keys: [mod, '3'], label: 'Dictation' },
      { keys: [mod, '4'], label: 'Models' },
      { keys: [mod, '5'], label: 'History' },
      { keys: [mod, ','], label: 'Settings' },
    ],
  },
  {
    title: 'Anywhere',
    items: [
      { keys: [mod, 'B'], label: 'Show or hide the sidebar' },
      { keys: ['?'], label: 'This list' },
    ],
  },
  {
    title: 'Dictation, in any app (defaults; change them in Dictation)',
    items: [
      { keys: ['Ctrl', 'Win'], label: 'Hold to dictate, tap for hands-free' },
      { keys: ['Esc'], label: 'Cancel a dictation' },
      { keys: ['Alt', 'Shift', 'V'], label: 'Paste the last dictation again' },
    ],
  },
  {
    title: 'Zoom',
    items: [
      { keys: [mod, '+'], label: 'Zoom in' },
      { keys: [mod, '−'], label: 'Zoom out' },
      { keys: [mod, '0'], label: 'Reset zoom' },
    ],
  },
  {
    title: 'Speak and History',
    items: [
      { keys: [mod, '⏎'], label: 'Generate speech' },
      { keys: [mod, 'F'], label: 'Search history' },
      { keys: ['Space'], label: 'Play or pause the focused player' },
      { keys: ['←', '→'], label: 'Seek 5 seconds' },
      { keys: ['Esc'], label: 'Close details, clear search' },
    ],
  },
]

/** Keyboard shortcut overview, opened with "?". */
export function ShortcutsDialog({ onClose }: { onClose: () => void }) {
  const closeRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null
    closeRef.current?.focus()
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault()
        e.stopPropagation()
        onClose()
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => {
      window.removeEventListener('keydown', onKey, true)
      previous?.focus?.()
    }
  }, [onClose])

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/30 p-6 backdrop-blur-[2px]" onMouseDown={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="shortcuts-title"
        onMouseDown={(e) => e.stopPropagation()}
        className="w-full max-w-md animate-pop rounded-2xl border border-line bg-surface p-5 shadow-[var(--shadow-pop)]"
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 id="shortcuts-title" className="text-[15px] font-semibold tracking-[-0.01em]">
            Keyboard shortcuts
          </h2>
          <button
            ref={closeRef}
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="grid size-7 place-items-center rounded-md text-subtle transition-colors hover:bg-fg/[0.06] hover:text-fg"
          >
            <X className="size-4" strokeWidth={2} />
          </button>
        </div>
        <div className="space-y-4">
          {groups(modKey()).map((group) => (
            <section key={group.title}>
              <h3 className="mb-1.5 font-mono text-[10px] uppercase tracking-[0.14em] text-subtle">{group.title}</h3>
              <ul className="space-y-1">
                {group.items.map((item) => (
                  <li key={item.label} className="flex items-center justify-between gap-4 text-[13px]">
                    <span className="text-muted">{item.label}</span>
                    <span className="flex shrink-0 gap-1">
                      {item.keys.map((k) => (
                        <Kbd key={k}>{k}</Kbd>
                      ))}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
      </div>
    </div>
  )
}
