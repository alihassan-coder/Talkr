// Test helper: build WAV files in memory.

export type WavOptions = { rate?: number; channels?: number; bits?: 8 | 16 | 24 | 32; float?: boolean; extensible?: boolean }

/** Build a WAV file from per-channel samples in -1..1 (interleaved on write). */
export function makeWav(channels: number[][], opts: WavOptions = {}): ArrayBuffer {
  const { rate = 24_000, bits = 16, float = false, extensible = false } = opts
  const count = channels.length
  const frames = channels[0]?.length ?? 0
  const bytes = bits / 8
  const fmtSize = extensible ? 40 : 16
  const dataSize = frames * count * bytes
  const buf = new ArrayBuffer(12 + 8 + fmtSize + 8 + dataSize)
  const v = new DataView(buf)
  const str = (o: number, s: string) => [...s].forEach((c, i) => v.setUint8(o + i, c.charCodeAt(0)))
  str(0, 'RIFF')
  v.setUint32(4, buf.byteLength - 8, true)
  str(8, 'WAVE')
  str(12, 'fmt ')
  v.setUint32(16, fmtSize, true)
  const tag = float ? 3 : 1
  v.setUint16(20, extensible ? 0xfffe : tag, true)
  v.setUint16(22, count, true)
  v.setUint32(24, rate, true)
  v.setUint32(28, rate * count * bytes, true)
  v.setUint16(32, count * bytes, true)
  v.setUint16(34, bits, true)
  if (extensible) {
    v.setUint16(36, 22, true)
    v.setUint16(38, bits, true)
    v.setUint32(40, 0, true)
    v.setUint16(44, tag, true) // first two bytes of the sub-format GUID
  }
  const data = 20 + fmtSize
  str(data, 'data')
  v.setUint32(data + 4, dataSize, true)
  let pos = data + 8
  for (let i = 0; i < frames; i++) {
    for (let c = 0; c < count; c++) {
      const s = channels[c]![i]!
      if (float) v.setFloat32(pos, s, true)
      else if (bits === 8) v.setUint8(pos, Math.round(s * 127) + 128)
      else if (bits === 16) v.setInt16(pos, Math.round(s * 32767), true)
      else if (bits === 24) {
        const x = Math.round(s * 8_388_607)
        v.setUint8(pos, x & 0xff)
        v.setUint8(pos + 1, (x >> 8) & 0xff)
        v.setInt8(pos + 2, x >> 16)
      } else v.setInt32(pos, Math.round(s * 2_147_483_647), true)
      pos += bytes
    }
  }
  return buf
}
