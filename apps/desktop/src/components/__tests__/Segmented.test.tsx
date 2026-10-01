import { useState } from 'react'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { Segmented } from '@/components/ui'

function Demo({ disabled = false }: { disabled?: boolean }) {
  const [value, setValue] = useState<'a' | 'b' | 'c'>('a')
  return (
    <Segmented
      label="Pick"
      value={value}
      onChange={setValue}
      disabled={disabled}
      options={[
        { value: 'a', label: 'Alpha' },
        { value: 'b', label: 'Beta' },
        { value: 'c', label: 'Gamma' },
      ]}
    />
  )
}

describe('Segmented', () => {
  it('is a radio group with one tab stop', () => {
    render(<Demo />)
    expect(screen.getByRole('radiogroup', { name: 'Pick' })).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'Alpha' })).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByRole('radio', { name: 'Alpha' })).toHaveAttribute('tabindex', '0')
    expect(screen.getByRole('radio', { name: 'Beta' })).toHaveAttribute('tabindex', '-1')
  })

  it('moves the selection with arrow keys, Home and End', async () => {
    const user = userEvent.setup()
    render(<Demo />)
    await user.tab()
    expect(screen.getByRole('radio', { name: 'Alpha' })).toHaveFocus()
    await user.keyboard('{ArrowRight}')
    expect(screen.getByRole('radio', { name: 'Beta' })).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByRole('radio', { name: 'Beta' })).toHaveFocus()
    await user.keyboard('{End}')
    expect(screen.getByRole('radio', { name: 'Gamma' })).toHaveAttribute('aria-checked', 'true')
    await user.keyboard('{ArrowRight}')
    expect(screen.getByRole('radio', { name: 'Alpha' })).toHaveAttribute('aria-checked', 'true')
    await user.keyboard('{ArrowLeft}{Home}')
    expect(screen.getByRole('radio', { name: 'Alpha' })).toHaveAttribute('aria-checked', 'true')
  })

  it('can be disabled', async () => {
    const user = userEvent.setup()
    render(<Demo disabled />)
    await user.click(screen.getByRole('radio', { name: 'Beta' }))
    expect(screen.getByRole('radio', { name: 'Alpha' })).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByRole('radio', { name: 'Beta' })).toBeDisabled()
  })
})
