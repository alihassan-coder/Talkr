import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { DownloadEvent } from '@tauri-apps/plugin-updater'
import { UpdateNotice } from '../UpdateNotice'
import { CHECK_EVERY_MS, FIRST_CHECK_DELAY_MS, resetUpdates, shouldOffer, startUpdateChecks, useUpdates } from '@/stores/updates'

const updater = vi.hoisted(() => ({ check: vi.fn() }))
const proc = vi.hoisted(() => ({ relaunch: vi.fn() }))
const log = vi.hoisted(() => ({ warn: vi.fn() }))
const api = vi.hoisted(() => ({ stopEngine: vi.fn() }))

vi.mock('@tauri-apps/plugin-updater', () => updater)
vi.mock('@tauri-apps/plugin-process', () => proc)
vi.mock('@tauri-apps/plugin-log', () => log)
vi.mock('@/lib/api', async (importOriginal) => ({ ...(await importOriginal<object>()), stopEngine: api.stopEngine }))

function fakeUpdate(
  download: (onEvent?: (e: DownloadEvent) => void) => Promise<void> = vi.fn(() => Promise.resolve()),
  install: () => Promise<void> = vi.fn(() => Promise.resolve()),
  body = 'Faster transcription.\nMP3 export.',
) {
  return { version: '9.9.9', currentVersion: '0.1.5', body, download, install }
}

/** Put an update on offer, as a finished check would. */
async function offer(update = fakeUpdate()) {
  updater.check.mockResolvedValue(update)
  await act(() => useUpdates.getState().check())
  return update
}

beforeEach(() => {
  resetUpdates()
  updater.check.mockReset()
  proc.relaunch.mockReset().mockResolvedValue(undefined)
  log.warn.mockReset().mockResolvedValue(undefined)
  api.stopEngine.mockReset().mockResolvedValue(undefined)
})
afterEach(() => vi.useRealTimers())

describe('update checks', () => {
  it('records that Talkr is current', async () => {
    updater.check.mockResolvedValue(null)
    await useUpdates.getState().check()
    expect(useUpdates.getState()).toMatchObject({ phase: 'current', update: null })
    expect(useUpdates.getState().lastChecked).toBeGreaterThan(0)
  })

  it('logs a failed background check without bothering anyone, but reports a manual one', async () => {
    updater.check.mockRejectedValue('offline')
    await useUpdates.getState().check()
    await waitFor(() => expect(log.warn).toHaveBeenCalledWith('Update check failed: offline'))
    expect(useUpdates.getState()).toMatchObject({ phase: 'idle', error: null })
    await useUpdates.getState().check(true)
    expect(useUpdates.getState().phase).toBe('failed')
    expect(useUpdates.getState().error).toMatch(/Could not reach the update server/)
  })

  it('keeps an offered update when a later background check fails', async () => {
    await offer()
    updater.check.mockRejectedValue('offline')
    await useUpdates.getState().check()
    expect(useUpdates.getState().phase).toBe('available')
  })

  it('checks shortly after launch, then every few hours, only while enabled', async () => {
    vi.useFakeTimers()
    updater.check.mockResolvedValue(null)
    const stop = startUpdateChecks()
    await vi.advanceTimersByTimeAsync(FIRST_CHECK_DELAY_MS - 1)
    expect(updater.check).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(1)
    expect(updater.check).toHaveBeenCalledTimes(1)
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS)
    expect(updater.check).toHaveBeenCalledTimes(2)
    useUpdates.getState().setAutoCheck(false)
    await vi.advanceTimersByTimeAsync(CHECK_EVERY_MS * 2)
    expect(updater.check).toHaveBeenCalledTimes(2)
    stop()
  })

  it('"Later" hides a version for now; "Skip" hides it for good', async () => {
    await offer()
    expect(shouldOffer(useUpdates.getState())).toBe(true)
    useUpdates.getState().dismiss()
    expect(shouldOffer(useUpdates.getState())).toBe(false)
    useUpdates.setState({ dismissedVersion: null })
    useUpdates.getState().skip()
    expect(shouldOffer(useUpdates.getState())).toBe(false)
    expect(JSON.parse(localStorage.getItem('talkr.updates') ?? '{}').state).toEqual({ autoCheck: true, skippedVersion: '9.9.9' })
  })
})

describe('UpdateNotice', () => {
  it('does not check outside the packaged app (tests, browser preview)', async () => {
    vi.useFakeTimers()
    render(<UpdateNotice />)
    await vi.advanceTimersByTimeAsync(FIRST_CHECK_DELAY_MS * 2)
    expect(updater.check).not.toHaveBeenCalled()
  })

  it('renders nothing while there is nothing to offer', () => {
    const { container } = render(<UpdateNotice enabled={false} />)
    expect(container).toBeEmptyDOMElement()
  })

  it('offers the update with its release notes', async () => {
    await offer()
    const user = userEvent.setup()
    render(<UpdateNotice enabled={false} />)
    expect(screen.getByText('Talkr 9.9.9 is available')).toBeInTheDocument()
    expect(screen.getByText(/You have 0.1.5/)).toBeInTheDocument()
    expect(screen.queryByText(/MP3 export/)).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: "What's new" }))
    expect(screen.getByText(/MP3 export/)).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Later' }))
    expect(screen.queryByText('Talkr 9.9.9 is available')).not.toBeInTheDocument()
  })

  it('downloads with progress, stops the engine, installs, then relaunches', async () => {
    let finish!: () => void
    const download = vi.fn(
      (onEvent?: (e: DownloadEvent) => void) =>
        new Promise<void>((resolve) => {
          onEvent?.({ event: 'Started', data: { contentLength: 200 } })
          onEvent?.({ event: 'Progress', data: { chunkLength: 50 } })
          finish = () => {
            onEvent?.({ event: 'Finished' })
            resolve()
          }
        }),
    )
    const install = vi.fn(() => Promise.resolve())
    await offer(fakeUpdate(download, install))
    const user = userEvent.setup()
    render(<UpdateNotice enabled={false} />)
    await user.click(screen.getByRole('button', { name: 'Install and restart' }))
    expect(await screen.findByRole('button', { name: /Downloading… 25%/ })).toBeDisabled()
    expect(screen.queryByRole('button', { name: 'Later' })).not.toBeInTheDocument()
    expect(api.stopEngine).not.toHaveBeenCalled()
    await act(async () => finish())
    // Installing loads the process plugin on demand, which can take a moment under a busy test run.
    await waitFor(() => expect(proc.relaunch).toHaveBeenCalledTimes(1), { timeout: 5000 })
    expect(install).toHaveBeenCalledTimes(1)
    // The engine is stopped after the download and before the installer replaces its executable.
    expect(api.stopEngine.mock.invocationCallOrder[0]).toBeLessThan(install.mock.invocationCallOrder[0] ?? 0)
  })

  it('lets the user retry when the install fails', async () => {
    await offer(fakeUpdate(vi.fn(() => Promise.reject('bad signature'))))
    const user = userEvent.setup()
    render(<UpdateNotice enabled={false} />)
    await user.click(screen.getByRole('button', { name: 'Install and restart' }))
    expect(await screen.findByRole('button', { name: 'Try again' })).toBeEnabled()
    expect(screen.getByText(/could not be installed/)).toBeInTheDocument()
    expect(log.warn).toHaveBeenCalledWith('Update to 9.9.9 failed: bad signature')
    expect(proc.relaunch).not.toHaveBeenCalled()
  })
})
