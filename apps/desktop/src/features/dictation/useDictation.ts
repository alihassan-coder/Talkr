import { useCallback, useEffect, useRef, useState } from 'react'
import {
  dictationStatus,
  getSettings,
  isTauri,
  onDictationStatus,
  onSettingsChanged,
  updateSettings,
} from '@/lib/api'
import type { DictationSettings, DictationStatus, Settings } from '@/lib/types'
import { errorText } from '@/lib/errors'
import { toastError } from '@/stores/toast'
import { defaultDictation } from '@/features/dictation/shortcut'

const previewStatus: DictationStatus = {
  supported: true,
  capabilities: {
    os: 'windows',
    supported: true,
    holdToTalk: true,
    modifierOnly: true,
    recordsShortcut: true,
    verifiesInsertion: true,
    insertsText: true,
    metaKey: 'Win',
    note: null,
  },
  permission: { state: 'notNeeded' },
  active: false,
  error: null,
  modelId: null,
  warm: false,
  hasLast: false,
  recording: false,
}

/**
 * In a plain browser during development, the page can be shown as another system would see it:
 * `#/dictation?os=macos&permission=1`, `?os=wayland`, `?model=1&on=1`.
 */
function previewFromUrl(): { status: DictationStatus; enabled: boolean } {
  const query = typeof window === 'undefined' ? '' : (window.location.hash.split('?')[1] ?? window.location.search.slice(1))
  const q = new URLSearchParams(import.meta.env.DEV ? query : '')
  const os = q.get('os')
  const caps = { ...previewStatus.capabilities }
  if (os === 'macos') Object.assign(caps, { os: 'macos', metaKey: '⌘', modifierOnly: true })
  if (os === 'linux') Object.assign(caps, { os: 'linux', metaKey: 'Super', verifiesInsertion: false })
  if (os === 'wayland')
    Object.assign(caps, {
      os: 'linux',
      metaKey: 'Super',
      holdToTalk: false,
      modifierOnly: false,
      recordsShortcut: false,
      verifiesInsertion: false,
      insertsText: false,
      note: 'Wayland session: your desktop sets the shortcut, and text is copied for you to paste.',
    })
  const status: DictationStatus = {
    ...previewStatus,
    capabilities: caps,
    permission: q.has('permission')
      ? {
          state: 'missing',
          title: 'Accessibility',
          detail: 'Talkr needs Accessibility access to hear your shortcut and type into other apps.',
          canRequest: true,
        }
      : previewStatus.permission,
    modelId: q.has('model') ? 'whisper-small-en-q5' : null,
    warm: q.has('model'),
    active: q.has('on'),
    supported: q.get('supported') !== '0',
  }
  status.capabilities.supported = status.supported
  return { status, enabled: q.has('on') }
}

/** Shown in a plain browser, where there is no backend. */
const previewSettings: Settings = {
  version: 1,
  device: 'auto',
  cpuThreads: 0,
  defaultTtsModel: null,
  defaultVoice: null,
  defaultSttModel: null,
  sttLanguage: 'auto',
  sttQuality: 'auto',
  speechRate: 1,
  historyRetentionDays: 0,
  saveRecordings: true,
  dictation: defaultDictation,
}

/** How often the page asks for the status while it waits on the system. */
export const STATUS_POLL_MS = 3000

/**
 * The settings and live status behind the Dictation page. Saves are optimistic: the page
 * changes at once, and a failed save puts back what was there (unless a newer change landed).
 */
