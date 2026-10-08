import { memo, useId, useRef, useState } from 'react'
import type { KeyboardEvent, PointerEvent as ReactPointerEvent, ReactNode } from 'react'
import { Check, Minus, Monitor, Moon, Plus, RotateCcw, Sparkles, Sun } from 'lucide-react'
import { Button } from '@/components/ui'
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
import { DEFAULT_ZOOM, formatZoom, normalizeZoom, stepZoom, ZOOM_LEVELS } from '@/lib/zoom'
import { useUi, type ColorMode } from '@/stores/ui'
import { Section } from '@/features/settings/controls'
import { AppPreview } from '@/features/settings/AppPreview'
import { paletteVars } from '@/features/settings/paletteVars'
import '@/features/settings/appearance.css'

type Mode = 'light' | 'dark'

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

const capitalize = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)

/**
 * Settings > Appearance. A live preview of the whole window in the chosen look,
 * then the three choices that make it: mode, theme and zoom. Pointing at a
 * theme previews it on the stage before it is picked.
 */
export function AppearanceSection() {
  // The theme under the pointer, shown on the stage without applying it.
  const [peek, setPeek] = useState<ThemeChoice | null>(null)

  return (
    <Section title="Appearance">
      <Stage peek={peek} />
      <ModePicker />
      <ThemePicker onPeek={setPeek} />
      <ZoomControl />
    </Section>
  )
}

