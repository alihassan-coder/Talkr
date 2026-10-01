import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/react'
import { clearMocks } from '@tauri-apps/api/mocks'
import { afterEach, beforeEach, vi } from 'vitest'
import { resetJobs } from '@/stores/jobs'

// jsdom lacks these browser APIs; the app uses them for the colour scheme and the Select list.
function installMatchMedia(dark = true) {
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    writable: true,
    value: vi.fn((query: string) => ({
      matches: query.includes('dark') ? dark : !dark,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  })
}

beforeEach(() => {
  installMatchMedia()
  Element.prototype.scrollIntoView = vi.fn()
  HTMLMediaElement.prototype.play = vi.fn(() => Promise.resolve())
  HTMLMediaElement.prototype.pause = vi.fn()
  HTMLMediaElement.prototype.load = vi.fn()
  URL.createObjectURL = vi.fn(() => 'blob:talkr-audio')
  URL.revokeObjectURL = vi.fn()
})

afterEach(async () => {
  cleanup()
  // Components unsubscribe from events asynchronously; let that finish while the mocks still exist.
  await new Promise((resolve) => setTimeout(resolve, 0))
  // Jobs live in an app-wide store: start every test without them or their event listeners.
  await resetJobs()
  clearMocks()
  // clearMocks keeps the (now empty) internals object; the app treats its presence as "in Tauri".
  delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__
  delete (window as { __TAURI_EVENT_PLUGIN_INTERNALS__?: unknown }).__TAURI_EVENT_PLUGIN_INTERNALS__
  localStorage.clear()
  document.documentElement.removeAttribute('data-theme')
  document.documentElement.removeAttribute('data-mode')
})
