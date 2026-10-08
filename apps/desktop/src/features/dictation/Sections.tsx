import { useEffect, useState } from 'react'
import type { ReactNode } from 'react'
import { ArrowRight } from 'lucide-react'
import { dictationWarmUp, isTauri, listInstalledModels, listMicrophones } from '@/lib/api'
import type {
  ActivationMode,
  DictationSettings,
  DictationStatus,
  FocusPolicy,
  InsertMethod,
  InstalledModel,
  Microphone,
  OverlayPosition,
  Settings,
  Shortcut,
} from '@/lib/types'
import { Badge, Segmented } from '@/components/ui'
import { Select } from '@/components/Select'
import { languages } from '@/lib/languages'
import { Row, Section, Switch } from '@/features/settings/controls'
import { ShortcutField } from '@/features/dictation/ShortcutField'
import { AppRulesEditor, ReplacementsEditor, VocabularyEditor } from '@/features/dictation/Editors'
import { altShiftV, ctrlWin, describeShortcut, pasteKeys, sameKeys } from '@/features/dictation/shortcut'
import { osWords, useKeyPlatform } from '@/features/dictation/platform'
import { CopyCommand, Explainer } from '@/features/dictation/Notices'
import { PillPreview } from '@/features/dictation/PillPreview'
import { TryIt } from '@/features/dictation/TryIt'

type Save = (patch: Partial<DictationSettings>) => Promise<boolean>

export type SectionProps = {
  settings: Settings
  dictation: DictationSettings
  status: DictationStatus
  save: Save
  /** Dictation cannot work on this system: its controls are shown but not changeable. */
  off: boolean
}

const set =
  (save: Save) =>
  <K extends keyof DictationSettings>(key: K) =>
  (value: DictationSettings[K]) =>
    void save({ [key]: value })

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

const insertHelp = (verifies: boolean): Record<InsertMethod, string> => ({
  auto: verifies
    ? 'Picks the best way per field, checks the text arrived, and switches method if an app refuses it.'
    : 'Picks the best way for each app.',
  paste: 'Pastes through the clipboard everywhere. Fast, and works in most apps.',
  type: 'Types the text key by key. Slower, but works in remote desktops and games.',
})

const focusOptions: { value: FocusPolicy; label: string }[] = [
  { value: 'original', label: 'Go back to where I started' },
  { value: 'current', label: 'Type where my cursor is now' },
  { value: 'copy', label: 'Only copy it' },
]

const positionOptions: { value: OverlayPosition; label: string }[] = [
  { value: 'bottom', label: 'Bottom' },
  { value: 'top', label: 'Top' },
]

