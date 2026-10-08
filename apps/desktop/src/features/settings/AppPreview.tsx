import { memo } from 'react'
import { AudioLines, Boxes, ChevronDown, History, Mic, Play, Search, Settings, Volume2 } from 'lucide-react'
import type { Palette } from '@/lib/themes'
import { paletteVars } from '@/features/settings/paletteVars'

type Mode = 'light' | 'dark'

const NAV = [
  { label: 'Speak', icon: Volume2 },
  { label: 'Transcribe', icon: Mic },
  { label: 'Dictation', icon: AudioLines },
  { label: 'Models', icon: Boxes },
  { label: 'History', icon: History },
]

const RECENT = [
  ['Chapter one, read aloud', 'af_heart', '2 min ago'],
  ['Notes for Monday', 'am_echo', 'Yesterday'],
]

/** A fixed, speech-like waveform: phrases with pauses between them. */
const WAVE = [
  0.3, 0.55, 0.8, 0.62, 0.95, 0.7, 0.45, 0.6, 0.85, 0.5, 0.25, 0.12, 0.35, 0.7, 1, 0.78, 0.55, 0.82, 0.6, 0.4, 0.2, 0.1, 0.3,
  0.58, 0.74, 0.9, 0.66, 0.48, 0.7, 0.52, 0.3, 0.15, 0.4, 0.62, 0.8, 0.56, 0.36, 0.22,
]

/**
 * The Talkr window drawn small but real: the sidebar with Speak open, the
 * composer with text and a Generate button, and a player. Every colour comes
 * from the palette it is given, so it shows a theme before (or without) the app
 * switching to it. Decorative: the section describes the look in words.
 */
