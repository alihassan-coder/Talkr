import { useEffect, useState } from 'react'
import type { Update } from '@tauri-apps/plugin-updater'
import { Button } from '@/components/ui'
import { Notice } from '@/components/Notice'
import { isTauri, stopEngine } from '@/lib/api'

type State =
  | { phase: 'idle' }
  | { phase: 'available'; update: Update }
  | { phase: 'installing'; update: Update; downloaded: number; total?: number }
  | { phase: 'failed'; update: Update; message: string }

/** Update problems never interrupt the user: they go to the log file and the banner stays away. */
async function logWarning(message: string) {
  try {
    const { warn } = await import('@tauri-apps/plugin-log')
    await warn(message)
  } catch {
    console.warn(message)
  }
}

/**
 * Only the packaged app checks: not the browser preview (no Tauri), not `tauri dev` and not tests
 * (both run a development build).
 */
const shouldCheck = () => isTauri() && import.meta.env.PROD

/**
 * Checks GitHub Releases once at startup (tauri-plugin-updater verifies the signature) and offers a
 * small banner when a newer Talkr is out.
 */
export function UpdateNotice({ enabled = shouldCheck() }: { enabled?: boolean }) {
  const [state, setState] = useState<State>({ phase: 'idle' })
  const [dismissed, setDismissed] = useState(false)

  useEffect(() => {
    if (!enabled) return
    let cancelled = false
    void (async () => {
      try {
        const { check } = await import('@tauri-apps/plugin-updater')
        const update = await check()
        if (!cancelled && update) setState({ phase: 'available', update })
      } catch (e) {
        void logWarning(`Update check failed: ${String(e)}`)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [enabled])

  if (state.phase === 'idle' || dismissed) return null
  const { update } = state

  const install = async () => {
    let downloaded = 0
    let total: number | undefined
    setState({ phase: 'installing', update, downloaded })
    try {
      await update.download((event) => {
        if (event.event === 'Started') total = event.data.contentLength
        else if (event.event === 'Progress') downloaded += event.data.chunkLength
        setState({ phase: 'installing', update, downloaded, total })
      })
      // The Windows installer has to replace the engine executable, which is locked while it runs.
      await stopEngine()
      await update.install()
      // Windows quits for the installer on its own; macOS and Linux need the relaunch.
      const { relaunch } = await import('@tauri-apps/plugin-process')
      await relaunch()
    } catch (e) {
      void logWarning(`Update to ${update.version} failed: ${String(e)}`)
      setState({ phase: 'failed', update, message: String(e) })
    }
  }

  const installing = state.phase === 'installing'
  const percent =
    installing && state.total ? Math.min(100, Math.round((state.downloaded / state.total) * 100)) : undefined

  return (
    <div className="fixed right-5 top-5 z-40 w-80">
      <Notice
        tone={state.phase === 'failed' ? 'error' : 'info'}
        title={`Talkr ${update.version} is available`}
        onDismiss={installing ? undefined : () => setDismissed(true)}
        action={
          <Button size="sm" variant="primary" loading={installing} onClick={() => void install()}>
            {installing
              ? percent === undefined
                ? 'Downloading…'
                : `Downloading… ${percent}%`
              : state.phase === 'failed'
                ? 'Try again'
                : 'Install and restart'}
          </Button>
        }
      >
        {state.phase === 'failed'
          ? 'The update could not be installed. Try again, or download it from the website.'
          : 'Talkr will restart to finish the update.'}
      </Notice>
    </div>
  )
}