/** A section with an anchor, so the page's index can jump to it. */
export function Anchored({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  return (
    <div id={`dictation-${id}`} className="scroll-mt-20">
      <Section title={title}>{children}</Section>
    </div>
  )
}

export function TrySection({ dictation, status }: SectionProps) {
  return (
    <Anchored id="try" title="Try it">
      <TryIt dictation={dictation} capabilities={status.capabilities} hasLast={status.hasLast} />
    </Anchored>
  )
}

export function ShortcutSection({ dictation, status, save, off }: SectionProps) {
  const platform = useKeyPlatform()
  const caps = status.capabilities
  const describe = (s: Shortcut) => describeShortcut(s, platform)
  return (
    <Anchored id="shortcut" title="Shortcut">
      {caps.recordsShortcut ? (
        <Row label="Dictation shortcut" description={`Works in every app, even when Talkr is in ${osWords(caps.os).tray}.`}>
          <ShortcutField
            label="dictation shortcut"
            value={dictation.shortcut}
            fallback={ctrlWin}
            disabled={off}
            onChange={(shortcut) => void save({ shortcut })}
            validate={(s) =>
              dictation.pasteLastEnabled && sameKeys(s, dictation.pasteLastShortcut)
                ? `That is already the paste-again shortcut (${describe(dictation.pasteLastShortcut)}).`
                : null
            }
          />
        </Row>
      ) : (
        <Explainer title="Your desktop chooses the shortcut">
          <p>
            On this system apps cannot watch the keyboard, so your desktop asks you for a shortcut the first time
            dictation turns on. Change it any time in your desktop’s keyboard settings, under shortcuts for Talkr.
          </p>
          <p className="mt-2.5 flex flex-wrap items-center gap-2">
            You can also bind any key to this command, which starts and stops dictation:
            <CopyCommand command="talkr --dictate" />
          </p>
        </Explainer>
      )}
      {caps.holdToTalk ? (
        <Row label="How it starts" description={modeHelp[dictation.mode]}>
          <Segmented
            label="How dictation starts"
            value={dictation.mode}
            options={modeOptions}
            onChange={set(save)('mode')}
            disabled={off}
          />
        </Row>
      ) : (
        <Row
          label="How it starts"
          description="Press once to start and again to insert. This system does not report when a shortcut is let go, so holding to talk is not possible here."
        >
          <Badge>Press twice</Badge>
        </Row>
      )}
      {caps.recordsShortcut ? (
        <>
          <Row
            label="Paste last dictation"
            description="Inserts your last dictation again, wherever the cursor is now. Handy if it went to the wrong place."
          >
            <Switch
              label="Paste last dictation"
              checked={dictation.pasteLastEnabled}
              disabled={off}
              onChange={set(save)('pasteLastEnabled')}
            />
          </Row>
          {dictation.pasteLastEnabled ? (
            <Row label="Paste-again shortcut">
              <ShortcutField
                label="paste-again shortcut"
                value={dictation.pasteLastShortcut}
                fallback={altShiftV}
                disabled={off}
                onChange={(pasteLastShortcut) => void save({ pasteLastShortcut })}
                validate={(s) =>
                  sameKeys(s, dictation.shortcut) ? `That is already the dictation shortcut (${describe(dictation.shortcut)}).` : null
                }
              />
            </Row>
          ) : null}
        </>
      ) : null}
    </Anchored>
  )
}

export function SpeechSection({ settings, dictation, status, save, off }: SectionProps) {
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

  const hasLanguage = languages.some(([code]) => code === dictation.language)
  const transcribeLanguage = languages.find(([c]) => c === settings.sttLanguage)?.[1] ?? settings.sttLanguage
  return (
    <Anchored id="speech" title="Speech">
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
            { value: '', label: `Same as Transcribe (${transcribeLanguage})` },
            ...languages.map(([code, name]) => ({ value: code, label: name })),
            ...(dictation.language && !hasLanguage ? [{ value: dictation.language, label: dictation.language }] : []),
          ]}
        />
      </Row>
      <Row label="Keep the model ready" description="Text appears without waiting for the model to load. Uses memory while dictation is on.">
        <Switch label="Keep the model ready" checked={dictation.keepWarm} disabled={off} onChange={set(save)('keepWarm')} />
      </Row>
      <Row
        label="Microphone"
        description={
          micError ? <span role="alert">Could not list microphones: {micError}</span> : 'Used for dictation and for recordings in Transcribe.'
        }
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
    </Anchored>
  )
}

export function TypingSection({ dictation, status, save, off }: SectionProps) {
  const platform = useKeyPlatform()
  const caps = status.capabilities
  const on = set(save)
  return (
    <Anchored id="typing" title="Typing">
      {caps.insertsText ? (
        <>
          <Row label="How text goes in" description={insertHelp(caps.verifiesInsertion)[dictation.insertMethod]}>
            <Segmented label="How text goes in" value={dictation.insertMethod} options={insertOptions} onChange={on('insertMethod')} disabled={off} />
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
            <Switch label="Keep my clipboard" checked={dictation.restoreClipboard} disabled={off} onChange={on('restoreClipboard')} />
          </Row>
          <Row
            label="Smart spacing and capitals"
            description="Adds a space after a word and a capital at the start of a sentence, based on the text before the cursor."
          >
            <Switch label="Smart spacing and capitals" checked={dictation.smartSpacing} disabled={off} onChange={on('smartSpacing')} />
          </Row>
        </>
      ) : (
        <Explainer title="Your words are copied, ready to paste">
          On this system Talkr cannot type into other apps. When you finish speaking, the text is on the clipboard:
          press <span className="font-medium text-fg">{pasteKeys(platform)}</span> where you want it.
        </Explainer>
      )}
      <Row label="Remove filler words" description="Drops “um”, “uh” and similar.">
        <Switch label="Remove filler words" checked={dictation.removeFillers} disabled={off} onChange={on('removeFillers')} />
      </Row>
      <Row label="Voice commands" description="Say “new line” or “new paragraph” for line breaks (English).">
        <Switch label="Voice commands" checked={dictation.voiceCommands} disabled={off} onChange={on('voiceCommands')} />
      </Row>
    </Anchored>
  )
}

