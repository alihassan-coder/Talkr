//! Dictation's sounds: short, quiet two-note chimes for start, stop and problems. Synthesized once
//! (no sound files to ship), as samples and as WAV images for players that take a file.

use std::sync::OnceLock;
use super::backend::Cue;

pub const RATE: u32 = 44_100;

/// The notes of each cue: (frequency in Hz, seconds), and the volume.
fn score(cue: Cue) -> (&'static [(f32, f32)], f32) {
    match cue {
        Cue::Start => (&[(880.0, 0.045), (1318.5, 0.07)], 0.16),
        Cue::Stop => (&[(1318.5, 0.045), (987.8, 0.07)], 0.13),
        Cue::Problem => (&[(392.0, 0.09), (311.1, 0.12)], 0.15),
    }
}

/// A soft sine chime through the given notes, with a quick attack and a gentle release so it
/// never clicks.
fn synthesize(notes: &[(f32, f32)], volume: f32) -> Vec<f32> {
    let mut out = Vec::new();
    for &(freq, seconds) in notes {
        let n = (seconds * RATE as f32) as usize;
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            let attack = (i as f32 / (0.006 * RATE as f32)).min(1.0);
            let release = ((n - i) as f32 / (0.04 * RATE as f32)).min(1.0);
            let s = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.8 + (4.0 * std::f32::consts::PI * freq * t).sin() * 0.2;
            out.push(s * attack * release * volume);
        }
    }
    out
}

/// Mono samples at [`RATE`], -1..1.
pub fn samples(cue: Cue) -> &'static [f32] {
    static START: OnceLock<Vec<f32>> = OnceLock::new();
    static STOP: OnceLock<Vec<f32>> = OnceLock::new();
    static PROBLEM: OnceLock<Vec<f32>> = OnceLock::new();
    let slot = match cue {
        Cue::Start => &START,
        Cue::Stop => &STOP,
        Cue::Problem => &PROBLEM,
    };
    slot.get_or_init(|| {
        let (notes, volume) = score(cue);
        synthesize(notes, volume)
    })
}

/// A 16-bit mono WAV image of the cue (for players that take a file, like PlaySound).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn wav(cue: Cue) -> &'static [u8] {
    static START: OnceLock<Vec<u8>> = OnceLock::new();
    static STOP: OnceLock<Vec<u8>> = OnceLock::new();
    static PROBLEM: OnceLock<Vec<u8>> = OnceLock::new();
    let slot = match cue {
        Cue::Start => &START,
        Cue::Stop => &STOP,
        Cue::Problem => &PROBLEM,
    };
    slot.get_or_init(|| encode_wav(samples(cue)))
}

#[cfg_attr(not(windows), allow(dead_code))]
fn encode_wav(samples: &[f32]) -> Vec<u8> {
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
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

/// Play a cue on the default output device, on a short-lived thread. Portable (cpal: WASAPI,
/// Core Audio, ALSA/PulseAudio/PipeWire); failures are logged once and otherwise ignored, since a
/// missing chime must never break dictation.
pub fn play(cue: Cue) {
    let _ = std::thread::Builder::new().name("talkr-cue".into()).spawn(move || {
        if let Err(e) = play_blocking(cue) {
            static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                log::warn!("could not play the dictation sound: {}", e);
            }
        }
    });
}

fn play_blocking(cue: Cue) -> Result<(), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::SampleFormat;
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("no output device")?;
    let config = device.default_output_config().map_err(|e| e.to_string())?;
    let rate = config.sample_rate();
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();
    let source = Chime::new(samples(cue), rate);
    let (done_tx, done_rx) = flume::bounded::<()>(1);
    // The formats output devices use in practice (the recorder takes more for microphones).
    let stream = match format {
        SampleFormat::F32 => build_output::<f32>(&device, config, source, done_tx),
        SampleFormat::I16 => build_output::<i16>(&device, config, source, done_tx),
        SampleFormat::U16 => build_output::<u16>(&device, config, source, done_tx),
        SampleFormat::I32 => build_output::<i32>(&device, config, source, done_tx),
        other => return Err(format!("unsupported output format {:?}", other)),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    let _ = done_rx.recv_timeout(std::time::Duration::from_millis(800));
    // Let the device drain the last buffer.
    std::thread::sleep(std::time::Duration::from_millis(60));
    Ok(())
}

