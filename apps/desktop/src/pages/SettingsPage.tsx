import { useEffect, useRef, useState } from 'react'
import { getVersion } from '@tauri-apps/api/app'
import { ArrowUpRight, FolderOpen, RotateCw, Trash2, TriangleAlert } from 'lucide-react'
import {
  getAppPaths,
  getHardwareInfo,
  getSettings,
  getStorageUsage,
  historyClear,
  isTauri,
  listInstalledModels,
  listVoices,
  openDataFolder,
  runRetention,
  updateSettings,
} from '@/lib/api'
import type {
  AppPaths,
  HardwareInfo,
  InstalledModel,
  PartialSettings,
  Settings,
  StorageUsage,
  Voice,
} from '@/lib/types'
import { Button, EmptyState, PageHeader } from '@/components/ui'
import { Notice } from '@/components/Notice'
import { Select } from '@/components/Select'
import { cx } from '@/lib/cx'
import { errorText } from '@/lib/errors'
import { toast, toastError } from '@/stores/toast'
import { Row, Section, Switch } from '@/features/settings/controls'
import { AppearanceSection } from '@/features/settings/Appearance'
import { ComputeSection } from '@/features/settings/Compute'
import { formatBytes } from '@/features/history/utils'

const REPO_URL = 'https://github.com/alihassan-coder/Talkr'

// Mirrors `Settings::default()` in config.rs; used as the browser preview.
const defaultSettings: Settings = {
  version: 1,
  device: 'auto',
  cpuThreads: 0,
  defaultTtsModel: null,
  defaultVoice: null,
  defaultSttModel: null,
  sttLanguage: 'auto',
  speechRate: 1,
  historyRetentionDays: 0,
  saveRecordings: true,
}

/**
 * Undo a failed save field by field: a field goes back to its previous value only while it
 * still holds the value that failed, so a newer change made meanwhile is kept.
 */
function rollbackFields(current: Settings, patch: PartialSettings, previous: Settings): Settings {
  const next: Settings = { ...current }
  for (const key of Object.keys(patch) as (keyof PartialSettings)[]) {
    if (current[key] === patch[key]) (next as unknown as Record<string, unknown>)[key] = previous[key]
  }
  return next
}

const languages: [string, string][] = [
  ['auto', 'Detect automatically'],
  ['en', 'English'],
  ['es', 'Spanish'],
  ['fr', 'French'],
  ['de', 'German'],
  ['it', 'Italian'],
  ['pt', 'Portuguese'],
  ['nl', 'Dutch'],
  ['pl', 'Polish'],
  ['tr', 'Turkish'],
  ['ru', 'Russian'],
  ['uk', 'Ukrainian'],
  ['ar', 'Arabic'],
  ['hi', 'Hindi'],
  ['ur', 'Urdu'],
  ['zh', 'Chinese'],
  ['ja', 'Japanese'],
  ['ko', 'Korean'],
]

const retentionOptions: [number, string][] = [
  [0, 'Forever'],
  [30, '30 days'],
  [90, '90 days'],
  [365, '1 year'],
]

const backendNames: Record<HardwareInfo['recommendedBackend'], string> = {
  metal: 'Metal',
  cuda: 'CUDA',
  vulkan: 'Vulkan',
  cpu: 'CPU',
}

