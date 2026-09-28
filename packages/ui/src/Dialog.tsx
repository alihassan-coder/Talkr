"use client"

import { Fragment, ReactNode, useEffect } from 'react'
import { createPortal } from 'react-dom'
import { X } from 'lucide-react'
import { Button } from './Button'

interface DialogProps {
  open: boolean
  onClose: () => void
  title?: string
  description?: string
  children: ReactNode
  size?: 'sm' | 'md' | 'lg' | 'xl'
}

export function Dialog({ open, onClose, title, description, children, size = 'md' }: DialogProps) {
  if (!open) return null

  const sizes = {
    sm: 'max-w-md',
    md: 'max-w-lg',
    lg: 'max-w-2xl',
    xl: 'max-w-4xl',
  }

  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', handleEscape)
    document.body.style.overflow = 'hidden'
    return () => {
      document.removeEventListener('keydown', handleEscape)
      document.body.style.overflow = ''
    }
  }, [onClose])

  return createPortal(
    <Fragment>
      <div
        className="fixed inset-0 bg-black/40 backdrop-blur-sm z-50 animate-fade-in"
        onClick={onClose}
        aria-hidden="true"
      />
      <div
        className={`fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full ${sizes[size]} z-50 animate-slide-up`}
        role="dialog"
        aria-modal="true"
        aria-labelledby={title ? 'dialog-title' : undefined}
        aria-describedby={description ? 'dialog-description' : undefined}
      >
        <div className="bg-card rounded-[var(--radius-card)] shadow-[var(--shadow-soft)] border border-border overflow-hidden">
          {(title || description) && (
            <div className="flex items-start justify-between p-6 border-b border-border">
              <div>
                {title && (
                  <h2 id="dialog-title" className="text-lg font-semibold text-text-primary">
                    {title}
                  </h2>
                )}
                {description && (
                  <p id="dialog-description" className="mt-1 text-sm text-text-secondary">
                    {description}
                  </p>
                )}
              </div>
              <button
                onClick={onClose}
                className="p-1.5 rounded-[var(--radius-input)] text-text-muted hover:text-text-primary hover:bg-app-canvas transition-colors"
                aria-label="Close dialog"
              >
                <X className="w-5 h-5" strokeWidth={2} />
              </button>
            </div>
          )}
          <div className="p-6">{children}</div>
        </div>
      </div>
    </Fragment>,
    document.body
  )
}