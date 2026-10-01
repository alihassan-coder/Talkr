import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router'
import { AudioWaveform, Mic } from 'lucide-react'
import { Button, Card, EmptyState, PageHeader, Progress, Segmented } from '@/components/ui'
import { Notice } from '@/components/Notice'
import { Select } from '@/components/Select'
import { getSettings, isTauri, listInstalledModels, transcribeFile } from '@/lib/api'
import type { InstalledModel } from '@/lib/types'
import { errorText } from '@/lib/errors'
import { PreviewNotice } from '@/features/speak/PreviewNotice'
import { useJob } from '@/features/speak/useJob'
import { formatDuration } from '@/features/speak/utils'
import { DropZone } from '@/features/transcribe/DropZone'
import { Recorder } from '@/features/transcribe/Recorder'
import { Switch } from '@/features/transcribe/Switch'
import { TranscriptResult } from '@/features/transcribe/TranscriptResult'
import { baseName, LANGUAGES } from '@/features/transcribe/utils'

type Mode = 'record' | 'file'

export function TranscribePage() {
  const navigate = useNavigate()
  const tauri = isTauri()

  const [mode, setMode] = useState<Mode>('record')
  const [models, setModels] = useState<InstalledModel[] | null>(tauri ? null : [])
  const [modelId, setModelId] = useState('')
  const [language, setLanguage] = useState('auto')
  const [translate, setTranslate] = useState(false)
  const [recording, setRecording] = useState(false)
  // The job lives in a store: leaving the page keeps its progress, Cancel and result.
  const job = useJob('stt')
  const result = job.result

  const [loadError, setLoadError] = useState<string | null>(null)
  const [loadAttempt, setLoadAttempt] = useState(0)

  useEffect(() => {
    if (!tauri) return
    let alive = true
    Promise.all([listInstalledModels(), getSettings()])
      .then(([installed, settings]) => {
        if (!alive) return
        const stt = installed.filter((m) => m.kind === 'stt')
        setModels(stt)
        setModelId((stt.find((m) => m.id === settings.defaultSttModel) ?? stt[0])?.id ?? '')
        if (LANGUAGES.some((l) => l.code === settings.sttLanguage)) setLanguage(settings.sttLanguage)
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

  const transcribe = (path: string, name: string) => {
    if (!modelId || job.running || job.blockedBy) return
    // `translate` is ignored by backends that do not support it yet.
    const args = { path, modelId, language, translate }
    void job.start(() => transcribeFile(args), { label: name, clearResult: true })
  }

  const header = (
    <PageHeader
      title="Transcribe"
      description="Speech to text with timestamps. Nothing leaves this machine."
      actions={
        <Segmented<Mode>
          label="Source"
          value={mode}
          onChange={setMode}
          disabled={recording}
          options={[
            { value: 'record', label: 'Record' },
            { value: 'file', label: 'File' },
          ]}
        />
      }
    />
  )

  if (models === null) {
    return (
      <div className="space-y-8">
        {header}
        <Progress value={null} label="Loading" className="mx-auto max-w-40" />
      </div>
    )
  }

  if (loadError) {
    return (
      <div className="space-y-8">
        {header}
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
        {header}
        <EmptyState
          icon={<Mic className="size-5" strokeWidth={1.75} />}
          title="No Whisper model installed"
          description="Whisper Base is a good first pick: small, fast and accurate."
          action={
            <Button variant="primary" onClick={() => navigate('/models?kind=stt')}>
              Browse models
            </Button>
          }
        />
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {header}

      {tauri ? null : <PreviewNotice>Preview mode. Recording and transcription run in the Talkr desktop app.</PreviewNotice>}

      <div className="flex flex-wrap items-center gap-3">
        <Select
          label="Model"
          value={modelId}
          onChange={setModelId}
          disabled={job.running}
          options={
            models.length === 0
              ? [{ value: '', label: 'Whisper Base' }]
              : models.map((m) => ({ value: m.id, label: m.name }))
          }
        />
        <Select
          label="Language"
          value={language}
          onChange={setLanguage}
          disabled={job.running}
          options={LANGUAGES.map((l) => ({ value: l.code, label: l.label }))}
        />
        <Switch label="Translate to English" checked={translate} onChange={setTranslate} disabled={job.running} />
      </div>

      {job.running ? (
        <Card className="animate-rise px-6 py-5">
          <div className="flex items-center gap-4">
            <span className="grid size-10 shrink-0 place-items-center rounded-full border border-line text-muted">
              <AudioWaveform className="size-4" strokeWidth={1.75} />
            </span>
            <div className="min-w-0 flex-1">
              <p className="truncate text-[14px] font-medium">{job.label}</p>
              <p className="mt-0.5 font-mono text-[11px] tabular-nums text-subtle">
                {job.progress === null ? 'Transcribing…' : `Transcribing · ${Math.round(job.progress * 100)}%`}
              </p>
            </div>
            <Button variant="ghost" size="sm" disabled={job.cancelRequested} onClick={() => void job.cancel()}>
              {job.cancelRequested ? 'Cancelling…' : 'Cancel'}
            </Button>
          </div>
          <Progress value={job.progress} label="Transcribing" className="mt-4" />
        </Card>
      ) : mode === 'record' ? (
        <Recorder
          onRecordingChange={setRecording}
          disabled={(!modelId && tauri) || !!job.blockedBy}
          disabledReason={job.blockedBy}
          onRecorded={(path, durationMs) => transcribe(path, `Recording · ${formatDuration(durationMs)}`)}
        />
      ) : (
        <DropZone
          disabled={(!modelId && tauri) || !!job.blockedBy}
          disabledReason={job.blockedBy}
          onFile={(path) => transcribe(path, baseName(path))}
        />
      )}

      {job.error ? (
        <Notice tone="error" title="Transcription failed" onDismiss={job.dismissError}>
          {job.error}
        </Notice>
      ) : null}

      {result ? <TranscriptResult key={result.item.id} item={result.item} name={result.label} /> : null}
    </div>
  )
}
