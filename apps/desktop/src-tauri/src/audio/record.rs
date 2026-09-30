use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;
use crate::error::{AppError, Result};

/// Longest recording kept in memory. Past this, capture stops adding samples (the recording
/// stays "on" until the user stops it, and `limit_reached()` turns true). Two hours of 48 kHz
/// mono f32 is about 1.4 GB.
pub const MAX_RECORDING: Duration = Duration::from_secs(2 * 60 * 60);

/// How long `start()` waits for the audio device to open before giving up. Opening can take a
/// few seconds (Bluetooth headsets waking up); a driver that never answers must not freeze the app.
const DEVICE_OPEN_TIMEOUT: Duration = Duration::from_secs(10);

/// Samples per storage chunk: about one second at 48 kHz. Chunks mean no reallocation of one
/// huge buffer inside the audio callback, and no overshoot of the memory the cap allows.
const CHUNK_SAMPLES: usize = 48_000;

/// Mono samples captured so far, stored in fixed-size chunks and capped at `max_samples`.
#[derive(Debug)]
pub(crate) struct SampleBuffer {
    chunks: Vec<Vec<f32>>,
    len: usize,
    max_samples: usize,
    chunk_samples: usize,
}

impl SampleBuffer {
    pub(crate) fn new(max_samples: usize) -> Self {
        Self::with_chunk_size(max_samples, CHUNK_SAMPLES)
    }

    fn with_chunk_size(max_samples: usize, chunk_samples: usize) -> Self {
        Self { chunks: Vec::new(), len: 0, max_samples, chunk_samples: chunk_samples.max(1) }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn is_full(&self) -> bool {
        self.len >= self.max_samples
    }

    /// Append one sample. Returns false (and drops it) once the cap is reached.
    pub(crate) fn push(&mut self, sample: f32) -> bool {
        if self.is_full() {
            return false;
        }
        match self.chunks.last_mut() {
            Some(chunk) if chunk.len() < chunk.capacity() => chunk.push(sample),
            _ => {
                let size = self.chunk_samples.min(self.max_samples - self.len);
                let mut chunk = Vec::with_capacity(size);
                chunk.push(sample);
                self.chunks.push(chunk);
            }
        }
        self.len += 1;
        true
    }

    /// All samples in order, leaving the buffer empty.
    pub(crate) fn take(&mut self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.len);
        for chunk in self.chunks.drain(..) {
            out.extend_from_slice(&chunk);
        }
        self.len = 0;
        out
    }
}

