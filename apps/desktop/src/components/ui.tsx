import type { ButtonHTMLAttributes, ReactNode, Ref } from 'react'
import { LoaderCircle } from 'lucide-react'
import { cx } from '@/lib/cx'

type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger'

const buttonVariants: Record<ButtonVariant, string> = {
  primary: 'bg-accent text-on-accent hover:bg-accent/88 disabled:bg-accent/30',
  secondary:
    'border border-line text-fg hover:border-line-strong hover:bg-fg/[0.05] disabled:border-line disabled:bg-transparent disabled:text-fg/30',
  ghost: 'text-muted hover:bg-fg/[0.06] hover:text-fg disabled:bg-transparent disabled:text-fg/30',
  danger: 'border border-line text-muted hover:border-line-strong hover:text-fg disabled:border-line disabled:text-fg/30',
}

export function Button({
  variant = 'secondary',
  size = 'md',
  loading = false,
  icon,
  className,
  children,
  disabled,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  ref?: Ref<HTMLButtonElement>
  variant?: ButtonVariant
  size?: 'sm' | 'md' | 'lg'
  loading?: boolean
  icon?: ReactNode
}) {
  const sizes = { sm: 'h-8 px-3 text-xs gap-1.5', md: 'h-9 px-4 text-[13px] gap-2', lg: 'h-11 px-5 text-sm gap-2' }
  return (
    <button
      type="button"
      disabled={disabled || loading}
      className={cx(
        'inline-flex shrink-0 items-center justify-center rounded-full font-medium transition-[background-color,color,border-color,transform] duration-200 ease-out-quint active:scale-[0.97] disabled:cursor-not-allowed disabled:active:scale-100',
        sizes[size],
        buttonVariants[variant],
        className,
      )}
      {...props}
    >
      {loading ? <LoaderCircle className="size-3.5 animate-spin" strokeWidth={2} /> : icon}
      {children}
    </button>
  )
}

export function IconButton({
  label,
  className,
  children,
  active = false,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string; active?: boolean }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={cx(
        'grid size-8 shrink-0 place-items-center rounded-lg transition-colors duration-200 disabled:opacity-30',
        active ? 'bg-fg/[0.08] text-fg' : 'text-subtle hover:bg-fg/[0.06] hover:text-fg',
        className,
      )}
      {...props}
    >
      {children}
    </button>
  )
}

export function Card({ className, children }: { className?: string; children: ReactNode }) {
  return <div className={cx('rounded-2xl border border-line bg-surface shadow-[var(--shadow-card)]', className)}>{children}</div>
}

/** Small mono uppercase label. */
export function Kicker({ className, children }: { className?: string; children: ReactNode }) {
  return <p className={cx('font-mono text-[10.5px] uppercase tracking-[0.16em] text-subtle', className)}>{children}</p>
}

export function Badge({ solid = false, children }: { solid?: boolean; children: ReactNode }) {
  return (
    <span
      className={cx(
        'inline-flex items-center rounded-full px-2 py-px text-[10.5px] font-medium',
        solid ? 'bg-accent text-on-accent' : 'border border-line text-muted',
      )}
    >
      {children}
    </span>
  )
}

export function PageHeader({ title, description, actions }: { title: string; description?: string; actions?: ReactNode }) {
  return (
    <header className="flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 className="text-[28px] font-semibold leading-none tracking-[-0.035em]">{title}</h1>
        {description ? <p className="mt-2 text-[13.5px] text-muted">{description}</p> : null}
      </div>
      {actions ? <div className="flex items-center gap-2">{actions}</div> : null}
    </header>
  )
}

/** Horizontal segmented control with a sliding thumb. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T
  options: { value: T; label: string; icon?: ReactNode }[]
  onChange: (value: T) => void
  label: string
}) {
  const index = Math.max(
    0,
    options.findIndex((o) => o.value === value),
  )
  return (
    <div
      role="tablist"
      aria-label={label}
      className="relative inline-grid rounded-full border border-line bg-fg/[0.04] p-0.5"
      style={{ gridTemplateColumns: `repeat(${options.length}, minmax(0, 1fr))` }}
    >
      <span
        aria-hidden="true"
        className="absolute inset-y-0.5 left-0.5 rounded-full bg-accent shadow-[var(--shadow-card)] transition-transform duration-400 ease-out-quint"
        style={{ width: `calc((100% - 4px) / ${options.length})`, transform: `translateX(${index * 100}%)` }}
      />
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="tab"
          aria-selected={o.value === value}
          onClick={() => onChange(o.value)}
          className={cx(
            'relative z-10 inline-flex items-center justify-center gap-1.5 rounded-full px-4 py-1.5 text-[13px] font-medium transition-colors duration-300',
            o.value === value ? 'text-on-accent' : 'text-muted hover:text-fg',
          )}
        >
          {o.icon}
          {o.label}
        </button>
      ))}
    </div>
  )
}

/** Thin progress bar; `value` is 0..1, or null while unknown. `label` names it for screen readers. */
export function Progress({ value, label, className }: { value: number | null; label: string; className?: string }) {
  const percent = value === null ? null : Math.round(Math.min(1, Math.max(0, value)) * 100)
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent ?? undefined}
      aria-valuetext={percent === null ? 'In progress' : `${percent}%`}
      className={cx('relative h-[3px] overflow-hidden rounded-full bg-fg/10', className)}
    >
      {value === null ? (
        <span className="absolute inset-y-0 w-1/3 animate-shimmer rounded-full bg-accent/80" />
      ) : (
        <span
          className="absolute inset-y-0 left-0 rounded-full bg-accent transition-[width] duration-300 ease-out-quint"
          style={{ width: `${Math.min(100, Math.max(0, value * 100))}%` }}
        />
      )}
    </div>
  )
}

export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon: ReactNode
  title: string
  description: string
  action?: ReactNode
}) {
  return (
    <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed border-line-strong px-8 py-14 text-center">
      <span className="grid size-11 place-items-center rounded-full border border-line bg-surface text-muted shadow-[var(--shadow-card)]">{icon}</span>
      <h2 className="mt-5 text-[15px] font-medium tracking-[-0.01em]">{title}</h2>
      <p className="mt-1.5 max-w-sm text-[13px] leading-relaxed text-muted">{description}</p>
      {action ? <div className="mt-6">{action}</div> : null}
    </div>
  )
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded border border-line bg-surface px-1.5 py-px font-mono text-[10.5px] text-muted">{children}</kbd>
  )
}