export const AppPreview = memo(function AppPreview({ palette, mode }: { palette: Palette; mode: Mode }) {
  return (
    <div aria-hidden="true" className="ap-window-frame w-full">
      <div
        data-mode={mode}
        className="ap-palette ap-window flex flex-col overflow-hidden text-[var(--p-fg)]"
        style={paletteVars(palette, mode)}
      >
        {/* Title bar */}
        <div className="flex h-[2.3em] shrink-0 items-center gap-[0.45em] border-b border-[var(--p-line)] bg-[var(--p-panel)] px-[0.9em]">
          {[0, 1, 2].map((n) => (
            <span key={n} className="size-[0.62em] rounded-full bg-[var(--p-line-strong)]" />
          ))}
          <span className="mx-auto pr-[3em] text-[0.78em] font-medium text-[var(--p-subtle)]">Talkr</span>
        </div>

        <div className="flex min-h-0 flex-1">
          {/* Sidebar */}
          <div className="flex w-[13.5em] shrink-0 flex-col border-r border-[var(--p-line)] bg-[var(--p-panel)] p-[0.8em]">
            <div className="mb-[0.9em] flex items-center gap-[0.5em] px-[0.3em]">
              <span className="grid size-[1.6em] place-items-center rounded-[0.45em] bg-[var(--p-accent)]">
                <span className="flex items-end gap-[0.12em]">
                  {[0.5, 1, 0.65].map((h, i) => (
                    <span
                      key={i}
                      className="w-[0.18em] rounded-full bg-[var(--p-on-accent)]"
                      style={{ height: `${h * 0.85}em` }}
                    />
                  ))}
                </span>
              </span>
              <span className="text-[0.92em] font-semibold tracking-[-0.02em]">Talkr</span>
            </div>
            <div className="mb-[0.8em] flex h-[1.9em] items-center gap-[0.45em] rounded-[0.5em] border border-[var(--p-line)] px-[0.55em] text-[0.74em] text-[var(--p-subtle)]">
              <Search className="size-[1.05em]" strokeWidth={2} />
              Search
            </div>
            <div className="space-y-[0.15em]">
              {NAV.map(({ label, icon: Icon }, i) => (
                <div
                  key={label}
                  className="flex h-[1.95em] items-center gap-[0.55em] rounded-[0.5em] px-[0.55em] text-[0.8em]"
                  style={
                    i === 0
                      ? { backgroundColor: 'var(--p-raised)', boxShadow: 'inset 0 0 0 1px var(--p-line)', fontWeight: 500 }
                      : { color: 'var(--p-muted)' }
                  }
                >
                  <Icon
                    className="size-[1.1em]"
                    strokeWidth={1.9}
                    style={i === 0 ? { color: 'var(--p-accent)' } : undefined}
                  />
                  {label}
                </div>
              ))}
            </div>
            <div className="mt-auto space-y-[0.4em]">
              <div className="rounded-[0.6em] border border-[var(--p-line)] p-[0.55em]">
                <div className="flex items-center gap-[0.4em] text-[0.68em] text-[var(--p-muted)]">
                  <span
                    className="size-[0.55em] rounded-full bg-[var(--p-accent)]"
                    style={{ boxShadow: '0 0 0.5em var(--p-glow)' }}
                  />
                  GPU · Vulkan
                </div>
                <div className="mt-[0.2em] font-mono text-[0.6em] text-[var(--p-subtle)]">Offline · on device</div>
              </div>
              <div className="flex h-[1.95em] items-center gap-[0.55em] px-[0.55em] text-[0.8em] text-[var(--p-muted)]">
                <Settings className="size-[1.1em]" strokeWidth={1.9} />
                Settings
              </div>
            </div>
          </div>

          {/* Page */}
          <div className="relative flex min-w-0 flex-1 flex-col gap-[0.9em] overflow-hidden px-[1.8em] pt-[1.5em]">
            <div
              className="pointer-events-none absolute -top-[6em] left-1/2 h-[10em] w-[30em] -translate-x-1/2 rounded-full opacity-60 blur-[2.5em]"
              style={{ background: 'var(--p-soft)' }}
            />
            <div className="relative">
              <div className="text-[1.75em] font-semibold leading-none tracking-[-0.035em]">Speak</div>
              <div className="mt-[0.5em] text-[0.8em] text-[var(--p-muted)]">Turn text into natural speech, on this computer.</div>
            </div>

            <div
              className="relative rounded-[0.85em] border border-[var(--p-line)] bg-[var(--p-surface)]"
              style={{ boxShadow: '0 0 0 0.22em var(--p-soft)', borderColor: 'color-mix(in oklab, var(--p-accent) 55%, transparent)' }}
            >
              <div className="px-[1.1em] pt-[0.9em] text-[0.95em] leading-[1.5]">
                Every word you type here is read aloud by a voice that lives on this computer
                <span className="ap-caret ml-[0.1em] inline-block h-[1.05em] w-[0.12em] translate-y-[0.18em] bg-[var(--p-accent)]" />
              </div>
              <div className="flex items-center gap-[0.5em] px-[0.8em] pb-[0.75em] pt-[0.9em]">
                <span className="flex h-[1.9em] items-center gap-[0.35em] rounded-full border border-[var(--p-line)] px-[0.7em] text-[0.7em]">
                  <span className="text-[var(--p-subtle)]">Voice</span>
                  <span className="font-medium">Heart</span>
                  <ChevronDown className="size-[1em] text-[var(--p-subtle)]" strokeWidth={2} />
                </span>
                <span className="font-mono text-[0.62em] text-[var(--p-subtle)]">1.00×</span>
                <span
                  className="ml-auto flex h-[2.1em] items-center gap-[0.4em] rounded-full px-[0.95em] text-[0.74em] font-medium"
                  style={{
                    backgroundColor: 'var(--p-accent)',
                    color: 'var(--p-on-accent)',
                    boxShadow: '0 0.4em 1.2em -0.4em var(--p-glow), inset 0 1px 0 rgb(255 255 255 / 0.18)',
                  }}
                >
                  <AudioLines className="size-[1.15em]" strokeWidth={2.2} />
                  Generate
                </span>
              </div>
            </div>

            <div className="relative flex items-center gap-[0.9em] rounded-[0.85em] border border-[var(--p-line)] bg-[var(--p-surface)] px-[0.9em] py-[0.75em]">
              <span
                className="grid size-[2.3em] shrink-0 place-items-center rounded-full"
                style={{ backgroundColor: 'var(--p-accent)', color: 'var(--p-on-accent)', boxShadow: '0 0.3em 1em -0.3em var(--p-glow)' }}
              >
                <Play className="ml-[0.1em] size-[0.95em]" fill="currentColor" strokeWidth={0} />
              </span>
              <span className="flex h-[2.2em] min-w-0 flex-1 items-center gap-[0.2em]">
                {WAVE.map((h, i) => (
                  <span
                    key={i}
                    className="min-w-0 flex-1 rounded-full"
                    style={{
                      height: `${Math.max(12, h * 100)}%`,
                      backgroundColor: i < 23 ? 'var(--p-accent)' : 'var(--p-line-strong)',
                    }}
                  />
                ))}
              </span>
              <span className="shrink-0 font-mono text-[0.62em] tabular-nums text-[var(--p-subtle)]">0:07 / 0:12</span>
            </div>

            <div className="relative">
              <div className="mb-[0.5em] px-[0.2em] font-mono text-[0.58em] uppercase tracking-[0.16em] text-[var(--p-subtle)]">Recent</div>
              <div className="overflow-hidden rounded-[0.85em] border border-[var(--p-line)]">
                {RECENT.map(([title, voice, time], i) => (
                  <div
                    key={title}
                    className="flex items-center gap-[0.7em] px-[0.9em] py-[0.55em] text-[0.74em]"
                    style={i > 0 ? { borderTop: '1px solid var(--p-line)' } : undefined}
                  >
                    <span className="grid size-[1.7em] shrink-0 place-items-center rounded-full border border-[var(--p-line)] text-[var(--p-subtle)]">
                      <Play className="ml-[0.08em] size-[0.75em]" fill="currentColor" strokeWidth={0} />
                    </span>
                    <span className="min-w-0 flex-1 truncate">{title}</span>
                    <span className="font-mono text-[0.85em] text-[var(--p-subtle)]">{voice}</span>
                    <span className="w-[5.5em] whitespace-nowrap text-right text-[0.9em] text-[var(--p-subtle)]">{time}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}, (a, b) => a.mode === b.mode && JSON.stringify(a.palette) === JSON.stringify(b.palette))
