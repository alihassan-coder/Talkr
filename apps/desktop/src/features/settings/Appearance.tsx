import { useRef } from 'react'
import type { CSSProperties, KeyboardEvent, ReactNode } from 'react'
import { Check, Monitor, Moon, Sun } from 'lucide-react'
import { cx } from '@/lib/cx'
import { useResolvedMode } from '@/lib/appearance'
import { findTheme, themes, type Palette } from '@/lib/themes'
import { useUi, type ColorMode } from '@/stores/ui'
import { Section } from '@/features/settings/controls'

type Mode = 'light' | 'dark'

const modeOptions: { value: ColorMode; label: string; icon: ReactNode }[] = [
  { value: 'system', label: 'System', icon: <Monitor className="size-3.5" strokeWidth={2} /> },
  { value: 'light', label: 'Light', icon: <Sun className="size-3.5" strokeWidth={2} /> },
  { value: 'dark', label: 'Dark', icon: <Moon className="size-3.5" strokeWidth={2} /> },
]

export function AppearanceSection() {
  const resolved = useResolvedMode()

  return (
    <Section title="Appearance">
      <div className="border-b border-line px-5 py-4">
        <p className="text-[13.5px] font-medium tracking-[-0.005em]">Mode</p>
        <p className="mt-0.5 text-[13px] leading-relaxed text-muted">System follows your computer&apos;s light or dark setting.</p>
        <ModePicker />
      </div>
      <div className="px-5 py-4">
        <p className="text-[13.5px] font-medium tracking-[-0.005em]">Theme</p>
        <p className="mt-0.5 text-[13px] leading-relaxed text-muted">
          Each palette comes in a light and a dark version. Previews show {resolved}.
        </p>
        <ThemePicker />
      </div>
    </Section>
  )
}

/**
 * Keyboard for a radio group, like native radios: arrows move and select,
 * Home and End jump. Only the checked item is in the tab order.
 */
function useRadioKeys(count: number, select: (index: number) => void) {
  const refs = useRef<(HTMLButtonElement | null)[]>([])
  const onKeyDown = (e: KeyboardEvent, index: number) => {
    const moves: Record<string, number> = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }
    let next: number
    if (e.key in moves) next = (index + (moves[e.key] ?? 0) + count) % count
    else if (e.key === 'Home') next = 0
    else if (e.key === 'End') next = count - 1
    else return
    e.preventDefault()
    select(next)
    refs.current[next]?.focus()
  }
  const setRef = (index: number) => (el: HTMLButtonElement | null) => {
    refs.current[index] = el
  }
  return { onKeyDown, setRef }
}

function ModePicker() {
  const mode = useUi((s) => s.mode)
  const setMode = useUi((s) => s.setMode)
  const theme = findTheme(useUi((s) => s.theme))
  const { onKeyDown, setRef } = useRadioKeys(modeOptions.length, (i) => {
    const option = modeOptions[i]
    if (option) setMode(option.value)
  })

  return (
    <div role="radiogroup" aria-label="Mode" className="mt-4 grid grid-cols-3 gap-3">
      {modeOptions.map((o, i) => {
        const selected = o.value === mode
        return (
          <button
            key={o.value}
            ref={setRef(i)}
            type="button"
            role="radio"
            aria-checked={selected}
            tabIndex={selected ? 0 : -1}
            onClick={() => setMode(o.value)}
            onKeyDown={(e) => onKeyDown(e, i)}
            className={cardClass(selected)}
            style={selected ? ringStyle('var(--color-accent)') : undefined}
          >
            <span className="relative block h-[88px] overflow-hidden rounded-lg">
              {o.value === 'system' ? (
                <>
                  <MiniWindow palette={theme.light} mode="light" />
                  {/* Dark half laid over the light one, cut on a slant. */}
                  <span className="absolute inset-0 [clip-path:polygon(58%_0,100%_0,100%_100%,42%_100%)]">
                    <MiniWindow palette={theme.dark} mode="dark" />
                  </span>
                </>
              ) : (
                <MiniWindow palette={theme[o.value]} mode={o.value} />
              )}
              <CheckBadge selected={selected} style={{ backgroundColor: 'var(--color-accent)', color: 'var(--color-on-accent)' }} />
            </span>
            <span
              className={cx(
                'flex items-center gap-1.5 px-1 pb-0.5 pt-2.5 text-[13px] transition-colors',
                selected ? 'font-medium text-fg' : 'text-muted group-hover:text-fg',
              )}
            >
              {o.icon}
              {o.label}
            </span>
          </button>
        )
      })}
    </div>
  )
}

