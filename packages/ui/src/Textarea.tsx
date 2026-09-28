import { forwardRef, TextareaHTMLAttributes } from 'react'

interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> {
  label?: string
  error?: string
  helperText?: string
  showCharCount?: boolean
  maxLength?: number
}

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(
  ({ className = '', label, error, helperText, showCharCount, maxLength, id, ...props }, ref) => {
    const textareaId = id || label?.toLowerCase().replace(/\s+/g, '-')
    const length = (props.value as string)?.length || 0

    return (
      <div className="w-full">
        {label && (
          <div className="flex items-baseline justify-between mb-1.5">
            <label htmlFor={textareaId} className="block text-sm font-medium text-text-primary">
              {label}
            </label>
            {showCharCount && maxLength && (
              <span className={`text-sm ${length > maxLength ? 'text-danger' : 'text-text-muted'}`}>
                {length}/{maxLength}
              </span>
            )}
          </div>
        )}
        <textarea
          ref={ref}
          id={textareaId}
          className={`
            w-full rounded-[var(--radius-card)] border bg-white
            px-4 py-3 text-text-primary placeholder:text-text-muted
            transition-colors duration-fast resize-none
            focus:outline-none focus:ring-2 focus:ring-accent focus:border-transparent
            disabled:bg-app-canvas disabled:cursor-not-allowed
            ${error ? 'border-danger focus:ring-danger' : 'border-border hover:border-text-muted'}
            ${className}
          `}
          aria-invalid={error ? 'true' : 'false'}
          aria-describedby={error ? `${textareaId}-error` : helperText ? `${textareaId}-helper` : undefined}
          {...props}
        />
        {error && (
          <p id={`${textareaId}-error`} className="mt-1.5 text-sm text-danger" role="alert">
            {error}
          </p>
        )}
        {helperText && !error && (
          <p id={`${textareaId}-helper`} className="mt-1.5 text-sm text-text-muted">
            {helperText}
          </p>
        )}
      </div>
    )
  }
)

Textarea.displayName = 'Textarea'