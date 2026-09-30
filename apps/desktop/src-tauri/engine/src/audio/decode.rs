//! Packet-by-packet decoding of any audio file symphonia supports, downmixed to mono.
//!
//! Nothing here holds more than one decoded packet: each is handed to a callback as soon as it
//! is decoded, so a long file costs no more memory than a short one.

use std::fs::File;
use std::io::ErrorKind;
use std::path::Path;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use crate::{EngineError, JobControl, Result};

/// The longest audio accepted by default: 6 hours.
pub const MAX_DURATION_SECS: u64 = 6 * 60 * 60;

/// After this many undecodable packets in a row the file is treated as broken, not merely damaged.
const MAX_CONSECUTIVE_ERRORS: u32 = 100;

/// A guard against a container that keeps asking for resets.
const MAX_RESETS: u32 = 64;

/// One decoded packet, downmixed to mono.
pub struct Block<'a> {
    pub samples: &'a [f32],
    pub sample_rate: u32,
    /// The whole stream's length in frames, when the container says.
    pub total_frames: Option<u64>,
}

/// What a decode saw.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DecodeStats {
    /// Mono frames handed to the sink.
    pub frames: u64,
    pub seconds: f64,
    /// Packets that failed to decode and were skipped.
    pub skipped_packets: u64,
}

/// Decode `path` packet by packet, handing each packet's mono samples to `sink`.
///
/// Fails with [`EngineError::Invalid`] when the file is missing, empty, not audio, has no sample
/// rate, decodes to nothing, or is longer than `max_seconds`. Isolated corrupt packets are
/// skipped. With `ctl`, cancellation is checked between packets.
pub fn decode_mono(
    path: &Path,
    max_seconds: u64,
    ctl: Option<&JobControl>,
    mut sink: impl FnMut(Block<'_>) -> Result<()>,
) -> Result<DecodeStats> {
    let file = File::open(path).map_err(|e| match e.kind() {
        ErrorKind::NotFound => EngineError::Invalid(format!("Audio file not found: {}", path.display())),
        _ => EngineError::Invalid(format!("Could not open the audio file {}: {}", path.display(), e)),
    })?;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    if len == 0 {
        return Err(EngineError::Invalid(format!("The audio file is empty: {}", path.display())));
    }
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| match e {
            SymphoniaError::Unsupported(_) | SymphoniaError::DecodeError(_) => EngineError::Invalid(format!(
                "Not a supported audio file (or it is damaged): {}",
                path.display()
            )),
            SymphoniaError::IoError(e) if e.kind() == ErrorKind::UnexpectedEof => {
                EngineError::Invalid(format!("The audio file is truncated or not audio: {}", path.display()))
            }
            other => read_error(other),
        })?;

    let mut stream = open_track(format.as_ref())?;
    if let Some(frames) = stream.total_frames {
        check_duration(frames as f64 / stream.sample_rate as f64, max_seconds)?;
    }

    let mut stats = DecodeStats::default();
    let mut mono: Vec<f32> = Vec::new();
    let mut interleaved: Vec<f32> = Vec::new();
    let mut consecutive_errors = 0u32;
    let mut resets = 0u32;
    // Duration so far, kept exact: whole seconds of earlier rates plus frames at the current one.
    let (mut earlier_seconds, mut rate_frames, mut current_rate) = (0f64, 0u64, 0u32);

    loop {
        if let Some(ctl) = ctl {
            ctl.check_cancelled()?;
        }
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            // Some readers report the end of a file (or a truncated last packet) this way.
            Err(SymphoniaError::IoError(e)) if e.kind() == ErrorKind::UnexpectedEof => break,
            Err(SymphoniaError::ResetRequired) => {
                // The track list changed (e.g. chained Ogg streams): pick the track again and
                // start a fresh decoder. The sample rate may change; the sink sees it per block.
                resets += 1;
                if resets > MAX_RESETS {
                    return Err(EngineError::Invalid("The audio file keeps changing its track layout".into()));
                }
                stream = open_track(format.as_ref())?;
                continue;
            }
            Err(e) => return Err(read_error(e)),
        };
        if packet.track_id != stream.track_id {
            continue;
        }

        let decoded = match stream.decoder.decode(&packet) {
            Ok(decoded) => decoded,
            // A corrupt packet: drop it and carry on, unless nothing around it decodes either.
            Err(SymphoniaError::DecodeError(_)) | Err(SymphoniaError::IoError(_)) => {
                stats.skipped_packets += 1;
                consecutive_errors += 1;
                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    return Err(EngineError::Invalid(format!(
                        "The audio file is too damaged to decode: {}",
                        path.display()
                    )));
                }
                continue;
            }
            Err(SymphoniaError::ResetRequired) => {
                resets += 1;
                if resets > MAX_RESETS {
                    return Err(EngineError::Invalid("The audio decoder keeps asking to be reset".into()));
                }
                stream.decoder = make_decoder(stream.decoder.codec_params())?;
                continue;
            }
            Err(e) => return Err(read_error(e)),
        };
        consecutive_errors = 0;
        if decoded.frames() == 0 {
            continue;
        }

        let sample_rate = decoded.spec().rate();
        if sample_rate == 0 {
            return Err(EngineError::Invalid("The audio has a sample rate of 0 Hz".into()));
        }
        let channels = decoded.spec().channels().count().max(1);
        decoded.copy_to_vec_interleaved(&mut interleaved);
        downmix(&interleaved, channels, &mut mono);

        stats.frames += mono.len() as u64;
        if sample_rate != current_rate {
            if current_rate != 0 {
                earlier_seconds += rate_frames as f64 / current_rate as f64;
            }
            (rate_frames, current_rate) = (0, sample_rate);
        }
        rate_frames += mono.len() as u64;
        stats.seconds = earlier_seconds + rate_frames as f64 / sample_rate as f64;
        check_duration(stats.seconds, max_seconds)?;
        sink(Block { samples: &mono, sample_rate, total_frames: stream.total_frames })?;
    }

    if stats.frames == 0 {
        return Err(EngineError::Invalid(format!("The audio file contains no decodable audio: {}", path.display())));
    }
    if stats.skipped_packets > 0 {
        log::warn!("skipped {} undecodable packet(s) in {}", stats.skipped_packets, path.display());
    }
    Ok(stats)
}

