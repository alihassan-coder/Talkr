use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::CODEC_TYPE_NULL;
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

    let hint = Hint::new();
    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())?;

    let mut format = probed.format;
    let track = format.default_track()
        .ok_or_else(|| AppError::Audio("No audio track found".into()))?;

    let codec_params = track.codec_params.clone();
    let sample_rate = codec_params.sample_rate.unwrap_or(16000);

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &Default::default())?;

    let mut samples = Vec::new();

    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track.id() {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let duration = decoded.frames() as usize;
                let mut buf = SampleBuffer::<f32>::new(duration, spec);
                buf.copy_interleaved_ref(decoded);
                samples.extend_from_slice(buf.samples());
            }
            Err(_) => continue,
        }
    }

    Ok((samples, sample_rate))
}