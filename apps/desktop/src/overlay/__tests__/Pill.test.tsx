import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { Pill } from '@/overlay/Pill'
import { mockBackend } from '@/test/tauri'

describe('dictation pill', () => {
  it('is hidden until there is something to show', () => {
    const { container } = render(<Pill />)
    expect(container.querySelector('.pill')).toHaveAttribute('data-visible', 'false')
  })

  it('shows listening with the target app', () => {
    render(<Pill initial={{ kind: 'listening', session: 1, locked: false, app: 'Slack' }} />)
    expect(screen.getByRole('status')).toHaveAccessibleName('Listening')
    expect(screen.getByText('→ Slack')).toBeInTheDocument()
    expect(screen.getByText('0:00')).toBeInTheDocument()
    // No buttons while holding: the pill lets clicks through.
    expect(screen.queryByRole('button')).toBeNull()
  })

  it('offers cancel and done when hands-free', async () => {
    const user = userEvent.setup()
    const b = mockBackend({ dictation_stop: null, dictation_cancel: null })
    render(<Pill initial={{ kind: 'listening', session: 2, locked: true, app: null }} />)
    expect(screen.getByRole('status')).toHaveAccessibleName('Dictating hands-free')
    await user.click(screen.getByRole('button', { name: 'Done' }))
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(b.count('dictation_stop')).toBe(1)
    expect(b.count('dictation_cancel')).toBe(1)
  })

  it('shows progress, results and problems', () => {
    const { rerender } = render(<Pill initial={{ kind: 'working', session: 3, label: 'Transcribing' }} />)
    expect(screen.getByText('Transcribing')).toBeInTheDocument()
    rerender(<Pill key="done" initial={{ kind: 'done', session: 3, preview: 'Hello there', app: 'Notepad' }} />)
    expect(screen.getByRole('status')).toHaveAccessibleName('Inserted: Hello there')
    rerender(
      <Pill
        key="notice"
        initial={{ kind: 'notice', session: 4, tone: 'warn', title: 'Copied — press Ctrl+V to paste', detail: 'The window changed' }}
      />,
    )
    expect(screen.getByText('Copied — press Ctrl+V to paste')).toBeInTheDocument()
    expect(screen.getByText('The window changed')).toBeInTheDocument()
  })
})