/// A cue resampled on the fly to the device's rate. Nearest-neighbour is plenty for a 100 ms
/// chime.
struct Chime {
    source: &'static [f32],
    step: f64,
    total: usize,
    position: usize,
}

impl Chime {
    fn new(source: &'static [f32], device_rate: u32) -> Self {
        let step = RATE as f64 / device_rate.max(1) as f64;
        Self { source, step, total: (source.len() as f64 / step) as usize, position: 0 }
    }

    /// The next frame's value; silence once the chime is over.
    fn next_value(&mut self) -> f32 {
        let value = if self.position < self.total && !self.source.is_empty() {
            self.source[((self.position as f64) * self.step) as usize % self.source.len()]
        } else {
            0.0
        };
        self.position += 1;
        value
    }

    fn finished(&self) -> bool {
        self.position >= self.total
    }

    /// Fill an interleaved buffer, the same value on every channel.
    fn fill<T: cpal::FromSample<f32> + Copy>(&mut self, out: &mut [T], channels: usize) {
        for frame in out.chunks_mut(channels.max(1)) {
            frame.fill(T::from_sample_(self.next_value()));
        }
    }
}

fn build_output<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut chime: Chime,
    done: flume::Sender<()>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + cpal::FromSample<f32> + Send + 'static,
{
    use cpal::traits::DeviceTrait;
    let channels = config.channels as usize;
    device
        .build_output_stream(
            config,
            move |out: &mut [T], _| {
                chime.fill(out, channels);
                if chime.finished() {
                    let _ = done.try_send(());
                }
            },
            |e| log::debug!("cue output error: {}", e),
            None,
        )
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_are_short_quiet_and_valid_wav() {
        for cue in [Cue::Start, Cue::Stop, Cue::Problem] {
            let s = samples(cue);
            assert!(s.len() > RATE as usize / 20 && s.len() < RATE as usize / 2, "{cue:?}: {}", s.len());
            assert!(s.iter().all(|v| v.abs() <= 0.2), "{cue:?} is too loud");
            let reader = hound::WavReader::new(std::io::Cursor::new(wav(cue))).unwrap();
            assert_eq!(reader.spec().sample_rate, RATE);
            assert_eq!(reader.duration() as usize, s.len());
        }
    }

    #[test]
    fn chime_converts_to_integer_outputs() {
        let source: &'static [f32] = Box::leak(vec![0.5f32, -0.5].into_boxed_slice());
        let mut chime = Chime::new(source, RATE);
        let mut out = [0i16; 6];
        chime.fill(&mut out, 2);
        assert_eq!(out[0], out[1], "every channel gets the same value");
        assert!(out[0] > 16_000 && out[2] < -16_000, "{out:?}");
        assert_eq!(out[4], 0, "silence after the end");
        assert!(chime.finished());

        let mut chime = Chime::new(source, RATE);
        let mut out = [0u16; 2];
        chime.fill(&mut out, 1);
        assert!(out[0] > 32_768 + 16_000 && out[1] < 32_768 - 16_000, "{out:?}");

        let mut chime = Chime::new(source, RATE);
        let mut out = [0i32; 1];
        chime.fill(&mut out, 1);
        assert!(out[0] > i32::MAX / 3, "{out:?}");
    }

    #[test]
    fn chime_resamples_to_the_device_rate() {
        assert_eq!(Chime::new(samples(Cue::Start), RATE * 2).total, samples(Cue::Start).len() * 2);
    }
}
