import { useId, useRef, useState } from 'react'
import type { CSSProperties, KeyboardEvent, ReactNode } from 'react'
import { Check, Minus, Monitor, Moon, Plus, RotateCcw, Sun } from 'lucide-react'
import { Button, Segmented } from '@/components/ui'
import { cx } from '@/lib/cx'
import { contrast, normalizeHex } from '@/lib/color'
import { useResolvedMode } from '@/lib/appearance'
import { modKey } from '@/lib/platform'
import {
  ACCENT_SUGGESTIONS,
  CUSTOM_THEME,
  deriveCustomTheme,
  findTheme,
  themes,
  type Palette,
  type ThemeChoice,
} from '@/lib/themes'
import { DEFAULT_ZOOM, formatZoom, stepZoom, ZOOM_LEVELS } from '@/lib/zoom'
import { useUi, type ColorMode } from '@/stores/ui'
import { Row, Section } from '@/features/settings/controls'

type Mode = 'light' | 'dark'

const modeOptions: { value: ColorMode; label: string; icon: ReactNode }[] = [
  { value: 'system', label: 'System', icon: <Monitor className="size-3.5" strokeWidth={2} /> },
  { value: 'light', label: 'Light', icon: <Sun className="size-3.5" strokeWidth={2} /> },
  { value: 'dark', label: 'Dark', icon: <Moon className="size-3.5" strokeWidth={2} /> },
]

const modeHelp = (mode: ColorMode, resolved: Mode) =>
  mode === 'system' ? `Follows your computer's setting, which is ${resolved} right now.` : `Always ${mode}, whatever your computer uses.`

/** Theme cards: the built-in palettes, then the custom one. */
const choices: ThemeChoice[] = [...themes.map((t) => t.id), CUSTOM_THEME]

/** Name, mood and palette of a theme choice in one mode. */
function describe(choice: ThemeChoice, customAccent: string, mode: Mode) {
  if (choice === CUSTOM_THEME) {
    return { name: 'Custom', mood: customAccent.toUpperCase(), palette: deriveCustomTheme(customAccent)[mode] }
  }
  const theme = findTheme(choice)
  return { name: theme.name, mood: theme.mood, palette: theme[mode] }
}

export function AppearanceSection() {
  const mode = useUi((s) => s.mode)
  const setMode = useUi((s) => s.setMode)
  const resolved = useResolvedMode()

  return (
    <Section title="Appearance">
      <Summary />
      <Row label="Mode" description={modeHelp(mode, resolved)}>
        <Segmented label="Mode" value={mode} options={modeOptions} onChange={setMode} />
      </Row>
      <div className="border-b border-line px-5 py-4">
        <p className="text-[13.5px] font-medium tracking-[-0.005em]">Theme</p>
        <p className="mt-0.5 text-[13px] leading-relaxed text-muted">
          Every theme has a light and a dark version. Previews show {resolved}.
        </p>
        <ThemePicker />
      </div>
      <ZoomRow />
    </Section>
  )
}

/** The look on screen right now: a preview of the app plus the choices behind it. */
function Summary() {
  const mode = useUi((s) => s.mode)
  const theme = useUi((s) => s.theme)
  const customAccent = useUi((s) => s.customAccent)
  const zoom = useUi((s) => s.zoom)
  const resolved = useResolvedMode()
  const { name, mood, palette } = describe(theme, customAccent, resolved)
  const modeText = mode === 'system' ? `${capitalize(resolved)} (system)` : capitalize(resolved)

  return (
    <div className="flex items-center gap-4 border-b border-line px-5 py-4">
      <span className="relative block h-[60px] w-24 shrink-0 overflow-hidden rounded-lg">
        <MiniWindow palette={palette} mode={resolved} />
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-[14px] font-medium tracking-[-0.01em]">
          {name}
          <span className={cx('font-normal text-muted', theme === CUSTOM_THEME && 'font-mono text-[12.5px]')}> · {mood}</span>
        </p>
        <p className="mt-0.5 text-[12.5px] text-muted">
          {modeText} · {formatZoom(zoom)} zoom
        </p>
      </div>
      <ul aria-label="Colours" className="hidden shrink-0 items-center gap-1.5 sm:flex">
        {(
          [
            ['Canvas', palette.bg],
            ['Surface', palette.surface],
            ['Text', palette.fg],
            ['Accent', palette.accent],
          ] as const
        ).map(([label, color]) => (
          <li key={label} title={`${label} ${color.toUpperCase()}`}>
            <span
              className="block size-5 rounded-full"
              style={{ backgroundColor: color, boxShadow: 'inset 0 0 0 1px var(--color-line-strong)' }}
            />
            <span className="sr-only">
              {label} {color}
            </span>
          </li>
        ))}
      </ul>
    </div>
  )
}

