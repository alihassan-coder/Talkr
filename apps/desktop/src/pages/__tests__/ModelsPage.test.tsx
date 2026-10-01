import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { catalogFixture, emitEvent, flush, hardwareFixture, mockBackend, reject } from '@/test/tauri'

const GIB = 1024 ** 3
const catalog = [
  catalogFixture({ id: 'whisper-base', name: 'Whisper Base (English)', tags: ['recommended', 'fast'] }),
  catalogFixture({ id: 'whisper-small', name: 'Whisper Small (English)', tags: ['accurate'], ramRecommendedBytes: 6 * GIB }),
  catalogFixture({
    id: 'whisper-small-q5',
    name: 'Whisper Small (English) (compressed)',
    tags: ['accurate', 'quantized', 'low-memory'],
    ramRecommendedBytes: 1.2 * GIB,
  }),
  catalogFixture({ id: 'kokoro', kind: 'tts', name: 'Kokoro', engine: 'sherpa-onnx' }),
]

let installedNow: unknown[] = []
const handlers = (extra: Record<string, unknown> = {}) => ({
  list_catalog: catalog,
  list_installed_models: () => installedNow,
  get_hardware_info: hardwareFixture({ ramBytes: 4 * GIB }),
  get_storage_usage: { modelsBytes: 0, audioBytes: 0, dbBytes: 0, totalBytes: 0 },
  download_model: 'job-1',
  cancel_download: null,
  delete_model: null,
  ...extra,
})

const progress = (patch: Record<string, unknown>) => ({
  jobId: 'job-1',
  modelId: 'whisper-base',
  receivedBytes: 0,
  totalBytes: 0,
  bytesPerSec: 0,
  etaSec: null,
  state: 'downloading',
  error: null,
  ...patch,
})

// Fresh store per test: it keeps module-level state.
let ModelsPage: typeof import('@/pages/ModelsPage').ModelsPage
beforeEach(async () => {
  installedNow = []
  vi.resetModules()
  ModelsPage = (await import('@/pages/ModelsPage')).ModelsPage
})

const row = (name: RegExp) => screen.getByText(name).closest('li')!

async function renderPage() {
  render(
    <MemoryRouter>
      <ModelsPage />
    </MemoryRouter>,
  )
  await screen.findByText('Whisper Base')
  await flush()
}

