export const VERSION = '0.1.0'
export const REPO_URL = 'https://github.com/talkr/talkr'
export const RELEASES_URL = `${REPO_URL}/releases`
export const ISSUES_URL = `${REPO_URL}/issues`
export const CONTACT_EMAIL = 'hello@talkr.app'

export const downloadUrl = (file: string) => `${RELEASES_URL}/latest/download/${file}`

export type ReleaseFile = {
  name: string
  format: string
  arch: string
  size: string
}

export type Platform = {
  id: 'windows' | 'macos-arm' | 'macos-intel' | 'linux'
  name: string
  detail: string
  requirement: string
  files: ReleaseFile[]
}

export const platforms: Platform[] = [
  {
    id: 'windows',
    name: 'Windows',
    detail: '10 and 11',
    requirement: 'Windows 10 or later, x64',
    files: [
      { name: 'talkr-x64.msi', format: 'Installer', arch: 'x64', size: '~120 MB' },
      { name: 'talkr-x64.exe', format: 'Portable', arch: 'x64', size: '~115 MB' },
    ],
  },
  {
    id: 'macos-arm',
    name: 'macOS',
    detail: 'Apple Silicon',
    requirement: 'macOS 11 or later, M1 and newer',
    files: [{ name: 'talkr-aarch64.dmg', format: 'Disk image', arch: 'arm64', size: '~110 MB' }],
  },
  {
    id: 'macos-intel',
    name: 'macOS',
    detail: 'Intel',
    requirement: 'macOS 11 or later, Intel Macs',
    files: [{ name: 'talkr-x64.dmg', format: 'Disk image', arch: 'x64', size: '~110 MB' }],
  },
  {
    id: 'linux',
    name: 'Linux',
    detail: 'x86_64',
    requirement: 'glibc 2.31 or later',
    files: [
      { name: 'talkr-x86_64.AppImage', format: 'AppImage', arch: 'x86_64', size: '~115 MB' },
      { name: 'talkr-x86_64.deb', format: 'Debian, Ubuntu', arch: 'x86_64', size: '~115 MB' },
      { name: 'talkr-x86_64.rpm', format: 'Fedora, openSUSE', arch: 'x86_64', size: '~115 MB' },
    ],
  },
]

export type DetectedDownload = {
  label: string
  file: ReleaseFile
}

let cached: DetectedDownload | null | undefined

/** Best guess from the browser, computed once. Returns null on phones and unknown systems. */
export function detectDownload(): DetectedDownload | null {
  if (cached === undefined) cached = detect()
  return cached
}

function detect(): DetectedDownload | null {
  const nav = navigator as Navigator & { userAgentData?: { platform?: string } }
  const ua = nav.userAgent.toLowerCase()
  const platform = (nav.userAgentData?.platform ?? '').toLowerCase()

  if (/android|iphone|ipad|ipod/.test(ua)) return null

  const pick = (id: Platform['id']) => platforms.find((p) => p.id === id)?.files[0]

  if (platform.includes('win') || ua.includes('windows')) {
    const file = pick('windows')
    return file ? { label: 'Windows', file } : null
  }
  if (platform.includes('mac') || ua.includes('mac os')) {
    const file = pick('macos-arm')
    return file ? { label: 'macOS', file } : null
  }
  if (platform.includes('linux') || ua.includes('linux')) {
    const file = pick('linux')
    return file ? { label: 'Linux', file } : null
  }
  return null
}