export function SettingsPage() {
  const tauri = isTauri()
  const [settings, setSettings] = useState<Settings | null>(tauri ? null : defaultSettings)
  const [settingsError, setSettingsError] = useState<string | null>(null)
  const [settingsAttempt, setSettingsAttempt] = useState(0)
  const [models, setModels] = useState<InstalledModel[]>([])
  const [modelsError, setModelsError] = useState<string | null>(null)
  const [modelsAttempt, setModelsAttempt] = useState(0)
  const [voices, setVoices] = useState<Voice[]>([])
  const [voicesError, setVoicesError] = useState<string | null>(null)
  const [usage, setUsage] = useState<StorageUsage | null>(null)
  const [usageError, setUsageError] = useState<string | null>(null)
  const [paths, setPaths] = useState<AppPaths | null>(null)
  const [hardware, setHardware] = useState<HardwareInfo | null>(null)
  const [hardwareError, setHardwareError] = useState<string | null>(null)
  const [version, setVersion] = useState('')
  const [status, setStatus] = useState<'idle' | 'saving' | 'saved'>('idle')
  const [cleaning, setCleaning] = useState(false)
  const [cleanedCount, setCleanedCount] = useState<number | null>(null)
  const [confirmClear, setConfirmClear] = useState(false)
  const [clearing, setClearing] = useState(false)
  const rateTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  // Only the newest save may replace the page's settings with what the backend returned.
  const saveSeq = useRef(0)

  const refreshUsage = () => {
    if (!tauri) return
    getStorageUsage()
      .then((u) => {
        setUsage(u)
        setUsageError(null)
      })
      .catch((e: unknown) => {
        setUsage(null)
        setUsageError(errorText(e))
      })
  }

  useEffect(() => {
    if (!isTauri()) return
    let alive = true
    getSettings()
      .then((s) => {
        if (alive) setSettings(s)
      })
      .catch((e: unknown) => {
        if (alive) setSettingsError(errorText(e))
      })
    return () => {
      alive = false
    }
  }, [settingsAttempt])

  useEffect(() => {
    if (!isTauri()) return
    let alive = true
    listInstalledModels()
      .then((m) => {
        if (!alive) return
        setModels(m)
        setModelsError(null)
      })
      .catch((e: unknown) => {
        if (!alive) return
        setModels([])
        setModelsError(errorText(e))
      })
    return () => {
      alive = false
    }
  }, [modelsAttempt])

  useEffect(() => {
    if (!isTauri()) return
    getStorageUsage()
      .then(setUsage)
      .catch((e: unknown) => setUsageError(errorText(e)))
    getAppPaths()
      .then(setPaths)
      .catch(() => setPaths(null))
    getHardwareInfo()
      .then(setHardware)
      .catch((e: unknown) => setHardwareError(errorText(e)))
    getVersion()
      .then(setVersion)
      .catch(() => setVersion(''))
  }, [])

  // Voices come from the chosen TTS model (loading it may take a moment).
  const ttsModel = settings?.defaultTtsModel ?? null
  useEffect(() => {
    if (!isTauri() || !ttsModel) return
    let cancelled = false
    listVoices({ modelId: ttsModel })
      .then((v) => {
        if (cancelled) return
        setVoices(v)
        setVoicesError(null)
      })
      .catch((e: unknown) => {
        if (cancelled) return
        setVoices([])
        setVoicesError(errorText(e))
      })
    return () => {
      cancelled = true
    }
  }, [ttsModel])

  useEffect(() => {
    if (status !== 'saved') return
    const t = setTimeout(() => setStatus('idle'), 1800)
    return () => clearTimeout(t)
  }, [status])

  useEffect(() => {
    if (!confirmClear) return
    const t = setTimeout(() => setConfirmClear(false), 3000)
    return () => clearTimeout(t)
  }, [confirmClear])

  /** Resolves to true once saved. On failure, rolls back only the fields that still hold the failed value. */
  const persist = async (patch: PartialSettings, previous: Settings): Promise<boolean> => {
    if (!tauri) return true
    const seq = ++saveSeq.current
    setStatus('saving')
    try {
      const saved = await updateSettings({ settings: patch })
      if (seq === saveSeq.current) {
        setSettings(saved)
        setStatus('saved')
      }
      return true
    } catch (e) {
      setSettings((current) => (current ? rollbackFields(current, patch, previous) : current))
      if (seq === saveSeq.current) setStatus('idle')
      toastError(e)
      return false
    }
  }

  /** Optimistic: the page updates at once and rolls back if saving fails. Resolves once saved. */
  const save = (patch: PartialSettings) => {
    if (!settings) return Promise.resolve(false)
    setSettings((current) => (current ? { ...current, ...patch } : current))
    return persist(patch, settings)
  }

  const saveRate = (speechRate: number) => {
    if (!settings) return
    const previous = settings
    setSettings((current) => (current ? { ...current, speechRate } : current))
    if (rateTimer.current) clearTimeout(rateTimer.current)
    rateTimer.current = setTimeout(() => void persist({ speechRate }, previous), 350)
  }

  /** A new speech model: keep the default voice only if the new model has it. */
  const changeTtsModel = async (defaultTtsModel: string) => {
    const voice = settings?.defaultVoice ?? null
    if (!(await save({ defaultTtsModel })) || !voice || !tauri) return
    let list: Voice[]
    try {
      list = await listVoices({ modelId: defaultTtsModel })
    } catch {
      return // The Voice row shows the error.
    }
    const first = list[0]
    if (first && !list.some((v) => v.id === voice)) await save({ defaultVoice: first.id })
  }

  const cleanUp = async () => {
    if (!tauri) return
    setCleaning(true)
    try {
      const count = await runRetention()
      setCleanedCount(count)
      refreshUsage()
    } catch (e) {
      toastError(e)
    } finally {
      setCleaning(false)
    }
  }

  const clearHistory = async () => {
    if (!confirmClear) return setConfirmClear(true)
    setConfirmClear(false)
    if (!tauri) return
    setClearing(true)
    try {
      const count = await historyClear()
      toast(count === 1 ? 'Deleted 1 item' : `Deleted ${count} items`)
      refreshUsage()
    } catch (e) {
      toastError(e)
    } finally {
      setClearing(false)
    }
  }

  const openRepo = async () => {
    try {
      if (!tauri) return void window.open(REPO_URL, '_blank', 'noopener')
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl(REPO_URL)
    } catch (e) {
      toastError(e)
    }
  }

  if (!settings) {
    return (
      <div className="space-y-8">
        <PageHeader title="Settings" />
        {settingsError ? (
          <EmptyState
            icon={<TriangleAlert className="size-4" strokeWidth={1.75} />}
            title="Could not load settings"
            description={settingsError}
            action={
              <Button
                icon={<RotateCw className="size-3.5" strokeWidth={2} />}
                onClick={() => {
                  setSettingsError(null)
                  setSettingsAttempt((n) => n + 1)
                }}
              >
                Retry
              </Button>
            }
          />
        ) : (
        <div className="space-y-3">
          {[0, 1, 2].map((i) => (
            <div key={i} className="h-28 animate-pulse rounded-2xl border border-line bg-surface" />
          ))}
        </div>
        )}
      </div>
    )
  }

  const sttModels = models.filter((m) => m.kind === 'stt')
  const ttsModels = models.filter((m) => m.kind === 'tts')
  const threadMax = hardware?.cpuCores || navigator.hardwareConcurrency || 8
  const threadOptions = Array.from({ length: Math.max(threadMax, settings.cpuThreads) }, (_, i) => i + 1)
  const hasRetentionOption = retentionOptions.some(([d]) => d === settings.historyRetentionDays)
  const hasLanguageOption = languages.some(([code]) => code === settings.sttLanguage)

  return (
    <div className="space-y-10 pb-6">
      <PageHeader
        title="Settings"
        description="Saved to ~/.talkr/config.json as you change them."
        actions={
          <span
            aria-live="polite"
            className={cx(
              'font-mono text-[11px] text-subtle transition-opacity duration-300',
              status === 'idle' ? 'opacity-0' : 'opacity-100',
            )}
          >
            {status === 'saving' ? 'Saving' : 'Saved'}
          </span>
        }
      />

      {!tauri ? (
        <p className="-mt-6 font-mono text-[11px] text-subtle">Preview. Changes are not saved outside the desktop app.</p>
      ) : null}

      <AppearanceSection />

      <Section title="Defaults">
        {modelsError ? (
          <div className="border-b border-line px-5 py-4">
            <Notice
              tone="error"
              title="Could not load installed models"
              action={
                <Button size="sm" onClick={() => setModelsAttempt((n) => n + 1)}>
                  Retry
                </Button>
              }
            >
              {modelsError}
            </Notice>
          </div>
        ) : null}
        <Row label="Transcription model" description="Used by Transcribe unless you pick another.">
          <ModelSelect
            label="Transcription model"
            value={settings.defaultSttModel}
            models={sttModels}
            onChange={(defaultSttModel) => void save({ defaultSttModel })}
          />
        </Row>
        <Row label="Language" description="Spoken language for transcription. Auto works well for most audio.">
          <Select
            label=""
            aria-label="Transcription language"
            value={settings.sttLanguage}
            onChange={(sttLanguage) => void save({ sttLanguage })}
            options={[
              ...languages.map(([code, name]) => ({ value: code, label: name })),
              ...(hasLanguageOption ? [] : [{ value: settings.sttLanguage, label: settings.sttLanguage }]),
            ]}
          />
        </Row>
        <Row label="Speech model" description="Used by Speak unless you pick another.">
          <ModelSelect
            label="Speech model"
            value={settings.defaultTtsModel}
            models={ttsModels}
            onChange={(defaultTtsModel) => void changeTtsModel(defaultTtsModel)}
          />
        </Row>
        {settings.defaultTtsModel ? (
          <Row
            label="Voice"
            description={
              voicesError ? (
                <span role="alert">Could not load the voices: {voicesError}</span>
              ) : (
                'Default voice for the speech model.'
              )
            }
          >
            <Select
              label=""
              aria-label="Default voice"
              value={settings.defaultVoice ?? ''}
              onChange={(defaultVoice) => void save({ defaultVoice })}
              placeholder="Not set"
              options={[
                ...voices.map((v) => ({ value: v.id, label: v.name })),
                ...(settings.defaultVoice && !voices.some((v) => v.id === settings.defaultVoice)
                  ? [{ value: settings.defaultVoice, label: settings.defaultVoice }]
                  : []),
              ]}
            />
          </Row>
        ) : null}
        <Row label="Speed" description="Speaking rate for generated speech.">
          <input
            type="range"
            min={0.5}
            max={2}
            step={0.05}
            value={settings.speechRate}
            aria-label="Speech rate"
            onChange={(e) => saveRate(Number(e.target.value))}
            className="w-32"
          />
          <span className="w-10 text-right font-mono text-[12px] tabular-nums text-muted">
            {settings.speechRate.toFixed(2)}x
          </span>
        </Row>
      </Section>

      <ComputeSection device={settings.device} onDeviceChange={(device) => save({ device })}>
        <Row label="CPU threads" description="Threads used when running on the CPU. Auto uses your physical cores, up to 8.">
          <Select
            label=""
            aria-label="CPU threads"
            value={String(settings.cpuThreads)}
            onChange={(n) => void save({ cpuThreads: Number(n) })}
            options={[{ value: '0', label: 'Auto' }, ...threadOptions.map((n) => ({ value: String(n), label: String(n) }))]}
          />
        </Row>
      </ComputeSection>

      <Section title="History">
        <Row label="Keep history" description="Older items are removed automatically. Favorites are always kept.">
          <Select
            label=""
            aria-label="Keep history"
            value={String(settings.historyRetentionDays)}
            onChange={(days) => {
              setCleanedCount(null)
              void save({ historyRetentionDays: Number(days) })
            }}
            options={[
              ...retentionOptions.map(([days, name]) => ({ value: String(days), label: name })),
              ...(hasRetentionOption
                ? []
                : [{ value: String(settings.historyRetentionDays), label: `${settings.historyRetentionDays} days` }]),
            ]}
          />
        </Row>
        <Row
          label="Clean up now"
          description={
            cleanedCount !== null
              ? cleanedCount === 0
                ? 'Nothing to remove.'
                : `Removed ${cleanedCount} ${cleanedCount === 1 ? 'item' : 'items'}.`
              : settings.historyRetentionDays === 0
                ? 'History is kept forever, so there is nothing to clean up.'
                : `Remove items older than ${settings.historyRetentionDays} days right away.`
          }
        >
          <Button
            size="sm"
            loading={cleaning}
            disabled={settings.historyRetentionDays === 0 || status === 'saving'}
            onClick={() => void cleanUp()}
          >
            Clean up now
          </Button>
        </Row>
        <Row label="Keep recordings" description="Save microphone recordings with their transcripts so you can play them back.">
          <Switch
            label="Keep recordings"
            checked={settings.saveRecordings}
            onChange={(saveRecordings) => void save({ saveRecordings })}
          />
        </Row>
      </Section>

      <Section title="Storage">
        <div className="space-y-4 border-b border-line px-5 py-5">
          <div className="flex items-baseline justify-between gap-4">
            <p className="text-[13.5px] font-medium">Disk usage</p>
            <p className="font-mono text-[12px] tabular-nums text-muted">
              {usage ? formatBytes(usage.totalBytes) : usageError ? 'Unknown' : '—'}
            </p>
          </div>
          <StorageBar usage={usage} />
          {usageError && !usage ? (
            <p role="alert" className="text-[12.5px] text-muted">
              Could not measure disk usage: {usageError}
            </p>
          ) : null}
        </div>
        <Row
          label="Data folder"
          description={
            <span data-selectable className="break-all font-mono text-[12px] text-subtle">
              {paths?.home ?? '~/.talkr'}
            </span>
          }
        >
          <Button
            size="sm"
            icon={<FolderOpen className="size-3.5" strokeWidth={2} />}
            disabled={!tauri}
            onClick={() => void openDataFolder().catch(toastError)}
          >
            Open folder
          </Button>
        </Row>
      </Section>

      <Section title="Danger zone">
        <Row label="Delete all history" description="Removes every item and its audio from this computer. Favorites are kept.">
          <Button
            size="sm"
            variant={confirmClear ? 'primary' : 'danger'}
            loading={clearing}
            icon={<Trash2 className="size-3.5" strokeWidth={2} />}
            onClick={() => void clearHistory()}
          >
            {confirmClear ? 'Click again to confirm' : 'Delete history'}
          </Button>
        </Row>
      </Section>

      <Section title="About">
        <Row label="Talkr" description="Free and open source under MIT.">
          <span className="font-mono text-[12px] text-subtle">{version ? `v${version}` : 'dev'}</span>
        </Row>
        {hardware ? (
          <Row
            label="This computer"
            description={
              <span className="font-mono text-[12px] text-subtle">
                {[
                  hardware.cpuName,
                  `${hardware.cpuCores} cores`,
                  `${formatBytes(hardware.ramBytes)} RAM`,
                  ...hardware.gpus.map((g) => g.name),
                ].join(' · ')}
              </span>
            }
          >
            <span className="font-mono text-[11px] uppercase tracking-[0.12em] text-subtle">
              {backendNames[hardware.recommendedBackend]}
            </span>
          </Row>
        ) : hardwareError ? (
          <Row
            label="This computer"
            description={<span className="text-[12.5px] text-muted">Could not read the hardware: {hardwareError}</span>}
          >
            <span className="font-mono text-[11px] uppercase tracking-[0.12em] text-subtle">Unknown</span>
          </Row>
        ) : null}
        <Row label="Source code" description="Report issues, read the code, or contribute.">
          <Button size="sm" variant="ghost" icon={<ArrowUpRight className="size-3.5" strokeWidth={2} />} onClick={() => void openRepo()}>
            GitHub
          </Button>
        </Row>
      </Section>
    </div>
  )
}

