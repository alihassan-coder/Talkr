import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it } from 'vitest'
import { AppearanceSection } from '@/features/settings/Appearance'
import { useApplyAppearance, useResolvedMode } from '@/lib/appearance'
import { DEFAULT_CUSTOM_ACCENT, deriveCustomTheme, themes } from '@/lib/themes'
import { useUi } from '@/stores/ui'

afterEach(() => useUi.setState({ mode: 'system', theme: 'graphite', customAccent: DEFAULT_CUSTOM_ACCENT, zoom: 1 }))

function Harness() {
  useApplyAppearance()
  const resolved = useResolvedMode()
  return (
    <>
      <p data-testid="resolved">{resolved}</p>
      <AppearanceSection />
    </>
  )
}

describe('Appearance picker', () => {
  it('shows the mode and theme radio groups with the current choice checked', () => {
    render(<Harness />)
    const mode = screen.getByRole('radiogroup', { name: 'Mode' })
    expect(within(mode).getAllByRole('radio')).toHaveLength(3)
    expect(within(mode).getByRole('radio', { name: /System/ })).toHaveAttribute('aria-checked', 'true')
    const theme = screen.getByRole('radiogroup', { name: 'Theme' })
    // The built-in palettes plus Custom.
    expect(within(theme).getAllByRole('radio')).toHaveLength(themes.length + 1)
    expect(within(theme).getByRole('radio', { name: 'Graphite, Monochrome' })).toHaveAttribute('aria-checked', 'true')
    // Roving tab index: only the checked option is tabbable.
    expect(within(theme).getByRole('radio', { name: 'Ocean, Deep teal' })).toHaveAttribute('tabindex', '-1')
  })

  it('selects a mode on click and applies it to <html>', async () => {
    const user = userEvent.setup()
    render(<Harness />)
    expect(document.documentElement.dataset.mode).toBe('dark') // system is dark in the test setup
    await user.click(screen.getByRole('radio', { name: /Light/ }))
    expect(useUi.getState().mode).toBe('light')
    expect(screen.getByTestId('resolved')).toHaveTextContent('light')
    expect(document.documentElement.dataset.mode).toBe('light')
    expect(screen.getByText(/Previews show light/)).toBeInTheDocument()
  })

  it('selects a theme on click and applies it to <html>', async () => {
    const user = userEvent.setup()
    render(<Harness />)
    await user.click(screen.getByRole('radio', { name: 'Forest, Evergreen' }))
    expect(useUi.getState().theme).toBe('forest')
    expect(document.documentElement.dataset.theme).toBe('forest')
    expect(screen.getByRole('radio', { name: 'Forest, Evergreen' })).toHaveAttribute('aria-checked', 'true')
  })

  it('moves through themes with the arrow keys, Home and End', async () => {
    const user = userEvent.setup()
    render(<Harness />)
    screen.getByRole('radio', { name: 'Graphite, Monochrome' }).focus()
    await user.keyboard('{ArrowRight}')
    expect(useUi.getState().theme).toBe('nightfall')
    expect(screen.getByRole('radio', { name: 'Nightfall, Indigo night' })).toHaveFocus()
    await user.keyboard('{End}')
    expect(useUi.getState().theme).toBe('custom')
    await user.keyboard('{ArrowLeft}')
    expect(useUi.getState().theme).toBe('rose')
    await user.keyboard('{ArrowRight}')
    await user.keyboard('{ArrowDown}')
    expect(useUi.getState().theme).toBe('graphite') // wraps around
    await user.keyboard('{ArrowLeft}')
    expect(useUi.getState().theme).toBe('custom')
    await user.keyboard('{Home}')
    expect(useUi.getState().theme).toBe('graphite')
  })

  it('moves through modes with the keyboard', async () => {
    const user = userEvent.setup()
    render(<Harness />)
    screen.getByRole('radio', { name: /System/ }).focus()
    await user.keyboard('{ArrowRight}')
    expect(useUi.getState().mode).toBe('light')
    await user.keyboard('{ArrowRight}')
    expect(useUi.getState().mode).toBe('dark')
    expect(screen.getByRole('radio', { name: /Dark/ })).toHaveFocus()
  })

  it('summarises the current look and follows the mode in previews', async () => {
    const user = userEvent.setup()
    render(<Harness />)
    expect(screen.getByText(/Dark \(system\) · 100% zoom/)).toBeInTheDocument()
    expect(screen.getByText(/Previews show dark/)).toBeInTheDocument()
    await user.click(screen.getByRole('radio', { name: 'Ocean, Deep teal' }))
    await user.click(screen.getByRole('radio', { name: /Light/ }))
    expect(screen.getByText(/^Light · 100% zoom/)).toBeInTheDocument()
    expect(screen.getByText(/Previews show light/)).toBeInTheDocument()
  })

  it('applies a custom accent live and rejects invalid hex with an inline message', async () => {
    const user = userEvent.setup()
    render(<Harness />)
    expect(screen.queryByLabelText('Accent colour')).not.toBeInTheDocument()
    await user.click(screen.getByRole('radio', { name: /Custom/ }))
    expect(useUi.getState().theme).toBe('custom')
    expect(document.documentElement.dataset.theme).toBe('custom')
    const field = screen.getByLabelText('Accent colour')
    await user.clear(field)
    await user.type(field, '#2f9e5b')
    expect(useUi.getState().customAccent).toBe('#2f9e5b')
    const dark = deriveCustomTheme('#2f9e5b').dark
    expect(document.documentElement.style.getPropertyValue('--color-accent')).toBe(dark.accent)
    expect(document.documentElement.style.getPropertyValue('--color-on-accent')).toBe(dark.onAccent)

    await user.clear(field)
    await user.type(field, '#12zz')
    await user.tab()
    expect(field).toHaveAttribute('aria-invalid', 'true')
    expect(screen.getByRole('alert')).toHaveTextContent(/hex colour/)
    // The theme keeps the last good colour.
    expect(useUi.getState().customAccent).toBe('#2f9e5b')
    expect(document.documentElement.style.getPropertyValue('--color-accent')).toBe(dark.accent)

    await user.click(screen.getByRole('button', { name: 'Use #D63C8A' }))
    expect(useUi.getState().customAccent).toBe('#d63c8a')
    expect(field).toHaveValue('#D63C8A')
    expect(field).not.toHaveAttribute('aria-invalid')

    // Back to a built-in theme: the inline tokens go away.
    await user.click(screen.getByRole('radio', { name: 'Forest, Evergreen' }))
    expect(document.documentElement.style.getPropertyValue('--color-accent')).toBe('')
  })

  it('changes and resets the interface zoom', async () => {
    const user = userEvent.setup()
    render(<Harness />)
    const group = screen.getByRole('group', { name: 'Interface zoom' })
    expect(group).toHaveTextContent('100%')
    expect(screen.getByRole('button', { name: 'Reset' })).toBeDisabled()
    await user.click(within(group).getByRole('button', { name: 'Zoom in' }))
    await user.click(within(group).getByRole('button', { name: 'Zoom in' }))
    expect(useUi.getState().zoom).toBe(1.2)
    expect(group).toHaveTextContent('120%')
    await user.click(screen.getByRole('button', { name: 'Reset' }))
    expect(useUi.getState().zoom).toBe(1)
    useUi.setState({ zoom: 0.8 })
    expect(await within(group).findByRole('button', { name: 'Zoom out' })).toBeDisabled()
  })
})
