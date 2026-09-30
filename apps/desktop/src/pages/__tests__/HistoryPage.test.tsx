import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'
import { HistoryPage } from '@/pages/HistoryPage'
import { historyFixture, mockBackend, pathsFixture } from '@/test/tauri'
import type { HistoryItem } from '@/lib/types'

const items: HistoryItem[] = [
  historyFixture({ id: 'a', title: 'Q3 planning call', text: 'Revenue is up eleven percent', kind: 'stt' }),
  historyFixture({ id: 'b', title: 'Welcome message', text: 'Welcome to Talkr', kind: 'tts', voiceId: 'af_heart' }),
]

function backendWith(list: HistoryItem[]) {
  let current = [...list]
  return mockBackend({
    get_app_paths: pathsFixture,
    history_list: (args: Record<string, unknown>) => {
      const q = String(args.query ?? '').toLowerCase()
      const filtered = current.filter(
        (i) =>
          (!q || `${i.title} ${i.text}`.toLowerCase().includes(q)) &&
          (!args.kind || i.kind === args.kind) &&
          (!args.favoritesOnly || i.favorite),
      )
      return { items: filtered, nextCursor: null }
    },
    history_delete: (args: Record<string, unknown>) => {
      current = current.filter((i) => i.id !== args.id)
      return null
    },
    history_toggle_favorite: (args: Record<string, unknown>) => !current.find((i) => i.id === args.id)?.favorite,
  })
}

const renderPage = () =>
  render(
    <MemoryRouter>
      <HistoryPage />
    </MemoryRouter>,
  )

describe('HistoryPage', () => {
  it('lists items grouped by day', async () => {
    const backend = backendWith(items)
    renderPage()
    expect(await screen.findByText('Q3 planning call')).toBeInTheDocument()
    expect(screen.getByText('Welcome message')).toBeInTheDocument()
    expect(screen.getByText('Today')).toBeInTheDocument()
    expect(backend.argsOf('history_list')[0]).toEqual({ cursor: null, query: '', kind: null, favoritesOnly: false })
  })

  it('shows the empty state', async () => {
    backendWith([])
    renderPage()
    expect(await screen.findByText('Your history is empty')).toBeInTheDocument()
  })

  it('searches with a debounce and shows no-results with a way back', async () => {
    const backend = backendWith(items)
    const user = userEvent.setup()
    renderPage()
    await screen.findByText('Q3 planning call')
    await user.type(screen.getByRole('searchbox', { name: 'Search history' }), 'revenue')
    await waitFor(() => expect(screen.queryByText('Welcome message')).not.toBeInTheDocument())
    expect(screen.getByText('Q3 planning call')).toBeInTheDocument()
    expect(screen.queryByText('Welcome message')).not.toBeInTheDocument()
    expect(backend.argsOf('history_list').at(-1)).toMatchObject({ query: 'revenue' })

    await user.clear(screen.getByRole('searchbox', { name: 'Search history' }))
    await user.type(screen.getByRole('searchbox', { name: 'Search history' }), 'zzz')
    expect(await screen.findByText('No results')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Clear filters' }))
    expect(await screen.findByText('Welcome message')).toBeInTheDocument()
  })

  it('filters by kind', async () => {
    const backend = backendWith(items)
    const user = userEvent.setup()
    renderPage()
    await screen.findByText('Q3 planning call')
    await user.click(screen.getByRole('tab', { name: 'Speech' }))
    expect(await screen.findByText('Welcome message')).toBeInTheDocument()
    expect(screen.queryByText('Q3 planning call')).not.toBeInTheDocument()
    expect(backend.argsOf('history_list').at(-1)).toMatchObject({ kind: 'tts' })
  })

  it('deletes an item after confirming', async () => {
    const backend = backendWith(items)
    const user = userEvent.setup()
    renderPage()
    await user.click(await screen.findByRole('button', { name: /Q3 planning call/ }))
    await user.click(screen.getByRole('button', { name: 'Delete' }))
    await user.click(screen.getByRole('button', { name: 'Click again to delete' }))
    expect(backend.argsOf('history_delete')).toEqual([{ id: 'a' }])
    expect(screen.queryByText('Q3 planning call')).not.toBeInTheDocument()
    expect(screen.getByText('Welcome message')).toBeInTheDocument()
  })

  it('toggles a favorite', async () => {
    const backend = backendWith(items)
    const user = userEvent.setup()
    renderPage()
    await screen.findByText('Q3 planning call')
    await user.click(screen.getAllByRole('button', { name: 'Add to favorites' })[0]!)
    expect(backend.argsOf('history_toggle_favorite')).toEqual([{ id: 'a' }])
    expect(await screen.findByRole('button', { name: 'Remove from favorites' })).toBeInTheDocument()
  })
})
