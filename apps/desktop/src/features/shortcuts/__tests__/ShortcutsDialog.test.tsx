import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ShortcutsDialog } from '../ShortcutsDialog'
import { mockBackend } from '@/test/tauri'

describe('ShortcutsDialog', () => {
  it('lists the shortcuts, takes focus and closes on Escape or outside clicks', async () => {
    const onClose = vi.fn()
    const user = userEvent.setup()
    render(<ShortcutsDialog onClose={onClose} />)
    const dialog = screen.getByRole('dialog', { name: 'Keyboard shortcuts' })
    expect(dialog).toHaveTextContent('Generate speech')
    expect(dialog).toHaveTextContent('Search history')
    expect(dialog).toHaveTextContent('Zoom in')
    expect(dialog).toHaveTextContent('Zoom out')
    expect(dialog).toHaveTextContent('Reset zoom')
    expect(screen.getByRole('button', { name: 'Close' })).toHaveFocus()
    await user.keyboard('{Escape}')
    expect(onClose).toHaveBeenCalledTimes(1)
    await user.click(dialog)
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('names the dictation keys the way this system does', async () => {
    mockBackend({
      dictation_status: {
        supported: true,
        capabilities: {
          os: 'linux',
          supported: true,
          holdToTalk: true,
          modifierOnly: true,
          recordsShortcut: true,
          verifiesInsertion: false,
          insertsText: true,
          metaKey: 'Super',
          note: null,
        },
        permission: { state: 'notNeeded' },
        active: true,
        error: null,
        modelId: null,
        warm: false,
        hasLast: false,
        recording: false,
      },
    })
    render(<ShortcutsDialog onClose={vi.fn()} />)
    expect(await screen.findByText('Super')).toBeInTheDocument()
    expect(screen.queryByText('Win')).toBeNull()
  })
})