/** The look on screen right now: a large preview of the app plus the choices behind it. */
function Stage({ peek }: { peek: ThemeChoice | null }) {
  const mode = useUi((s) => s.mode)
  const theme = useUi((s) => s.theme)
  const customAccent = useUi((s) => s.customAccent)
  const zoom = useUi((s) => s.zoom)
  const resolved = useResolvedMode()
  const current = describe(theme, customAccent, resolved)
  const peeking = peek !== null && peek !== theme
  const shown = peeking ? describe(peek, customAccent, resolved) : current
  const modeText = mode === 'system' ? `${capitalize(resolved)} (system)` : capitalize(resolved)

  return (
    <div className="border-b border-line">
      <div
        className="ap-palette ap-stage rounded-t-2xl px-6 pb-7 pt-5 sm:px-10"
        style={paletteVars(shown.palette, resolved)}
      >
        <div className="mb-5 flex items-center justify-between gap-3">
          <span
            className="inline-flex h-7 items-center gap-2 rounded-full border border-[var(--p-line)] px-3 text-[11.5px] font-medium text-[var(--p-muted)] backdrop-blur-md"
            style={{ backgroundColor: 'color-mix(in oklab, var(--p-surface) 70%, transparent)' }}
          >
            <span className="relative flex size-1.5">
              <span className="absolute inset-0 animate-ping rounded-full bg-[var(--p-accent)] opacity-60 [animation-duration:2s]" />
              <span className="relative size-1.5 rounded-full bg-[var(--p-accent)]" />
            </span>
            {peeking ? `Previewing ${shown.name}` : 'Live preview'}
          </span>
          <span className="font-mono text-[10.5px] uppercase tracking-[0.16em] text-[var(--p-subtle)]">{resolved}</span>
        </div>
        <div className="mx-auto max-w-[600px]">
          <AppPreview palette={shown.palette} mode={resolved} />
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-6 gap-y-3 px-5 py-4">
        <div className="min-w-0 flex-1">
          <p className="truncate text-[15px] font-semibold tracking-[-0.015em]">
            {current.name}
            <span className={cx('font-normal text-muted', theme === CUSTOM_THEME && 'font-mono text-[13px]')}> · {current.mood}</span>
          </p>
          <p className="mt-0.5 text-[12.5px] text-muted">
            {modeText} · {formatZoom(zoom)} zoom
          </p>
        </div>
        <ul aria-label="Colours" className="flex shrink-0 items-center gap-2">
          {(
            [
              ['Canvas', current.palette.bg],
              ['Surface', current.palette.surface],
              ['Text', current.palette.fg],
              ['Accent', current.palette.accent],
            ] as const
          ).map(([label, color]) => (
            <li
              key={label}
              title={`${label} ${color.toUpperCase()}`}
              className="flex items-center gap-1.5 rounded-full border border-line py-1 pl-1 pr-2.5"
            >
              <span
                className="block size-4 rounded-full"
                style={{
                  backgroundColor: color,
                  boxShadow:
                    label === 'Accent'
                      ? `inset 0 0 0 1px var(--color-line-strong), 0 0 10px -1px ${color}`
                      : 'inset 0 0 0 1px var(--color-line-strong)',
                }}
              />
              <span className="font-mono text-[10.5px] uppercase text-subtle">
                <span className="sr-only">{label} </span>
                {color.replace('#', '')}
              </span>
            </li>
          ))}
        </ul>
      </div>
    </div>
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

/** Heading of one of the choices below the stage. */
function Heading({ id, title, children, aside }: { id?: string; title: string; children: ReactNode; aside?: ReactNode }) {
  return (
    <div className="mb-4 flex items-end justify-between gap-4">
      <div className="min-w-0">
        <p id={id} className="text-[13.5px] font-medium tracking-[-0.005em]">
          {title}
        </p>
        <p className="mt-0.5 text-[13px] leading-relaxed text-muted">{children}</p>
      </div>
      {aside}
    </div>
  )
}

const modes: { value: ColorMode; label: string; detail: string; icon: typeof Sun }[] = [
  { value: 'system', label: 'System', detail: 'Matches your computer', icon: Monitor },
  { value: 'light', label: 'Light', detail: 'Bright and crisp', icon: Sun },
  { value: 'dark', label: 'Dark', detail: 'Easy on the eyes', icon: Moon },
]

const GAP = 12

/**
 * Three tiles, each showing the current theme in that mode; System is split
 * down the diagonal. The selection ring springs from tile to tile.
 */
const ModePicker = memo(function ModePicker() {
  const mode = useUi((s) => s.mode)
  const setMode = useUi((s) => s.setMode)
  const theme = useUi((s) => s.theme)
  const customAccent = useUi((s) => s.customAccent)
  const resolved = useResolvedMode()
  const index = Math.max(
    0,
    modes.findIndex((m) => m.value === mode),
  )
  const { onKeyDown, setRef } = useRadioKeys(modes.length, (i) => {
    const target = modes[i]
    if (target) setMode(target.value)
  })
  const light = describe(theme, customAccent, 'light').palette
  const dark = describe(theme, customAccent, 'dark').palette

  return (
    <div className="border-b border-line px-5 py-5">
      <Heading title="Mode">
        {modeHelp(mode, resolved)}
      </Heading>
      <div role="radiogroup" aria-label="Mode" className="relative grid grid-cols-3" style={{ gap: GAP }}>
        <span
          aria-hidden="true"
          className="ap-mode-ring pointer-events-none absolute inset-y-0 left-0 z-10 rounded-[14px]"
          style={{
            width: `calc((100% - ${GAP * 2}px) / 3)`,
            transform: `translateX(calc(${index} * (100% + ${GAP}px)))`,
          }}
        />
        {modes.map((m, i) => {
          const selected = m.value === mode
          const Icon = m.icon
          return (
            <button
              key={m.value}
              ref={setRef(i)}
              type="button"
              role="radio"
              aria-checked={selected}
              tabIndex={selected ? 0 : -1}
              onClick={() => setMode(m.value)}
              onKeyDown={(e) => onKeyDown(e, i)}
              className={cx(
                'group relative overflow-hidden rounded-[14px] border bg-surface p-1.5 text-left transition-[border-color,transform] duration-200 ease-out-quint active:scale-[0.98]',
                selected ? 'border-transparent' : 'border-line hover:border-line-strong',
              )}
            >
              <span className="relative block h-[74px] overflow-hidden rounded-[9px]">
                {m.value === 'system' ? (
                  <>
                    <MiniWindow palette={light} mode="light" />
                    <span className="absolute inset-0 [clip-path:polygon(62%_0,100%_0,100%_100%,38%_100%)]">
                      <MiniWindow palette={dark} mode="dark" />
                    </span>
                  </>
                ) : (
                  <MiniWindow palette={m.value === 'light' ? light : dark} mode={m.value} />
                )}
              </span>
              <span className="flex items-center gap-2.5 px-1.5 pb-1 pt-2.5">
                <span
                  aria-hidden="true"
                  className={cx(
                    'grid size-6 shrink-0 place-items-center rounded-[7px] transition-[background-color,color,transform] duration-300',
                    selected ? 'scale-105 bg-accent text-on-accent' : 'bg-fg/[0.06] text-muted group-hover:text-fg',
                  )}
                >
                  <Icon className="size-3.5" strokeWidth={2} />
                </span>
                <span className="min-w-0">
                  <span className="block text-[13px] font-medium leading-tight text-fg">{m.label}</span>
                  <span className="mt-0.5 block truncate text-[11.5px] leading-tight text-muted">{m.detail}</span>
                </span>
              </span>
            </button>
          )
        })}
      </div>
    </div>
  )
})

const ThemePicker = memo(function ThemePicker({ onPeek }: { onPeek: (choice: ThemeChoice | null) => void }) {
  const theme = useUi((s) => s.theme)
  const setTheme = useUi((s) => s.setTheme)
  const customAccent = useUi((s) => s.customAccent)
  const resolved = useResolvedMode()
  const { onKeyDown, setRef } = useRadioKeys(choices.length, (i) => {
    const target = choices[i]
    if (target) setTheme(target)
  })

  return (
    <div className="border-b border-line px-5 py-5">
      <Heading title="Theme">
        Every theme has a light and a dark version. Previews show {resolved}. Point at one to try it on the preview above.
      </Heading>
      <div className="@container">
        <div
          role="radiogroup"
          aria-label="Theme"
          onMouseLeave={() => onPeek(null)}
          className="grid grid-cols-2 gap-3 @min-[34rem]:grid-cols-3"
        >
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
                onMouseEnter={() => onPeek(choice)}
                data-theme={choice}
                className="ap-palette ap-swatch relative isolate flex h-[118px] flex-col justify-between overflow-hidden rounded-[14px] p-3.5 text-left"
                style={{ ...paletteVars(palette, resolved), backgroundColor: 'var(--p-bg)' }}
              >
                <span aria-hidden="true" className={cx('ap-orb absolute -bottom-14 -right-12 -z-10 size-32 rounded-full', custom && 'ap-orb-custom')} />
                <span aria-hidden="true" className="flex items-end gap-2.5">
                  <span className="text-[22px] font-semibold leading-none tracking-[-0.03em] text-[var(--p-fg)]">Aa</span>
                  <span className="flex h-[18px] items-end gap-[3px]">
                    {[0.45, 0.85, 0.6, 1, 0.7, 0.4].map((h, n) => (
                      <span
                        key={n}
                        className="ap-bar w-[3px] rounded-full bg-[var(--p-accent)]"
                        style={{ height: `${h * 100}%`, transitionDelay: `${n * 30}ms` }}
                      />
                    ))}
                  </span>
                </span>
                <span
                  aria-hidden="true"
                  className={cx(
                    'ap-check absolute right-3 top-3 grid size-5 place-items-center rounded-full bg-[var(--p-accent)] text-[var(--p-on-accent)] shadow-[0_2px_8px_-2px_var(--p-glow)]',
                    selected ? 'scale-100 opacity-100' : 'scale-50 opacity-0',
                  )}
                >
                  <Check className="size-3" strokeWidth={3.25} />
                </span>
                <span className="flex items-end justify-between gap-2">
                  <span className="min-w-0">
                    <span className="block truncate text-[13px] font-semibold leading-tight tracking-[-0.01em] text-[var(--p-fg)]">
                      {name}
                    </span>
                    <span
                      className={cx(
                        'mt-0.5 block truncate text-[11.5px] leading-tight text-[var(--p-muted)]',
                        custom && 'font-mono',
                      )}
                    >
                      {mood}
                    </span>
                  </span>
                  <span aria-hidden="true" className="flex shrink-0 -space-x-1">
                    {[palette.surface, palette.fg, palette.accent].map((color, n) => (
                      <span
                        key={n}
                        className="size-3 rounded-full"
                        style={{ backgroundColor: color, boxShadow: '0 0 0 1.5px var(--p-bg), inset 0 0 0 1px var(--p-line-strong)' }}
                      />
                    ))}
                  </span>
                </span>
              </button>
            )
          })}
        </div>
      </div>
      {theme === CUSTOM_THEME ? <CustomAccent /> : null}
    </div>
  )
})

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
    <div className="mt-4 animate-pop overflow-hidden rounded-[14px] border border-line bg-fg/[0.02] [--pop-from:-2px]">
      <div className="flex flex-wrap items-start gap-x-8 gap-y-4 p-4">
        <div>
          <label htmlFor={`${id}-hex`} className="block text-[12.5px] font-medium">
            Accent colour
          </label>
          <div className="mt-2 flex items-center gap-2">
            <span
              className="relative grid size-9 shrink-0 place-items-center rounded-[10px]"
              style={{ boxShadow: `0 0 0 1px var(--color-line-strong), 0 6px 18px -8px ${accent}` }}
            >
              <input
                type="color"
                aria-label="Pick accent colour"
                value={accent}
                onChange={(e) => pick(e.target.value)}
                className="size-9 shrink-0 cursor-pointer appearance-none overflow-hidden rounded-[10px] border-0 bg-transparent p-0 [&::-moz-color-swatch]:border-0 [&::-webkit-color-swatch-wrapper]:p-0 [&::-webkit-color-swatch]:border-0"
              />
              <Sparkles
                aria-hidden="true"
                className="pointer-events-none absolute size-3.5"
                strokeWidth={2}
                style={{ color: markOn(accent) }}
              />
            </span>
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
                'h-9 w-[6.75rem] rounded-[10px] border bg-surface px-3 font-mono text-[12.5px] uppercase tabular-nums text-fg outline-none transition-[border-color,box-shadow] placeholder:text-subtle focus-visible:border-accent focus-visible:shadow-[0_0_0_3px_color-mix(in_oklab,var(--color-accent)_20%,transparent)]',
                showError ? 'border-fg/60' : 'border-line-strong',
              )}
            />
          </div>
        </div>
        <div>
          <p className="text-[12.5px] font-medium">Suggestions</p>
          <div className="mt-2 flex h-9 flex-wrap items-center gap-2">
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
                  className="ap-spring grid size-7 place-items-center rounded-full transition-[transform,box-shadow] duration-300 hover:scale-115 active:scale-95"
                  style={{
                    backgroundColor: hex,
                    boxShadow: current
                      ? `0 0 0 2px var(--color-surface), 0 0 0 3.5px ${hex}, 0 4px 14px -4px ${hex}`
                      : 'inset 0 0 0 1px var(--color-line)',
                  }}
                >
                  {current ? <Check aria-hidden="true" className="size-3.5" strokeWidth={3} style={{ color: markOn(hex) }} /> : null}
                </button>
              )
            })}
          </div>
        </div>
        {/* Both derived versions side by side, so the adjustment for each mode is visible. */}
        <div aria-hidden="true" className="ml-auto hidden items-center gap-2 md:flex">
          {(['light', 'dark'] as const).map((m) => (
            <span key={m} className="flex flex-col items-center gap-1.5">
              <span
                className="grid h-9 w-14 place-items-center rounded-[10px]"
                style={{ backgroundColor: derived[m].bg, boxShadow: 'inset 0 0 0 1px var(--color-line-strong)' }}
              >
                <span
                  className="rounded-full px-2 py-0.5 text-[10px] font-semibold"
                  style={{ backgroundColor: derived[m].accent, color: derived[m].onAccent }}
                >
                  Aa
                </span>
              </span>
              <span className="font-mono text-[10px] uppercase tracking-[0.12em] text-subtle">{m}</span>
            </span>
          ))}
        </div>
      </div>
      <p
        id={`${id}-note`}
        role={showError ? 'alert' : undefined}
        className="border-t border-line bg-fg/[0.015] px-4 py-3 text-[12.5px] leading-relaxed text-muted"
      >
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

