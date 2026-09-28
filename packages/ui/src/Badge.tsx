import { HTMLAttributes, forwardRef } from 'react'

interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: 'default' | 'success' | 'warning' | 'danger' | 'accent'
  size?: 'sm' | 'md'
  dot?: boolean
}

export const Badge = forwardRef<HTMLSpanElement, BadgeProps>(
  ({ className = '', variant = 'default', size = 'md', dot, children, ...props }, ref) => {
    const variants = {
      default: 'bg-app-canvas text-text-secondary border border-border',
      success: 'bg-success-soft text-success border border-success/20',
      warning: 'bg-warning-soft text-warning border border-warning/20',
      danger: 'bg-danger-soft text-danger border border-danger/20',
      accent: 'bg-accent-soft text-accent border border-accent/20',
    }

    const sizes = {
      sm: 'px-2 py-0.5 text-xs gap-1',
      md: 'px-2.5 py-1 text-sm gap-1.5',
    }

    return (
      <span
        ref={ref}
        className={`
          inline-flex items-center font-medium rounded-[var(--radius-pill)]
          ${variants[variant]} ${sizes[size]} ${className}
        `}
        {...props}
      >
        {dot && (
          <span className={`w-1.5 h-1.5 rounded-full ${variant === 'default' ? 'bg-text-muted' : `bg-${variant}`}`} />
        )}
        {children}
      </span>
    )
  }
)

Badge.displayName = 'Badge'