/// Microphone recorder.
///
/// The stream lives on a dedicated thread for the duration of the recording, so the recorder
/// itself stays `Send` on every backend. Each recording gets its own sample buffer: if a device
/// thread hangs and is abandoned, it can never write into a later recording.
pub struct AudioRecorder {
    samples: Arc<Mutex<SampleBuffer>>,
    level: Arc<AtomicU32>,
    recording: Arc<AtomicBool>,
    limit_reached: Arc<AtomicBool>,
    sample_rate: u32,
    stop_tx: Option<flume::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Mutex::new(SampleBuffer::new(0))),
            level: Arc::new(AtomicU32::new(0)),
            recording: Arc::new(AtomicBool::new(false)),
            limit_reached: Arc::new(AtomicBool::new(false)),
            sample_rate: 16000,
            stop_tx: None,
            thread: None,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::Relaxed)
    }

    /// Shared flag that is `true` while recording; handy for level-polling tasks.
    pub fn recording_flag(&self) -> Arc<AtomicBool> {
        self.recording.clone()
    }

    pub fn level_handle(&self) -> Arc<AtomicU32> {
        self.level.clone()
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Whether the current (or last) recording hit [`MAX_RECORDING`] and stopped keeping audio.
    pub fn limit_reached(&self) -> bool {
        self.limit_reached.load(Ordering::Relaxed)
    }

    pub fn start(&mut self) -> Result<()> {
        if self.is_recording() || self.stop_tx.is_some() {
            return Err(AppError::Audio("Already recording".into()));
        }
        self.level.store(0f32.to_bits(), Ordering::Relaxed);
        self.limit_reached.store(false, Ordering::Relaxed);

        let (stop_tx, stop_rx) = flume::bounded::<()>(1);
        let (ready_tx, ready_rx) = flume::bounded::<Result<(u32, Arc<Mutex<SampleBuffer>>)>>(1);

        let level = self.level.clone();
        let limit_reached = self.limit_reached.clone();

        let thread = std::thread::Builder::new()
            .name("talkr-recorder".into())
            .spawn(move || {
                let (stream, rate, samples) = match open_input_stream(level, limit_reached) {
                    Ok(opened) => opened,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                if let Err(e) = stream.play() {
                    let _ = ready_tx.send(Err(audio_error(e)));
                    return;
                }
                // If start() already gave up on us, close the device right away.
                if ready_tx.send(Ok((rate, samples))).is_err() {
                    return;
                }
                // Block until stop is requested (or the recorder is dropped).
                let _ = stop_rx.recv();
                drop(stream);
            })?;

        match ready_rx.recv_timeout(DEVICE_OPEN_TIMEOUT) {
            Ok(Ok((rate, samples))) => {
                self.sample_rate = rate;
                self.samples = samples;
                self.stop_tx = Some(stop_tx);
                self.thread = Some(thread);
                self.recording.store(true, Ordering::Relaxed);
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(flume::RecvTimeoutError::Timeout) => {
                // Do not join: the thread is stuck in the driver. Dropping `ready_rx` and
                // `stop_tx` makes it release the device as soon as it gets unstuck.
                log::error!("Microphone did not open within {:?}; abandoning the device thread", DEVICE_OPEN_TIMEOUT);
                Err(AppError::Audio(format!(
                    "The microphone did not respond within {} seconds. Check that it is connected \
                     and not in use by another app, then try again.",
                    DEVICE_OPEN_TIMEOUT.as_secs()
                )))
            }
            Err(flume::RecvTimeoutError::Disconnected) => {
                let _ = thread.join();
                Err(AppError::Audio("Recorder thread exited unexpectedly".into()))
            }
        }
    }

    /// Stop recording and return the captured mono samples (at `sample_rate()`).
    pub fn stop(&mut self) -> Result<Vec<f32>> {
        let Some(stop_tx) = self.stop_tx.take() else {
            return Err(AppError::Audio("Not recording".into()));
        };
        let _ = stop_tx.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.recording.store(false, Ordering::Relaxed);
        self.level.store(0f32.to_bits(), Ordering::Relaxed);
        let samples = lock(&self.samples).take();
        Ok(samples)
    }
}

impl Default for AudioRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Turn a cpal error into a message the user can act on.
fn audio_error(e: cpal::Error) -> AppError {
    use cpal::ErrorKind;
    let hint = match e.kind() {
        ErrorKind::PermissionDenied => {
            "Talkr is not allowed to use the microphone. Allow microphone access for Talkr in your \
             system's privacy settings, then try again."
        }
        ErrorKind::DeviceBusy => "The microphone is in use by another app. Close it and try again.",
        ErrorKind::DeviceNotAvailable | ErrorKind::DeviceChanged => {
            "The microphone is not available. Check that it is connected, then try again."
        }
        ErrorKind::UnsupportedConfig => "The microphone does not support a usable recording format.",
        _ => return AppError::Audio(e.to_string()),
    };
    AppError::Audio(format!("{} ({})", hint, e))
}

type Opened = (cpal::Stream, u32, Arc<Mutex<SampleBuffer>>);

fn open_input_stream(level: Arc<AtomicU32>, limit_reached: Arc<AtomicBool>) -> Result<Opened> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| AppError::Audio("No microphone found. Connect one and try again.".into()))?;

    let config = device.default_input_config().map_err(audio_error)?;
    let sample_rate = config.sample_rate();
    if sample_rate == 0 {
        return Err(AppError::Audio("The microphone reported a sample rate of 0".into()));
    }
    let format = config.sample_format();
    let stream_config: StreamConfig = config.into();

    let max_samples = (MAX_RECORDING.as_secs() as usize).saturating_mul(sample_rate as usize);
    let samples = Arc::new(Mutex::new(SampleBuffer::new(max_samples)));
    let sink = Sink { samples: samples.clone(), level, limit_reached };

    let stream = match format {
        SampleFormat::F32 => build_stream::<f32>(&device, stream_config, sink),
        SampleFormat::F64 => build_stream::<f64>(&device, stream_config, sink),
        SampleFormat::I8 => build_stream::<i8>(&device, stream_config, sink),
        SampleFormat::I16 => build_stream::<i16>(&device, stream_config, sink),
        SampleFormat::I24 => build_stream::<cpal::I24>(&device, stream_config, sink),
        SampleFormat::I32 => build_stream::<i32>(&device, stream_config, sink),
        SampleFormat::U8 => build_stream::<u8>(&device, stream_config, sink),
        SampleFormat::U16 => build_stream::<u16>(&device, stream_config, sink),
        SampleFormat::U24 => build_stream::<cpal::U24>(&device, stream_config, sink),
        SampleFormat::U32 => build_stream::<u32>(&device, stream_config, sink),
        _ => return Err(AppError::Audio(format!("Unsupported microphone sample format: {:?}", format))),
    }?;
    Ok((stream, sample_rate, samples))
}

/// Where the audio callback puts what it captures.
struct Sink {
    samples: Arc<Mutex<SampleBuffer>>,
    level: Arc<AtomicU32>,
    limit_reached: Arc<AtomicBool>,
}

