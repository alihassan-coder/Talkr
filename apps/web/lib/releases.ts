export const VERSION = '0.1.5'
export const REPO_URL = 'https://github.com/alihassan-coder/Talkr'
export const RELEASES_URL = `${REPO_URL}/releases`
export const ISSUES_URL = `${REPO_URL}/issues`

/** Tauri puts the version in every file name, so links are pinned to this version's tag. */
export const downloadUrl = (file: string) => `${RELEASES_URL}/download/v${VERSION}/${file}`

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
      { name: `Talkr_${VERSION}_x64-setup.exe`, format: 'Installer', arch: 'x64', size: '15 MB' },
      { name: `Talkr_${VERSION}_x64_en-US.msi`, format: 'MSI package', arch: 'x64', size: '31 MB' },
    ],
  },
  {
    id: 'macos-arm',
    name: 'macOS',
    detail: 'Apple Silicon',
    requirement: 'macOS 11 or later, M1 and newer',
    files: [{ name: `Talkr_${VERSION}_aarch64.dmg`, format: 'Disk image', arch: 'arm64', size: '12 MB' }],
  },
  {
    id: 'macos-intel',
    name: 'macOS',
    detail: 'Intel',
    requirement: 'macOS 11 or later, Intel Macs',
    files: [{ name: `Talkr_${VERSION}_x64.dmg`, format: 'Disk image', arch: 'x64', size: '14 MB' }],
  },
  {
    id: 'linux',
    name: 'Linux',
    detail: 'x86_64',
    requirement: 'glibc 2.31 or later',
    files: [
      { name: `Talkr_${VERSION}_amd64.AppImage`, format: 'AppImage', arch: 'x86_64', size: '109 MB' },
      { name: `Talkr_${VERSION}_amd64.deb`, format: 'Debian, Ubuntu', arch: 'x86_64', size: '28 MB' },
      { name: `Talkr-${VERSION}-1.x86_64.rpm`, format: 'Fedora, openSUSE', arch: 'x86_64', size: '28 MB' },
    ],
  },
]

/** Where a Mac goes when the browser won't say which chip it has: both Mac downloads, side by side. */
export const MAC_CHOOSER = '/download#macos-arm'

export type DetectedDownload = {
  label: string
  /** The file to download, or null when the visitor has to pick (a Mac of unknown architecture). */
  file: ReleaseFile | null
  href: string
}

type MacArch = 'arm' | 'intel' | null

type UserAgentData = {
  platform?: string
  getHighEntropyValues?: (hints: string[]) => Promise<{ architecture?: string }>
}

const pick = (id: Platform['id']) => platforms.find((p) => p.id === id)?.files[0] ?? null

const fileDownload = (label: string, file: ReleaseFile | null): DetectedDownload | null =>
  file ? { label, file, href: downloadUrl(file.name) } : null

const macDownload = (arch: MacArch): DetectedDownload => {
  const file = arch === 'arm' ? pick('macos-arm') : arch === 'intel' ? pick('macos-intel') : null
  return file ? { label: 'macOS', file, href: downloadUrl(file.name) } : { label: 'macOS', file: null, href: MAC_CHOOSER }
}

let cached: DetectedDownload | null | undefined
const listeners = new Set<() => void>()

/**
 * Best guess from the browser, computed once. Returns null on phones and unknown systems. A Mac
 * starts on the chooser and switches to its disk image once the architecture is known
 * (`subscribeDownload` hears about that).
 */
export function detectDownload(): DetectedDownload | null {
  if (cached === undefined) {
    cached = detect()
    if (cached?.label === 'macOS' && !cached.file) {
      void detectMacArch().then((arch) => {
        if (!arch) return
        cached = macDownload(arch)
        listeners.forEach((notify) => notify())
      })
    }
  }
  return cached
}

export function subscribeDownload(onChange: () => void) {
  listeners.add(onChange)
  return () => {
    listeners.delete(onChange)
  }
}

function detect(): DetectedDownload | null {
  const nav = navigator as Navigator & { userAgentData?: UserAgentData }
  const ua = nav.userAgent.toLowerCase()
  const platform = (nav.userAgentData?.platform ?? '').toLowerCase()

  if (/android|iphone|ipad|ipod/.test(ua)) return null

  if (platform.includes('win') || ua.includes('windows')) return fileDownload('Windows', pick('windows'))
  if (platform.includes('mac') || ua.includes('mac os')) return macDownload(null)
  if (platform.includes('linux') || ua.includes('linux')) return fileDownload('Linux', pick('linux'))
  return null
}

/**
 * Every Mac browser claims "Intel Mac OS X", so ask Chromium's client hints, then the GPU name
 * (Firefox and Chromium report "Apple M1" and the like). Safari reports only "Apple GPU": unknown.
 */
export async function detectMacArch(): Promise<MacArch> {
  const nav = navigator as Navigator & { userAgentData?: UserAgentData }
  try {
    const hints = await nav.userAgentData?.getHighEntropyValues?.(['architecture'])
    const arch = hints?.architecture?.toLowerCase()
    if (arch === 'arm') return 'arm'
    if (arch === 'x86') return 'intel'
  } catch {
    // Hints refused: fall through to the GPU.
  }
  return archFromRenderer(webglRenderer())
}

export function archFromRenderer(renderer: string | null): MacArch {
  if (!renderer) return null
  if (/apple m\d/i.test(renderer)) return 'arm'
  if (/intel|amd|radeon|nvidia|geforce/i.test(renderer) && !/apple gpu/i.test(renderer)) return 'intel'
  return null
}

function webglRenderer(): string | null {
  try {
    if (typeof document === 'undefined') return null
    const gl = document.createElement('canvas').getContext('webgl') as WebGLRenderingContext | null
    if (!gl) return null
    const info = gl.getExtension('WEBGL_debug_renderer_info')
    const renderer = gl.getParameter(info ? info.UNMASKED_RENDERER_WEBGL : gl.RENDERER) as unknown
    return typeof renderer === 'string' ? renderer : null
  } catch {
    return null
  }
}
