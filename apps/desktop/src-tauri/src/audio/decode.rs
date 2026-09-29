use symphonia::core::audio::SampleBuffer;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use std::fs::File;
use std::path::Path;
use crate::error::{AppError, Result};

pub fn decode_audio_file(path: &Path) -> Result<(Vec<f32>, u32)> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())?;

    let mut format = probed.format;
    let track = format.default_track()
        .ok_or_else(|| AppError::Audio("No audio track found".into()))?;

    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    let sample_rate = codec_params.sample_rate.unwrap_or(16000);

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &Default::default())?;

    let mut samples = Vec::new();

    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let channels = spec.channels.count().max(1);
                let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                buf.copy_interleaved_ref(decoded);
                // Downmix interleaved frames to mono.
                samples.extend(
                    buf.samples()
                        .chunks(channels)
                        .map(|frame| frame.iter().sum::<f32>() / channels as f32),
                );
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        }
    }

    if samples.is_empty() {
        return Err(AppError::Audio("Audio file contains no decodable samples".into()));
    }

    Ok((samples, sample_rate))
}