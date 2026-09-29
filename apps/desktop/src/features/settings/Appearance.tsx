import { useRef } from 'react'
import type { KeyboardEvent, ReactNode } from 'react'
import { Check, Monitor, Moon, Sun } from 'lucide-react'
import { Segmented } from '@/components/ui'
import { cx } from '@/lib/cx'
import { useResolvedMode } from '@/lib/appearance'
import { themes } from '@/lib/themes'
import { useUi, type ColorMode } from '@/stores/ui'
import { Row, Section } from '@/features/settings/controls'

const modeOptions: { value: ColorMode; label: string; icon: ReactNode }[] = [
  { value: 'system', label: 'System', icon: <Monitor className="size-3.5" strokeWidth={2} /> },
  { value: 'light', label: 'Light', icon: <Sun className="size-3.5" strokeWidth={2} /> },
  { value: 'dark', label: 'Dark', icon: <Moon className="size-3.5" strokeWidth={2} /> },
]

export function AppearanceSection() {
  const mode = useUi((s) => s.mode)
  const setMode = useUi((s) => s.setMode)

  return (
    <Section title="Appearance">
      <Row label="Mode" description="System follows your computer's light or dark setting.">
        <Segmented label="Mode" value={mode} options={modeOptions} onChange={setMode} />
      </Row>
      <div className="px-5 py-4">
        <p className="text-[13.5px] font-medium tracking-[-0.005em]">Theme</p>
        <p className="mt-0.5 text-[13px] leading-relaxed text-fg/50">Two colours each, in a light and a dark version.</p>
        <ThemePicker />
      </div>
    </Section>
  )
}

/** Radio group of palette swatches. Arrow keys move the selection, like native radios. */
function ThemePicker() {
  const theme = useUi((s) => s.theme)
  const setTheme = useUi((s) => s.setTheme)
  const resolved = useResolvedMode()
  const refs = useRef<(HTMLButtonElement | null)[]>([])

  const onKeyDown = (e: KeyboardEvent, index: number) => {
    const moves: Record<string, number> = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }
    let next: number
    if (e.key in moves) next = (index + (moves[e.key] ?? 0) + themes.length) % themes.length
    else if (e.key === 'Home') next = 0
    else if (e.key === 'End') next = themes.length - 1
    else return
    e.preventDefault()
    const target = themes[next]
    if (!target) return
    setTheme(target.id)
    refs.current[next]?.focus()
  }

  return (
    <div role="radiogroup" aria-label="Theme" className="mt-4 grid grid-cols-3 gap-3 lg:grid-cols-6">
      {themes.map((t, i) => (
        <Swatch
          key={t.id}
          ref={(el) => {
            refs.current[i] = el
          }}
          name={t.name}
          palette={t[resolved]}
          selected={t.id === theme}
          onSelect={() => setTheme(t.id)}
          onKeyDown={(e) => onKeyDown(e, i)}
        />
      ))}
    </div>
  )
}

function Swatch({
  ref,
  name,
  palette,
  selected,
  onSelect,
  onKeyDown,
}: {
  ref: (el: HTMLButtonElement | null) => void
  name: string
  palette: { bg: string; fg: string }
  selected: boolean
  onSelect: () => void
  onKeyDown: (e: KeyboardEvent) => void
}) {
  // Tints inside the preview are mixed from the palette itself, the same way the app mixes them.
  const tint = (percent: number) => `color-mix(in oklab, ${palette.fg} ${percent}%, transparent)`

  return (
    <button
      ref={ref}
      type="button"
      role="radio"
      aria-checked={selected}
      aria-label={name}
      tabIndex={selected ? 0 : -1}
      onClick={onSelect}
      onKeyDown={onKeyDown}
      className={cx(
        'group rounded-xl border p-1.5 text-left transition-[border-color,box-shadow] duration-200',
        selected ? 'border-fg shadow-[0_0_0_1px_var(--color-fg)]' : 'border-fg/10 hover:border-fg/25',
      )}
    >
      <span
        aria-hidden="true"
        className="relative flex h-16 overflow-hidden rounded-lg"
        style={{ backgroundColor: palette.bg, boxShadow: `inset 0 0 0 1px ${tint(10)}` }}
      >
        {/* A tiny version of the app: sidebar, heading, two lines and a button. */}
        <span className="flex w-5 flex-col gap-1 p-1.5" style={{ backgroundColor: tint(4), borderRight: `1px solid ${tint(8)}` }}>
          <span className="h-1 rounded-full" style={{ backgroundColor: tint(80) }} />
          <span className="h-1 rounded-full" style={{ backgroundColor: tint(25) }} />
          <span className="h-1 rounded-full" style={{ backgroundColor: tint(25) }} />
        </span>
        <span className="flex flex-1 flex-col gap-1.5 p-2">
          <span className="h-1.5 w-3/5 rounded-full" style={{ backgroundColor: palette.fg }} />
          <span className="h-1 w-4/5 rounded-full" style={{ backgroundColor: tint(30) }} />
          <span className="h-1 w-1/2 rounded-full" style={{ backgroundColor: tint(30) }} />
          <span className="mt-auto h-2.5 w-7 rounded-full" style={{ backgroundColor: palette.fg }} />
        </span>
        <span
          className={cx(
            'absolute right-1.5 top-1.5 grid size-4 place-items-center rounded-full transition-[opacity,transform] duration-200 ease-out-quint',
            selected ? 'scale-100 opacity-100' : 'scale-75 opacity-0',
          )}
          style={{ backgroundColor: palette.fg, color: palette.bg }}
        >
          <Check className="size-2.5" strokeWidth={3} />
        </span>
      </span>
      <span className={cx('block px-1 pb-0.5 pt-2 text-[12.5px] transition-colors', selected ? 'font-medium text-fg' : 'text-fg/60 group-hover:text-fg/85')}>
        {name}
      </span>
    </button>
  )
}
