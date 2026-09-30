pub mod decode;
pub mod resample;
pub mod wav;

use std::path::Path;
use crate::{JobControl, Result};
use decode::{Block, MAX_DURATION_SECS};
use resample::{StreamResampler, TARGET_RATE};

/// Limits and controls for [`load_16k_mono_with`].
#[derive(Clone, Copy)]
pub struct LoadOptions<'a> {
    /// Longer audio fails with `EngineError::Invalid`.
    pub max_seconds: u64,
    /// Checked between packets for cancellation.
    pub ctl: Option<&'a JobControl>,
}

impl Default for LoadOptions<'_> {
    fn default() -> Self {
        Self { max_seconds: MAX_DURATION_SECS, ctl: None }
    }
}

/// Decode any supported audio file to the 16 kHz mono samples whisper.cpp expects, with the
/// default 6-hour limit.
pub fn load_16k_mono(path: &Path) -> Result<Vec<f32>> {
    load_16k_mono_with(path, LoadOptions::default())
}

/// Decode `path` packet by packet, downmix to mono and resample to 16 kHz as it goes, so peak
/// memory is the 16 kHz output (64 KB per second of audio) plus one packet, whatever the source
/// rate, channel count or length.
pub fn load_16k_mono_with(path: &Path, opts: LoadOptions<'_>) -> Result<Vec<f32>> {
    let mut out: Vec<f32> = Vec::new();
    let mut resampler: Option<StreamResampler> = None;
    decode::decode_mono(path, opts.max_seconds, opts.ctl, |block: Block<'_>| {
        if resampler.as_ref().map(StreamResampler::from_rate) != Some(block.sample_rate) {
            // First block, or the stream changed rate (a chained stream): finish the old part.
            if let Some(old) = resampler.take() {
                old.finish(&mut out)?;
            }
            let next = StreamResampler::new(block.sample_rate)?;
            if out.is_empty() {
                // Size the output once when the container gives the length, instead of letting
                // the vector double past it.
                if let Some(frames) = block.total_frames {
                    let limit = opts.max_seconds.saturating_mul(TARGET_RATE as u64);
                    out.reserve_exact(next.output_len_for(frames).min(limit) as usize);
                }
            }
            resampler = Some(next);
        }
        resampler.as_mut().expect("set above").push(block.samples, &mut out)
    })?;
    if let Some(r) = resampler {
        r.finish(&mut out)?;
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod test_util {
    use std::f32::consts::TAU;
    use std::path::Path;

    pub fn sine(freq: f32, rate: u32, seconds: f32) -> Vec<f32> {
        (0..(rate as f32 * seconds) as usize).map(|i| 0.5 * (TAU * freq * i as f32 / rate as f32).sin()).collect()
    }

    /// Frequency estimated from rising zero crossings, ignoring the first and last tenth.
    pub fn frequency(samples: &[f32], rate: u32) -> f32 {
        let edge = samples.len() / 10;
        let body = &samples[edge..samples.len() - edge];
        let crossings = body.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        crossings as f32 * rate as f32 / body.len() as f32
    }

    /// Write a 16-bit WAV with hound. `sample(frame, channel, sine)` gives each sample, where
    /// `sine` is a 440 Hz sine at half scale.
    pub fn write_wav_file(
        path: &Path,
        rate: u32,
        channels: u16,
        seconds: f32,
        sample: impl Fn(usize, u16, f32) -> f32,
    ) {
        let spec = hound::WavSpec { channels, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for (i, s) in sine(440.0, rate, seconds).into_iter().enumerate() {
            for ch in 0..channels {
                w.write_sample((sample(i, ch, s) * 32767.0).round() as i16).unwrap();
            }
        }
        w.finalize().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::{frequency, write_wav_file};
    use super::*;
    use crate::EngineError;

    #[test]
    fn stereo_files_at_common_rates_load_as_16k_mono() {
        let dir = tempfile::tempdir().unwrap();
        for rate in [44_100u32, 48_000] {
            let path = dir.path().join(format!("stereo-{rate}.wav"));
            // Left carries the sine, right is silent: the mix is the sine at half level.
            write_wav_file(&path, rate, 2, 3.0, |_, ch, s| if ch == 0 { s } else { 0.0 });
            let out = load_16k_mono(&path).unwrap();
            assert_eq!(out.len(), 48_000, "{rate} Hz: 3 s at 16 kHz");
            let f = frequency(&out, 16_000);
            assert!((f - 440.0).abs() < 3.0, "{rate} Hz came out at {f} Hz");
            let body = &out[1600..out.len() - 1600];
            let peak = body.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!((peak - 0.25).abs() < 0.02, "{rate} Hz: downmixed peak {peak}");
        }
    }

    /// Regression guard: odd-length mono files at common speech rates load quickly and at the
    /// exact expected length (a Windows SAPI 22.05 kHz WAV was suspected of hanging the loader).
    #[test]
    fn odd_length_mono_files_load_quickly_at_every_common_rate() {
        let dir = tempfile::tempdir().unwrap();
        for rate in [8_000u32, 11_025, 16_000, 22_050, 32_000, 44_100, 48_000] {
            let path = dir.path().join(format!("odd-{rate}.wav"));
            write_wav_file(&path, rate, 1, 7.3, |_, _, s| s);
            let frames = hound::WavReader::open(&path).unwrap().duration() as u64;
            let started = std::time::Instant::now();
            let out = load_16k_mono(&path).unwrap();
            let took = started.elapsed();
            assert!(took < std::time::Duration::from_secs(2), "{rate} Hz took {took:?}");
            assert_eq!(out.len() as u64, (frames * 16_000).div_ceil(rate as u64), "{rate} Hz");
            let f = frequency(&out, 16_000);
            assert!((f - 440.0).abs() < 3.0, "{rate} Hz came out at {f} Hz");
        }
    }

    #[test]
    fn identical_channels_downmix_to_the_same_signal() {
        let dir = tempfile::tempdir().unwrap();
        let stereo = dir.path().join("stereo.wav");
        let mono = dir.path().join("mono.wav");
        write_wav_file(&stereo, 48_000, 2, 1.0, |_, _, s| s);
        write_wav_file(&mono, 48_000, 1, 1.0, |_, _, s| s);
        let a = load_16k_mono(&stereo).unwrap();
        let b = load_16k_mono(&mono).unwrap();
        assert_eq!(a.len(), b.len());
        assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-4));
    }

    #[test]
    fn sixteen_khz_mono_is_passed_through() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m.wav");
        write_wav_file(&path, 16_000, 1, 0.5, |_, _, s| s);
        let out = load_16k_mono(&path).unwrap();
        let expected: Vec<f32> = hound::WavReader::open(&path)
            .unwrap()
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect();
        assert_eq!(out.len(), expected.len());
        assert!(out.iter().zip(&expected).all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn the_length_limit_applies() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("l.wav");
        write_wav_file(&path, 48_000, 1, 2.5, |_, _, s| s);
        let err = load_16k_mono_with(&path, LoadOptions { max_seconds: 2, ctl: None }).unwrap_err();
        assert!(matches!(&err, EngineError::Invalid(m) if m.contains("too long")), "{err:?}");
    }

    #[test]
    fn bad_files_are_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("empty.wav");
        std::fs::write(&empty, b"").unwrap();
        let garbage = dir.path().join("garbage.flac");
        std::fs::write(&garbage, vec![0xAB; 10_000]).unwrap();
        let zero = dir.path().join("zero.wav");
        write_wav_file(&zero, 44_100, 2, 0.0, |_, _, s| s);
        for path in [&empty, &garbage, &zero, &dir.path().join("missing.wav")] {
            let err = load_16k_mono(path).unwrap_err();
            assert!(matches!(err, EngineError::Invalid(_)), "{}: {err:?}", path.display());
        }
    }
}
