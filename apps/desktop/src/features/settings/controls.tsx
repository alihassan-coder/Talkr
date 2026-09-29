import type { ReactNode } from 'react'
import { Card, Kicker } from '@/components/ui'
import { cx } from '@/lib/cx'

export function Switch({
  checked,
  onChange,
  label,
  disabled = false,
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
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cx(
        'relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors duration-300 ease-out-quint disabled:opacity-40',
        checked ? 'bg-accent' : 'bg-line-strong hover:bg-fg/30',
      )}
    >
      <span
        aria-hidden="true"
        className={cx(
          'size-4 rounded-full shadow-[var(--shadow-knob)] transition-[transform,background-color] duration-300 ease-out-quint',
          checked ? 'translate-x-[18px] bg-on-accent' : 'translate-x-0.5 bg-[var(--knob-off)]',
        )}
      />
    </button>
  )
}

export function Section({ title, children, className }: { title: string; children: ReactNode; className?: string }) {
  return (
    <section className={cx('space-y-3', className)}>
      <Kicker className="px-1">{title}</Kicker>
      <Card>{children}</Card>
    </section>
  )
}

export function Row({ label, description, children }: { label: string; description?: ReactNode; children?: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 border-b border-line px-5 py-4 last:border-0">
      <div className="min-w-0">
        <p className="text-[13.5px] font-medium tracking-[-0.005em]">{label}</p>
        {description ? <div className="mt-0.5 text-[13px] leading-relaxed text-muted">{description}</div> : null}
      </div>
      {children ? <div className="flex shrink-0 items-center gap-2">{children}</div> : null}
    </div>
  )
}
