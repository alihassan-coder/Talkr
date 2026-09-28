use hound::{WavSpec, WavWriter};
use std::fs::File;
use std::path::Path;
use crate::error::{AppError, Result};

pub fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = WavWriter::create(path, spec)?;

    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let int_sample = (clamped * 32767.0) as i16;
        writer.write_sample(int_sample)?;
    }

    writer.finalize()?;
    Ok(())
}

pub fn write_wav_from_buffer(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    write_wav(path, samples, sample_rate)
}