function ModelSelect({
  label,
  value,
  models,
  onChange,
}: {
  label: string
  value: string | null
  models: InstalledModel[]
  onChange: (id: string) => void
}) {
  if (models.length === 0 && !value) {
    return <span className="font-mono text-[12px] text-subtle">No models installed</span>
  }
  return (
    <Select
      label=""
      aria-label={label}
      value={value ?? ''}
      onChange={onChange}
      placeholder="Not set"
      className="max-w-60"
      options={[
        ...models.map((m) => ({ value: m.id, label: m.name })),
        ...(value && !models.some((m) => m.id === value) ? [{ value, label: `${value} (not installed)` }] : []),
      ]}
    />
  )
}

const storageParts = [
  { key: 'modelsBytes', label: 'Models', tone: 'bg-accent' },
  { key: 'audioBytes', label: 'Audio', tone: 'bg-accent/55' },
  { key: 'dbBytes', label: 'History database', tone: 'bg-accent/25' },
] as const

function StorageBar({ usage }: { usage: StorageUsage | null }) {
  const total = usage?.totalBytes ?? 0
  return (
    <div className="space-y-3">
      <div className="flex h-1.5 gap-0.5 overflow-hidden rounded-full bg-fg/[0.06]">
        {usage && total > 0
          ? storageParts.map((p) =>
              usage[p.key] > 0 ? (
                <span
                  key={p.key}
                  className={cx('h-full min-w-[3px] rounded-full transition-[width] duration-500 ease-out-quint', p.tone)}
                  style={{ width: `${(usage[p.key] / total) * 100}%` }}
                />
              ) : null,
            )
          : null}
      </div>
      <ul className="flex flex-wrap gap-x-6 gap-y-1.5">
        {storageParts.map((p) => (
          <li key={p.key} className="flex items-center gap-2 text-[12.5px] text-muted">
            <span className={cx('size-2 rounded-full', p.tone)} />
            {p.label}
            <span className="font-mono text-[11.5px] tabular-nums text-subtle">{usage ? formatBytes(usage[p.key]) : '—'}</span>
          </li>
        ))}
      </ul>
    </div>
  )
}