const MIN_ZOOM = ZOOM_LEVELS[0]
const MAX_ZOOM = ZOOM_LEVELS.at(-1) ?? DEFAULT_ZOOM
/** Where a zoom level sits on the track, 0..1. Steps are even, so this is linear. */
const position = (level: number) => (level - MIN_ZOOM) / (MAX_ZOOM - MIN_ZOOM)

/**
 * Interface zoom: a stepped slider with a stop for every level, the − and +
 * buttons beside it, and a reset. The sample letters grow with the level.
 */
const ZoomControl = memo(function ZoomControl() {
  const zoom = useUi((s) => s.zoom)
  const setZoom = useUi((s) => s.setZoom)
  const mod = modKey()
  const [dragging, setDragging] = useState(false)

  const fromPointer = (e: ReactPointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect()
    if (rect.width === 0) return
    const ratio = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width))
    setZoom(normalizeZoom(MIN_ZOOM + ratio * (MAX_ZOOM - MIN_ZOOM)))
  }

  const onSliderKey = (e: KeyboardEvent) => {
    const keys: Record<string, () => number> = {
      ArrowRight: () => stepZoom(zoom, 1),
      ArrowUp: () => stepZoom(zoom, 1),
      ArrowLeft: () => stepZoom(zoom, -1),
      ArrowDown: () => stepZoom(zoom, -1),
      PageUp: () => stepZoom(zoom, 1),
      PageDown: () => stepZoom(zoom, -1),
      Home: () => MIN_ZOOM,
      End: () => MAX_ZOOM,
    }
    const next = keys[e.key]
    if (!next) return
    e.preventDefault()
    setZoom(next())
  }

  const at = position(zoom)

  return (
    <div className="px-5 py-5">
      <Heading
        title="Interface zoom"
        aside={
          <Button
            size="sm"
            variant="ghost"
            icon={<RotateCcw className="size-3.5" strokeWidth={2} />}
            disabled={zoom === DEFAULT_ZOOM}
            onClick={() => setZoom(DEFAULT_ZOOM)}
          >
            Reset
          </Button>
        }
      >
        Text and controls on this computer. {mod} + and {mod} − also zoom, {mod} 0 resets, and so does Ctrl with the scroll wheel.
      </Heading>

      <div className="flex flex-wrap items-center gap-x-5 gap-y-4 rounded-[14px] border border-line bg-fg/[0.02] px-4 py-3.5">
        <div role="group" aria-label="Interface zoom" className="inline-flex items-center rounded-full border border-line bg-surface p-0.5 shadow-[var(--shadow-card)]">
          <StepButton label="Zoom out" disabled={zoom <= MIN_ZOOM} onClick={() => setZoom(stepZoom(zoom, -1))}>
            <Minus className="size-3.5" strokeWidth={2} />
          </StepButton>
          <output aria-live="polite" className="w-14 text-center font-mono text-[12.5px] font-medium tabular-nums text-fg">
            {formatZoom(zoom)}
          </output>
          <StepButton label="Zoom in" disabled={zoom >= MAX_ZOOM} onClick={() => setZoom(stepZoom(zoom, 1))}>
            <Plus className="size-3.5" strokeWidth={2} />
          </StepButton>
        </div>

        <div className="flex min-w-[14rem] flex-1 items-center gap-3">
          <span aria-hidden="true" className="text-[11px] font-semibold text-subtle">
            Aa
          </span>
          <div
            role="slider"
            tabIndex={0}
            aria-label="Zoom level"
            aria-valuemin={Math.round(MIN_ZOOM * 100)}
            aria-valuemax={Math.round(MAX_ZOOM * 100)}
            aria-valuenow={Math.round(zoom * 100)}
            aria-valuetext={formatZoom(zoom)}
            data-dragging={dragging || undefined}
            onKeyDown={onSliderKey}
            onPointerDown={(e) => {
              e.currentTarget.setPointerCapture?.(e.pointerId)
              setDragging(true)
              fromPointer(e)
            }}
            onPointerMove={(e) => {
              if (dragging) fromPointer(e)
            }}
            onPointerUp={() => setDragging(false)}
            onPointerCancel={() => setDragging(false)}
            className="ap-zoom-track group relative h-8 min-w-0 flex-1 cursor-pointer touch-none rounded-full outline-offset-4"
          >
            <span className="absolute inset-x-0 top-1/2 h-1 -translate-y-1/2 rounded-full bg-fg/10" />
            <span
              className="ap-zoom-fill absolute left-0 top-1/2 h-1 -translate-y-1/2 rounded-full bg-accent"
              style={{ width: `${at * 100}%` }}
            />
            {ZOOM_LEVELS.map((level) => {
              const p = position(level)
              return (
                <span
                  key={level}
                  aria-hidden="true"
                  className={cx(
                    'absolute top-1/2 -translate-x-1/2 -translate-y-1/2 rounded-full transition-colors duration-300',
                    // 100% is marked a little larger: the level the reset goes back to.
                    level === DEFAULT_ZOOM ? 'size-2' : 'size-1',
                    level <= zoom ? 'bg-on-accent/45' : 'bg-fg/30',
                  )}
                  style={{ left: `${p * 100}%` }}
                />
              )
            })}
            <span
              aria-hidden="true"
              className="ap-zoom-thumb absolute top-1/2 grid h-6 min-w-6 -translate-x-1/2 -translate-y-1/2 place-items-center rounded-full border border-line-strong bg-surface px-1.5 shadow-[var(--shadow-knob)] group-hover:shadow-[var(--shadow-knob),0_0_0_4px_color-mix(in_oklab,var(--color-accent)_14%,transparent)] group-focus-visible:shadow-[var(--shadow-knob),0_0_0_4px_color-mix(in_oklab,var(--color-accent)_22%,transparent)]"
              style={{ left: `${at * 100}%` }}
            >
              <span className="size-1.5 rounded-full bg-accent" />
            </span>
          </div>
          <span aria-hidden="true" className="text-[17px] font-semibold leading-none text-subtle">
            Aa
          </span>
        </div>
      </div>
    </div>
  )
})

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
      className="grid size-8 place-items-center rounded-full text-muted transition-[background-color,color,transform] duration-200 enabled:hover:bg-fg/[0.06] enabled:hover:text-fg enabled:active:scale-90 disabled:opacity-35"
    >
      {children}
    </button>
  )
}

