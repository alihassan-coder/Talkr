import { RotateCw, TriangleAlert } from 'lucide-react'
import { Button, EmptyState, PageHeader } from '@/components/ui'
import { cx } from '@/lib/cx'
import { useDictation } from '@/features/dictation/useDictation'
import { Hero, ModelDownload } from '@/features/dictation/Hero'
import { Setup } from '@/features/dictation/Setup'
import { useSetup } from '@/features/dictation/useSetup'
import { PermissionBanner } from '@/features/dictation/Notices'
import { SectionNav } from '@/features/dictation/SectionNav'
import {
  AppsSection,
  BackgroundSection,
  PillSection,
  ShortcutSection,
  SpeechSection,
  TrySection,
  TypingSection,
  WordsSection,
} from '@/features/dictation/Sections'
import type { SectionProps } from '@/features/dictation/Sections'
import { KeyPlatformContext, keyPlatformOf } from '@/features/dictation/platform'
import { describeShortcut, sameKeys } from '@/features/dictation/shortcut'
import '@/features/dictation/dictation.css'

export function DictationPage() {
  const { settings, dictation, status, error, saving, save, retry, refreshStatus } = useDictation()
  const setup = useSetup(status)
  const platform = keyPlatformOf(status.capabilities)

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
          <div className="space-y-3" aria-busy="true">
            <div className="h-44 animate-pulse rounded-2xl border border-line bg-surface" />
            {[0, 1].map((i) => (
              <div key={i} className="h-28 animate-pulse rounded-2xl border border-line bg-surface" />
            ))}
          </div>
        )}
      </div>
    )
  }

  const props: SectionProps = { settings, dictation, status, save, off: !status.supported }
  const permission = status.permission.state === 'missing' ? status.permission : null
  const sections = [
    ...(status.supported ? [['try', 'Try it'] as const] : []),
    ['shortcut', 'Shortcut'] as const,
    ['speech', 'Speech'] as const,
    ['typing', 'Typing'] as const,
    ['words', 'Your words'] as const,
    ...(status.capabilities.insertsText ? [['apps', 'Apps'] as const] : []),
    ['pill', 'Pill'] as const,
    ['privacy', 'Privacy'] as const,
  ]

  return (
    <KeyPlatformContext value={platform}>
      <div className="space-y-8 pb-6">
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

        {permission ? <PermissionBanner permission={permission} onCheck={refreshStatus} /> : null}

        <Hero settings={settings} status={status} onToggle={(enabled) => void save({ enabled })}>
          {status.supported && !status.modelId && !setup.open ? <ModelDownload settings={settings} /> : null}
        </Hero>

        {setup.open ? (
          <Setup
            settings={settings}
            status={status}
            setup={setup}
            onShortcut={(shortcut) => void save({ shortcut })}
            onEnable={() => void save({ enabled: true })}
            validateShortcut={(s) =>
              dictation.pasteLastEnabled && sameKeys(s, dictation.pasteLastShortcut)
                ? `That is already the paste-again shortcut (${describeShortcut(dictation.pasteLastShortcut, platform)}).`
                : null
            }
          />
        ) : null}

        <SectionNav sections={sections} />

        {status.supported ? <TrySection {...props} /> : null}
        <ShortcutSection {...props} />
        <SpeechSection {...props} />
        <TypingSection {...props} />
        <WordsSection {...props} />
        <AppsSection {...props} />
        <PillSection {...props} />
        <BackgroundSection {...props} />
      </div>
    </KeyPlatformContext>
  )
}
