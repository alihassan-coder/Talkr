import { useState } from 'react'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { Select, type SelectOption } from '@/components/Select'

const options: SelectOption[] = [
  { value: 'en', label: 'English' },
  { value: 'es', label: 'Spanish', hint: 'es-ES' },
  { value: 'de', label: 'German' },
  { value: 'da', label: 'Danish' },
  { value: 'fr', label: 'French' },
]

function Controlled({ onChange = vi.fn(), initial = 'en' }: { onChange?: (v: string) => void; initial?: string }) {
  const [value, setValue] = useState(initial)
  return (
    <Select
      label="Language"
      options={options}
      value={value}
      onChange={(v) => {
        setValue(v)
        onChange(v)
      }}
    />
  )
}

const activeLabel = () => {
  const trigger = screen.getByRole('combobox')
  const id = trigger.getAttribute('aria-activedescendant')
  return id ? document.getElementById(id)?.querySelector('span')?.textContent : null
}

describe('Select', () => {
  it('renders a labelled combobox showing the selected option', () => {
    render(<Controlled />)
    const trigger = screen.getByRole('combobox', { name: 'Language' })
    expect(trigger).toHaveAttribute('aria-haspopup', 'listbox')
    expect(trigger).toHaveAttribute('aria-expanded', 'false')
    expect(trigger).toHaveTextContent('English')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
  })

  it('opens on click with ARIA wiring and the selected option marked', async () => {
    const user = userEvent.setup()
    render(<Controlled />)
    const trigger = screen.getByRole('combobox')
    await user.click(trigger)
    const listbox = screen.getByRole('listbox', { name: 'Language' })
    expect(trigger).toHaveAttribute('aria-expanded', 'true')
    expect(trigger).toHaveAttribute('aria-controls', listbox.id)
    expect(screen.getAllByRole('option')).toHaveLength(5)
    expect(screen.getByRole('option', { name: /English/ })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByRole('option', { name: /Spanish/ })).toHaveTextContent('es-ES')
    expect(activeLabel()).toBe('English')
  })

  it('moves with the arrow keys, Home and End, and selects with Enter', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<Controlled onChange={onChange} />)
    screen.getByRole('combobox').focus()
    await user.keyboard('{ArrowDown}')
    expect(screen.getByRole('listbox')).toBeInTheDocument()
    await user.keyboard('{ArrowDown}{ArrowDown}')
    expect(activeLabel()).toBe('German')
    await user.keyboard('{ArrowUp}')
    expect(activeLabel()).toBe('Spanish')
    await user.keyboard('{End}')
    expect(activeLabel()).toBe('French')
    await user.keyboard('{ArrowDown}')
    expect(activeLabel()).toBe('French')
    await user.keyboard('{Home}')
    expect(activeLabel()).toBe('English')
    await user.keyboard('{ArrowDown}{Enter}')
    expect(onChange).toHaveBeenCalledWith('es')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
    expect(screen.getByRole('combobox')).toHaveTextContent('Spanish')
  })

  it('does not call onChange when the same option is chosen', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<Controlled onChange={onChange} />)
    screen.getByRole('combobox').focus()
    await user.keyboard('{Enter}{Enter}')
    expect(onChange).not.toHaveBeenCalled()
  })

  it('closes on Escape without changing', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<Controlled onChange={onChange} />)
    screen.getByRole('combobox').focus()
    await user.keyboard('{ArrowDown}{ArrowDown}{Escape}')
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
    expect(onChange).not.toHaveBeenCalled()
  })

  it('type-ahead jumps to matching labels and cycles on repeated letters', async () => {
    const user = userEvent.setup()
    render(<Controlled />)
    screen.getByRole('combobox').focus()
    await user.keyboard('g')
    expect(activeLabel()).toBe('German')
    await new Promise((r) => setTimeout(r, 650))
    await user.keyboard('d')
    expect(activeLabel()).toBe('Danish')
    await new Promise((r) => setTimeout(r, 650))
    await user.keyboard('fr')
    expect(activeLabel()).toBe('French')
  })

  it('selects an option with the mouse', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<Controlled onChange={onChange} />)
    await user.click(screen.getByRole('combobox'))
    await user.click(screen.getByRole('option', { name: /German/ }))
    expect(onChange).toHaveBeenCalledWith('de')
  })

  it('closes on an outside click', async () => {
    const user = userEvent.setup()
    render(
      <>
        <Controlled />
        <button type="button">outside</button>
      </>,
    )
    await user.click(screen.getByRole('combobox'))
    await user.click(screen.getByRole('button', { name: 'outside' }))
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
  })

  it('shows the placeholder, supports aria-label, and respects disabled', async () => {
    const user = userEvent.setup()
    const { rerender } = render(
      <Select label="" aria-label="Voice" options={options} value="zz" onChange={vi.fn()} placeholder="Pick one" />,
    )
    const trigger = screen.getByRole('combobox', { name: 'Voice' })
    expect(trigger).toHaveTextContent('Pick one')
    rerender(<Select label="" aria-label="Voice" options={options} value="en" onChange={vi.fn()} disabled />)
    await user.click(screen.getByRole('combobox'))
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument()
  })

  it('renders option groups', async () => {
    const user = userEvent.setup()
    render(
      <Select
        label="Model"
        value="a"
        onChange={vi.fn()}
        options={[
          { value: 'a', label: 'A', group: 'Installed' },
          { value: 'b', label: 'B', group: 'Available' },
        ]}
      />,
    )
    await user.click(screen.getByRole('combobox'))
    expect(screen.getByRole('group', { name: 'Installed' })).toBeInTheDocument()
    expect(screen.getByRole('group', { name: 'Available' })).toBeInTheDocument()
  })
})