describe('ModelsPage', () => {
  it('lists speech-to-text models, recommended first, with hardware facts', async () => {
    mockBackend(handlers())
    await renderPage()
    const items = within(screen.getByRole('list')).getAllByRole('listitem')
    expect(items).toHaveLength(3)
    expect(items[0]).toHaveTextContent('Recommended')
    expect(screen.getByText('4 GB')).toBeInTheDocument()
    expect(screen.getByText('0 of 3 installed', { exact: false })).toBeInTheDocument()
  })

  it('marks compressed variants and points to them when memory is short', async () => {
    mockBackend(handlers())
    await renderPage()
    // Two rows share the base name; the compressed one carries the hint.
    const rows = screen.getAllByText('Whisper Small').map((el) => el.closest('li')!)
    expect(rows).toHaveLength(2)
    const full = rows[0]!
    expect(rows[1]).toHaveTextContent('Compressed · uses less memory')
    expect(rows[1]).not.toHaveTextContent('(compressed)')
    expect(rows[0]).not.toHaveTextContent('Compressed')
    expect(full).toHaveTextContent('Needs more memory than this computer has')
    expect(full).toHaveTextContent('the compressed version fits')
  })

  it('shows download progress and then the installed state', async () => {
    const backend = mockBackend(handlers())
    const user = userEvent.setup()
    await renderPage()
    await user.click(within(row(/^Whisper Base$/)).getByRole('button', { name: /Download/ }))
    expect(backend.argsOf('download_model')).toEqual([{ modelId: 'whisper-base' }])
    expect(within(row(/^Whisper Base$/)).getByText('Starting')).toBeInTheDocument()

    await emitEvent('download://progress', progress({ receivedBytes: 75_000_000, totalBytes: 150_000_000, bytesPerSec: 5_000_000 }))
    const base = row(/^Whisper Base$/)
    expect(within(base).getByText('50%')).toBeInTheDocument()
    expect(within(base).getByText(/75 MB \/ 150 MB · 5 MB\/s/)).toBeInTheDocument()
    expect(within(base).getByRole('progressbar')).toHaveAttribute('aria-valuenow', '50')

    await emitEvent('download://progress', progress({ state: 'extracting' }))
    expect(within(row(/^Whisper Base$/)).getByText('Unpacking')).toBeInTheDocument()

    installedNow = [
      {
        id: 'whisper-base',
        kind: 'stt',
        name: 'Whisper Base',
        engine: 'whisper',
        path: 'x',
        manifest: { id: 'whisper-base', version: '1', sha256: 'a', installedAt: 0, sizeBytes: 1, files: [] },
      },
    ]
    await emitEvent('download://progress', progress({ state: 'installed' }))
    await flush()
    expect(within(row(/^Whisper Base$/)).getByText('Installed')).toBeInTheDocument()
    expect(within(row(/^Whisper Base$/)).getByRole('button', { name: 'Delete model' })).toBeInTheDocument()
  })

  it.each([
    'Not enough disk space to install Whisper Base: needs about 160 MB free on D:, only 90 MB available. Free up space or delete other models in Models.',
    'Not enough free memory to run whisper-base: it needs about 1.1 GB, and only 700 MB is free. Close other apps, or pick a smaller or compressed model in Models.',
  ])('shows a failed download inline: %s', async (message) => {
    mockBackend(handlers())
    const user = userEvent.setup()
    await renderPage()
    await user.click(within(row(/^Whisper Base$/)).getByRole('button', { name: /Download/ }))
    await emitEvent('download://progress', progress({ state: 'failed', error: message }))
    const base = row(/^Whisper Base$/)
    expect(within(base).getByRole('alert')).toHaveTextContent(message)
    expect(within(base).getByRole('button', { name: /Try again/ })).toBeInTheDocument()
    await user.click(within(base).getByRole('button', { name: 'Dismiss error' }))
    expect(within(row(/^Whisper Base$/)).queryByRole('alert')).not.toBeInTheDocument()
  })

  it('shows an error when the download command is rejected up front', async () => {
    mockBackend(handlers({ download_model: reject('Not enough disk space to install Whisper Base.') }))
    const user = userEvent.setup()
    await renderPage()
    await user.click(within(row(/^Whisper Base$/)).getByRole('button', { name: /Download/ }))
    expect(await within(row(/^Whisper Base$/)).findByRole('alert')).toHaveTextContent('Not enough disk space')
  })

  it('cancels a download', async () => {
    const backend = mockBackend(handlers())
    const user = userEvent.setup()
    await renderPage()
    await user.click(within(row(/^Whisper Base$/)).getByRole('button', { name: /Download/ }))
    await user.click(screen.getByRole('button', { name: 'Cancel download' }))
    expect(backend.argsOf('cancel_download')).toEqual([{ jobId: 'job-1' }])
    expect(within(row(/^Whisper Base$/)).getByRole('button', { name: /Download/ })).toBeInTheDocument()
  })

  it('deletes an installed model after confirming', async () => {
    const backend = mockBackend(handlers({ list_catalog: catalog.map((m) => ({ ...m, installed: m.id === 'whisper-base' })) }))
    const user = userEvent.setup()
    await renderPage()
    await user.click(within(row(/^Whisper Base$/)).getByRole('button', { name: 'Delete model' }))
    await user.click(screen.getByRole('button', { name: 'Delete?' }))
    expect(backend.argsOf('delete_model')).toEqual([{ modelId: 'whisper-base' }])
  })

  it('switches to text-to-speech models', async () => {
    mockBackend(handlers())
    const user = userEvent.setup()
    await renderPage()
    await user.click(screen.getByRole('radio', { name: 'Text to speech' }))
    expect(screen.getByText('Kokoro')).toBeInTheDocument()
    expect(screen.queryByText('Whisper Base')).not.toBeInTheDocument()
  })

  it('shows a load error with a retry', async () => {
    mockBackend(handlers({ list_catalog: reject('catalog.json is missing') }))
    render(
      <MemoryRouter>
        <ModelsPage />
      </MemoryRouter>,
    )
    expect(await screen.findByText('Could not load models')).toBeInTheDocument()
    expect(screen.getByText('catalog.json is missing')).toBeInTheDocument()
  })

  it('opens the list asked for in the link', async () => {
    mockBackend(handlers())
    render(
      <MemoryRouter initialEntries={['/models?kind=tts']}>
        <ModelsPage />
      </MemoryRouter>,
    )
    expect(await screen.findByText('Kokoro')).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'Text to speech' })).toHaveAttribute('aria-checked', 'true')
  })

  it('names download progress for screen readers', async () => {
    mockBackend(handlers())
    const user = userEvent.setup()
    await renderPage()
    await user.click(within(row(/^Whisper Base$/)).getByRole('button', { name: /Download/ }))
    await emitEvent('download://progress', progress({ receivedBytes: 30, totalBytes: 100 }))
    const bar = screen.getByRole('progressbar', { name: 'Downloading Whisper Base (English)' })
    expect(bar).toHaveAttribute('aria-valuetext', '30%')
  })

  it('moves through the import menu with the keyboard', async () => {
    mockBackend(handlers())
    const user = userEvent.setup()
    await renderPage()
    const trigger = screen.getByRole('button', { name: 'Import model' })
    await user.click(trigger)
    const items = within(screen.getByRole('menu', { name: 'Import model' })).getAllByRole('menuitem')
    expect(items[0]).toHaveFocus()
    await user.keyboard('{ArrowDown}')
    expect(items[1]).toHaveFocus()
    await user.keyboard('{ArrowDown}')
    expect(items[0]).toHaveFocus()
    await user.keyboard('{ArrowUp}')
    expect(items[1]).toHaveFocus()
    await user.keyboard('{Home}')
    expect(items[0]).toHaveFocus()
    await user.keyboard('{End}')
    expect(items[1]).toHaveFocus()
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('menu')).not.toBeInTheDocument()
    expect(trigger).toHaveFocus()
  })
})
