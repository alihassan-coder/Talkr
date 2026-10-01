/** True on macOS, where shortcuts use ⌘ instead of Ctrl. */
export const isMac = () => typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.userAgent)

/** The modifier key's label for shortcut hints. */
export const modKey = () => (isMac() ? '⌘' : 'Ctrl')
