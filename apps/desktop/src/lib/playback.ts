/**
 * `audio.play()`, minus the rejection a quick pause (or a new source) causes: that AbortError
 * only means "interrupted", not that the audio is broken. Real failures still reject, and the
 * element's `error` event reports files that cannot be decoded.
 */
export async function play(audio: HTMLAudioElement): Promise<void> {
  try {
    await audio.play()
  } catch (err) {
    if (err instanceof Error && err.name === 'AbortError') return
    if (typeof DOMException !== 'undefined' && err instanceof DOMException && err.name === 'AbortError') return
    throw err
  }
}

/** Playback speeds the players cycle through. */
export const SPEEDS = [1, 1.25, 1.5, 2, 0.75] as const

export const nextSpeed = (speed: number) => SPEEDS[(SPEEDS.indexOf(speed as (typeof SPEEDS)[number]) + 1) % SPEEDS.length]!

export const speedLabel = (speed: number) => `${speed}×`
