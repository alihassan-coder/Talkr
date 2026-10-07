import { useEffect, useState } from 'react'
import { RotateCw, TriangleAlert } from 'lucide-react'
import { dictationWarmUp, isTauri, listInstalledModels, listMicrophones } from '@/lib/api'
import type {
  ActivationMode,
  DictationSettings,
  FocusPolicy,
  InsertMethod,
  InstalledModel,
  Microphone,
  OverlayPosition,
  Shortcut,
} from '@/lib/types'
import { Button, EmptyState, PageHeader, Segmented } from '@/components/ui'
import { Select } from '@/components/Select'
import { cx } from '@/lib/cx'
import { languages } from '@/lib/languages'
import { Row, Section, Switch } from '@/features/settings/controls'
import { useDictation } from '@/features/dictation/useDictation'
import { Hero, TryIt } from '@/features/dictation/Hero'
import { ShortcutField } from '@/features/dictation/ShortcutField'
import { AppRulesEditor, ReplacementsEditor, VocabularyEditor } from '@/features/dictation/Editors'
import { altShiftV, ctrlWin, describeShortcut, sameKeys } from '@/features/dictation/shortcut'

const modeOptions: { value: ActivationMode; label: string }[] = [
  { value: 'auto', label: 'Hold or tap' },
  { value: 'hold', label: 'Hold' },
  { value: 'toggle', label: 'Press twice' },
]

const modeHelp: Record<ActivationMode, string> = {
  auto: 'Hold to talk and let go to insert. A quick tap keeps listening hands-free until you press again.',
  hold: 'Listens only while you hold the shortcut.',
  toggle: 'Press once to start, press again to insert.',
}

const insertOptions: { value: InsertMethod; label: string }[] = [
  { value: 'auto', label: 'Automatic' },
  { value: 'paste', label: 'Paste' },
  { value: 'type', label: 'Type keys' },
]

const insertHelp: Record<InsertMethod, string> = {
  auto: 'Picks the best way per field, checks the text arrived, and switches method if an app refuses it.',
  paste: 'Pastes through the clipboard everywhere. Fast, and works in most apps.',
  type: 'Types the text key by key. Slower, but works in remote desktops and games.',
}

const focusOptions: { value: FocusPolicy; label: string }[] = [
  { value: 'original', label: 'Go back to where I started' },
  { value: 'current', label: 'Type where my cursor is now' },
  { value: 'copy', label: 'Only copy it' },
]

const positionOptions: { value: OverlayPosition; label: string }[] = [
  { value: 'bottom', label: 'Bottom' },
  { value: 'top', label: 'Top' },
]