struct Stream {
    track_id: u32,
    sample_rate: u32,
    total_frames: Option<u64>,
    decoder: Box<dyn AudioDecoder>,
}

fn open_track(format: &dyn FormatReader) -> Result<Stream> {
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| EngineError::Invalid("The file has no audio track".into()))?;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| EngineError::Invalid("The file has no audio track".into()))?;
    let sample_rate = params
        .sample_rate
        .filter(|&r| r > 0)
        .ok_or_else(|| EngineError::Invalid("The audio file does not say what its sample rate is".into()))?;
    Ok(Stream { track_id: track.id, sample_rate, total_frames: track.num_frames, decoder: make_decoder(params)? })
}

fn make_decoder(params: &symphonia::core::codecs::audio::AudioCodecParameters) -> Result<Box<dyn AudioDecoder>> {
    symphonia::default::get_codecs().make_audio_decoder(params, &AudioDecoderOptions::default()).map_err(|e| match e {
        SymphoniaError::Unsupported(what) => {
            EngineError::Invalid(format!("The audio codec in this file is not supported ({})", what))
        }
        other => read_error(other),
    })
}

fn read_error(e: SymphoniaError) -> EngineError {
    match e {
        SymphoniaError::IoError(e) => EngineError::Invalid(format!("Could not read the audio file: {}", e)),
        SymphoniaError::DecodeError(what) => EngineError::Invalid(format!("The audio file is damaged: {}", what)),
        SymphoniaError::Unsupported(what) => {
            EngineError::Invalid(format!("The audio file uses an unsupported feature: {}", what))
        }
        SymphoniaError::LimitError(what) => EngineError::Invalid(format!("The audio file is too large to read: {}", what)),
        other => EngineError::Invalid(format!("Could not read the audio file: {}", other)),
    }
}

fn check_duration(seconds: f64, max_seconds: u64) -> Result<()> {
    if seconds > max_seconds as f64 {
        return Err(EngineError::Invalid(format!(
            "The audio is too long ({}); the limit is {}. Split it into shorter files.",
            format_duration(seconds),
            format_duration(max_seconds as f64)
        )));
    }
    Ok(())
}

fn format_duration(seconds: f64) -> String {
    let total = seconds.round() as u64;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    match (h, m) {
        (0, 0) => format!("{} s", s),
        (0, _) => format!("{} min {} s", m, s),
        (_, 0) => format!("{} h", h),
        _ => format!("{} h {} min", h, m),
    }
}

