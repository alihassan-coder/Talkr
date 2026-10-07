import { useCallback, useEffect, useState } from 'react'
import { Navigate, Route, Routes, useLocation, useNavigate } from 'react-router'
import { Sidebar } from '@/components/Sidebar'
import { SystemStatus } from '@/components/SystemStatus'
import { Toaster } from '@/components/Toaster'
import { SpeakPage } from '@/pages/SpeakPage'
import { TranscribePage } from '@/pages/TranscribePage'
import { DictationPage } from '@/pages/DictationPage'
import { ModelsPage } from '@/pages/ModelsPage'
import { HistoryPage } from '@/pages/HistoryPage'
import { SettingsPage } from '@/pages/SettingsPage'
import { UpdateNotice } from '@/features/updates/UpdateNotice'
import { ShortcutsDialog } from '@/features/shortcuts/ShortcutsDialog'
import { useApplyAppearance, useZoom } from '@/lib/appearance'
import { useUi } from '@/stores/ui'

const shortcuts: Record<string, string> = {
  '1': '/speak',
  '2': '/transcribe',
  '3': '/dictation',
  '4': '/models',
  '5': '/history',
  ',': '/settings',
}

/** Ctrl/Cmd + 1-5 and Ctrl/Cmd + , jump between screens, like a native app. Ctrl/Cmd + B toggles the sidebar. */
function useNavigationShortcuts() {
  const navigate = useNavigate()
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey || e.shiftKey) return
      if (e.key.toLowerCase() === 'b') {
        e.preventDefault()
        useUi.getState().toggleSidebar()
        return
      }
      const target = shortcuts[e.key]
      if (!target) return
      e.preventDefault()
      navigate(target)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [navigate])
}

/** Whether a key press is going into a text field (where "?" is just a character). */
const isTyping = (target: EventTarget | null) =>
  target instanceof HTMLElement &&
  (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))

/**
 * The packaged app is not a web page: reload (F5, Ctrl/Cmd+R) would drop running jobs' events,
 * and printing makes no sense. Development builds keep them.
 */
function isBrowserKey(e: KeyboardEvent) {
  const mod = e.ctrlKey || e.metaKey
  const key = e.key.toLowerCase()
  return e.key === 'F5' || (mod && (key === 'r' || key === 'p'))
}

export function App() {
  useNavigationShortcuts()
  useApplyAppearance()
  useZoom()
  const location = useLocation()
  const [shortcutsOpen, setShortcutsOpen] = useState(false)
  const closeShortcuts = useCallback(() => setShortcutsOpen(false), [])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (import.meta.env.PROD && isBrowserKey(e)) {
        e.preventDefault()
        return
      }
      if (e.key === '?' && !e.ctrlKey && !e.metaKey && !e.altKey && !isTyping(e.target)) {
        e.preventDefault()
        setShortcutsOpen(true)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  // Stop the WebView from behaving like a browser: no reload menu, no file drops navigating away.
  useEffect(() => {
    const block = (e: Event) => e.preventDefault()
    window.addEventListener('contextmenu', block)
    window.addEventListener('dragover', block)
    window.addEventListener('drop', block)
    return () => {
      window.removeEventListener('contextmenu', block)
      window.removeEventListener('dragover', block)
      window.removeEventListener('drop', block)
    }
  }, [])

  return (
    <div className="flex h-full">
      <Sidebar footer={<SystemStatus />} />
      <main className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-4xl px-10 pt-6 empty:hidden">
          <UpdateNotice />
        </div>
        <div key={location.pathname} className="mx-auto max-w-4xl animate-rise px-10 py-9">
          <Routes>
            <Route path="/" element={<Navigate to="/speak" replace />} />
            <Route path="/speak" element={<SpeakPage />} />
            <Route path="/transcribe" element={<TranscribePage />} />
            <Route path="/dictation" element={<DictationPage />} />
            <Route path="/models" element={<ModelsPage />} />
            <Route path="/history" element={<HistoryPage />} />
            <Route path="/settings" element={<SettingsPage />} />
            <Route path="*" element={<Navigate to="/speak" replace />} />
          </Routes>
        </div>
      </main>
      {shortcutsOpen ? <ShortcutsDialog onClose={closeShortcuts} /> : null}
      <Toaster />
    </div>
  )
}