export function useDictation() {
  const tauri = isTauri()
  const [preview] = useState(previewFromUrl)
  const [settings, setSettings] = useState<Settings | null>(() =>
    tauri ? null : { ...previewSettings, dictation: { ...previewSettings.dictation, enabled: preview.enabled } },
  )
  const [error, setError] = useState<string | null>(null)
  const [status, setStatus] = useState<DictationStatus>(preview.status)
  // In the app, the page waits for the real status: the preview's Windows keys and rules would
  // flash on a Mac or Linux.
  const [statusLoaded, setStatusLoaded] = useState(!tauri)
  const [saving, setSaving] = useState<'idle' | 'saving' | 'saved'>('idle')
  const seq = useRef(0)
  // The newest dictation settings, including a change not rendered yet: two quick changes in a
  // row must build on each other.
  const latest = useRef<DictationSettings | null>(null)
  const queue = useRef<Promise<unknown>>(Promise.resolve())
  const [attempt, setAttempt] = useState(0)

  useEffect(() => {
    latest.current = settings?.dictation ?? null
  }, [settings])

  useEffect(() => {
    if (!tauri) return
    let alive = true
    getSettings()
      .then((s) => alive && setSettings(s))
      .catch((e: unknown) => alive && setError(errorText(e)))
    dictationStatus()
      .then((s) => {
        if (!alive) return
        setStatus(s)
        setStatusLoaded(true)
      })
      .catch((e: unknown) => alive && setError(errorText(e)))
    const unlisten = [
      onDictationStatus((s) => {
        if (!alive) return
        setStatus(s)
        setStatusLoaded(true)
      }),
      onSettingsChanged((s) => alive && setSettings(s)),
    ]
    return () => {
      alive = false
      for (const p of unlisten) void p.then((f) => f()).catch(() => {})
    }
  }, [tauri, attempt])

  useEffect(() => {
    if (saving !== 'saved') return
    const t = setTimeout(() => setSaving('idle'), 1600)
    return () => clearTimeout(t)
  }, [saving])

  /** Ask the backend again (after a permission was granted, for one). */
  const refreshStatus = useCallback(() => {
    if (!isTauri()) return
    dictationStatus()
      .then(setStatus)
      .catch(() => {})
  }, [])

  // Waiting on the system (a permission being granted in its settings, a listener starting up):
  // look again now and then, so the page follows without a click.
  const enabled = settings?.dictation.enabled ?? false
  const waiting =
    tauri && statusLoaded && status.supported && (status.permission.state === 'missing' || (enabled && !status.active))
  useEffect(() => {
    if (!waiting) return
    const t = setInterval(refreshStatus, STATUS_POLL_MS)
    window.addEventListener('focus', refreshStatus)
    return () => {
      clearInterval(t)
      window.removeEventListener('focus', refreshStatus)
    }
  }, [waiting, refreshStatus])

  /** Change some dictation settings. Resolves to whether it was saved. */
  const save = useCallback(
    async (patch: Partial<DictationSettings>): Promise<boolean> => {
      const previous = latest.current
      if (!previous) return false
      const next = { ...previous, ...patch }
      latest.current = next
      setSettings((cur) => (cur ? { ...cur, dictation: { ...cur.dictation, ...patch } } : cur))
      if (!isTauri()) {
        // No backend: pretend the shortcut listener follows the switch.
        if (patch.enabled !== undefined) setStatus((st) => ({ ...st, active: !!patch.enabled && st.supported }))
        return true
      }
      const mine = ++seq.current
      setSaving('saving')
      try {
        // One save at a time, each sending the newest settings: two in flight could land in
        // the wrong order and leave the older one saved.
        const send = () => updateSettings({ settings: { dictation: latest.current ?? next } })
        const turn = queue.current.then(send, send)
        queue.current = turn.catch(() => undefined)
        const saved = await turn
        if (mine === seq.current) {
          setSettings(saved)
          setSaving('saved')
        }
        refreshStatus()
        return true
      } catch (e) {
        // Put back only what still holds the value that failed.
        setSettings((cur) => {
          if (!cur) return cur
          const restored = { ...cur.dictation }
          for (const key of Object.keys(patch) as (keyof DictationSettings)[]) {
            if (cur.dictation[key] === patch[key]) (restored as Record<string, unknown>)[key] = previous[key]
          }
          return { ...cur, dictation: restored }
        })
        if (mine === seq.current) setSaving('idle')
        toastError(e)
        return false
      }
    },
    [refreshStatus],
  )

  return {
    // Both arrive before the page shows (see `statusLoaded`).
    settings: statusLoaded ? settings : null,
    dictation: statusLoaded ? (settings?.dictation ?? null) : null,
    status,
    error,
    saving,
    save,
    refreshStatus,
    retry: () => {
      setError(null)
      setAttempt((n) => n + 1)
    },
  }
}
