import { HTMLAttributes, forwardRef } from 'react'

interface ProgressProps extends HTMLAttributes<HTMLDivElement> {
  value: number
  max?: number
  size?: 'sm' | 'md' | 'lg'
  showLabel?: boolean
  label?: string
  variant?: 'default' | 'success' | 'warning'
}

export const Progress = forwardRef<HTMLDivElement, ProgressProps>(
  ({ className = '', value, max = 100, size = 'md', showLabel, label, variant = 'default', ...props }, ref) => {
    const percentage = Math.min(Math.max((value / max) * 100, 0), 100)

    const variants = {
      default: 'bg-accent',
      success: 'bg-success',
      warning: 'bg-warning',
    }

    const sizes = {
      sm: 'h-1.5',
      md: 'h-2.5',
      lg: 'h-4',
    }

    return (
      <div ref={ref} className={`w-full ${className}`} {...props}>
        {(showLabel || label) && (
          <div className="flex items-center justify-between mb-1.5">
            {label && <span className="text-sm font-medium text-text-primary">{label}</span>}
            {showLabel && <span className="text-sm text-text-secondary font-mono tabular-nums">{Math.round(percentage)}%</span>}
          </div>
        )}
        <div className={`w-full bg-app-canvas rounded-full overflow-hidden ${sizes[size]}`}>
          <div
            className={`${variants[variant]} h-full rounded-full transition-all duration-normal ease-out`}
            style={{ width: `${percentage}%` } as React.CSSProperties}
            role="progressbar"
            aria-valuenow={value}
            aria-valuemin={0}
            aria-valuemax={max}
            aria-label={label}
          />
        </div>
      </div>
    )
  }
)

Progress.displayName = 'Progress'