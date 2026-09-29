import type { HistoryItem, HistoryKind } from '@/lib/types'

// Shown only in a plain browser (no Tauri backend) so the screen can be designed without the app.
const now = Date.now()
const hour = 3_600_000

const base = {
  audioPath: null,
  voiceId: null,
  language: 'en',
  device: 'cpu',
  processingMs: 1200,
  favorite: false,
  segmentsJson: null,
} satisfies Partial<HistoryItem>

export const sampleHistory: HistoryItem[] = [
  {
    ...base,
    id: 'sample-1',
    kind: 'stt',
    createdAt: now - 0.4 * hour,
    title: 'Q3 planning call',
    text: 'Okay, let us start with the quarterly numbers. Revenue is up eleven percent, mostly from the new plan. Next, hiring: we want two more engineers before October.',
    durationMs: 2_520_000,
    modelId: 'whisper-base.en',
    favorite: true,
    segmentsJson: JSON.stringify([
      { startMs: 0, endMs: 4200, text: 'Okay, let us start with the quarterly numbers.' },
      { startMs: 4200, endMs: 9800, text: 'Revenue is up eleven percent, mostly from the new plan.' },
      { startMs: 9800, endMs: 14100, text: 'Next, hiring: we want two more engineers before October.' },
    ]),
  },
  {
    ...base,
    id: 'sample-2',
    kind: 'tts',
    createdAt: now - 3 * hour,
    title: 'Quarterly update intro',
    text: 'Welcome to the quarterly update. In the next ten minutes we will walk through what shipped, what slipped, and what comes next.',
    durationMs: 9_400,
    modelId: 'kokoro-en-v0_19',
    voiceId: 'af_heart',
  },
  {
    ...base,
    id: 'sample-3',
    kind: 'tts',
    createdAt: now - 26 * hour,
    title: 'Chapter one, opening',
    text: 'The house at the end of the lane had been empty for eleven years when the lights came back on.',
    durationMs: 6_100,
    modelId: 'piper-en_US-amy',
    voiceId: 'amy',
  },
  {
    ...base,
    id: 'sample-4',
    kind: 'stt',
    createdAt: now - 5 * 24 * hour,
    title: 'Voice memo',
    text: 'Remember to send the draft to Maria and book the room for Thursday.',
    durationMs: 7_800,
    modelId: 'whisper-small',
  },
]

export function filterSample(query: string, kind: HistoryKind | null, favoritesOnly: boolean) {
  const q = query.trim().toLowerCase()
  return sampleHistory.filter(
    (item) =>
      (!kind || item.kind === kind) &&
      (!favoritesOnly || item.favorite) &&
      (!q || item.title.toLowerCase().includes(q) || item.text.toLowerCase().includes(q)),
  )
}
