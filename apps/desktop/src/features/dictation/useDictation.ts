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
  active: false,
  error: null,
  modelId: null,
  warm: false,
  hasLast: false,
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

/**
 * The settings and live status behind the Dictation page. Saves are optimistic: the page
 * changes at once, and a failed save puts back what was there (unless a newer change landed).
 */
export function useDictation() {
  const tauri = isTauri()
  const [settings, setSettings] = useState<Settings | null>(() => (tauri ? null : previewSettings))
  const [error, setError] = useState<string | null>(null)
  const [status, setStatus] = useState<DictationStatus>(previewStatus)
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
      .then((s) => alive && setStatus(s))
      .catch(() => {})
    const unlisten = [
      onDictationStatus((s) => alive && setStatus(s)),
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

  const refreshStatus = useCallback(() => {
    if (!isTauri()) return
    dictationStatus()
      .then(setStatus)
      .catch(() => {})
  }, [])

  /** Change some dictation settings. Resolves to whether it was saved. */
  const save = useCallback(
    async (patch: Partial<DictationSettings>): Promise<boolean> => {
      const previous = latest.current
      if (!previous) return false
      const next = { ...previous, ...patch }
      latest.current = next
      setSettings((cur) => (cur ? { ...cur, dictation: { ...cur.dictation, ...patch } } : cur))
      if (!isTauri()) return true
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
    settings,
    dictation: settings?.dictation ?? null,
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
