import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { toast, toastError, useToasts } from '@/stores/toast'

beforeEach(() => {
  vi.useFakeTimers()
  useToasts.setState({ toasts: [] })
})
afterEach(() => vi.useRealTimers())

describe('toast store', () => {
  it('adds a toast and removes it after a while', () => {
    toast('Saved')
    expect(useToasts.getState().toasts).toMatchObject([{ message: 'Saved', tone: 'default' }])
    vi.advanceTimersByTime(3200)
    expect(useToasts.getState().toasts).toEqual([])
  })

  it('keeps errors longer and makes them readable', () => {
    toastError('{"message":"Disk full"}')
    expect(useToasts.getState().toasts).toMatchObject([{ message: 'Disk full', tone: 'error' }])
    vi.advanceTimersByTime(3200)
    expect(useToasts.getState().toasts).toHaveLength(1)
    vi.advanceTimersByTime(2800)
    expect(useToasts.getState().toasts).toHaveLength(0)
  })

  it('shows at most three at once', () => {
    for (const m of ['a', 'b', 'c', 'd']) toast(m)
    expect(useToasts.getState().toasts.map((t) => t.message)).toEqual(['b', 'c', 'd'])
  })

  it('dismisses by id', () => {
    toast('x')
    const id = useToasts.getState().toasts[0]!.id
    useToasts.getState().dismiss(id)
    expect(useToasts.getState().toasts).toEqual([])
  })
})
