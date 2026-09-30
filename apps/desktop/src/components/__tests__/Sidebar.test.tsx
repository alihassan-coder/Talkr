import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router'
import { afterEach, describe, expect, it } from 'vitest'
import { App } from '@/App'
import { Sidebar } from '@/components/Sidebar'
import { UI_STORAGE_KEY, useUi } from '@/stores/ui'

afterEach(() => useUi.setState({ sidebarCollapsed: false }))

const renderSidebar = () =>
  render(
    <MemoryRouter initialEntries={['/transcribe']}>
      <Sidebar footer={<p>status card</p>} />
    </MemoryRouter>,
  )

describe('Sidebar', () => {
  it('shows the navigation with the current page marked', () => {
    renderSidebar()
    const nav = screen.getByRole('navigation', { name: 'Main' })
    expect(nav).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /Transcribe/ })).toHaveAttribute('aria-current', 'page')
    expect(screen.getByRole('link', { name: /Speak/ })).not.toHaveAttribute('aria-current')
    expect(screen.getByRole('link', { name: /Settings/ })).toHaveAttribute('href', '/settings')
  })

  it('collapses and expands with the toggle button, and remembers it', async () => {
    const user = userEvent.setup()
    renderSidebar()
    const collapse = screen.getByRole('button', { name: 'Collapse sidebar' })
    expect(collapse).toHaveAttribute('aria-expanded', 'true')
    await user.click(collapse)
    expect(useUi.getState().sidebarCollapsed).toBe(true)
    expect(JSON.parse(localStorage.getItem(UI_STORAGE_KEY)!).state.sidebarCollapsed).toBe(true)
    // Collapsed: icon links are named by aria-label and the footer is hidden from assistive tech.
    expect(screen.getByRole('link', { name: 'Models' })).toBeInTheDocument()
    expect(screen.getByText('status card').closest('[aria-hidden]')).toHaveAttribute('aria-hidden', 'true')
    const expand = screen.getByRole('button', { name: 'Expand sidebar' })
    expect(expand).toHaveAttribute('aria-expanded', 'false')
    await user.click(expand)
    expect(useUi.getState().sidebarCollapsed).toBe(false)
  })

  it('toggles with Ctrl+B anywhere in the app', async () => {
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/history']}>
        <App />
      </MemoryRouter>,
    )
    expect(screen.getByRole('button', { name: 'Collapse sidebar' })).toBeInTheDocument()
    await user.keyboard('{Control>}b{/Control}')
    expect(useUi.getState().sidebarCollapsed).toBe(true)
    expect(screen.getByRole('button', { name: 'Expand sidebar' })).toBeInTheDocument()
    await user.keyboard('{Control>}B{/Control}')
    expect(useUi.getState().sidebarCollapsed).toBe(false)
    // Ctrl+Shift+B and a plain "b" do nothing.
    await user.keyboard('{Control>}{Shift>}b{/Shift}{/Control}b')
    expect(useUi.getState().sidebarCollapsed).toBe(false)
  })

  it('navigates with Ctrl+number shortcuts', async () => {
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/history']}>
        <App />
      </MemoryRouter>,
    )
    await user.keyboard('{Control>}3{/Control}')
    expect(await screen.findByRole('heading', { name: 'Models' })).toBeInTheDocument()
  })
})