/// Average interleaved frames into `mono` (replacing its contents). Non-finite samples, which a
/// damaged float file can hold, become silence.
fn downmix(interleaved: &[f32], channels: usize, mono: &mut Vec<f32>) {
    mono.clear();
    let finite = |s: f32| if s.is_finite() { s } else { 0.0 };
    if channels == 1 {
        mono.extend(interleaved.iter().copied().map(finite));
    } else {
        let scale = 1.0 / channels as f32;
        mono.extend(interleaved.chunks_exact(channels).map(|f| f.iter().copied().map(finite).sum::<f32>() * scale));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::test_util::{frequency, write_wav_file};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn collect(path: &Path, max_seconds: u64) -> Result<(Vec<f32>, u32, DecodeStats)> {
        let mut all = Vec::new();
        let mut rate = 0;
        let stats = decode_mono(path, max_seconds, None, |b| {
            rate = b.sample_rate;
            all.extend_from_slice(b.samples);
            Ok(())
        })?;
        Ok((all, rate, stats))
    }

    fn invalid_message(r: Result<impl std::fmt::Debug>) -> String {
        match r {
            Err(EngineError::Invalid(m)) => m,
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn downmix_averages_channels() {
        let mut mono = Vec::new();
        downmix(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0], 2, &mut mono);
        assert_eq!(mono, [0.5, 0.5, 0.0]);
        downmix(&[0.3, 0.6, 0.9], 3, &mut mono);
        assert!((mono[0] - 0.6).abs() < 1e-6);
        downmix(&[f32::NAN, 0.2, f32::INFINITY], 1, &mut mono);
        assert_eq!(mono, [0.0, 0.2, 0.0]);
    }

    #[test]
    fn decodes_stereo_wav_to_mono_at_the_native_rate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.wav");
        // Left: a 440 Hz sine; right: its negation. The mono mix cancels to silence.
        write_wav_file(&path, 48_000, 2, 1.0, |_, ch, s| if ch == 0 { s } else { -s });
        let (mono, rate, stats) = collect(&path, MAX_DURATION_SECS).unwrap();
        assert_eq!(rate, 48_000);
        assert_eq!(mono.len(), 48_000);
        assert_eq!(stats.frames, 48_000);
        assert_eq!(stats.skipped_packets, 0);
        assert!(mono.iter().all(|s| s.abs() < 1e-3), "opposite channels cancel");

        // Left only: the mix is the sine at half level, same pitch.
        write_wav_file(&path, 44_100, 2, 1.0, |_, ch, s| if ch == 0 { s } else { 0.0 });
        let (mono, rate, _) = collect(&path, MAX_DURATION_SECS).unwrap();
        assert_eq!(rate, 44_100);
        let peak = mono.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.25).abs() < 0.01, "peak {peak}");
        assert!((frequency(&mono, 44_100) - 440.0).abs() < 3.0);
    }

    #[test]
    fn a_missing_file_is_invalid() {
        let m = invalid_message(collect(Path::new("definitely/not/here.wav"), 10));
        assert!(m.contains("not found"), "{m}");
    }

    #[test]
    fn an_empty_file_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.mp3");
        std::fs::write(&path, b"").unwrap();
        let m = invalid_message(collect(&path, 10));
        assert!(m.contains("empty"), "{m}");
    }

    #[test]
    fn a_garbage_file_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        for (name, bytes) in [
            ("noise.wav", (0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect::<Vec<_>>()),
            ("text.mp3", b"this is not audio at all, just some text".to_vec()),
            ("short.ogg", vec![0x4f]),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            invalid_message(collect(&path, 10));
        }
    }

    #[test]
    fn a_zero_length_wav_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zero.wav");
        write_wav_file(&path, 16_000, 1, 0.0, |_, _, s| s);
        let m = invalid_message(collect(&path, 10));
        assert!(m.contains("no decodable audio"), "{m}");
    }

    #[test]
    fn a_truncated_wav_keeps_what_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cut.wav");
        write_wav_file(&path, 16_000, 1, 1.0, |_, _, s| s);
        let bytes = std::fs::read(&path).unwrap();
        // Cut the file mid-way (and mid-sample): the header still claims a full second.
        std::fs::write(&path, &bytes[..bytes.len() / 2 + 1]).unwrap();
        let (mono, _, _) = collect(&path, 10).unwrap();
        assert!((7_000..=8_100).contains(&mono.len()), "{} samples", mono.len());
    }

    #[test]
    fn audio_over_the_length_limit_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("long.wav");
        write_wav_file(&path, 8_000, 1, 3.0, |_, _, s| s);
        let m = invalid_message(collect(&path, 2));
        assert!(m.contains("too long") && m.contains("3 s") && m.contains("2 s"), "{m}");
        let (mono, _, stats) = collect(&path, 3).unwrap();
        assert_eq!(mono.len(), 24_000);
        assert_eq!(stats.seconds, 3.0);
    }

    #[test]
    fn cancellation_is_checked_between_packets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.wav");
        write_wav_file(&path, 16_000, 1, 1.0, |_, _, s| s);
        let cancel = Arc::new(AtomicBool::new(true));
        let ctl = JobControl::new(Arc::new(|_| {}), cancel);
        let r = decode_mono(&path, 10, Some(&ctl), |_| Ok(()));
        assert!(matches!(r, Err(EngineError::Cancelled)), "{:?}", r.err());
    }

    #[test]
    fn sink_errors_stop_the_decode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.wav");
        write_wav_file(&path, 16_000, 1, 1.0, |_, _, s| s);
        let mut calls = 0;
        let r = decode_mono(&path, 10, None, |_| {
            calls += 1;
            Err(EngineError::Engine("stop".into()))
        });
        assert!(matches!(r, Err(EngineError::Engine(_))));
        assert_eq!(calls, 1);
    }

    #[test]
    fn durations_read_well() {
        assert_eq!(format_duration(5.4), "5 s");
        assert_eq!(format_duration(125.0), "2 min 5 s");
        assert_eq!(format_duration(21_600.0), "6 h");
        assert_eq!(format_duration(25_260.0), "7 h 1 min");
    }
}
