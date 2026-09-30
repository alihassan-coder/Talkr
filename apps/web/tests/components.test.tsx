// @vitest-environment jsdom
import { render, screen, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { models, sttModels, ttsModels } from '@/lib/models'
import { VERSION } from '@/lib/releases'

describe('DownloadButton', () => {
  const renderWith = async (userAgent: string) => {
    vi.resetModules()
    vi.stubGlobal('navigator', { userAgent })
    const { DownloadButton } = await import('@/components/DownloadButton')
    render(<DownloadButton />)
    return screen.getByRole('link')
  }

  it('links straight to the installer for the visitor platform', async () => {
    const link = await renderWith('Mozilla/5.0 (Windows NT 10.0; Win64; x64)')
    expect(link).toHaveTextContent('Download for Windows')
    expect(link).toHaveAttribute(
      'href',
      `https://github.com/alihassan-coder/Talkr/releases/download/v${VERSION}/Talkr_${VERSION}_x64-setup.exe`,
    )
  })

  it('falls back to the download page on unknown systems', async () => {
    const link = await renderWith('Mozilla/5.0 (iPhone)')
    expect(link).toHaveTextContent('Download Talkr')
    expect(link).toHaveAttribute('href', '/download')
  })
})

describe('Models section', () => {
  it('lists every catalog model with its size and marks compressed ones', async () => {
    const { Models } = await import('@/components/home/Models')
    render(<Models />)
    const stt = screen.getByRole('heading', { name: 'Speech to text' }).closest('div')!.parentElement!
    const tts = screen.getByRole('heading', { name: 'Text to speech' }).closest('div')!.parentElement!
    expect(within(stt).getAllByRole('listitem')).toHaveLength(sttModels.length)
    expect(within(tts).getAllByRole('listitem')).toHaveLength(ttsModels.length)
    const compressedCount = models.filter((m) => m.tags.includes('quantized')).length
    expect(screen.getAllByText('Compressed · less memory')).toHaveLength(compressedCount)
    expect(screen.queryByText(/\(compressed\)/)).not.toBeInTheDocument()
    expect(screen.getByText(`${models.length} open models.`, { exact: false })).toBeInTheDocument()
  })
})
