import { describe, expect, it, vi } from 'vitest'
import { encodeMp3, mp3Bitrate, parseWav } from '../audio'
import { makeWav } from '@/test/wav'

const sine = (n: number, freq = 440, rate = 24_000) => Array.from({ length: n }, (_, i) => 0.5 * Math.sin((2 * Math.PI * freq * i) / rate))

describe('parseWav', () => {
  it('reads 16-bit mono exactly', () => {
    const wav = parseWav(makeWav([[0, 0.5, -0.5, 1, -1]]))
    expect(wav.sampleRate).toBe(24_000)
    // makeWav writes Math.round(s * 32767), and JS rounds -16383.5 up.
    expect(Array.from(wav.channels[0]!)).toEqual([0, 16384, -16383, 32767, -32767])
  })

  it('reads 8, 24 and 32-bit integer and float audio as 16-bit', () => {
    const samples = [[0, 0.5, -0.5]]
    for (const [bits, float] of [[8, false], [24, false], [32, false], [32, true]] as const) {
      const got = Array.from(parseWav(makeWav(samples, { bits, float })).channels[0]!)
      got.forEach((s, i) => expect(s / 32768).toBeCloseTo(samples[0]![i]!, bits === 8 ? 1 : 3))
    }
  })

  it('deinterleaves stereo and reads WAVE_FORMAT_EXTENSIBLE', () => {
    const wav = parseWav(makeWav([[0.25, 0.5], [-0.25, -0.5]], { extensible: true, rate: 44_100 }))
    expect(wav.sampleRate).toBe(44_100)
    expect(wav.channels).toHaveLength(2)
    expect(Array.from(wav.channels[1]!)).toEqual([-8192, -16383])
  })

  it('tolerates a data chunk that claims more bytes than the file has', () => {
    const buf = makeWav([[0.1, 0.2, 0.3, 0.4]])
    new DataView(buf).setUint32(40, 0xffff_fff0, true)
    expect(parseWav(buf).channels[0]).toHaveLength(4)
  })

  it('rejects what it cannot convert', () => {
    expect(() => parseWav(new ArrayBuffer(4))).toThrow('not a WAV')
    expect(() => parseWav(new TextEncoder().encode('RIFF\0\0\0\0AVI LIST').buffer)).toThrow('not a WAV')
    const adpcm = makeWav([[0, 0]])
    new DataView(adpcm).setUint16(20, 2, true)
    expect(() => parseWav(adpcm)).toThrow('cannot be converted')
  })
})

describe('encodeMp3', () => {
  it('encodes speech-rate mono into MPEG frames, much smaller than the WAV', async () => {
    const pcm = parseWav(makeWav([sine(24_000 * 3)]))
    const progress = vi.fn()
    const mp3 = await encodeMp3(pcm, progress)
    // Frame sync: 11 set bits.
    expect(mp3[0]).toBe(0xff)
    expect(mp3[1]! & 0xe0).toBe(0xe0)
    // 3 s at 64 kbps is about 24 KB; the WAV is 144 KB.
    expect(mp3.length).toBeGreaterThan(15_000)
    expect(mp3.length).toBeLessThan(40_000)
    expect(progress).toHaveBeenLastCalledWith(1)
  })

  it('encodes stereo and downmixes wider layouts', async () => {
    const stereo = await encodeMp3(parseWav(makeWav([sine(4_410, 440, 44_100), sine(4_410, 660, 44_100)], { rate: 44_100 })))
    expect(stereo[0]).toBe(0xff)
    const surround = parseWav(makeWav([sine(2_000), sine(2_000), sine(2_000)], { rate: 48_000 }))
    expect((await encodeMp3(surround))[0]).toBe(0xff)
  })

  it('refuses sample rates MP3 cannot hold', async () => {
    await expect(encodeMp3(parseWav(makeWav([sine(100)], { rate: 37_000 })))).rejects.toThrow('37000 Hz')
  })

  it('picks a speech bitrate per MPEG version', () => {
    expect(mp3Bitrate(8_000, 1)).toBe(32)
    expect(mp3Bitrate(16_000, 1)).toBe(64)
    expect(mp3Bitrate(24_000, 2)).toBe(96)
    expect(mp3Bitrate(44_100, 1)).toBe(128)
    expect(mp3Bitrate(48_000, 2)).toBe(160)
  })
})
