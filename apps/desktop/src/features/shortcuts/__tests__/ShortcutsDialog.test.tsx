import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ShortcutsDialog } from '../ShortcutsDialog'

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
})