export function DictationPage() {
  const { settings, dictation, status, error, saving, save, retry } = useDictation()
  const [models, setModels] = useState<InstalledModel[]>([])
  const [mics, setMics] = useState<Microphone[] | null>(null)
  const [micError, setMicError] = useState<string | null>(null)

  useEffect(() => {
    if (!isTauri()) return
    let alive = true
    listInstalledModels()
      .then((m) => alive && setModels(m.filter((x) => x.kind === 'stt')))
      .catch(() => {})
    listMicrophones()
      .then((m) => alive && setMics(m))
      .catch((e: unknown) => alive && setMicError(String(e)))
    return () => {
      alive = false
    }
  }, [status.modelId])

  if (!settings || !dictation) {
    return (
      <div className="space-y-8">
        <PageHeader title="Dictation" />
        {error ? (
          <EmptyState
            icon={<TriangleAlert className="size-4" strokeWidth={1.75} />}
            title="Could not load dictation settings"
            description={error}
            action={
              <Button icon={<RotateCw className="size-3.5" strokeWidth={2} />} onClick={retry}>
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

  const set = <K extends keyof DictationSettings>(key: K) => (value: DictationSettings[K]) => void save({ [key]: value })
  const off = !status.supported
  const hasLanguage = languages.some(([code]) => code === dictation.language)

  return (
    <div className="space-y-10 pb-6">
      <PageHeader
        title="Dictation"
        description="Speak in any app. Text goes where your cursor is."
        actions={
          <span
            aria-live="polite"
            className={cx('font-mono text-[11px] text-subtle transition-opacity duration-300', saving === 'idle' ? 'opacity-0' : 'opacity-100')}
          >
            {saving === 'saving' ? 'Saving' : 'Saved'}
          </span>
        }
      />

      <Hero settings={settings} status={status} onToggle={(enabled) => void save({ enabled })} />

      {status.supported ? (
        <Section title="Try it">
          <TryIt dictation={dictation} enabled={dictation.enabled} />
        </Section>
      ) : null}

      <Section title="Shortcut">
        <Row label="Dictation shortcut" description="Works in every app, even when Talkr is in the tray.">
          <ShortcutField
            label="dictation shortcut"
            value={dictation.shortcut}
            fallback={ctrlWin}
            disabled={off}
            onChange={(shortcut: Shortcut) => void save({ shortcut })}
            validate={(s) =>
              dictation.pasteLastEnabled && sameKeys(s, dictation.pasteLastShortcut)
                ? `That is already the paste-again shortcut (${describeShortcut(dictation.pasteLastShortcut)}).`
                : null
            }
          />
        </Row>
        <Row label="How it starts" description={modeHelp[dictation.mode]}>
          <Segmented label="How dictation starts" value={dictation.mode} options={modeOptions} onChange={set('mode')} disabled={off} />
        </Row>
        <Row
          label="Paste last dictation"
          description="Inserts your last dictation again, wherever the cursor is now. Handy if it went to the wrong place."
        >
          <Switch label="Paste last dictation" checked={dictation.pasteLastEnabled} disabled={off} onChange={set('pasteLastEnabled')} />
        </Row>
        {dictation.pasteLastEnabled ? (
          <Row label="Paste-again shortcut">
            <ShortcutField
              label="paste-again shortcut"
              value={dictation.pasteLastShortcut}
              fallback={altShiftV}
              disabled={off}
              onChange={(pasteLastShortcut: Shortcut) => void save({ pasteLastShortcut })}
              validate={(s) =>
                sameKeys(s, dictation.shortcut) ? `That is already the dictation shortcut (${describeShortcut(dictation.shortcut)}).` : null
              }
            />
          </Row>
        ) : null}
      </Section>

      <Section title="Speech">
        <Row label="Model" description="Turbo models are fast and accurate on a graphics card. Small ones suit older computers.">
          <Select
            label=""
            aria-label="Dictation model"
            className="max-w-60"
            value={dictation.model ?? ''}
            placeholder="Automatic"
            onChange={(id) => {
              void save({ model: id || null }).then((ok) => ok && dictation.keepWarm && void dictationWarmUp().catch(() => {}))
            }}
            options={[
              { value: '', label: 'Automatic (best installed)' },
              ...models.map((m) => ({ value: m.id, label: m.name })),
              ...(dictation.model && !models.some((m) => m.id === dictation.model)
                ? [{ value: dictation.model, label: `${dictation.model} (not installed)` }]
                : []),
            ]}
          />
        </Row>
        <Row label="Language" description="The language you speak. Auto handles switching between languages.">
          <Select
            label=""
            aria-label="Dictation language"
            value={dictation.language ?? ''}
            onChange={(code) => void save({ language: code || null })}
            options={[
              { value: '', label: `Same as Transcribe (${languages.find(([c]) => c === settings.sttLanguage)?.[1] ?? settings.sttLanguage})` },
              ...languages.map(([code, name]) => ({ value: code, label: name })),
              ...(dictation.language && !hasLanguage ? [{ value: dictation.language, label: dictation.language }] : []),
            ]}
          />
        </Row>
        <Row
          label="Keep the model ready"
          description="Text appears without waiting for the model to load. Uses memory while dictation is on."
        >
          <Switch label="Keep the model ready" checked={dictation.keepWarm} disabled={off} onChange={set('keepWarm')} />
        </Row>
        <Row
          label="Microphone"
          description={micError ? <span role="alert">Could not list microphones: {micError}</span> : 'Used for dictation and for recordings in Transcribe.'}
        >
          <Select
            label=""
            aria-label="Microphone"
            className="max-w-64"
            value={dictation.microphone ?? ''}
            onChange={(id) => void save({ microphone: id || null })}
            options={[
              { value: '', label: 'System default' },
              ...(mics ?? []).map((m) => ({ value: m.id, label: m.isDefault ? `${m.name} (default)` : m.name })),
              ...(dictation.microphone && mics && !mics.some((m) => m.id === dictation.microphone)
                ? [{ value: dictation.microphone, label: 'Disconnected microphone' }]
                : []),
            ]}
          />
        </Row>
      </Section>

      <Section title="Typing">
        <Row label="How text goes in" description={insertHelp[dictation.insertMethod]}>
          <Segmented label="How text goes in" value={dictation.insertMethod} options={insertOptions} onChange={set('insertMethod')} disabled={off} />
        </Row>
        <Row
          label="If you switch windows while speaking"
          description="Talkr remembers the field you started in, so text never lands in the wrong app."
        >
          <Select
            label=""
            aria-label="If you switch windows while speaking"
            value={dictation.focusPolicy}
            onChange={(v) => void save({ focusPolicy: v as FocusPolicy })}
            options={focusOptions}
          />
        </Row>
        <Row label="Keep my clipboard" description="Puts back what you had copied after pasting, and keeps dictations out of clipboard history.">
          <Switch label="Keep my clipboard" checked={dictation.restoreClipboard} disabled={off} onChange={set('restoreClipboard')} />
        </Row>
        <Row label="Smart spacing and capitals" description="Adds a space after a word and a capital at the start of a sentence, based on the text before the cursor.">
          <Switch label="Smart spacing and capitals" checked={dictation.smartSpacing} disabled={off} onChange={set('smartSpacing')} />
        </Row>
        <Row label="Remove filler words" description="Drops “um”, “uh” and similar.">
          <Switch label="Remove filler words" checked={dictation.removeFillers} disabled={off} onChange={set('removeFillers')} />
        </Row>
        <Row label="Voice commands" description="Say “new line” or “new paragraph” for line breaks (English).">
          <Switch label="Voice commands" checked={dictation.voiceCommands} disabled={off} onChange={set('voiceCommands')} />
        </Row>
      </Section>

      <Section title="Your words">
        <div className="border-b border-line px-5 pt-4">
          <p className="text-[13.5px] font-medium tracking-[-0.005em]">Vocabulary</p>
          <p className="mt-0.5 text-[13px] leading-relaxed text-muted">
            Names and terms the model should spell your way. Works best with Small and Turbo models; for a word it
            still gets wrong, add a replacement below.
          </p>
        </div>
        <div className="border-b border-line">
          <VocabularyEditor words={dictation.vocabulary} onChange={set('vocabulary')} />
        </div>
        <div className="px-5 pt-4">
          <p className="text-[13.5px] font-medium tracking-[-0.005em]">Replacements</p>
          <p className="mt-0.5 text-[13px] leading-relaxed text-muted">
            Fix words it keeps getting wrong, or make shortcuts: say “my email”, get your address.
          </p>
        </div>
        <ReplacementsEditor items={dictation.replacements} onChange={set('replacements')} />
      </Section>

      <Section title="Apps">
        <div className="px-5 pt-4">
          <p className="text-[13.5px] font-medium tracking-[-0.005em]">Per-app rules</p>
          <p className="mt-0.5 text-[13px] leading-relaxed text-muted">
            Choose how text goes into a particular app, or turn dictation off there.
          </p>
        </div>
        <AppRulesEditor rules={dictation.appRules} onChange={set('appRules')} />
      </Section>

      <Section title="Pill and sounds">
        <Row label="Position" description="Where the dictation pill appears, on the screen you are working on.">
          <Segmented label="Pill position" value={dictation.overlayPosition} options={positionOptions} onChange={set('overlayPosition')} disabled={off} />
        </Row>
        <Row label="Show the target app" description="The pill shows where your words will go, like “→ Slack”.">
          <Switch label="Show the target app" checked={dictation.showTarget} disabled={off} onChange={set('showTarget')} />
        </Row>
        <Row label="Sounds" description="Soft chimes when listening starts and stops.">
          <Switch label="Sounds" checked={dictation.sounds} disabled={off} onChange={set('sounds')} />
        </Row>
      </Section>

      <Section title="Background and privacy">
        <Row label="Start with Windows" description="Talkr starts in the tray when you sign in, ready to dictate.">
          <Switch label="Start with Windows" checked={dictation.launchAtLogin} disabled={off} onChange={set('launchAtLogin')} />
        </Row>
        <Row label="Keep running when closed" description="Closing the window keeps Talkr in the tray while dictation is on. Quit from the tray icon.">
          <Switch label="Keep running when closed" checked={dictation.closeToTray} disabled={off} onChange={set('closeToTray')} />
        </Row>
        <Row
          label="Save dictations to History"
          description="Keeps what you dictate so you can find it later. Text typed into password fields is never saved."
        >
          <Switch label="Save dictations to History" checked={dictation.saveHistory} onChange={set('saveHistory')} />
        </Row>
        <Row
          label="Privacy"
          description="Audio and text never leave this computer. Talkr reads the keyboard only to spot your shortcut, and never records what you type."
        />
      </Section>
    </div>
  )
}
