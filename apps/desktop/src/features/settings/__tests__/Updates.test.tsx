import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { UpdatesSection } from '../Updates'
import { resetUpdates, useUpdates } from '@/stores/updates'
import { mockBackend } from '@/test/tauri'

const updater = vi.hoisted(() => ({ check: vi.fn() }))
vi.mock('@tauri-apps/plugin-updater', () => updater)
vi.mock('@tauri-apps/plugin-log', () => ({ warn: vi.fn(() => Promise.resolve()) }))

// restoreMocks (vitest.config.ts) resets `updater.check` between tests.
beforeEach(resetUpdates)

describe('Settings → Updates', () => {
  it('checks on demand and says when Talkr is current', async () => {
    mockBackend({})
    updater.check.mockResolvedValue(null)
    const user = userEvent.setup()
    render(<UpdatesSection version="0.1.5" />)
    expect(screen.getByText('Talkr 0.1.5')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Check now' }))
    expect(await screen.findByText(/You have the latest version, checked just now/)).toBeInTheDocument()
  })

  it('offers a found update, even one skipped earlier', async () => {
    mockBackend({})
    updater.check.mockResolvedValue({ version: '9.9.9', currentVersion: '0.1.5', body: '' })
    useUpdates.setState({ skippedVersion: '9.9.9' })
    const user = userEvent.setup()
    render(<UpdatesSection version="0.1.5" />)
    await user.click(screen.getByRole('button', { name: 'Check now' }))
    expect(await screen.findByText('Talkr 9.9.9 is available (skipped).')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Install and restart' })).toBeEnabled()
  })

  it('reports a failed manual check', async () => {
    mockBackend({})
    updater.check.mockImplementation(() => Promise.reject('offline'))
    const user = userEvent.setup()
    render(<UpdatesSection version="0.1.5" />)
    await user.click(screen.getByRole('button', { name: 'Check now' }))
    expect(await screen.findByText(/Could not reach the update server/)).toBeInTheDocument()
  })

  it('turns automatic checks off and remembers it', async () => {
    const user = userEvent.setup()
    render(<UpdatesSection version="0.1.5" />)
    await user.click(screen.getByRole('switch', { name: 'Check for updates automatically' }))
    expect(useUpdates.getState().autoCheck).toBe(false)
    expect(JSON.parse(localStorage.getItem('talkr.updates') ?? '{}').state.autoCheck).toBe(false)
  })
})
