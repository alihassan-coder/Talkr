import type { Shortcut } from '@/lib/types'
import { cx } from '@/lib/cx'
import { shortcutKeys, spokenKeys } from '@/features/dictation/shortcut'
import { useKeyPlatform } from '@/features/dictation/platform'

type Size = 'sm' | 'md' | 'lg'

const sizes: Record<Size, string> = {
  sm: 'h-[22px] min-w-[22px] px-1.5 text-[11px] rounded-[6px]',
  md: 'h-6 min-w-6 px-1.5 text-[11.5px] rounded-[7px]',
  lg: 'h-9 min-w-9 px-3 text-[14px] rounded-[10px]',
}

/** One key, drawn as a physical keycap. `pressed` sinks it and lights it with the accent. */
export function Keycap({ label, size = 'md', pressed = false }: { label: string; size?: Size; pressed?: boolean }) {
  // Symbols (⌘ ⌥ ⇧ ⌃ arrows) read better a touch larger than words.
  const symbol = [...label].length === 1 && !/[A-Za-z0-9]/.test(label)
  return (
    <kbd
      className={cx(
        'dict-keycap inline-flex items-center justify-center border font-sans font-medium leading-none tracking-[-0.01em] transition-[transform,box-shadow,border-color,background-color,color] duration-150 ease-out',
        sizes[size],
        symbol && (size === 'lg' ? 'text-[17px]' : 'text-[13px]'),
        pressed ? 'dict-keycap-pressed border-accent/70 text-fg' : 'border-line-strong text-fg',
      )}
    >
      {label}
    </kbd>
  )
}

/** The keys of a shortcut as keycaps, in the system's own order and names. */
export function Keys({
  shortcut,
  size = 'md',
  pressed = false,
  className,
}: {
  shortcut: Shortcut
  size?: Size
  pressed?: boolean
  className?: string
}) {
  const platform = useKeyPlatform()
  const keys = shortcutKeys(shortcut, platform)
  const joined = platform.os === 'macos'
  return (
    <span
      role="img"
      className={cx('inline-flex items-center align-middle', joined ? 'gap-0.5' : 'gap-1', className)}
      aria-label={spokenKeys(shortcut, platform)}
    >
      {keys.map((k, i) => (
        <span key={`${k}-${i}`} className={cx('inline-flex items-center', joined ? 'gap-0.5' : 'gap-1')} aria-hidden="true">
          {i > 0 && !joined ? <span className="text-[10.5px] text-subtle">+</span> : null}
          <Keycap label={k} size={size} pressed={pressed} />
        </span>
      ))}
    </span>
  )
}
