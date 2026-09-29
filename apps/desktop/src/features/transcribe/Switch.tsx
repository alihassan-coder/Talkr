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
      className="group inline-flex h-9 items-center gap-2.5 rounded-full border border-fg/12 pl-3.5 pr-2 text-[13px] transition-colors hover:border-fg/20 disabled:opacity-40"
    >
      <span className={cx('transition-colors', checked ? 'text-fg' : 'text-fg/55')}>{label}</span>
      <span
        className={cx(
          'relative h-5 w-9 rounded-full transition-colors duration-300 ease-out-quint',
          checked ? 'bg-fg' : 'bg-fg/15',
        )}
      >
        <span
          className={cx(
            'absolute top-0.5 left-0.5 size-4 rounded-full transition-[transform,background-color] duration-300 ease-out-quint',
            checked ? 'translate-x-4 bg-bg' : 'bg-fg/70',
          )}
        />
      </span>
    </button>
  )
}
