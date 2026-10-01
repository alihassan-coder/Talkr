import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router'
import { AudioLines, Download, Volume2, X } from 'lucide-react'
import { Button, Card, EmptyState, Kbd, PageHeader, Progress } from '@/components/ui'
import { Notice } from '@/components/Notice'
import { Select } from '@/components/Select'
import {
  getSettings,
  historyExport,
  historyList,
  isTauri,
  listInstalledModels,
  listVoices,
  synthesize,
} from '@/lib/api'
import type { HistoryItem, InstalledModel, Voice } from '@/lib/types'
import { errorText } from '@/lib/errors'
import { useJobs } from '@/stores/jobs'
import { toast, toastError } from '@/stores/toast'
import { AudioPlayer } from '@/features/speak/AudioPlayer'
import { PreviewNotice } from '@/features/speak/PreviewNotice'
import { RecentList } from '@/features/speak/RecentList'
import { SpeedControl } from '@/features/speak/SpeedControl'
import { useJob } from '@/features/speak/useJob'
import { formatDuration, formatSeconds, MAX_CHARS } from '@/features/speak/utils'

const fetchRecent = () => historyList({ kind: 'tts' }).then((r) => r.items.slice(0, 5))

export function SpeakPage() {
  const navigate = useNavigate()
  const tauri = isTauri()

  const [models, setModels] = useState<InstalledModel[] | null>(tauri ? null : [])
  const [modelId, setModelId] = useState('')
  const [defaultVoice, setDefaultVoice] = useState<string | null>(null)
  const [voices, setVoices] = useState<Voice[] | null>(null)
  const [voiceId, setVoiceId] = useState('')
  const [speed, setSpeed] = useState(1)
  const [text, setText] = useState('')
  // A recent item the user picked; `afterSeq` is the job result it was picked after.
  const [picked, setPicked] = useState<{ item: HistoryItem; afterSeq: number } | null>(null)
  const [recent, setRecent] = useState<HistoryItem[]>([])
  const [recentError, setRecentError] = useState<string | null>(null)
  const [recentReload, setRecentReload] = useState(0)

  // The job lives in a store, so a result that finished while the user was elsewhere is shown
  // on return, but only plays by itself when it arrives while the page is open.
  const job = useJob('tts')
  const [mountSeq] = useState(() => useJobs.getState().tts.result?.seq ?? 0)
  const resultSeq = job.result?.seq ?? 0

  useEffect(() => {
    if (!tauri) return
    let alive = true
    Promise.all([listInstalledModels(), getSettings()])
      .then(([installed, settings]) => {
        if (!alive) return
        const tts = installed.filter((m) => m.kind === 'tts')
        setModels(tts)
        setModelId((tts.find((m) => m.id === settings.defaultTtsModel) ?? tts[0])?.id ?? '')
        setDefaultVoice(settings.defaultVoice)
        setSpeed(settings.speechRate > 0 ? settings.speechRate : 1)
      })
      .catch((err: unknown) => {
        if (!alive) return
        setModels([])
        toastError(err)
      })
    return () => {
      alive = false
    }
  }, [tauri])

  // Recent items: on open, after every finished job, and on Retry.
  useEffect(() => {
    if (!tauri) return
    let alive = true
    fetchRecent()
      .then((items) => {
        if (!alive) return
        setRecent(items)
        setRecentError(null)
      })
      .catch((err: unknown) => {
        if (alive) setRecentError(errorText(err))
      })
    return () => {
      alive = false
    }
  }, [tauri, resultSeq, recentReload])

  // Loading voices also warms up the engine, so the first Generate is quicker.
  useEffect(() => {
    if (!modelId) return
    let alive = true
    listVoices({ modelId })
      .then((list) => {
        if (!alive) return
        setVoices(list)
        setVoiceId((prev) =>
          list.some((v) => v.id === prev) ? prev : ((list.find((v) => v.id === defaultVoice) ?? list[0])?.id ?? ''),
        )
      })
      .catch((err: unknown) => {
        if (!alive) return
        setVoices([])
        toastError(err)
      })
    return () => {
      alive = false
    }
  }, [modelId, defaultVoice])

  const canGenerate = tauri && text.trim().length > 0 && !!modelId && !!voiceId && !job.running && !job.blockedBy

  const generate = () => {
    if (!canGenerate) return
    void job.start(() => synthesize({ text, modelId, voiceId, speed }))
  }

  const changeModel = (id: string) => {
    setVoices(null)
    setModelId(id)
  }

  const saveWav = async (item: HistoryItem) => {
    try {
      const saved = await historyExport({ id: item.id, format: 'wav' })
      if (saved) toast('Saved as WAV')
    } catch (err) {
      toastError(err)
    }
  }

  if (models === null) {
    return (
      <div className="space-y-8">
        <Header />
        <Progress value={null} label="Loading" className="mx-auto max-w-40" />
      </div>
    )
  }

  if (tauri && models.length === 0) {
    return (
      <div className="space-y-8">
        <Header />
        <EmptyState
          icon={<Volume2 className="size-5" strokeWidth={1.75} />}
          title="No voice installed yet"
          description="Download Kokoro or a Piper voice from the Models page. It takes about a minute."
          action={
            <Button variant="primary" onClick={() => navigate('/models?kind=tts')}>
              Browse models
            </Button>
          }
        />
      </div>
    )
  }

  const current =
    job.result && (!picked || job.result.seq > picked.afterSeq)
      ? { item: job.result.item, autoPlay: job.result.seq > mountSeq }
      : picked
        ? { item: picked.item, autoPlay: true }
        : null
  const currentItem = current?.item ?? null

  return (
    <div className="space-y-8">
      <Header />

      {tauri ? null : <PreviewNotice>Preview mode. Speech is generated in the Talkr desktop app.</PreviewNotice>}

      <div className="space-y-4">
        <Card className="transition-colors duration-200 focus-within:border-line-strong">
          <textarea
            value={text}
            dir="auto"
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
                e.preventDefault()
                generate()
              }
            }}
            maxLength={MAX_CHARS}
            placeholder="Type or paste something to hear it…"
            aria-label="Text to speak"
            className="block min-h-52 w-full resize-y bg-transparent px-6 pt-5 text-[15px] leading-relaxed text-fg outline-none placeholder:text-subtle"
          />
          <div className="flex items-center justify-between px-4 pb-3 pl-6">
            <span className="font-mono text-[11px] tabular-nums text-subtle">
              {text.length.toLocaleString('en-US')} / {MAX_CHARS.toLocaleString('en-US')}
            </span>
            <Button
              variant="ghost"
              size="sm"
              icon={<X className="size-3.5" strokeWidth={2} />}
              disabled={text.length === 0}
              onClick={() => setText('')}
            >
              Clear
            </Button>
          </div>
        </Card>

        <div className="flex flex-wrap items-center gap-3">
          <Select
            label="Voice"
            value={voiceId}
            onChange={setVoiceId}
            disabled={!voices || voices.length === 0}
            placeholder={voices === null ? (tauri ? 'Loading…' : 'af_heart') : 'No voices'}
            options={(voices ?? []).map((v) => ({ value: v.id, label: v.name, hint: v.language || undefined }))}
          />

          {models.length > 1 ? (
            <Select
              label="Model"
              value={modelId}
              onChange={changeModel}
              disabled={job.running}
              options={models.map((m) => ({ value: m.id, label: m.name }))}
            />
          ) : null}

          <SpeedControl value={speed} onChange={setSpeed} />

          <div className="ml-auto flex items-center gap-3">
            {job.blockedBy ? (
              <span id="generate-blocked" className="text-[12px] text-muted">
                {job.blockedBy}
              </span>
            ) : null}
            <span className="hidden text-[12px] text-subtle md:inline">
              <Kbd>Ctrl ⏎</Kbd>
            </span>
            <Button
              variant="primary"
              size="lg"
              icon={<AudioLines className="size-4" strokeWidth={2} />}
              loading={job.running}
              disabled={!canGenerate}
              aria-describedby={job.blockedBy ? 'generate-blocked' : undefined}
              onClick={generate}
            >
              {job.running ? 'Generating' : 'Generate'}
            </Button>
          </div>
        </div>
      </div>

      {job.running ? (
        <Card className="flex animate-rise items-center gap-4 px-6 py-4">
          <div className="min-w-0 flex-1">
            <div className="mb-2.5 flex items-center justify-between text-[13px]">
              <span className="text-muted">Generating speech…</span>
              {job.progress === null ? null : (
                <span className="font-mono text-[11px] tabular-nums text-subtle">{Math.round(job.progress * 100)}%</span>
              )}
            </div>
            <Progress value={job.progress} label="Generating speech" />
          </div>
          <Button variant="ghost" size="sm" disabled={job.cancelRequested} onClick={() => void job.cancel()}>
            {job.cancelRequested ? 'Cancelling…' : 'Cancel'}
          </Button>
        </Card>
      ) : null}

      {job.error ? (
        <Notice tone="error" title="Could not generate speech" onDismiss={job.dismissError}>
          {job.error}
        </Notice>
      ) : null}

      {currentItem?.audioPath ? (
        <AudioPlayer
          key={currentItem.id}
          id={currentItem.id}
          path={currentItem.audioPath}
          seed={currentItem.text.length}
          autoPlay={current?.autoPlay}
          fallbackDurationMs={currentItem.durationMs ?? 0}
          title={currentItem.title}
          meta={[currentItem.voiceId, currentItem.modelId, `${currentItem.text.length} characters`]
            .filter(Boolean)
            .join(' · ')}
          footer={`Rendered in ${formatSeconds(currentItem.processingMs)} · ${formatDuration(currentItem.durationMs ?? 0)} of audio · WAV`}
          actions={
            <Button
              size="sm"
              icon={<Download className="size-3.5" strokeWidth={2} />}
              onClick={() => void saveWav(currentItem)}
            >
              Save as WAV
            </Button>
          }
        />
      ) : null}

      {recentError ? (
        <Notice
          tone="error"
          title="Could not load recent speech"
          action={
            <Button size="sm" onClick={() => setRecentReload((n) => n + 1)}>
              Retry
            </Button>
          }
        >
          {recentError}
        </Notice>
      ) : null}

      <RecentList
        items={recent}
        activeId={currentItem?.id ?? null}
        onSelect={(item) => setPicked({ item, afterSeq: resultSeq })}
      />
    </div>
  )
}

function Header() {
  return <PageHeader title="Speak" description="Turn text into natural speech, on this computer." />
}
