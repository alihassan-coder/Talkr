import type { ButtonHTMLAttributes, ReactNode, SelectHTMLAttributes } from 'react'
import { ChevronDown, LoaderCircle } from 'lucide-react'
import { cx } from '@/lib/cx'

type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger'

const buttonVariants: Record<ButtonVariant, string> = {
  primary: 'bg-fg text-bg hover:bg-fg/90 disabled:bg-fg/30',
  secondary: 'border border-fg/12 text-fg/85 hover:bg-fg/[0.06] hover:text-fg disabled:text-fg/30',
  ghost: 'text-fg/60 hover:bg-fg/[0.06] hover:text-fg disabled:text-fg/25',
  danger: 'border border-fg/12 text-fg/70 hover:border-fg/30 hover:text-fg disabled:text-fg/25',
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
        active ? 'bg-fg/[0.08] text-fg' : 'text-fg/50 hover:bg-fg/[0.06] hover:text-fg',
        className,
      )}
      {...props}
    >
      {children}
    </button>
  )
}

export function Card({ className, children }: { className?: string; children: ReactNode }) {
  return <div className={cx('rounded-2xl border border-fg/10 bg-fg/[0.025]', className)}>{children}</div>
}

/** Small mono uppercase label. */
export function Kicker({ className, children }: { className?: string; children: ReactNode }) {
  return <p className={cx('font-mono text-[10.5px] uppercase tracking-[0.16em] text-fg/45', className)}>{children}</p>
}

export function Badge({ solid = false, children }: { solid?: boolean; children: ReactNode }) {
  return (
    <span
      className={cx(
        'inline-flex items-center rounded-full px-2 py-px text-[10.5px] font-medium',
        solid ? 'bg-fg text-bg' : 'border border-fg/12 text-fg/60',
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
        {description ? <p className="mt-2 text-[13.5px] text-fg/50">{description}</p> : null}
      </div>
      {actions ? <div className="flex items-center gap-2">{actions}</div> : null}
    </header>
  )
}

export function Select({
  label,
  className,
  children,
  ...props
}: SelectHTMLAttributes<HTMLSelectElement> & { label: string }) {
  return (
    <label className={cx('relative inline-flex h-9 items-center rounded-full border border-fg/12 pl-3.5 pr-8 text-[13px] transition-colors hover:border-fg/20 focus-within:border-fg/30', className)}>
      <span className="mr-1.5 text-fg/45">{label}</span>
      <select
        className="min-w-0 cursor-pointer appearance-none bg-transparent font-medium text-fg outline-none [&>option]:bg-bg"
        {...props}
      >
        {children}
      </select>
      <ChevronDown className="pointer-events-none absolute right-3 size-3.5 text-fg/45" strokeWidth={2} />
    </label>
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
  options: { value: T; label: string }[]
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
      className="relative inline-grid rounded-full border border-fg/10 bg-fg/[0.03] p-0.5"
      style={{ gridTemplateColumns: `repeat(${options.length}, minmax(0, 1fr))` }}
    >
      <span
        aria-hidden="true"
        className="absolute inset-y-0.5 left-0.5 rounded-full bg-fg transition-transform duration-400 ease-out-quint"
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
            'relative z-10 rounded-full px-4 py-1.5 text-[13px] font-medium transition-colors duration-300',
            o.value === value ? 'text-bg' : 'text-fg/55 hover:text-fg',
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  )
}

export function Progress({ value, className }: { value: number | null; className?: string }) {
  return (
    <div
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={value === null ? undefined : Math.round(value * 100)}
      className={cx('relative h-[3px] overflow-hidden rounded-full bg-fg/10', className)}
    >
      {value === null ? (
        <span className="absolute inset-y-0 w-1/3 animate-shimmer rounded-full bg-fg/70" />
      ) : (
        <span
          className="absolute inset-y-0 left-0 rounded-full bg-fg transition-[width] duration-300 ease-out-quint"
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
    <div className="flex flex-col items-center justify-center rounded-2xl border border-dashed border-fg/12 px-8 py-14 text-center">
      <span className="grid size-11 place-items-center rounded-full border border-fg/10 text-fg/60">{icon}</span>
      <h2 className="mt-5 text-[15px] font-medium tracking-[-0.01em]">{title}</h2>
      <p className="mt-1.5 max-w-sm text-[13px] leading-relaxed text-fg/50">{description}</p>
      {action ? <div className="mt-6">{action}</div> : null}
    </div>
  )
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded border border-fg/12 bg-fg/[0.04] px-1.5 py-px font-mono text-[10.5px] text-fg/55">{children}</kbd>
  )
}
