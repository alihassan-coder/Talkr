use hound::{WavSpec, WavWriter};
use std::path::Path;
use crate::error::{AppError, Result};

/// Write mono `samples` (-1.0..=1.0; louder values are clipped, NaN becomes silence) as a
/// 16-bit PCM WAV file.
pub fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    if sample_rate == 0 {
        return Err(AppError::Audio("Cannot write a WAV file with a sample rate of 0".into()));
    }
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = WavWriter::create(path, spec)?;
    {
        // The i16 writer skips per-sample format checks.
        let mut samples_out = writer.get_i16_writer(samples.len() as u32);
        for &sample in samples {
            samples_out.write_sample(to_i16(sample));
        }
        samples_out.flush()?;
    }
    writer.finalize()?;
    Ok(())
}

fn to_i16(sample: f32) -> i16 {
    if sample.is_nan() {
        return 0;
    }
    (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_readable_mono_16_bit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.wav");
        let samples = [0.0, 0.5, -0.5, 1.0, -1.0, 2.0, -3.0, f32::NAN, 0.25];
        write_wav(&path, &samples, 22_050).unwrap();

        let mut reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.sample_rate, 22_050);
        assert_eq!(spec.bits_per_sample, 16);
        assert_eq!(spec.sample_format, hound::SampleFormat::Int);
        let read: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        assert_eq!(read, [0, 16384, -16384, 32767, -32767, 32767, -32767, 0, 8192]);
        assert_eq!(reader.duration(), samples.len() as u32);
    }

    #[test]
    fn empty_input_gives_a_valid_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.wav");
        write_wav(&path, &[], 16_000).unwrap();
        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.duration(), 0);
    }

    #[test]
    fn long_input_round_trips_length() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("long.wav");
        let samples: Vec<f32> = (0..48_000).map(|i| (i as f32 / 10.0).sin() * 0.8).collect();
        write_wav(&path, &samples, 48_000).unwrap();
        let mut reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.duration(), 48_000);
        let first: Vec<i16> = reader.samples::<i16>().take(3).map(|s| s.unwrap()).collect();
        assert_eq!(first[0], 0);
        assert_eq!(first[1], to_i16((0.1f32).sin() * 0.8));
    }

    #[test]
    fn rejects_zero_sample_rate_and_bad_paths() {
        let dir = tempfile::tempdir().unwrap();
        assert!(write_wav(&dir.path().join("x.wav"), &[0.0], 0).is_err());
        assert!(write_wav(&dir.path().join("missing").join("x.wav"), &[0.0], 16_000).is_err());
    }
}
