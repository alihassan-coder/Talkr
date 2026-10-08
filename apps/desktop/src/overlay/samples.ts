import type { PillState } from '@/overlay/state'

/** The pill's states with believable content, for the Dictation page's preview. */
export const pillSamples = {
  listening: { kind: 'listening', session: 1, locked: false, app: 'Slack' },
  handsFree: { kind: 'listening', session: 1, locked: true, app: 'Slack' },
  working: { kind: 'working', session: 1, label: 'Transcribing' },
  done: { kind: 'done', session: 1, preview: 'Let’s ship the new onboarding on Friday.', app: 'Slack' },
  copied: { kind: 'notice', session: 2, tone: 'warn', title: 'Copied, press Ctrl + V to paste', detail: 'The window changed' },
  quiet: { kind: 'notice', session: 3, tone: 'info', title: 'No speech heard', detail: null },
  micError: { kind: 'notice', session: 4, tone: 'error', title: 'Microphone unavailable', detail: 'Check it is plugged in' },
  cancelled: { kind: 'cancelled', session: 5 },
} satisfies Record<string, PillState>

export type PillSample = keyof typeof pillSamples

/**
 * A microphone RMS that sounds like someone talking: syllables at about 4 a second, phrases
 * that swell and fade. Deterministic in `t` (seconds).
 */
export function speechLevel(t: number): number {
  const phrase = 0.3 + 0.35 * (1 + Math.sin(t * 0.9))
  const syllable = Math.max(0, Math.sin(t * 8.6 + Math.sin(t * 2.3) * 1.4)) ** 2
  const breath = 0.6 + 0.4 * Math.sin(t * 1.7 + 1)
  return 0.0025 + 0.16 * phrase * syllable * breath
}