const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)

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

function ThemePicker() {
  const theme = useUi((s) => s.theme)
  const setTheme = useUi((s) => s.setTheme)
  const customAccent = useUi((s) => s.customAccent)
  const resolved = useResolvedMode()
  const { onKeyDown, setRef } = useRadioKeys(choices.length, (i) => {
    const target = choices[i]
    if (target) setTheme(target)
  })

  return (
    <div className="@container mt-4">
      <div role="radiogroup" aria-label="Theme" className="grid grid-cols-2 gap-2.5 @min-[34rem]:grid-cols-3">
        {choices.map((choice, i) => {
          const { name, mood, palette } = describe(choice, customAccent, resolved)
          const selected = choice === theme
          const custom = choice === CUSTOM_THEME
          return (
            <button
              key={choice}
              ref={setRef(i)}
              type="button"
              role="radio"
              aria-checked={selected}
              aria-label={custom ? 'Custom, your own accent colour' : `${name}, ${mood}`}
              tabIndex={selected ? 0 : -1}
              onClick={() => setTheme(choice)}
              onKeyDown={(e) => onKeyDown(e, i)}
              className={cardClass(selected)}
              style={selected ? ringStyle : undefined}
            >
              <span className="relative block h-[76px] overflow-hidden rounded-lg">
                <MiniWindow palette={palette} mode={resolved} />
                <CheckBadge selected={selected} style={{ backgroundColor: palette.accent, color: palette.onAccent }} />
              </span>
              <span className="flex items-center justify-between gap-2 px-1 pb-0.5 pt-2">
                <span className="min-w-0">
                  <span className="block truncate text-[13px] font-medium leading-tight tracking-[-0.005em] text-fg">
                    {name}
                  </span>
                  <span className={cx('mt-0.5 block truncate text-[11.5px] leading-tight text-subtle', custom && 'font-mono')}>
                    {mood}
                  </span>
                </span>
                <span aria-hidden="true" className="flex shrink-0 -space-x-1">
                  {[palette.bg, palette.fg, palette.accent].map((color, n) => (
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
      {theme === CUSTOM_THEME ? <CustomAccent /> : null}
    </div>
  )
}

/**
 * Same border width in every state, so nothing moves: selection adds a 1px
 * shadow outside the accent border, hover only darkens the border.
 */
const cardClass = (selected: boolean) =>
  cx(
    'group relative rounded-xl border bg-surface p-1.5 text-left transition-[border-color,box-shadow] duration-200 ease-out-quint',
    !selected && 'border-line hover:border-line-strong',
  )

const ringStyle: CSSProperties = {
  borderColor: 'var(--color-accent)',
  boxShadow: '0 0 0 1px var(--color-accent)',
}

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

/**
 * The custom theme's accent: a colour picker, a hex field and a few starting
 * points. A valid hex applies as you type; anything else waits, with a note,
 * and the theme keeps the last good colour.
 */
function CustomAccent() {
  const accent = useUi((s) => s.customAccent)
  const setAccent = useUi((s) => s.setCustomAccent)
  // What the hex field shows while it differs from the saved accent; null shows the accent.
  const [draft, setDraft] = useState<string | null>(null)
  const [checked, setChecked] = useState(false)
  const id = useId()

  const invalid = draft !== null && normalizeHex(draft) === null
  const showError = invalid && checked
  const derived = deriveCustomTheme(accent)
  const adjusted = (['light', 'dark'] as const).filter((m) => derived[m].accent !== accent)

  const pick = (hex: string) => {
    setAccent(hex)
    setDraft(null)
    setChecked(false)
  }

  const type = (value: string) => {
    setDraft(value)
    setChecked(false)
    if (normalizeHex(value)) setAccent(value)
  }

  // On leaving the field: tidy a valid entry to #RRGGBB, flag an invalid one.
  const commit = () => {
    if (draft === null) return
    if (normalizeHex(draft)) setDraft(null)
    else setChecked(true)
  }

  return (
    <div className="mt-3 animate-pop rounded-xl border border-line bg-fg/[0.02] p-4 [--pop-from:-2px]">
      <div className="flex flex-wrap items-start gap-x-6 gap-y-3">
        <div>
          <label htmlFor={`${id}-hex`} className="block text-[12.5px] font-medium">
            Accent colour
          </label>
          <div className="mt-2 flex items-center gap-2">
            <input
              type="color"
              aria-label="Pick accent colour"
              value={accent}
              onChange={(e) => pick(e.target.value)}
              className="size-8 shrink-0 cursor-pointer appearance-none overflow-hidden rounded-lg border border-line-strong bg-transparent p-0 [&::-moz-color-swatch]:border-0 [&::-webkit-color-swatch-wrapper]:p-0 [&::-webkit-color-swatch]:border-0"
            />
            <input
              id={`${id}-hex`}
              type="text"
              inputMode="text"
              spellCheck={false}
              autoComplete="off"
              maxLength={7}
              value={draft ?? accent.toUpperCase()}
              onChange={(e) => type(e.target.value)}
              onBlur={commit}
              onKeyDown={(e) => {
                if (e.key === 'Enter') commit()
              }}
              aria-invalid={showError || undefined}
              aria-describedby={`${id}-note`}
              className={cx(
                'h-8 w-[6.5rem] rounded-lg border bg-surface px-2.5 font-mono text-[12.5px] uppercase tabular-nums text-fg outline-none transition-colors placeholder:text-subtle focus-visible:border-accent',
                showError ? 'border-fg/60' : 'border-line-strong',
              )}
            />
          </div>
        </div>
        <div>
          <p className="text-[12.5px] font-medium">Suggestions</p>
          <div className="mt-2 flex h-8 flex-wrap items-center gap-1.5">
            {ACCENT_SUGGESTIONS.map((hex) => {
              const current = hex === accent
              return (
                <button
                  key={hex}
                  type="button"
                  aria-label={`Use ${hex.toUpperCase()}`}
                  aria-pressed={current}
                  title={hex.toUpperCase()}
                  onClick={() => pick(hex)}
                  className="grid size-6 place-items-center rounded-full transition-transform duration-150 ease-out-quint hover:scale-110"
                  style={{
                    backgroundColor: hex,
                    boxShadow: current ? `0 0 0 2px var(--color-surface), 0 0 0 3.5px ${hex}` : 'inset 0 0 0 1px var(--color-line)',
                  }}
                >
                  {current ? <Check aria-hidden="true" className="size-3" strokeWidth={3} style={{ color: markOn(hex) }} /> : null}
                </button>
              )
            })}
          </div>
        </div>
      </div>
      <p id={`${id}-note`} role={showError ? 'alert' : undefined} className="mt-3 text-[12.5px] leading-relaxed text-muted">
        {showError ? (
          <span className="font-medium text-fg">Use a hex colour like #3A6FF0 or #36F. The theme keeps {accent.toUpperCase()} until then.</span>
        ) : adjusted.length > 0 ? (
          `Light and dark versions are derived from this colour. It is ${adjusted.length === 2 ? 'adjusted in both modes' : `${adjusted[0] === 'light' ? 'darkened in light' : 'lightened in dark'} mode`} so text and icons in it stay readable.`
        ) : (
          'Light and dark versions are derived from this colour, with text on it in whichever of white or dark reads best.'
        )}
      </p>
    </div>
  )
}

/** White or near-black, whichever is easier to see on a swatch. */
const markOn = (hex: string) => (contrast('#ffffff', hex) >= contrast('#111111', hex) ? '#ffffff' : '#111111')

function ZoomRow() {
  const zoom = useUi((s) => s.zoom)
  const setZoom = useUi((s) => s.setZoom)
  const mod = modKey()
  const min = ZOOM_LEVELS[0]
  const max = ZOOM_LEVELS.at(-1) ?? DEFAULT_ZOOM

  return (
    <Row
      label="Interface zoom"
      description={
        <>
          Text and controls on this computer. {mod} + and {mod} − also zoom, {mod} 0 resets, and so does Ctrl with the
          scroll wheel.
        </>
      }
    >
      <div role="group" aria-label="Interface zoom" className="inline-flex items-center rounded-full border border-line p-0.5">
        <StepButton label="Zoom out" disabled={zoom <= min} onClick={() => setZoom(stepZoom(zoom, -1))}>
          <Minus className="size-3.5" strokeWidth={2} />
        </StepButton>
        <output aria-live="polite" className="w-12 text-center font-mono text-[12px] tabular-nums text-fg">
          {formatZoom(zoom)}
        </output>
        <StepButton label="Zoom in" disabled={zoom >= max} onClick={() => setZoom(stepZoom(zoom, 1))}>
          <Plus className="size-3.5" strokeWidth={2} />
        </StepButton>
      </div>
      <Button
        size="sm"
        variant="ghost"
        icon={<RotateCcw className="size-3.5" strokeWidth={2} />}
        disabled={zoom === DEFAULT_ZOOM}
        onClick={() => setZoom(DEFAULT_ZOOM)}
      >
        Reset
      </Button>
    </Row>
  )
}

function StepButton({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string
  disabled: boolean
  onClick: () => void
  children: ReactNode
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="grid size-7 place-items-center rounded-full text-muted transition-colors duration-200 enabled:hover:bg-fg/[0.06] enabled:hover:text-fg disabled:opacity-35"
    >
      {children}
    </button>
  )
}

const WAVE = [0.45, 0.8, 1, 0.6, 0.85, 0.4, 0.65]

/**
 * A tiny Talkr window drawn in one palette, laid out like the real one: the
 * sidebar with the current page highlighted and the status card at the foot,
 * then a page title and a card with text, a waveform and a button. Derived
 * colours are mixed the same way globals.css mixes them.
 */
function MiniWindow({ palette, mode }: { palette: Palette; mode: Mode }) {
  const { bg, surface, fg, accent } = palette
  const dark = mode === 'dark'
  const mix = (percent: number, base = 'transparent') => `color-mix(in oklab, ${fg} ${percent}%, ${base})`
  const line = mix(dark ? 10 : 12)
  const nav = mix(dark ? 30 : 26)

  return (
    <span aria-hidden="true" className="absolute inset-0 flex" style={{ backgroundColor: bg }}>
      <span
        className="flex w-[26%] shrink-0 flex-col gap-[4px] px-1.5 py-2"
        style={{ backgroundColor: mix(dark ? 2.5 : 3.5, bg), borderRight: `1px solid ${line}` }}
      >
        <span className="mb-1 flex items-center gap-[3px] px-0.5">
          <span className="size-[5px] rounded-[1.5px]" style={{ backgroundColor: fg }} />
          <span className="h-[3px] w-1/2 rounded-full" style={{ backgroundColor: fg }} />
        </span>
        <span className="flex h-[7px] items-center gap-[3px] rounded-[3px] px-1" style={{ backgroundColor: mix(dark ? 9 : 8) }}>
          <span className="size-[3px] shrink-0 rounded-full" style={{ backgroundColor: accent }} />
          <span className="h-[3px] w-3/5 rounded-full" style={{ backgroundColor: mix(dark ? 85 : 80) }} />
        </span>
        {[0.55, 0.65, 0.5].map((w, n) => (
          <span key={n} className="flex h-[7px] items-center gap-[3px] px-1">
            <span className="size-[3px] shrink-0 rounded-full" style={{ backgroundColor: nav }} />
            <span className="h-[3px] rounded-full" style={{ width: `${w * 100}%`, backgroundColor: nav }} />
          </span>
        ))}
        <span className="mt-auto h-[11px] rounded-[3px]" style={{ border: `1px solid ${line}` }} />
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
