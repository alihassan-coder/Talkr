import type { ReactNode } from 'react'
import { Info, TriangleAlert, X } from 'lucide-react'
import { cx } from '@/lib/cx'

/**
 * Inline message that stays until it is dealt with. `error` is announced right away
 * (role="alert"); `info` is polite (role="status").
 */
export function Notice({
  tone = 'info',
  title,
  children,
  action,
  onDismiss,
  className,
}: {
  tone?: 'info' | 'error'
  title?: string
  children: ReactNode
  action?: ReactNode
  onDismiss?: () => void
  className?: string
}) {
  const Icon = tone === 'error' ? TriangleAlert : Info
  return (
    <div
      role={tone === 'error' ? 'alert' : 'status'}
      className={cx(
        'flex animate-rise items-start gap-3 rounded-xl border bg-surface px-4 py-3 text-[13px] shadow-[var(--shadow-card)]',
        tone === 'error' ? 'border-line-strong' : 'border-line',
        className,
      )}
    >
      <Icon
        aria-hidden="true"
        className={cx('mt-0.5 size-4 shrink-0', tone === 'error' ? 'text-accent' : 'text-muted')}
        strokeWidth={1.75}
      />
      <div className="min-w-0 flex-1">
        {title ? <p className="font-medium tracking-[-0.005em] text-fg">{title}</p> : null}
        <div data-selectable className={cx('leading-relaxed text-muted', title && 'mt-0.5')}>
          {children}
        </div>
        {action ? <div className="mt-2.5">{action}</div> : null}
      </div>
      {onDismiss ? (
        <button
          type="button"
          onClick={onDismiss}
          aria-label="Dismiss"
          className="grid size-6 shrink-0 place-items-center rounded-md text-subtle transition-colors hover:bg-fg/[0.06] hover:text-fg"
        >
          <X className="size-3.5" strokeWidth={2} />
        </button>
      ) : null}
    </div>
  )
}
