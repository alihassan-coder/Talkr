import { historyExport, parseSegments, readHistoryAudio, saveExportBytes } from '@/lib/api'
import type { HistoryItem, SaveFormat } from '@/lib/types'
import { encodeMp3, parseWav } from './audio'

export type FormatInfo = { format: SaveFormat; label: string; description: string }

const AUDIO: FormatInfo[] = [
  { format: 'mp3', label: 'MP3', description: 'Small, plays everywhere' },
  { format: 'wav', label: 'WAV', description: 'Original, uncompressed' },
  { format: 'flac', label: 'FLAC', description: 'Lossless, about half the size' },
]

const TEXT: FormatInfo[] = [
  { format: 'txt', label: 'Plain text', description: '.txt' },
  { format: 'md', label: 'Markdown', description: '.md, with timestamps' },
  { format: 'srt', label: 'Subtitles', description: '.srt, for video players' },
  { format: 'vtt', label: 'Web subtitles', description: '.vtt, for the web' },
  { format: 'csv', label: 'Spreadsheet', description: '.csv, one row per segment' },
  { format: 'json', label: 'JSON', description: 'Text, timings and details' },
]

const TIMED = new Set<SaveFormat>(['srt', 'vtt', 'csv'])

export const formatLabel = (format: SaveFormat) =>
  [...AUDIO, ...TEXT].find((f) => f.format === format)?.label ?? format.toUpperCase()

/** Audio that can be re-encoded: a WAV Talkr wrote or recorded (transcribed files keep their own format). */
export const hasConvertibleAudio = (item: HistoryItem) => !!item.audioPath && /\.wav$/i.test(item.audioPath)

/** What `item` can be saved as, grouped for the menu. Timed formats need transcript segments. */
export function formatsFor(item: HistoryItem): { audio: FormatInfo[]; text: FormatInfo[] } {
  const timed = parseSegments(item).length > 0
  return {
    audio: hasConvertibleAudio(item) ? AUDIO : [],
    text: TEXT.filter((f) => timed || !TIMED.has(f.format)),
  }
}

/**
 * Save `item` as `format` through a native save dialog. Resolves to the saved path, or null when
 * the user cancelled. MP3 is encoded here first; `onProgress` reports that encoding (0..1).
 */
export async function saveItem(
  item: HistoryItem,
  format: SaveFormat,
  onProgress?: (progress: number) => void,
): Promise<string | null> {
  if (format !== 'mp3') return historyExport({ id: item.id, format })
  const wav = await readHistoryAudio({ id: item.id })
  const bytes = await encodeMp3(parseWav(wav), onProgress)
  return saveExportBytes({ id: item.id, format, bytes })
}
