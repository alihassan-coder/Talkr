import type { DictationSettings, DictationStatus } from '@/lib/types'

/** What dictation is doing right now, as the page's hero shows it. Most urgent first. */
export type LiveState =
  | 'unsupported'
  | 'permission'
  | 'error'
  | 'off'
  | 'noModel'
  | 'starting'
  | 'loading'
  | 'ready'
  | 'listening'

export function liveState(status: DictationStatus, dictation: DictationSettings): LiveState {
  if (!status.supported) return 'unsupported'
  if (status.permission.state === 'missing') return 'permission'
  if (!dictation.enabled) return 'off'
  if (status.error) return 'error'
  if (!status.modelId) return 'noModel'
  if (!status.active) return 'starting'
  if (status.recording) return 'listening'
  if (dictation.keepWarm && !status.warm) return 'loading'
  return 'ready'
}

/** The state in a few words, and whether it is something to fix. */
export const liveCopy: Record<LiveState, { label: string; tone: 'ok' | 'idle' | 'busy' | 'warn' }> = {
  unsupported: { label: 'Not available here yet', tone: 'idle' },
  permission: { label: 'Needs your permission', tone: 'warn' },
  error: { label: 'Stopped', tone: 'warn' },
  off: { label: 'Off', tone: 'idle' },
  noModel: { label: 'Needs a speech model', tone: 'warn' },
  starting: { label: 'Starting', tone: 'busy' },
  loading: { label: 'Loading the model', tone: 'busy' },
  ready: { label: 'Ready everywhere', tone: 'ok' },
  listening: { label: 'Listening', tone: 'ok' },
}
