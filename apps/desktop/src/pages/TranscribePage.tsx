import { useEffect, useRef, useState } from 'react'
import { useNavigate } from 'react-router'
import { AudioWaveform, Mic } from 'lucide-react'
import { Button, Card, EmptyState, PageHeader, Progress, Segmented } from '@/components/ui'
import { Select } from '@/components/Select'
import { getSettings, isTauri, listInstalledModels, transcribeFile } from '@/lib/api'
import type { HistoryItem, InstalledModel } from '@/lib/types'
import { toastError } from '@/stores/toast'
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
  const [activeName, setActiveName] = useState('')
  const [result, setResult] = useState<{ item: HistoryItem; name: string } | null>(null)
  const nameRef = useRef('')

  const job = useJob('stt', (item) => setResult({ item, name: nameRef.current }))

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
        setModels([])
        toastError(err)
      })
    return () => {
      alive = false
    }
  }, [tauri])

  const transcribe = (path: string, name: string) => {
    if (!modelId) return
    nameRef.current = name
    setActiveName(name)
    setResult(null)
    // `translate` is ignored by backends that do not support it yet.
    const args = { path, modelId, language, translate }
    void job.start(() => transcribeFile(args))
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
        <Progress value={null} className="mx-auto max-w-40" />
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
            <Button variant="primary" onClick={() => navigate('/models')}>
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
              <p className="truncate text-[14px] font-medium">{activeName}</p>
              <p className="mt-0.5 font-mono text-[11px] tabular-nums text-subtle">
                {job.progress === null ? 'Transcribing…' : `Transcribing · ${Math.round(job.progress * 100)}%`}
              </p>
            </div>
            <Button variant="ghost" size="sm" onClick={() => void job.cancel()}>
              Cancel
            </Button>
          </div>
          <Progress value={job.progress} className="mt-4" />
        </Card>
      ) : mode === 'record' ? (
        <Recorder
          disabled={!modelId && tauri}
          onRecorded={(path, durationMs) => transcribe(path, `Recording · ${formatDuration(durationMs)}`)}
        />
      ) : (
        <DropZone disabled={!modelId && tauri} onFile={(path) => transcribe(path, baseName(path))} />
      )}

      {result ? <TranscriptResult key={result.item.id} item={result.item} name={result.name} /> : null}
    </div>
  )
}
