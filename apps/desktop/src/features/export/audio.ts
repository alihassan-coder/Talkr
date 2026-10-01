// Audio conversions done in the webview: WAV parsing and MP3 encoding. FLAC is encoded by the
// backend (lossless, streamed from disk); MP3 needs LAME, which ships as pure JavaScript and is
// only loaded the first time someone exports an MP3.

export type PcmAudio = {
  sampleRate: number
  /** One Int16Array per channel. */
  channels: Int16Array[]
}

const text = (view: DataView, offset: number) =>
  String.fromCharCode(view.getUint8(offset), view.getUint8(offset + 1), view.getUint8(offset + 2), view.getUint8(offset + 3))

/**
 * Read a RIFF/WAVE file into 16-bit PCM channels. Accepts 8, 16, 24 and 32-bit integer PCM and
 * 32-bit float (plain or WAVE_FORMAT_EXTENSIBLE). Throws a readable error for anything else.
 */
export function parseWav(buffer: ArrayBuffer): PcmAudio {
  const view = new DataView(buffer)
  if (buffer.byteLength < 12 || text(view, 0) !== 'RIFF' || text(view, 8) !== 'WAVE') {
    throw new Error('The audio is not a WAV file')
  }

  let format = 0
  let channelCount = 0
  let sampleRate = 0
  let bits = 0
  let data: { offset: number; length: number } | null = null

  let offset = 12
  while (offset + 8 <= buffer.byteLength) {
    const id = text(view, offset)
    const size = view.getUint32(offset + 4, true)
    const body = offset + 8
    if (id === 'fmt ' && size >= 16) {
      format = view.getUint16(body, true)
      channelCount = view.getUint16(body + 2, true)
      sampleRate = view.getUint32(body + 4, true)
      bits = view.getUint16(body + 14, true)
      // WAVE_FORMAT_EXTENSIBLE keeps the real format in the sub-format GUID's first two bytes.
      if (format === 0xfffe && size >= 26) format = view.getUint16(body + 24, true)
    } else if (id === 'data') {
      // A file cut short (or written by a streaming tool) can claim more data than it has.
      data = { offset: body, length: Math.min(size, buffer.byteLength - body) }
      break
    }
    offset = body + size + (size % 2) // chunks are word aligned
  }

  const isInt = format === 1 && [8, 16, 24, 32].includes(bits)
  const isFloat = format === 3 && bits === 32
  if (!data || channelCount < 1 || sampleRate < 1 || (!isInt && !isFloat)) {
    throw new Error('This WAV format cannot be converted')
  }

  const bytesPerSample = bits / 8
  const frames = Math.floor(data.length / (bytesPerSample * channelCount))
  const channels = Array.from({ length: channelCount }, () => new Int16Array(frames))
  const read = (pos: number): number => {
    if (isFloat) {
      const v = view.getFloat32(pos, true)
      return Number.isFinite(v) ? Math.round(Math.max(-1, Math.min(1, v)) * 32767) : 0
    }
    switch (bits) {
      case 8:
        return (view.getUint8(pos) - 128) << 8
      case 16:
        return view.getInt16(pos, true)
      case 24:
        return ((view.getUint8(pos) | (view.getUint8(pos + 1) << 8) | (view.getInt8(pos + 2) << 16)) >> 8)
      default:
        return view.getInt32(pos, true) >> 16
    }
  }
  let pos = data.offset
  for (let i = 0; i < frames; i++) {
    for (let c = 0; c < channelCount; c++) {
      channels[c]![i] = read(pos)
      pos += bytesPerSample
    }
  }
  return { sampleRate, channels }
}

/** Sample rates MP3 (MPEG 1, 2 and 2.5) can carry. */
export const MP3_RATES = [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000]

/** Bitrate for clear speech at a given rate: MPEG-2.5 caps at 64 kbps, MPEG-2 at 160. */
export function mp3Bitrate(sampleRate: number, channels: number) {
  if (sampleRate <= 12000) return 32
  if (sampleRate <= 24000) return channels > 1 ? 96 : 64
  return channels > 1 ? 160 : 128
}

/** Downmix anything wider than stereo to mono; MP3 holds one or two channels. */
function toMp3Channels(channels: Int16Array[]): Int16Array[] {
  if (channels.length <= 2) return channels
  const frames = channels[0]!.length
  const mono = new Int16Array(frames)
  for (let i = 0; i < frames; i++) {
    let sum = 0
    for (const ch of channels) sum += ch[i]!
    mono[i] = Math.round(sum / channels.length)
  }
  return [mono]
}

/** LAME's frame length: encode whole frames per call. */
const FRAME = 1152
/** Frames encoded between pauses that let the UI paint (about a second of work at most). */
const FRAMES_PER_SLICE = 256

/**
 * Encode PCM audio as MP3. Work is cut into slices with a pause between them, so a long clip
 * never freezes the window. `onProgress` gets 0..1.
 */
export async function encodeMp3(audio: PcmAudio, onProgress?: (progress: number) => void): Promise<Uint8Array> {
  if (!MP3_RATES.includes(audio.sampleRate)) {
    throw new Error(`MP3 cannot hold ${audio.sampleRate} Hz audio. Save it as WAV or FLAC instead.`)
  }
  const channels = toMp3Channels(audio.channels)
  const { Mp3Encoder } = await import('@breezystack/lamejs')
  const encoder = new Mp3Encoder(channels.length, audio.sampleRate, mp3Bitrate(audio.sampleRate, channels.length))

  const frames = channels[0]?.length ?? 0
  const parts: Uint8Array[] = []
  const step = FRAME * FRAMES_PER_SLICE
  for (let start = 0; start < frames; start += step) {
    const end = Math.min(frames, start + step)
    for (let i = start; i < end; i += FRAME) {
      const stop = Math.min(end, i + FRAME)
      const left = channels[0]!.subarray(i, stop)
      const right = channels[1]?.subarray(i, stop)
      const chunk = right ? encoder.encodeBuffer(left, right) : encoder.encodeBuffer(left)
      if (chunk.length) parts.push(new Uint8Array(chunk))
    }
    onProgress?.(end / frames)
    if (end < frames) await new Promise((resolve) => setTimeout(resolve, 0))
  }
  const tail = encoder.flush()
  if (tail.length) parts.push(new Uint8Array(tail))

  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0))
  let pos = 0
  for (const p of parts) {
    out.set(p, pos)
    pos += p.length
  }
  if (out.length === 0) throw new Error('There is no audio to encode')
  return out
}
