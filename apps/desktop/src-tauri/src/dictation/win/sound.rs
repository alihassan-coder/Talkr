//! Short, quiet cues for start, stop and problems: synthesized once into WAV images in memory
//! and played asynchronously, so they cost nothing on the dictation path.

use std::sync::OnceLock;
use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};

#[derive(Debug, Clone, Copy)]
pub enum Cue {
    Start,
    Stop,
    Problem,
}

const RATE: u32 = 44_100;

/// A soft sine "blip" through the given frequencies, with a smooth envelope.
fn tone(notes: &[(f32, f32)], volume: f32) -> Vec<u8> {
    let mut samples: Vec<i16> = Vec::new();
    for &(freq, seconds) in notes {
        let n = (seconds * RATE as f32) as usize;
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            // Quick attack, gentle release: no clicks.
            let attack = (i as f32 / (0.006 * RATE as f32)).min(1.0);
            let release = ((n - i) as f32 / (0.04 * RATE as f32)).min(1.0);
            let env = attack * release;
            let s = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.8 + (4.0 * std::f32::consts::PI * freq * t).sin() * 0.2;
            samples.push((s * env * volume * i16::MAX as f32) as i16);
        }
    }
    wav(&samples)
}

fn wav(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

fn image(cue: Cue) -> &'static [u8] {
    static START: OnceLock<Vec<u8>> = OnceLock::new();
    static STOP: OnceLock<Vec<u8>> = OnceLock::new();
    static PROBLEM: OnceLock<Vec<u8>> = OnceLock::new();
    match cue {
        Cue::Start => START.get_or_init(|| tone(&[(880.0, 0.045), (1318.5, 0.07)], 0.16)),
        Cue::Stop => STOP.get_or_init(|| tone(&[(1318.5, 0.045), (987.8, 0.07)], 0.13)),
        Cue::Problem => PROBLEM.get_or_init(|| tone(&[(392.0, 0.09), (311.1, 0.12)], 0.15)),
    }
}

pub fn play(cue: Cue) {
    let data = image(cue);
    // SAFETY: SND_MEMORY reads the WAV image from `data`, which is 'static, so it outlives the
    // asynchronous playback.
    unsafe {
        let _ = PlaySoundW(PCWSTR(data.as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_are_valid_wav() {
        for cue in [Cue::Start, Cue::Stop, Cue::Problem] {
            let bytes = image(cue);
            let reader = hound::WavReader::new(std::io::Cursor::new(bytes)).unwrap();
            assert_eq!(reader.spec().sample_rate, RATE);
            assert!(reader.duration() > RATE / 20);
        }
    }
}
