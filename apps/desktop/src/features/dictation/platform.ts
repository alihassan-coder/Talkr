import { createContext, use } from 'react'
import type { DictationCapabilities } from '@/lib/types'
import { windowsKeys } from '@/features/dictation/shortcut'
import type { KeyPlatform, Os } from '@/features/dictation/shortcut'

/**
 * Makes every keycap below it speak the system's language (⌃ ⌥ ⇧ ⌘ on a Mac, Super on Linux).
 * The Dictation page provides it from the backend's capabilities.
 */
export const KeyPlatformContext = createContext<KeyPlatform>(windowsKeys)

export const useKeyPlatform = () => use(KeyPlatformContext)

export const keyPlatformOf = (c: DictationCapabilities): KeyPlatform => ({
  os: c.os,
  metaKey: c.metaKey,
  modifierOnly: c.modifierOnly,
})

export const osName: Record<Os, string> = { windows: 'Windows', macos: 'macOS', linux: 'Linux' }

/** Words that differ per system. */
export const osWords = (os: Os) => ({
  login: os === 'windows' ? 'Start with Windows' : os === 'macos' ? 'Open at login' : 'Start when you log in',
  tray: os === 'macos' ? 'the menu bar' : 'the tray',
  appPlaceholder:
    os === 'windows' ? 'App, e.g. mstsc.exe' : os === 'macos' ? 'Bundle id, e.g. com.microsoft.rdc.macos' : 'App, e.g. remmina',
  appHint:
    os === 'windows'
      ? 'Type the program name; “.exe” is added for you.'
      : os === 'macos'
        ? 'Use the app’s bundle id.'
        : 'Use the app’s WM_CLASS or app id.',
})
