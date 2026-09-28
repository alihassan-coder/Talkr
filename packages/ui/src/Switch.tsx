import { forwardRef, InputHTMLAttributes } from 'react'

interface SwitchProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type'> {
  label?: string
  description?: string
}

export const Switch = forwardRef<HTMLInputElement, SwitchProps>(
  ({ className = '', label, description, id, ...props }, ref) => {
    const switchId = id || label?.toLowerCase().replace(/\s+/g, '-')

    return (
      <label className={`inline-flex items-start gap-3 cursor-pointer ${className}`}>
        <div className="relative mt-1">
          <input
            ref={ref}
            id={switchId}
            type="checkbox"
            className={`
              peer h-5 w-5 appearance-none rounded-[var(--radius-pill)] border-2
              transition-all duration-fast cursor-pointer
              bg-white border-border
              checked:bg-accent checked:border-accent
              focus:outline-none focus:ring-2 focus:ring-accent focus:ring-offset-2
              disabled:opacity-50 disabled:cursor-not-allowed
            `}
            {...props}
          />
          <span className="absolute inset-0 flex items-center justify-center pointer-events-none">
            <svg className="w-3.5 h-3.5 text-white opacity-0 peer-checked:opacity-100 transition-opacity" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3">
              <path d="M20 6L9 17l-5-5" />
            </svg>
          </span>
        </div>
        <div className="min-w-0">
          {label && (
            <span className="block text-sm font-medium text-text-primary">{label}</span>
          )}
          {description && (
            <span className="block text-sm text-text-muted mt-0.5">{description}</span>
          )}
        </div>
      </label>
    )
  }
)

Switch.displayName = 'Switch'