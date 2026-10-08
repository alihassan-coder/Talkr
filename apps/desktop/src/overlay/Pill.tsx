import { useEffect, useRef, useState } from 'react'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { dictationCancel, dictationStop, isTauri } from '@/lib/api'
import { PillView } from '@/overlay/PillView'
import { notePillUp } from '@/overlay/state'
import type { PillState } from '@/overlay/state'

export type { PillState } from '@/overlay/state'

/** The overlay window's pill: follows the engine's `dictation://overlay` and `dictation://level`. */
export function Pill({ initial = { kind: 'hidden' } }: { initial?: PillState }) {
  const [state, setState] = useState<PillState>(initial)
  // Bumped whenever the window becomes visible, to replay the entrance.
  const [entrance, setEntrance] = useState(0)
  const level = useRef(0)

  useEffect(() => {
    if (!isTauri()) return
    const win = getCurrentWebviewWindow()
    const unlisten = [
      win.listen<PillState>('dictation://overlay', (e) => {
        notePillUp(e.payload.kind !== 'hidden')
        setState(e.payload)
        if (e.payload.kind !== 'listening') level.current = 0
      }),
      win.listen<{ session: number; level: number }>('dictation://level', (e) => {
        level.current = e.payload.level
      }),
    ]
    const onVisible = () => {
      if (document.visibilityState === 'visible') setEntrance((n) => n + 1)
    }
    document.addEventListener('visibilitychange', onVisible)
    return () => {
      document.removeEventListener('visibilitychange', onVisible)
      for (const p of unlisten) void p.then((f) => f())
    }
  }, [])

  return (
    <PillView
      className="pill-window"
      state={state}
      level={level}
      entrance={entrance}
      onStop={() => void dictationStop()}
      onCancel={() => void dictationCancel()}
    />
  )
}