impl Sink {
    /// Downmix interleaved frames to mono, store them and publish the RMS level.
    fn accept<T>(&self, data: &[T], channels: usize)
    where
        T: Copy,
        f32: cpal::FromSample<T>,
    {
        let mut sum_sq = 0.0f32;
        let mut frames = 0usize;
        let mut buf = lock(&self.samples);
        let mut full = buf.is_full();
        for frame in data.chunks(channels) {
            let s = frame.iter().map(|&x| <f32 as cpal::FromSample<T>>::from_sample_(x)).sum::<f32>() / frame.len() as f32;
            sum_sq += s * s;
            frames += 1;
            if !full && !buf.push(s) {
                full = true;
            }
        }
        drop(buf);
        if frames > 0 {
            let rms = (sum_sq / frames as f32).sqrt();
            self.level.store(rms.to_bits(), Ordering::Relaxed);
        }
        if full && !self.limit_reached.swap(true, Ordering::Relaxed) {
            log::warn!("Recording reached the {} minute limit; later audio is not kept", MAX_RECORDING.as_secs() / 60);
        }
    }
}

fn build_stream<T>(device: &cpal::Device, config: StreamConfig, sink: Sink) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + Send + 'static,
    f32: cpal::FromSample<T>,
{
    let channels = (config.channels as usize).max(1);
    let err_fn = |err: cpal::Error| log::error!("Audio stream error: {}", err);

    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| sink.accept(data, channels),
            err_fn,
            None,
        )
        .map_err(audio_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffer_keeps_order_across_chunks() {
        let mut b = SampleBuffer::with_chunk_size(100, 3);
        for i in 0..10 {
            assert!(b.push(i as f32));
        }
        assert_eq!(b.len(), 10);
        assert_eq!(b.take(), (0..10).map(|i| i as f32).collect::<Vec<_>>());
        assert_eq!(b.len(), 0);
        assert!(b.take().is_empty());
    }

    #[test]
    fn buffer_stops_at_the_cap() {
        let mut b = SampleBuffer::with_chunk_size(5, 2);
        let accepted = (0..8).filter(|&i| b.push(i as f32)).count();
        assert_eq!(accepted, 5);
        assert!(b.is_full());
        assert_eq!(b.take(), vec![0.0, 1.0, 2.0, 3.0, 4.0]);
        // Taking frees it for a new recording.
        assert!(!b.is_full());
        assert!(b.push(9.0));
    }

    #[test]
    fn buffer_never_allocates_past_the_cap() {
        let mut b = SampleBuffer::with_chunk_size(5, 4);
        for i in 0..5 {
            b.push(i as f32);
        }
        let capacity: usize = b.chunks.iter().map(|c| c.capacity()).sum();
        assert_eq!(capacity, 5);
    }

    #[test]
    fn zero_cap_accepts_nothing() {
        let mut b = SampleBuffer::new(0);
        assert!(!b.push(1.0));
        assert!(b.take().is_empty());
    }

    fn sink(max: usize) -> Sink {
        Sink {
            samples: Arc::new(Mutex::new(SampleBuffer::with_chunk_size(max, 4))),
            level: Arc::new(AtomicU32::new(0)),
            limit_reached: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn sink_downmixes_stereo_and_reports_level() {
        let s = sink(100);
        s.accept(&[1.0f32, 0.0, 0.5, 0.5, -1.0, -1.0], 2);
        assert_eq!(lock(&s.samples).take(), vec![0.5, 0.5, -1.0]);
        let level = f32::from_bits(s.level.load(Ordering::Relaxed));
        let expected = ((0.25f32 + 0.25 + 1.0) / 3.0).sqrt();
        assert!((level - expected).abs() < 1e-6, "{level} vs {expected}");
        assert!(!s.limit_reached.load(Ordering::Relaxed));
    }

    #[test]
    fn sink_converts_integer_samples() {
        let s = sink(100);
        s.accept(&[i16::MAX, 0i16, i16::MIN], 1);
        let got = lock(&s.samples).take();
        assert!((got[0] - 1.0).abs() < 1e-3);
        assert_eq!(got[1], 0.0);
        assert_eq!(got[2], -1.0);
    }

    #[test]
    fn sink_flags_the_limit_and_keeps_the_level_live() {
        let s = sink(3);
        s.accept(&[0.1f32, 0.1], 1);
        assert!(!s.limit_reached.load(Ordering::Relaxed));
        s.accept(&[0.2f32, 0.2, 0.9, 0.9], 1);
        assert!(s.limit_reached.load(Ordering::Relaxed));
        assert_eq!(lock(&s.samples).len(), 3);
        // The meter still moves after the cap.
        s.accept(&[0.5f32], 1);
        assert!((f32::from_bits(s.level.load(Ordering::Relaxed)) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn stop_without_start_is_an_error() {
        let mut r = AudioRecorder::new();
        assert!(!r.is_recording());
        assert!(r.stop().is_err());
        assert_eq!(r.level(), 0.0);
        assert!(!r.limit_reached());
    }

    #[test]
    fn two_hours_at_48k_fits_the_cap() {
        let max = (MAX_RECORDING.as_secs() as usize) * 48_000;
        assert_eq!(max, 345_600_000);
    }
}
