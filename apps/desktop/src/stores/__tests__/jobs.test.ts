import { act } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { busyReason, initJobs, useJobs } from '@/stores/jobs'
import { useToasts } from '@/stores/toast'
import { emitEvent, flush, historyFixture, mockBackend, reject } from '@/test/tauri'

/** A command result the test resolves by hand. */
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => (resolve = r))
  return { promise, resolve }
}

const start = (kind: 'tts' | 'stt', launch: () => Promise<string>, label = '') =>
  act(() => useJobs.getState().start(kind, launch, { label }))

describe('jobs store', () => {
  it('tracks progress and the result without any page listening', async () => {
    mockBackend({})
    initJobs()
    await flush()
    await start('stt', () => Promise.resolve('job-1'), 'interview.mp3')
    expect(useJobs.getState().stt).toMatchObject({ running: true, jobId: 'job-1', label: 'interview.mp3' })

    await emitEvent('job://progress', { jobId: 'job-1', progress: 0.5 })
    expect(useJobs.getState().stt.progress).toBe(0.5)

    const item = historyFixture({ id: 'job-1', text: 'Hello' })
    await emitEvent('stt://done', { jobId: 'job-1', historyItem: item })
    expect(useJobs.getState().stt).toMatchObject({ running: false, jobId: null, progress: null, error: null })
    expect(useJobs.getState().stt.result).toMatchObject({ item, label: 'interview.mp3' })
  })

  it('keeps an outcome that arrives before the job id', async () => {
    mockBackend({})
    const launch = deferred<string>()
    const started = start('tts', () => launch.promise)
    await flush()
    await emitEvent('job://error', { jobId: 'job-2', error: 'Engine crashed', cancelled: false })
    launch.resolve('job-2')
    await started
    expect(useJobs.getState().tts).toMatchObject({ running: false, error: 'Engine crashed' })
  })

  it('records a rejected command as the error', async () => {
    mockBackend({})
    await start('tts', reject('Not enough free memory'))
    expect(useJobs.getState().tts).toMatchObject({ running: false, error: 'Not enough free memory' })
    act(() => useJobs.getState().dismissError('tts'))
    expect(useJobs.getState().tts.error).toBeNull()
  })

  it('queues a cancel pressed before the job id arrives and sends it once it does', async () => {
    const backend = mockBackend({ cancel_job: null })
    const launch = deferred<string>()
    const started = start('stt', () => launch.promise)
    await flush()
    await act(() => useJobs.getState().cancel('stt'))
    expect(useJobs.getState().stt.cancelRequested).toBe(true)
    expect(backend.count('cancel_job')).toBe(0)

    launch.resolve('job-3')
    await started
    expect(backend.argsOf('cancel_job')).toEqual([{ jobId: 'job-3' }])

    await emitEvent('job://error', { jobId: 'job-3', error: 'cancelled', cancelled: true })
    expect(useJobs.getState().stt).toMatchObject({ running: false, error: null, cancelRequested: false })
    expect(useToasts.getState().toasts.at(-1)?.message).toBe('Cancelled')
  })

  it('runs one heavy job at a time', async () => {
    mockBackend({})
    let launched = 0
    await start('tts', () => {
      launched += 1
      return Promise.resolve('job-4')
    })
    expect(busyReason(useJobs.getState(), 'stt')).toBe('Wait for speech generation to finish.')
    expect(busyReason(useJobs.getState(), 'tts')).toBeNull()
    await start('stt', () => {
      launched += 1
      return Promise.resolve('job-5')
    })
    expect(launched).toBe(1)
    expect(useJobs.getState().stt.running).toBe(false)
  })
})
