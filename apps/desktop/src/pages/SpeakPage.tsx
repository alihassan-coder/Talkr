import { useEffect, useState } from 'react'
import type { ClipboardEvent } from 'react'
import { useNavigate } from 'react-router'
import { AudioLines, Volume2, X } from 'lucide-react'
import { Button, Card, EmptyState, Kbd, PageHeader, Progress } from '@/components/ui'
import { Notice } from '@/components/Notice'
import { Select } from '@/components/Select'
import {
  getSettings,
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
import { useDrafts } from '@/stores/drafts'
import { modKey } from '@/lib/platform'
import { ExportMenu } from '@/features/export/ExportMenu'
import { AudioPlayer } from '@/features/speak/AudioPlayer'
import { PreviewNotice } from '@/features/speak/PreviewNotice'
import { RecentList } from '@/features/speak/RecentList'
import { SpeedControl } from '@/features/speak/SpeedControl'
import { useJob } from '@/features/speak/useJob'
import { countWords, estimateSpeechMs, formatDuration, formatSeconds, MAX_CHARS } from '@/features/speak/utils'

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
  const text = useDrafts((s) => s.speakText)
  const setText = useDrafts((s) => s.setSpeakText)
  // A recent item the user picked; `afterSeq` is the job result it was picked after, `nonce`
  // tells picks apart so picking the same item again plays it again.
  const [picked, setPicked] = useState<{ item: HistoryItem; afterSeq: number; nonce: number } | null>(null)
  const [recent, setRecent] = useState<HistoryItem[]>([])
  const [recentError, setRecentError] = useState<string | null>(null)
  const [recentReload, setRecentReload] = useState(0)

  // The job lives in a store, so a result that finished while the user was elsewhere is shown
  // on return, but only plays by itself when it arrives while the page is open.
  const job = useJob('tts')
  const [mountSeq] = useState(() => useJobs.getState().tts.result?.seq ?? 0)
  const resultSeq = job.result?.seq ?? 0

  const [loadError, setLoadError] = useState<string | null>(null)
  const [loadAttempt, setLoadAttempt] = useState(0)

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
        // Not "nothing installed": that would send people off to download what they have.
        setLoadError(errorText(err))
        setModels([])
      })
    return () => {
      alive = false
    }
  }, [tauri, loadAttempt])

  const retryLoad = () => {
    setLoadError(null)
    setModels(null)
    setLoadAttempt((n) => n + 1)
  }

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

  // The voice must belong to the loaded list: right after a model switch the old voice is gone.
  const voiceReady = !!voices && voices.some((v) => v.id === voiceId)
  const canGenerate = tauri && text.trim().length > 0 && !!modelId && voiceReady && !job.running && !job.blockedBy

  const generate = () => {
    if (!canGenerate) return
    void job.start(() => synthesize({ text, modelId, voiceId, speed }))
  }

  const changeModel = (id: string) => {
    setVoices(null)
    setModelId(id)
  }

  // maxLength would cut a long paste silently; say what happened.
  const onPaste = (e: ClipboardEvent<HTMLTextAreaElement>) => {
    const box = e.currentTarget
    const pasted = e.clipboardData.getData('text').length
    const kept = text.length - (box.selectionEnd - box.selectionStart)
    if (kept + pasted > MAX_CHARS) {
      toast(`Only the first ${MAX_CHARS.toLocaleString('en-US')} characters fit. Split longer text into parts.`)
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

  if (loadError) {
    return (
      <div className="space-y-8">
        <Header />
        <Notice
          tone="error"
          title="Could not load your installed models"
          action={
            <Button size="sm" onClick={retryLoad}>
              Retry
            </Button>
          }
        >
          {loadError}
        </Notice>
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
        ? { item: picked.item, autoPlay: true, nonce: picked.nonce }
        : null
  const currentItem = current?.item ?? null

  return (
    <div className="space-y-8">
      <Header />

      {tauri ? null : <PreviewNotice>Preview mode. Speech is generated in the Talkr desktop app.</PreviewNotice>}

      <div className="space-y-4">
        <Card className="transition-[border-color,box-shadow] duration-200 focus-within:border-accent focus-within:shadow-[0_0_0_3px_color-mix(in_oklab,var(--color-accent)_14%,transparent)]">
          <textarea
            value={text}
            dir="auto"
            onChange={(e) => setText(e.target.value)}
            onPaste={onPaste}
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
              {text.trim() ? (
                <span className="hidden sm:inline">
                  {' '}
                  · {countWords(text).toLocaleString('en-US')} words · about {formatDuration(estimateSpeechMs(text, speed))}
                </span>
              ) : null}
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
              <Kbd>{modKey()} ⏎</Kbd>
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
          key={`${currentItem.id}-${current && 'nonce' in current ? current.nonce : 0}`}
          id={currentItem.id}
          path={currentItem.audioPath}
          seed={currentItem.text.length}
          autoPlay={current?.autoPlay}
          fallbackDurationMs={currentItem.durationMs ?? 0}
          title={currentItem.title}
          meta={[currentItem.voiceId, currentItem.modelId, `${currentItem.text.length} characters`]
            .filter(Boolean)
            .join(' · ')}
          footer={`Rendered in ${formatSeconds(currentItem.processingMs)} · ${formatDuration(currentItem.durationMs ?? 0)} of audio`}
          actions={<ExportMenu item={currentItem} />}
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
        onSelect={(item) => setPicked((prev) => ({ item, afterSeq: resultSeq, nonce: (prev?.nonce ?? 0) + 1 }))}
      />
    </div>
  )
}

function Header() {
  return <PageHeader title="Speak" description="Turn text into natural speech, on this computer." />
}