export function WordsSection({ dictation, save }: SectionProps) {
  return (
    <Anchored id="words" title="Your words">
      <div className="border-b border-line px-5 pt-4">
        <p className="text-[13.5px] font-medium tracking-[-0.005em]">Vocabulary</p>
        <p className="mt-0.5 text-[13px] leading-relaxed text-muted">
          Names and terms the model should spell your way. Works best with Small and Turbo models; for a word it still
          gets wrong, add a replacement below.
        </p>
      </div>
      <div className="border-b border-line">
        <VocabularyEditor words={dictation.vocabulary} onChange={set(save)('vocabulary')} />
      </div>
      <div className="px-5 pt-4">
        <p className="text-[13.5px] font-medium tracking-[-0.005em]">Replacements</p>
        <p className="mt-0.5 inline-flex flex-wrap items-center gap-1 text-[13px] leading-relaxed text-muted">
          Fix words it keeps getting wrong, or make shortcuts: say “my email” <ArrowRight className="size-3" strokeWidth={2} aria-label="and" /> get
          your address.
        </p>
      </div>
      <ReplacementsEditor items={dictation.replacements} onChange={set(save)('replacements')} />
    </Anchored>
  )
}

export function AppsSection({ dictation, status, save }: SectionProps) {
  const caps = status.capabilities
  if (!caps.insertsText) return null
  return (
    <Anchored id="apps" title="Apps">
      <div className="px-5 pt-4">
        <p className="text-[13.5px] font-medium tracking-[-0.005em]">Per-app rules</p>
        <p className="mt-0.5 text-[13px] leading-relaxed text-muted">Choose how text goes into a particular app, or turn dictation off there.</p>
      </div>
      <AppRulesEditor rules={dictation.appRules} onChange={set(save)('appRules')} os={caps.os} />
    </Anchored>
  )
}

export function PillSection({ dictation, save, off }: SectionProps) {
  const on = set(save)
  return (
    <Anchored id="pill" title="Pill and sounds">
      <div className="border-b border-line">
        <PillPreview position={dictation.overlayPosition} showTarget={dictation.showTarget} />
      </div>
      <Row label="Position" description="Where the pill appears, on the screen you are working on.">
        <Segmented label="Pill position" value={dictation.overlayPosition} options={positionOptions} onChange={on('overlayPosition')} disabled={off} />
      </Row>
      <Row label="Show the target app" description="The pill shows where your words will go, like “→ Slack”.">
        <Switch label="Show the target app" checked={dictation.showTarget} disabled={off} onChange={on('showTarget')} />
      </Row>
      <Row label="Sounds" description="Soft chimes when listening starts and stops.">
        <Switch label="Sounds" checked={dictation.sounds} disabled={off} onChange={on('sounds')} />
      </Row>
    </Anchored>
  )
}

export function BackgroundSection({ dictation, status, save, off }: SectionProps) {
  const words = osWords(status.capabilities.os)
  const on = set(save)
  return (
    <Anchored id="privacy" title="Background and privacy">
      <Row label={words.login} description={`Talkr starts in ${words.tray} when you sign in, ready to dictate.`}>
        <Switch label={words.login} checked={dictation.launchAtLogin} disabled={off} onChange={on('launchAtLogin')} />
      </Row>
      <Row
        label="Keep running when closed"
        description={`Closing the window keeps Talkr in ${words.tray} while dictation is on. Quit from its icon there.`}
      >
        <Switch label="Keep running when closed" checked={dictation.closeToTray} disabled={off} onChange={on('closeToTray')} />
      </Row>
      <Row
        label="Save dictations to History"
        description="Keeps what you dictate so you can find it later. Text typed into password fields is never saved."
      >
        <Switch label="Save dictations to History" checked={dictation.saveHistory} onChange={on('saveHistory')} />
      </Row>
      <Row
        label="Private by design"
        description={
          status.capabilities.recordsShortcut
            ? 'Audio and text never leave this computer. Talkr reads the keyboard only to spot your shortcut, and never records what you type.'
            : 'Audio and text never leave this computer. Talkr does not read the keyboard here: your desktop tells it when the shortcut is pressed.'
        }
      />
    </Anchored>
  )
}
