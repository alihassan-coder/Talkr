import { cx } from '@/lib/cx'

export function Switch({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean
  onChange: (checked: boolean) => void
  label: string
  disabled?: boolean
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className="group inline-flex h-9 items-center gap-2.5 rounded-full border border-line bg-surface pl-3.5 pr-2 text-[13px] transition-colors hover:border-line-strong disabled:opacity-40"
    >
      <span className={cx('transition-colors', checked ? 'text-fg' : 'text-muted')}>{label}</span>
      <span
        className={cx(
          'relative h-5 w-9 rounded-full transition-colors duration-300 ease-out-quint',
          checked ? 'bg-accent' : 'bg-line-strong',
        )}
      >
        <span
          className={cx(
            'absolute top-0.5 left-0.5 size-4 rounded-full shadow-[var(--shadow-knob)] transition-[transform,background-color] duration-300 ease-out-quint',
            checked ? 'translate-x-4 bg-on-accent' : 'bg-[var(--knob-off)]',
          )}
        />
      </span>
    </button>
  )
}
