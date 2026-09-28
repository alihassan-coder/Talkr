"use client"

import { useEffect, useState, ReactNode } from 'react'
import { createPortal } from 'react-dom'
import { X, CheckCircle, AlertCircle, Info } from 'lucide-react'

interface ToastProps {
  open: boolean
  onClose: () => void
  type?: 'success' | 'error' | 'info' | 'default'
  title: string
  message?: string
  action?: { label: string; onClick: () => void }
}

export function Toast({ open, onClose, type = 'default', title, message, action }: ToastProps) {
  const [visible, setVisible] = useState(open)

  useEffect(() => {
    setVisible(open)
    if (open) {
      const timer = setTimeout(() => {
        setVisible(false)
        setTimeout(onClose, 200)
      }, 5000)
      return () => clearTimeout(timer)
    }
  }, [open, onClose])

  if (!visible) return null

  const icons = {
    success: <CheckCircle className="w-5 h-5 text-success" />,
    error: <AlertCircle className="w-5 h-5 text-danger" />,
    info: <Info className="w-5 h-5 text-accent" />,
    default: <Info className="w-5 h-5 text-text-secondary" />,
  }

  const backgrounds = {
    success: 'bg-success-soft border-success/20',
    error: 'bg-danger-soft border-danger/20',
    info: 'bg-accent-soft border-accent/20',
    default: 'bg-app-canvas border-border',
  }

  return createPortal(
    <div className="fixed bottom-6 right-6 z-50 animate-slide-in">
      <div className={`flex items-start gap-3 p-4 rounded-[var(--radius-card)] border shadow-[var(--shadow-soft)] min-w-[300px] max-w-md ${backgrounds[type]}`}>
        <div className="flex-shrink-0 mt-0.5">{icons[type]}</div>
        <div className="flex-1 min-w-0">
          <p className="font-medium text-text-primary">{title}</p>
          {message && <p className="mt-0.5 text-sm text-text-secondary">{message}</p>}
        </div>
        <div className="flex items-center gap-2">
          {action && (
            <Button size="sm" variant="ghost" onClick={action.onClick}>
              {action.label}
            </Button>
          )}
          <button
            onClick={onClose}
            className="p-1 rounded-[var(--radius-input)] text-text-muted hover:text-text-primary hover:bg-white/50 transition-colors"
            aria-label="Dismiss"
          >
            <X className="w-4 h-4" strokeWidth={2} />
          </button>
        </div>
      </div>
    </div>,
    document.body
  )
}

interface ToastContainerProps {
  toasts: Array<{
    id: string
    type?: 'success' | 'error' | 'info' | 'default'
    title: string
    message?: string
    action?: { label: string; onClick: () => void }
  }>
  onClose: (id: string) => void
}

export function ToastContainer({ toasts, onClose }: ToastContainerProps) {
  return (
    <div className="fixed bottom-6 right-6 z-50 flex flex-col gap-2 pointer-events-none">
      {toasts.map((toast) => (
        <div key={toast.id} className="pointer-events-auto animate-slide-in">
          <Toast
            open
            onClose={() => onClose(toast.id)}
            type={toast.type}
            title={toast.title}
            message={toast.message}
            action={toast.action}
          />
        </div>
      ))}
    </div>
  )
}