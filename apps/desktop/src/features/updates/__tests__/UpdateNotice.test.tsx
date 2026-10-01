import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { DownloadEvent } from '@tauri-apps/plugin-updater'
import { UpdateNotice } from '../UpdateNotice'

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
) {
  return { version: '9.9.9', download, install }
}

beforeEach(() => {
  updater.check.mockReset()
  proc.relaunch.mockReset().mockResolvedValue(undefined)
  log.warn.mockReset().mockResolvedValue(undefined)
  api.stopEngine.mockReset().mockResolvedValue(undefined)
})

describe('UpdateNotice', () => {
  it('does not check outside the packaged app (tests, browser preview)', async () => {
    render(<UpdateNotice />)
    await new Promise((r) => setTimeout(r, 0))
    expect(updater.check).not.toHaveBeenCalled()
  })

  it('renders nothing when Talkr is up to date', async () => {
    updater.check.mockResolvedValue(null)
    const { container } = render(<UpdateNotice enabled />)
    await waitFor(() => expect(updater.check).toHaveBeenCalledTimes(1))
    expect(container).toBeEmptyDOMElement()
  })

  it('logs a failed check instead of showing anything', async () => {
    updater.check.mockRejectedValue('offline')
    const { container } = render(<UpdateNotice enabled />)
    await waitFor(() => expect(log.warn).toHaveBeenCalledWith('Update check failed: offline'))
    expect(container).toBeEmptyDOMElement()
  })

  it('offers the update and can be dismissed', async () => {
    updater.check.mockResolvedValue(fakeUpdate())
    render(<UpdateNotice enabled />)
    expect(await screen.findByText('Talkr 9.9.9 is available')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }))
    expect(screen.queryByText('Talkr 9.9.9 is available')).not.toBeInTheDocument()
  })

  it('downloads with progress, installs, then relaunches', async () => {
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
    updater.check.mockResolvedValue(fakeUpdate(download, install))
    render(<UpdateNotice enabled />)
    await userEvent.click(await screen.findByRole('button', { name: 'Install and restart' }))
    expect(await screen.findByRole('button', { name: /Downloading… 25%/ })).toBeDisabled()
    expect(screen.queryByRole('button', { name: 'Dismiss' })).not.toBeInTheDocument()
    expect(api.stopEngine).not.toHaveBeenCalled()
    finish()
    await waitFor(() => expect(proc.relaunch).toHaveBeenCalledTimes(1))
    // The engine is stopped after the download and before the installer replaces its executable.
    expect(install).toHaveBeenCalledTimes(1)
    expect(api.stopEngine.mock.invocationCallOrder[0]).toBeLessThan(install.mock.invocationCallOrder[0] ?? 0)
  })

  it('lets the user retry when the install fails', async () => {
    updater.check.mockResolvedValue(fakeUpdate(vi.fn(() => Promise.reject('bad signature'))))
    render(<UpdateNotice enabled />)
    await userEvent.click(await screen.findByRole('button', { name: 'Install and restart' }))
    expect(await screen.findByRole('button', { name: 'Try again' })).toBeEnabled()
    expect(log.warn).toHaveBeenCalledWith('Update to 9.9.9 failed: bad signature')
    expect(proc.relaunch).not.toHaveBeenCalled()
  })
})
