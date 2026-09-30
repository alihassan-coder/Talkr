import { create } from 'zustand'
import { errorText } from '@/lib/errors'

export type Toast = { id: number; message: string; tone: 'default' | 'error' }

type ToastState = {
  toasts: Toast[]
  push: (message: string, tone?: Toast['tone']) => void
  dismiss: (id: number) => void
}

let nextId = 1

export const useToasts = create<ToastState>((set, get) => ({
  toasts: [],
  push: (message, tone = 'default') => {
    const id = nextId++
    set((s) => ({ toasts: [...s.toasts.slice(-2), { id, message, tone }] }))
    setTimeout(() => get().dismiss(id), tone === 'error' ? 6000 : 3200)
  },
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}))

/** Shorthands usable outside React components. */
export const toast = (message: string) => useToasts.getState().push(message)

export const toastError = (error: unknown) => useToasts.getState().push(errorText(error), 'error')
