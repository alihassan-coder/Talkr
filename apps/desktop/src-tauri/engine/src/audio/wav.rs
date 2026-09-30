use hound::{WavSpec, WavWriter};
use std::path::Path;
use crate::{EngineError, Result};

/// Write mono samples in [-1, 1] as a 16-bit PCM WAV. Out-of-range samples are clipped and
/// non-finite ones written as silence.
pub fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    if sample_rate == 0 {
        return Err(EngineError::Invalid("Cannot write a WAV file with a sample rate of 0 Hz".into()));
    }
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = WavWriter::create(path, spec)?;
    for &sample in samples {
        writer.write_sample(to_i16(sample))?;
    }
    writer.finalize()?;
    Ok(())
}

fn to_i16(sample: f32) -> i16 {
    if sample.is_finite() {
        (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::test_util::sine;

    fn read(path: &Path) -> (hound::WavSpec, Vec<i16>) {
        let mut r = hound::WavReader::open(path).unwrap();
        let spec = r.spec();
        (spec, r.samples::<i16>().map(|s| s.unwrap()).collect())
    }

    #[test]
    fn round_trips_through_hound() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.wav");
        let samples = sine(440.0, 22_050, 0.5);
        write_wav(&path, &samples, 22_050).unwrap();
        let (spec, back) = read(&path);
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, 22_050);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(spec.sample_format, hound::SampleFormat::Int);
        assert_eq!(back.len(), samples.len());
        for (a, b) in samples.iter().zip(&back) {
            assert!((a - *b as f32 / 32767.0).abs() <= 1.0 / 32767.0, "{a} vs {b}");
        }
    }

    #[test]
    fn round_trips_through_the_decoder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.wav");
        let samples = sine(300.0, 16_000, 1.0);
        write_wav(&path, &samples, 16_000).unwrap();
        let back = crate::audio::load_16k_mono(&path).unwrap();
        assert_eq!(back.len(), samples.len());
        assert!(samples.iter().zip(&back).all(|(a, b)| (a - b).abs() < 1e-4));
    }

    #[test]
    fn clips_and_sanitizes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.wav");
        write_wav(&path, &[2.0, -2.0, 1.0, -1.0, 0.0, f32::NAN, f32::INFINITY], 8_000).unwrap();
        assert_eq!(read(&path).1, [32767, -32767, 32767, -32767, 0, 0, 0]);
    }

    #[test]
    fn empty_audio_makes_a_valid_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.wav");
        write_wav(&path, &[], 16_000).unwrap();
        assert!(read(&path).1.is_empty());
    }

    #[test]
    fn bad_rates_and_paths_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(write_wav(&dir.path().join("a.wav"), &[0.0], 0), Err(EngineError::Invalid(_))));
        assert!(write_wav(&dir.path().join("no/such/dir/a.wav"), &[0.0], 16_000).is_err());
    }
}
