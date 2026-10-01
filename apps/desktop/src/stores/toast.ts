import { create } from 'zustand'
import { errorText } from '@/lib/errors'

export type Toast = { id: number; message: string; tone: 'default' | 'error' }

type ToastState = {
  toasts: Toast[]
  push: (message: string, tone?: Toast['tone']) => void
  dismiss: (id: number) => void
  /** Hold a toast while the pointer or focus is on it, so it can be read (or its text copied). */
  pause: (id: number) => void
  resume: (id: number) => void
}

/** How long a toast stays: errors are longer and need the time. */
export const TOAST_MS = { default: 3200, error: 9000 } as const

let nextId = 1
const timers = new Map<number, { handle?: ReturnType<typeof setTimeout>; remaining: number; startedAt: number }>()

export const useToasts = create<ToastState>((set, get) => {
  const run = (id: number) => {
    const t = timers.get(id)
    if (!t) return
    clearTimeout(t.handle)
    t.startedAt = Date.now()
    t.handle = setTimeout(() => get().dismiss(id), t.remaining)
  }
  return {
    toasts: [],
    push: (message, tone = 'default') => {
      const id = nextId++
      const dropped = get().toasts.slice(0, -2)
      for (const old of dropped) {
        clearTimeout(timers.get(old.id)?.handle)
        timers.delete(old.id)
      }
      set((s) => ({ toasts: [...s.toasts.slice(-2), { id, message, tone }] }))
      timers.set(id, { remaining: TOAST_MS[tone], startedAt: Date.now() })
      run(id)
    },
    dismiss: (id) => {
      clearTimeout(timers.get(id)?.handle)
      timers.delete(id)
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }))
    },
    pause: (id) => {
      const t = timers.get(id)
      if (!t || t.handle === undefined) return
      clearTimeout(t.handle)
      t.handle = undefined
      t.remaining = Math.max(0, t.remaining - (Date.now() - t.startedAt))
    },
    resume: (id) => {
      const t = timers.get(id)
      if (!t || t.handle !== undefined) return
      // A moment to move away before it goes.
      t.remaining = Math.max(t.remaining, 1200)
      run(id)
    },
  }
})

/** Shorthands usable outside React components. */
export const toast = (message: string) => useToasts.getState().push(message)

export const toastError = (error: unknown) => useToasts.getState().push(errorText(error), 'error')