function ThemePicker() {
  const theme = useUi((s) => s.theme)
  const setTheme = useUi((s) => s.setTheme)
  const resolved = useResolvedMode()
  const { onKeyDown, setRef } = useRadioKeys(themes.length, (i) => {
    const target = themes[i]
    if (target) setTheme(target.id)
  })

  return (
    <div className="@container mt-4">
      <div role="radiogroup" aria-label="Theme" className="grid grid-cols-3 gap-3 @min-[40rem]:grid-cols-4">
        {themes.map((t, i) => {
          const palette = t[resolved]
          const selected = t.id === theme
          return (
            <button
              key={t.id}
              ref={setRef(i)}
              type="button"
              role="radio"
              aria-checked={selected}
              aria-label={`${t.name}, ${t.mood}`}
              tabIndex={selected ? 0 : -1}
              onClick={() => setTheme(t.id)}
              onKeyDown={(e) => onKeyDown(e, i)}
              className={cardClass(selected)}
              style={selected ? ringStyle(palette.accent) : undefined}
            >
              <span className="relative block h-[88px] overflow-hidden rounded-lg">
                <MiniWindow palette={palette} mode={resolved} />
                <CheckBadge selected={selected} style={{ backgroundColor: palette.accent, color: palette.onAccent }} />
              </span>
              <span className="flex items-end justify-between gap-2 px-1 pb-0.5 pt-2.5">
                <span className="min-w-0">
                  <span className="block truncate text-[13px] font-medium leading-tight tracking-[-0.005em] text-fg">
                    {t.name}
                  </span>
                  <span className="mt-0.5 block truncate text-[11.5px] leading-tight text-subtle">{t.mood}</span>
                </span>
                <span aria-hidden="true" className="mb-px flex shrink-0 -space-x-1">
                  {[palette.bg, palette.surface, palette.accent].map((color, n) => (
                    <span
                      key={n}
                      className="size-3 rounded-full"
                      style={{
                        backgroundColor: color,
                        boxShadow: 'inset 0 0 0 1px var(--color-line-strong), 0 0 0 1.5px var(--color-surface)',
                      }}
                    />
                  ))}
                </span>
              </span>
            </button>
          )
        })}
      </div>
    </div>
  )
}

const cardClass = (selected: boolean) =>
  cx(
    'group relative rounded-xl border bg-surface p-1.5 text-left transition-[border-color,box-shadow,transform] duration-200 ease-out-quint',
    selected ? 'border-transparent' : 'border-line hover:-translate-y-px hover:border-line-strong',
  )

/** 2px ring in the given colour: a 1px border plus a 1px shadow. */
const ringStyle = (color: string): CSSProperties => ({ borderColor: color, boxShadow: `0 0 0 1px ${color}` })

function CheckBadge({ selected, style }: { selected: boolean; style: CSSProperties }) {
  return (
    <span
      aria-hidden="true"
      className={cx(
        'absolute right-1.5 top-1.5 grid size-[18px] place-items-center rounded-full shadow-[0_1px_2px_rgb(0_0_0/0.2)] transition-[opacity,transform] duration-200 ease-out-quint',
        selected ? 'scale-100 opacity-100' : 'scale-75 opacity-0',
      )}
      style={style}
    >
      <Check className="size-2.5" strokeWidth={3.25} />
    </span>
  )
}

const WAVE = [0.45, 0.8, 1, 0.6, 0.85, 0.4, 0.65]

/**
 * A tiny Talkr window drawn in one palette: sidebar with the active item
 * marked in the accent, a heading, and a card with text, a waveform and a
 * button. Derived colours are mixed the same way globals.css mixes them.
 */
function MiniWindow({ palette, mode }: { palette: Palette; mode: Mode }) {
  const { bg, surface, fg, accent } = palette
  const dark = mode === 'dark'
  const mix = (percent: number, base = 'transparent') => `color-mix(in oklab, ${fg} ${percent}%, ${base})`
  const line = mix(dark ? 10 : 12)
  const nav = mix(dark ? 26 : 22)

  return (
    <span aria-hidden="true" className="absolute inset-0 flex" style={{ backgroundColor: bg }}>
      <span
        className="flex w-[24%] shrink-0 flex-col gap-[5px] px-1.5 py-2"
        style={{ backgroundColor: mix(dark ? 2.5 : 3.5, bg), borderRight: `1px solid ${line}` }}
      >
        <span className="mb-1 h-1 w-3/5 rounded-full" style={{ backgroundColor: fg }} />
        <span className="relative flex h-[7px] items-center rounded-[3px] pl-1.5 pr-1" style={{ backgroundColor: mix(dark ? 8 : 7) }}>
          <span className="absolute inset-y-px left-0 w-[2px] rounded-full" style={{ backgroundColor: accent }} />
          <span className="h-[3px] w-3/4 rounded-full" style={{ backgroundColor: mix(dark ? 80 : 75) }} />
        </span>
        <span className="flex h-[7px] items-center px-1">
          <span className="h-[3px] w-3/5 rounded-full" style={{ backgroundColor: nav }} />
        </span>
        <span className="flex h-[7px] items-center px-1">
          <span className="h-[3px] w-2/3 rounded-full" style={{ backgroundColor: nav }} />
        </span>
      </span>
      <span className="flex min-w-0 flex-1 flex-col gap-1.5 p-2">
        <span className="h-1.5 w-2/5 shrink-0 rounded-full" style={{ backgroundColor: fg }} />
        <span
          className="flex min-h-0 flex-1 flex-col justify-between rounded-[5px] p-1.5"
          style={{
            backgroundColor: surface,
            border: `1px solid ${line}`,
            boxShadow: dark ? undefined : '0 1px 2px rgb(0 0 0 / 0.05)',
          }}
        >
          <span className="flex flex-col gap-1">
            <span className="h-1 w-4/5 rounded-full" style={{ backgroundColor: mix(dark ? 85 : 80, surface) }} />
            <span className="h-1 w-1/2 rounded-full" style={{ backgroundColor: mix(dark ? 40 : 35, surface) }} />
          </span>
          <span className="flex items-end justify-between gap-1">
            <span className="flex h-3 items-center gap-[2px]">
              {WAVE.map((h, n) => (
                <span
                  key={n}
                  className="w-[2px] rounded-full"
                  style={{ height: `${h * 100}%`, backgroundColor: accent, opacity: n < 5 ? 1 : 0.35 }}
                />
              ))}
            </span>
            <span className="h-2.5 w-7 shrink-0 rounded-full" style={{ backgroundColor: accent }} />
          </span>
        </span>
      </span>
      <span className="pointer-events-none absolute inset-0 rounded-lg" style={{ boxShadow: `inset 0 0 0 1px ${line}` }} />
    </span>
  )
}
