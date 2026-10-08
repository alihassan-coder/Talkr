import { useEffect, useRef } from 'react'
import { PillView } from '@/overlay/PillView'
import { pillSamples, speechLevel } from '@/overlay/samples'
import type { PillSample } from '@/overlay/samples'

/** Development only: a sample pill with a synthetic voice, for looking at it in a browser. */
export function Demo() {
  const name = new URLSearchParams(window.location.search).get('state') ?? 'listening'
  const state = pillSamples[name as PillSample] ?? pillSamples.listening
  const level = useRef(0)
  useEffect(() => {
    let frame = 0
    const tick = (now: number) => {
      level.current = speechLevel(now / 1000)
      frame = requestAnimationFrame(tick)
    }
    frame = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(frame)
  }, [])
  return <PillView className="pill-window" state={state} level={level} onStop={() => {}} onCancel={() => {}} />
}
