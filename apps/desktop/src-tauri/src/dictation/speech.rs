//! A small energy-based voice detector for dictation clips. Whisper invents text for silence
//! ("Thanks for watching!"), so a clip with no speech is never sent to it, and the silence around
//! speech is cut off, which also makes transcription faster.

use std::path::Path;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use crate::error::Result;

const RATE: usize = 16_000;
/// 20 ms frames.
const FRAME: usize = RATE / 50;
/// Speech quieter than this (about -52 dBFS) is treated as silence whatever the noise floor.
const MIN_SPEECH_RMS: f32 = 0.0025;
/// Speech must be this much louder than the quiet parts of the clip.
const NOISE_FACTOR: f32 = 2.8;
/// Less speech than this is a click, a breath or a keyboard, not words.
const MIN_SPEECH_MS: u64 = 180;
/// Silence kept around the speech: cutting right at the first loud frame clips soft consonants.
const PAD_MS: usize = 250;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Analysis {
    /// Milliseconds of frames that look like speech.
    pub speech_ms: u64,
    /// The span worth transcribing, in samples (start, end), with padding.
    pub span: Option<(usize, usize)>,
    /// Loudest 20 ms of the clip and the room's noise floor (RMS), for diagnosing a microphone
    /// that is too quiet.
    pub peak: f32,
    pub floor: f32,
}

impl Analysis {
    pub fn has_speech(&self) -> bool {
        self.speech_ms >= MIN_SPEECH_MS && self.span.is_some()
    }
}

fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt()
}

/// Find the speech in 16 kHz mono samples.
pub fn analyze(samples: &[f32]) -> Analysis {
    let levels: Vec<f32> = samples.chunks(FRAME).map(rms).collect();
    if levels.is_empty() {
        return Analysis { speech_ms: 0, span: None, peak: 0.0, floor: 0.0 };
    }
    // The noise floor: the 15th percentile frame. Robust to a clip that is mostly speech.
    let mut sorted = levels.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let floor = sorted[sorted.len() * 15 / 100];
    let threshold = (floor * NOISE_FACTOR).max(MIN_SPEECH_RMS);

    let speech: Vec<bool> = levels.iter().map(|&l| l > threshold).collect();
    let speech_frames = speech.iter().filter(|s| **s).count();
    let first = speech.iter().position(|s| *s);
    let last = speech.iter().rposition(|s| *s);
    let span = first.zip(last).map(|(first, last)| {
        let pad = PAD_MS * RATE / 1000;
        let start = (first * FRAME).saturating_sub(pad);
        let end = ((last + 1) * FRAME + pad).min(samples.len());
        (start, end)
    });
    let peak = sorted.last().copied().unwrap_or(0.0);
    Analysis { speech_ms: (speech_frames * FRAME * 1000 / RATE) as u64, span, peak, floor }
}

/// Read a 16 kHz mono 16-bit WAV (what the recorder writes).
pub fn read_wav(path: &Path) -> Result<Vec<f32>> {
    let mut reader = WavReader::open(path)?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        SampleFormat::Int => reader.samples::<i16>().filter_map(|s| s.ok()).map(|s| s as f32 / 32768.0).collect(),
        SampleFormat::Float => reader.samples::<f32>().filter_map(|s| s.ok()).collect(),
    };
    Ok(samples)
}

/// Rewrite `path` with only `samples[start..end]`.
pub fn write_span(path: &Path, samples: &[f32], (start, end): (usize, usize)) -> Result<()> {
    let spec = WavSpec { channels: 1, sample_rate: RATE as u32, bits_per_sample: 16, sample_format: SampleFormat::Int };
    let mut writer = WavWriter::create(path, spec)?;
    for &s in &samples[start.min(samples.len())..end.min(samples.len())] {
        writer.write_sample((s.clamp(-1.0, 1.0) * 32767.0).round() as i16)?;
    }
    writer.finalize()?;
    Ok(())
}

/// Analyze a recorded clip and cut it down to its speech. Returns the analysis; the file is only
/// rewritten when there is speech and trimming saves something.
pub fn prepare_clip(path: &Path) -> Result<Analysis> {
    let samples = read_wav(path)?;
    let analysis = analyze(&samples);
    if let Some(span) = analysis.span.filter(|_| analysis.has_speech()) {
        if span.1 - span.0 + FRAME < samples.len() {
            write_span(path, &samples, span)?;
        }
    }
    Ok(analysis)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(ms: usize, amplitude: f32) -> Vec<f32> {
        (0..ms * RATE / 1000).map(|i| amplitude * (i as f32 * 0.12).sin()).collect()
    }

    fn noise(ms: usize, amplitude: f32) -> Vec<f32> {
        // Deterministic pseudo-noise.
        let mut x = 12345u32;
        (0..ms * RATE / 1000)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                amplitude * ((x >> 16) as f32 / 32768.0 - 1.0)
            })
            .collect()
    }

    #[test]
    fn silence_and_noise_are_not_speech() {
        assert!(!analyze(&[]).has_speech());
        assert!(!analyze(&vec![0.0; RATE]).has_speech());
        assert!(!analyze(&noise(2_000, 0.002)).has_speech());
        // A click: one loud frame.
        let mut click = noise(1_000, 0.001);
        click.extend(tone(40, 0.5));
        click.extend(noise(1_000, 0.001));
        assert!(!analyze(&click).has_speech());
    }

    #[test]
    fn speech_is_found_and_padded() {
        let mut clip = noise(1_000, 0.001);
        clip.extend(tone(800, 0.2));
        clip.extend(noise(1_500, 0.001));
        let a = analyze(&clip);
        assert!(a.has_speech(), "{a:?}");
        assert!((780..=820).contains(&a.speech_ms), "{a:?}");
        let (start, end) = a.span.unwrap();
        let pad = PAD_MS * RATE / 1000;
        assert!(start <= RATE - pad + FRAME && start >= RATE - pad - FRAME, "{start}");
        assert!(end >= RATE + 800 * RATE / 1000 + pad - FRAME, "{end}");
        assert!(end < clip.len());
    }

    #[test]
    fn quiet_speech_over_a_quiet_room_counts() {
        let mut clip = noise(500, 0.0005);
        clip.extend(tone(600, 0.01));
        clip.extend(noise(500, 0.0005));
        assert!(analyze(&clip).has_speech());
    }

    #[test]
    fn clips_are_trimmed_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.wav");
        let mut clip = noise(2_000, 0.001);
        clip.extend(tone(1_000, 0.3));
        clip.extend(noise(2_000, 0.001));
        write_span(&path, &clip, (0, clip.len())).unwrap();
        let a = prepare_clip(&path).unwrap();
        assert!(a.has_speech());
        let trimmed = read_wav(&path).unwrap();
        assert!(trimmed.len() < clip.len() / 2, "{} of {}", trimmed.len(), clip.len());
        assert!(trimmed.len() >= RATE, "speech kept");
    }
}
