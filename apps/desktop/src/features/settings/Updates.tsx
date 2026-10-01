import { RotateCw } from 'lucide-react'
import { Button } from '@/components/ui'
import { isTauri } from '@/lib/api'
import { relativeTime } from '@/features/speak/utils'
import { useUpdates } from '@/stores/updates'
import { Row, Section, Switch } from './controls'

/** Settings → Updates: automatic checks, a manual check, and the update on offer. */
export function UpdatesSection({ version }: { version: string }) {
  const s = useUpdates()
  const tauri = isTauri()
  const checking = s.phase === 'checking'
  const busy = s.phase === 'downloading' || s.phase === 'installing'
  const offered = s.update && (s.phase === 'available' || s.phase === 'failed' || busy) ? s.update : null

  const status = checking
    ? 'Checking…'
    : offered
      ? `Talkr ${offered.version} is available${offered.version === s.skippedVersion ? ' (skipped)' : ''}.`
      : s.phase === 'failed' && s.error
        ? s.error
        : s.phase === 'current'
          ? `You have the latest version${s.lastChecked ? `, checked ${relativeTime(s.lastChecked)}` : ''}.`
          : 'Talkr looks for new versions on GitHub. Updates are signed and verified before they install.'

  return (
    <Section title="Updates">
      <Row label={version ? `Talkr ${version}` : 'Talkr'} description={<span aria-live="polite">{status}</span>}>
        {offered ? (
          <Button size="sm" variant="primary" loading={busy} onClick={() => void s.install()}>
            {s.phase === 'failed' ? 'Try again' : 'Install and restart'}
          </Button>
        ) : (
          <Button
            size="sm"
            loading={checking}
            disabled={!tauri}
            icon={<RotateCw className="size-3.5" strokeWidth={2} />}
            onClick={() => void s.check(true)}
          >
            Check now
          </Button>
        )}
      </Row>
      <Row label="Check automatically" description="At launch and every few hours. Nothing installs without your click.">
        <Switch label="Check for updates automatically" checked={s.autoCheck} onChange={s.setAutoCheck} />
      </Row>
    </Section>
  )
}
