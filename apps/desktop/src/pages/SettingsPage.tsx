import { useEffect, useRef, useState } from 'react'
import { ArrowUpRight, FolderOpen, Trash2 } from 'lucide-react'
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
  DevicePreference,
  HardwareInfo,
  InstalledModel,
  PartialSettings,
  Settings,
  StorageUsage,
  Voice,
} from '@/lib/types'
import { Button, PageHeader, Segmented } from '@/components/ui'
import { Select } from '@/components/Select'
import { cx } from '@/lib/cx'
import { toast, toastError } from '@/stores/toast'
import { Row, Section, Switch } from '@/features/settings/controls'
import { AppearanceSection } from '@/features/settings/Appearance'
import { formatBytes } from '@/features/history/utils'

const VERSION = '0.1.1'
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

const deviceOptions: { value: DevicePreference; label: string }[] = [
  { value: 'auto', label: 'Auto' },
  { value: 'gpu', label: 'GPU' },
  { value: 'cpu', label: 'CPU' },
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
  const [models, setModels] = useState<InstalledModel[]>([])
  const [voices, setVoices] = useState<Voice[]>([])
  const [usage, setUsage] = useState<StorageUsage | null>(null)
  const [paths, setPaths] = useState<AppPaths | null>(null)
  const [hardware, setHardware] = useState<HardwareInfo | null>(null)
  const [status, setStatus] = useState<'idle' | 'saving' | 'saved'>('idle')
  const [cleaning, setCleaning] = useState(false)
  const [cleanedCount, setCleanedCount] = useState<number | null>(null)
  const [confirmClear, setConfirmClear] = useState(false)
  const [clearing, setClearing] = useState(false)
  const rateTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  const refreshUsage = () => {
    if (!tauri) return
    getStorageUsage()
      .then(setUsage)
      .catch(() => setUsage(null))
  }

  useEffect(() => {
    if (!isTauri()) return
    getSettings().then(setSettings).catch(toastError)
    listInstalledModels()
      .then(setModels)
      .catch(() => setModels([]))
    getStorageUsage()
      .then(setUsage)
      .catch(() => setUsage(null))
    getAppPaths()
      .then(setPaths)
      .catch(() => setPaths(null))
    getHardwareInfo()
      .then(setHardware)
      .catch(() => setHardware(null))
  }, [])

  // Voices come from the chosen TTS model (loading it may take a moment).
  const ttsModel = settings?.defaultTtsModel ?? null
  useEffect(() => {
    if (!isTauri() || !ttsModel) return
    let cancelled = false
    listVoices({ modelId: ttsModel })
      .then((v) => {
        if (!cancelled) setVoices(v)
      })
      .catch(() => {
        if (!cancelled) setVoices([])
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

  const persist = async (patch: PartialSettings, rollback: Settings) => {
    if (!tauri) return
    setStatus('saving')
    try {
      setSettings(await updateSettings({ settings: patch }))
      setStatus('saved')
    } catch (e) {
      setSettings(rollback)
      setStatus('idle')
      toastError(e)
    }
  }

  const save = (patch: PartialSettings) => {
    if (!settings) return
    setSettings({ ...settings, ...patch })
    void persist(patch, settings)
  }

  const saveRate = (speechRate: number) => {
    if (!settings) return
    const rollback = settings
    setSettings({ ...settings, speechRate })
    if (rateTimer.current) clearTimeout(rateTimer.current)
    rateTimer.current = setTimeout(() => void persist({ speechRate }, rollback), 350)
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
        <div className="space-y-3">
          {[0, 1, 2].map((i) => (
            <div key={i} className="h-28 animate-pulse rounded-2xl border border-line bg-surface" />
          ))}
        </div>
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
        <Row label="Transcription model" description="Used by Transcribe unless you pick another.">
          <ModelSelect
            label="Transcription model"
            value={settings.defaultSttModel}
            models={sttModels}
            onChange={(defaultSttModel) => save({ defaultSttModel })}
          />
        </Row>
        <Row label="Language" description="Spoken language for transcription. Auto works well for most audio.">
          <Select
            label=""
            aria-label="Transcription language"
            value={settings.sttLanguage}
            onChange={(sttLanguage) => save({ sttLanguage })}
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
            onChange={(defaultTtsModel) => save({ defaultTtsModel })}
          />
        </Row>
        {settings.defaultTtsModel ? (
          <Row label="Voice" description="Default voice for the speech model.">
            <Select
              label=""
              aria-label="Default voice"
              value={settings.defaultVoice ?? ''}
              onChange={(defaultVoice) => save({ defaultVoice })}
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

      <Section title="Performance">
        <Row
          label="Acceleration"
          description={
            hardware
              ? `Auto picks ${backendNames[hardware.recommendedBackend]} on this computer.`
              : 'Auto uses the GPU when one is available and falls back to the CPU.'
          }
        >
          <Segmented label="Acceleration" value={settings.device} options={deviceOptions} onChange={(device) => save({ device })} />
        </Row>
        <Row label="CPU threads" description="Threads used when running on the CPU. Auto uses your physical cores, up to 8.">
          <Select
            label=""
            aria-label="CPU threads"
            value={String(settings.cpuThreads)}
            onChange={(n) => save({ cpuThreads: Number(n) })}
            options={[{ value: '0', label: 'Auto' }, ...threadOptions.map((n) => ({ value: String(n), label: String(n) }))]}
          />
        </Row>
      </Section>

      <Section title="History">
        <Row label="Keep history" description="Older items are removed automatically. Favorites are always kept.">
          <Select
            label=""
            aria-label="Keep history"
            value={String(settings.historyRetentionDays)}
            onChange={(days) => {
              setCleanedCount(null)
              save({ historyRetentionDays: Number(days) })
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
            onChange={(saveRecordings) => save({ saveRecordings })}
          />
        </Row>
      </Section>

      <Section title="Storage">
        <div className="space-y-4 border-b border-line px-5 py-5">
          <div className="flex items-baseline justify-between gap-4">
            <p className="text-[13.5px] font-medium">Disk usage</p>
            <p className="font-mono text-[12px] tabular-nums text-muted">{usage ? formatBytes(usage.totalBytes) : '—'}</p>
          </div>
          <StorageBar usage={usage} />
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
          <span className="font-mono text-[12px] text-subtle">v{VERSION}</span>
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
