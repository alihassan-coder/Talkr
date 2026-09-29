import { X } from 'lucide-react'
import { useToasts } from '@/stores/toast'
import { cx } from '@/lib/cx'

export function Toaster() {
  const toasts = useToasts((s) => s.toasts)
  const dismiss = useToasts((s) => s.dismiss)

  return (
    <div aria-live="polite" className="pointer-events-none fixed bottom-5 right-5 z-50 flex w-80 flex-col gap-2">
      {toasts.map((t) => (
        <div
          key={t.id}
          role={t.tone === 'error' ? 'alert' : 'status'}
          className={cx(
            'pointer-events-auto flex animate-rise items-start gap-3 rounded-xl border px-4 py-3 text-[13px] shadow-[0_20px_60px_-20px_color-mix(in_oklab,var(--color-bg)_90%,transparent)]',
            t.tone === 'error' ? 'border-fg/25 bg-bg text-fg' : 'border-fg/10 bg-bg text-fg/85',
          )}
        >
          {t.tone === 'error' ? <span className="mt-1.5 size-1.5 shrink-0 rounded-full bg-fg" /> : null}
          <p className="flex-1 leading-relaxed" data-selectable>
            {t.message}
          </p>
          <button type="button" onClick={() => dismiss(t.id)} aria-label="Dismiss" className="text-fg/40 hover:text-fg">
            <X className="size-3.5" strokeWidth={2} />
          </button>
        </div>
      ))}
    </div>
  )
}