/** Palettes are rebuilt on every render; compare them by colour. */
const samePalette = (a: { palette: Palette; mode: Mode }, b: { palette: Palette; mode: Mode }) =>
  a.mode === b.mode &&
  a.palette.bg === b.palette.bg &&
  a.palette.surface === b.palette.surface &&
  a.palette.fg === b.palette.fg &&
  a.palette.accent === b.palette.accent &&
  a.palette.onAccent === b.palette.onAccent

const WAVE = [0.45, 0.8, 1, 0.6, 0.85, 0.4, 0.65]

/**
 * A tiny Talkr window drawn in one palette, laid out like the real one: the
 * sidebar with the current page highlighted and the status card at the foot,
 * then a page title and a card with text, a waveform and a button. Derived
 * colours are mixed the same way globals.css mixes them.
 */
const MiniWindow = memo(function MiniWindow({ palette, mode }: { palette: Palette; mode: Mode }) {
  const { bg, surface, fg, accent } = palette
  const dark = mode === 'dark'
  const mix = (percent: number, base = 'transparent') => `color-mix(in oklab, ${fg} ${percent}%, ${base})`
  const line = mix(dark ? 10 : 12)
  const nav = mix(dark ? 30 : 26)

  return (
    <span aria-hidden="true" className="absolute inset-0 flex transition-colors duration-500" style={{ backgroundColor: bg }}>
      <span
        className="flex w-[26%] shrink-0 flex-col gap-[4px] px-1.5 py-2"
        style={{ backgroundColor: mix(dark ? 2.5 : 3.5, bg), borderRight: `1px solid ${line}` }}
      >
        <span className="mb-1 flex items-center gap-[3px] px-0.5">
          <span className="size-[5px] rounded-[1.5px]" style={{ backgroundColor: accent }} />
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
    </span>
  )
}, samePalette)
