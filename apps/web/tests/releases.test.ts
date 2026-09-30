import { readFileSync } from 'node:fs'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ISSUES_URL, RELEASES_URL, REPO_URL, VERSION, downloadUrl, platforms } from '@/lib/releases'

const desktopVersion = (JSON.parse(readFileSync(new URL('../../desktop/package.json', import.meta.url), 'utf8')) as {
  version: string
}).version

const RELEASE_BASE = `https://github.com/alihassan-coder/Talkr/releases/download/v${VERSION}/`

describe('release links', () => {
  it('tracks the desktop app version', () => {
    expect(VERSION).toMatch(/^\d+\.\d+\.\d+$/)
    expect(VERSION).toBe(desktopVersion)
  })

  it('points at the repository', () => {
    expect(REPO_URL).toBe('https://github.com/alihassan-coder/Talkr')
    expect(RELEASES_URL).toBe(`${REPO_URL}/releases`)
    expect(ISSUES_URL).toBe(`${REPO_URL}/issues`)
  })

  it('pins download URLs to the version tag', () => {
    expect(downloadUrl('Talkr.exe')).toBe(`${RELEASE_BASE}Talkr.exe`)
  })

  it('covers Windows, both Macs and Linux', () => {
    expect(platforms.map((p) => p.id)).toEqual(['windows', 'macos-arm', 'macos-intel', 'linux'])
  })

  const files = platforms.flatMap((p) => p.files.map((f) => [p.id, f] as const))

  it.each(files)('%s: %s has a versioned name, a GitHub URL and a size', (_platform, file) => {
    expect(file.name).toContain(VERSION)
    expect(downloadUrl(file.name)).toBe(`${RELEASE_BASE}${file.name}`)
    expect(downloadUrl(file.name)).toMatch(/^https:\/\/github\.com\/alihassan-coder\/Talkr\/releases\/download\/v\d+\.\d+\.\d+\/[\w.-]+$/)
    expect(file.size).toMatch(/^\d+ MB$/)
    expect(file.format).not.toBe('')
    expect(file.arch).not.toBe('')
  })

  it('uses the file extensions each platform expects', () => {
    const names = (id: string) => platforms.find((p) => p.id === id)!.files.map((f) => f.name)
    expect(names('windows').every((n) => /\.(exe|msi)$/.test(n))).toBe(true)
    expect(names('macos-arm')).toEqual([`Talkr_${VERSION}_aarch64.dmg`])
    expect(names('macos-intel')).toEqual([`Talkr_${VERSION}_x64.dmg`])
    expect(names('linux').every((n) => /\.(AppImage|deb|rpm)$/.test(n))).toBe(true)
  })
})

describe('detectDownload', () => {
  afterEach(() => vi.resetModules())

  const detectWith = async (userAgent: string, platform = '') => {
    vi.resetModules()
    vi.stubGlobal('navigator', { userAgent, userAgentData: { platform } })
    const { detectDownload } = await import('@/lib/releases')
    return detectDownload()
  }

  it('picks the Windows installer', async () => {
    const d = await detectWith('Mozilla/5.0 (Windows NT 10.0; Win64; x64)')
    expect(d?.label).toBe('Windows')
    expect(d?.file.name).toBe(`Talkr_${VERSION}_x64-setup.exe`)
  })

  it('picks the Apple Silicon disk image on macOS', async () => {
    const d = await detectWith('Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)')
    expect(d?.label).toBe('macOS')
    expect(d?.file.name).toBe(`Talkr_${VERSION}_aarch64.dmg`)
  })

  it('picks the AppImage on Linux, using client hints when present', async () => {
    const d = await detectWith('Mozilla/5.0 (X11)', 'Linux')
    expect(d?.label).toBe('Linux')
    expect(d?.file.name).toBe(`Talkr_${VERSION}_amd64.AppImage`)
  })

  it('returns null on phones and unknown systems', async () => {
    expect(await detectWith('Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)')).toBeNull()
    expect(await detectWith('Mozilla/5.0 (Linux; Android 14)')).toBeNull()
    expect(await detectWith('SomeBot/1.0')).toBeNull()
  })

  it('computes the guess once', async () => {
    vi.resetModules()
    vi.stubGlobal('navigator', { userAgent: 'Windows' })
    const { detectDownload } = await import('@/lib/releases')
    const first = detectDownload()
    vi.stubGlobal('navigator', { userAgent: 'Macintosh; Mac OS X' })
    expect(detectDownload()).toBe(first)
  })
})
