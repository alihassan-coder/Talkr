use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use hound::{WavSpec, WavWriter};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use crate::error::{AppError, Result};

/// Rate recordings are saved at: what Whisper wants, so the engine does no resampling, and a
/// quarter of the disk a 48 kHz file would take.
pub const RECORDING_RATE: u32 = 16_000;

/// Longest recording kept. Past this, capture stops writing (the recording stays "on" until the
/// user stops it, and the result says the limit was reached). Two hours of 16 kHz 16-bit mono is
/// about 230 MB on disk; nothing grows in memory.
pub const MAX_RECORDING: Duration = Duration::from_secs(2 * 60 * 60);

/// How long `start()` waits for the audio device to open before giving up. Opening can take a
/// few seconds (Bluetooth headsets waking up); a driver that never answers must not freeze the app.
const DEVICE_OPEN_TIMEOUT: Duration = Duration::from_secs(10);

/// How long stopping waits for the recorder thread to save the file, and then again for it to
/// close the device. A driver that hangs on close must not hang the app.
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// How often the WAV header is rewritten while recording, so a crash still leaves a playable file.
const HEADER_FLUSH_INTERVAL: Duration = Duration::from_secs(5);

/// Audio callbacks queued for the writer. Callbacks come every ~10 ms, so this is tens of seconds
/// of slack for a slow disk; past it audio is dropped rather than piling up in memory.
const QUEUE_CALLBACKS: usize = 4096;

/// Converts mono audio from the device rate to [`RECORDING_RATE`]. Each output sample is the
/// average of the input that falls in its time slot (inputs straddling a slot boundary are split
/// by time). That box filter is a crude low-pass, but plenty for speech, and exact for whole
/// ratios like 48 kHz to 16 kHz. Upsampling (a rare 8 kHz mic) degrades to sample-and-hold.
#[derive(Debug)]
pub(crate) struct Resampler {
    in_rate: u64,
    out_rate: u64,
    /// Sum of input value times time, for the output slot being filled.
    acc: f64,
    /// How much of the current output slot is filled, in units of 1 / (in_rate * out_rate) s:
    /// one input sample lasts `out_rate` units, one output slot `in_rate` units.
    filled: u64,
}

impl Resampler {
    pub(crate) fn new(in_rate: u32, out_rate: u32) -> Self {
        Self { in_rate: in_rate.max(1) as u64, out_rate: out_rate.max(1) as u64, acc: 0.0, filled: 0 }
    }

    pub(crate) fn push(&mut self, sample: f32, out: &mut Vec<f32>) {
        let mut remaining = self.out_rate;
        while remaining > 0 {
            let take = remaining.min(self.in_rate - self.filled);
            self.acc += sample as f64 * take as f64;
            self.filled += take;
            remaining -= take;
            if self.filled == self.in_rate {
                out.push((self.acc / self.in_rate as f64) as f32);
                self.acc = 0.0;
                self.filled = 0;
            }
        }
    }
}

/// What a finished recording produced.
#[derive(Debug, Clone)]
pub struct Recorded {
    /// The finalized 16 kHz mono 16-bit WAV file.
    pub path: PathBuf,
    /// Samples written, at [`RECORDING_RATE`].
    pub samples: u64,
    /// The recording hit [`MAX_RECORDING`]; audio after that was not kept.
    pub limit_reached: bool,
    /// A microphone or disk error that ended capture early, if any.
    pub error: Option<String>,
}

impl Recorded {
    pub fn duration_ms(&self) -> i64 {
        (self.samples * 1000 / RECORDING_RATE as u64) as i64
    }
}

/// Writes a recording to disk as it arrives, so memory stays flat however long it runs.
pub(crate) struct RecordingWriter {
    writer: Option<WavWriter<BufWriter<File>>>,
    path: PathBuf,
    written: u64,
    max_samples: u64,
    limit_reached: bool,
    error: Option<String>,
    last_flush: Instant,
}

impl RecordingWriter {
    pub(crate) fn create(path: &Path, max_samples: u64) -> Result<Self> {
        let spec = WavSpec {
            channels: 1,
            sample_rate: RECORDING_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let writer = WavWriter::create(path, spec)?;
        Ok(Self {
            writer: Some(writer),
            path: path.to_path_buf(),
            written: 0,
            max_samples,
            limit_reached: false,
            error: None,
            last_flush: Instant::now(),
        })
    }

    /// Append samples at [`RECORDING_RATE`]. Stops keeping audio at the cap or after a write
    /// error (both are reported by `finish`); returns the error message the first time one occurs.
    pub(crate) fn write(&mut self, samples: &[f32]) -> Option<String> {
        let writer = self.writer.as_mut()?;
        if self.error.is_some() {
            return None;
        }
        for &s in samples {
            if self.written >= self.max_samples {
                if !self.limit_reached {
                    self.limit_reached = true;
                    log::warn!("Recording reached the {} minute limit; later audio is not kept", MAX_RECORDING.as_secs() / 60);
                }
                break;
            }
            if let Err(e) = writer.write_sample(to_i16(s)) {
                return Some(self.fail(e));
            }
            self.written += 1;
        }
        if self.last_flush.elapsed() >= HEADER_FLUSH_INTERVAL {
            self.last_flush = Instant::now();
            if let Err(e) = writer.flush() {
                return Some(self.fail(e));
            }
        }
        None
    }

    fn fail(&mut self, e: hound::Error) -> String {
        let message = format!("Could not save the recording: {}", e);
        log::error!("{}", message);
        self.error = Some(message.clone());
        message
    }

    /// Finalize the WAV header and describe the result. If finalizing fails the file may still be
    /// readable up to the last header flush, so the result is returned with the error noted.
    pub(crate) fn finish(mut self) -> Recorded {
        if let Some(writer) = self.writer.take() {
            if let Err(e) = writer.finalize() {
                self.fail(e);
            }
        }
        Recorded {
            path: self.path.clone(),
            samples: self.written,
            limit_reached: self.limit_reached,
            error: self.error.clone(),
        }
    }
}

// Dropping an unfinished writer (a panic on the recorder thread) still finalizes the header:
// hound's own Drop does that, ignoring errors.

fn to_i16(sample: f32) -> i16 {
    if sample.is_nan() {
        return 0;
    }
    (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// Shared slot for the first microphone or disk error of a recording. The UI polls it and
/// shows the message; the recording keeps what was captured until the user stops it.
pub type ErrorSlot = Arc<Mutex<Option<String>>>;

/// Handles the UI's polling task reads without touching the recorder itself, so a slow start
/// or stop (both hold the recorder lock) can never stall the level meter.
#[derive(Clone)]
pub struct RecorderSignals {
    pub recording: Arc<AtomicBool>,
    pub level: Arc<AtomicU32>,
    pub error: ErrorSlot,
}

impl RecorderSignals {
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// The first error of this recording, if any. It stays set until the next recording.
    pub fn error(&self) -> Option<String> {
        lock(&self.error).clone()
    }
}

/// A stop that has been requested but not yet waited for. Waiting needs no recorder lock.
pub struct PendingStop {
    done_rx: flume::Receiver<Recorded>,
    thread: JoinHandle<()>,
}

impl PendingStop {
    /// Wait for the recorder thread to finalize the file, then (briefly) for it to close the
    /// device. A thread stuck in the driver is abandoned rather than waited on forever.
    pub fn wait(self) -> Result<Recorded> {
        let recorded = match self.done_rx.recv_timeout(STOP_TIMEOUT) {
            Ok(recorded) => recorded,
            Err(_) => {
                log::error!("Recorder thread did not finish within {:?}; abandoning it", STOP_TIMEOUT);
                return Err(AppError::Audio(
                    "The microphone stopped responding and the recording could not be saved. \
                     Reconnect it and try again."
                        .into(),
                ));
            }
        };
        let deadline = Instant::now() + STOP_TIMEOUT;
        while !self.thread.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if self.thread.is_finished() {
            let _ = self.thread.join();
        } else {
            // The file is safe; only closing the device is stuck. Let the thread finish on its own.
            log::warn!("Microphone did not close within {:?}; leaving it to close in the background", STOP_TIMEOUT);
        }
        Ok(recorded)
    }
}

/// Microphone recorder that streams to a WAV file.
///
/// The stream and the file writer live on a dedicated thread for the duration of the recording,
/// so the recorder itself stays `Send` on every backend. The audio callback only downmixes,
/// resamples and queues; the thread does the disk writes, keeping file I/O out of the real-time
/// callback. Each recording gets fresh signal handles: a hung, abandoned device thread can never
/// write into a later recording.
pub struct AudioRecorder {
    signals: RecorderSignals,
    stop: Option<Arc<AtomicBool>>,
    pending: Option<PendingStop>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            signals: RecorderSignals {
                recording: Arc::new(AtomicBool::new(false)),
                level: Arc::new(AtomicU32::new(0)),
                error: Arc::new(Mutex::new(None)),
            },
            stop: None,
            pending: None,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.stop.is_some()
    }

    /// Handles for polling level, state and errors without the recorder lock.
    pub fn signals(&self) -> RecorderSignals {
        self.signals.clone()
    }

    pub fn level(&self) -> f32 {
        self.signals.level()
    }

    /// Start recording from the default microphone into a new WAV file at `path` (its folder
    /// must exist). The file is removed again if the microphone cannot be opened.
    pub fn start(&mut self, path: &Path) -> Result<()> {
        if self.is_recording() {
            return Err(AppError::Audio("Already recording".into()));
        }
        let signals = RecorderSignals {
            recording: Arc::new(AtomicBool::new(false)),
            level: Arc::new(AtomicU32::new(0)),
            error: Arc::new(Mutex::new(None)),
        };
        let max_samples = MAX_RECORDING.as_secs() * RECORDING_RATE as u64;
        let writer = RecordingWriter::create(path, max_samples)?;

        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = flume::bounded::<Result<()>>(1);
        let (done_tx, done_rx) = flume::bounded::<Recorded>(1);

        let thread_signals = signals.clone();
        let thread_stop = stop.clone();
        let spawned = std::thread::Builder::new().name("talkr-recorder".into()).spawn(move || {
            let (queue_tx, queue_rx) = flume::bounded::<Vec<f32>>(QUEUE_CALLBACKS);
            let stream = match open_input_stream(&thread_signals, queue_tx) {
                Ok(stream) => stream,
                Err(e) => {
                    discard(writer);
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            if let Err(e) = stream.play() {
                discard(writer);
                let _ = ready_tx.send(Err(audio_error(e)));
                return;
            }
            // If start() already gave up on us, close the device right away.
            if ready_tx.send(Ok(())).is_err() {
                discard(writer);
                return;
            }
            let recorded = write_until_stopped(writer, &queue_rx, &thread_stop, &thread_signals);
            // Hand the finished file over before closing the device: closing is where some
            // drivers hang, and the recording must not depend on it.
            let _ = done_tx.send(recorded);
            drop(stream);
        });
        let thread = match spawned {
            Ok(thread) => thread,
            Err(e) => {
                let _ = std::fs::remove_file(path);
                return Err(e.into());
            }
        };

        match ready_rx.recv_timeout(DEVICE_OPEN_TIMEOUT) {
            Ok(Ok(())) => {
                signals.recording.store(true, Ordering::Relaxed);
                self.signals = signals;
                self.stop = Some(stop);
                self.pending = Some(PendingStop { done_rx, thread });
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(flume::RecvTimeoutError::Timeout) => {
                // Do not join: the thread is stuck in the driver. Dropping `ready_rx` makes it
                // release the device (and delete the file) as soon as it gets unstuck.
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

    /// Ask the recording to stop. Quick: call it under the recorder lock, then `wait()` on the
    /// result without the lock.
    pub fn request_stop(&mut self) -> Result<PendingStop> {
        let (Some(stop), Some(pending)) = (self.stop.take(), self.pending.take()) else {
            return Err(AppError::Audio("Not recording".into()));
        };
        stop.store(true, Ordering::Relaxed);
        self.signals.recording.store(false, Ordering::Relaxed);
        self.signals.level.store(0f32.to_bits(), Ordering::Relaxed);
        Ok(pending)
    }

    /// Stop and wait for the finished file.
    pub fn stop(&mut self) -> Result<Recorded> {
        self.request_stop()?.wait()
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

/// Remove the file of a recording that never started.
fn discard(writer: RecordingWriter) {
    let path = writer.path.clone();
    drop(writer.finish());
    let _ = std::fs::remove_file(path);
}

/// The recorder thread's loop: move queued audio to disk until a stop is requested, then save
/// what is still queued and finalize the file.
fn write_until_stopped(
    mut writer: RecordingWriter,
    queue: &flume::Receiver<Vec<f32>>,
    stop: &AtomicBool,
    signals: &RecorderSignals,
) -> Recorded {
    while !stop.load(Ordering::Relaxed) {
        match queue.recv_timeout(Duration::from_millis(50)) {
            Ok(chunk) => {
                if let Some(message) = writer.write(&chunk) {
                    report_error(&signals.error, message);
                }
            }
            Err(flume::RecvTimeoutError::Timeout) => {}
            // The stream (the only sender) is gone; nothing more will arrive.
            Err(flume::RecvTimeoutError::Disconnected) => break,
        }
    }
    while let Ok(chunk) = queue.try_recv() {
        if let Some(message) = writer.write(&chunk) {
            report_error(&signals.error, message);
        }
    }
    let mut recorded = writer.finish();
    if recorded.error.is_none() {
        recorded.error = lock(&signals.error).clone();
    }
    recorded
}

/// Keep the first error of a recording; later ones are usually consequences of it.
fn report_error(slot: &ErrorSlot, message: String) {
    let mut slot = lock(slot);
    if slot.is_none() {
        *slot = Some(message);
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

/// Whether a stream error means capture has stopped. Glitches and reroutes are only logged.
fn is_fatal(kind: cpal::ErrorKind) -> bool {
    use cpal::ErrorKind;
    !matches!(kind, ErrorKind::Xrun | ErrorKind::RealtimeDenied | ErrorKind::DeviceChanged)
}

/// What to tell the user when the microphone fails mid-recording.
fn stream_error_message(e: &cpal::Error) -> String {
    use cpal::ErrorKind;
    match e.kind() {
        ErrorKind::DeviceNotAvailable | ErrorKind::HostUnavailable | ErrorKind::StreamInvalidated => {
            "The microphone was disconnected. What was recorded so far is kept; stop the recording \
             to use it."
                .into()
        }
        _ => format!("The microphone stopped working ({}). Stop the recording to keep what was captured.", e),
    }
}

fn open_input_stream(signals: &RecorderSignals, queue: flume::Sender<Vec<f32>>) -> Result<cpal::Stream> {
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

    let sink = Sink::new(sample_rate, queue, signals.level.clone());
    let errors = signals.error.clone();
    let level = signals.level.clone();
    let err_fn = move |err: cpal::Error| {
        if is_fatal(err.kind()) {
            log::error!("Audio stream error: {}", err);
            level.store(0f32.to_bits(), Ordering::Relaxed);
            report_error(&errors, stream_error_message(&err));
        } else {
            log::warn!("Audio stream glitch: {}", err);
        }
    };

    match format {
        SampleFormat::F32 => build_stream::<f32>(&device, stream_config, sink, err_fn),
        SampleFormat::F64 => build_stream::<f64>(&device, stream_config, sink, err_fn),
        SampleFormat::I8 => build_stream::<i8>(&device, stream_config, sink, err_fn),
        SampleFormat::I16 => build_stream::<i16>(&device, stream_config, sink, err_fn),
        SampleFormat::I24 => build_stream::<cpal::I24>(&device, stream_config, sink, err_fn),
        SampleFormat::I32 => build_stream::<i32>(&device, stream_config, sink, err_fn),
        SampleFormat::U8 => build_stream::<u8>(&device, stream_config, sink, err_fn),
        SampleFormat::U16 => build_stream::<u16>(&device, stream_config, sink, err_fn),
        SampleFormat::U24 => build_stream::<cpal::U24>(&device, stream_config, sink, err_fn),
        SampleFormat::U32 => build_stream::<u32>(&device, stream_config, sink, err_fn),
        _ => Err(AppError::Audio(format!("Unsupported microphone sample format: {:?}", format))),
    }
}

/// The audio callback's side: downmix, resample to [`RECORDING_RATE`], publish the level and
/// queue the result for the writer. No locks and no disk I/O here.
struct Sink {
    resampler: Resampler,
    queue: flume::Sender<Vec<f32>>,
    level: Arc<AtomicU32>,
    dropped_warned: bool,
}

impl Sink {
    fn new(device_rate: u32, queue: flume::Sender<Vec<f32>>, level: Arc<AtomicU32>) -> Self {
        Self { resampler: Resampler::new(device_rate, RECORDING_RATE), queue, level, dropped_warned: false }
    }

    fn accept<T>(&mut self, data: &[T], channels: usize)
    where
        T: Copy,
        f32: cpal::FromSample<T>,
    {
        let channels = channels.max(1);
        let mut sum_sq = 0.0f32;
        let mut frames = 0usize;
        let mut out = Vec::with_capacity(data.len() / channels / 2 + 2);
        for frame in data.chunks(channels) {
            let s = frame.iter().map(|&x| <f32 as cpal::FromSample<T>>::from_sample_(x)).sum::<f32>() / frame.len() as f32;
            sum_sq += s * s;
            frames += 1;
            self.resampler.push(s, &mut out);
        }
        if frames > 0 {
            let rms = (sum_sq / frames as f32).sqrt();
            self.level.store(rms.to_bits(), Ordering::Relaxed);
        }
        if !out.is_empty() && self.queue.try_send(out).is_err() && !self.dropped_warned {
            // The writer is stuck (a stalled disk) or gone; drop audio rather than grow memory.
            self.dropped_warned = true;
            log::warn!("Recording writer is falling behind; dropping audio");
        }
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut sink: Sink,
    err_fn: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + Send + 'static,
    f32: cpal::FromSample<T>,
{
    let channels = (config.channels as usize).max(1);
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

    fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
        let mut r = Resampler::new(from, to);
        let mut out = Vec::new();
        for &s in input {
            r.push(s, &mut out);
        }
        out
    }

    #[test]
    fn resampler_averages_whole_ratios() {
        let out = resample(&[0.0, 0.3, 0.6, 1.0, 1.0, 1.0, 0.5], 48_000, 16_000);
        // The incomplete last slot waits for more input.
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.3).abs() < 1e-6);
        assert!((out[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn resampler_passes_matching_rates_through() {
        let input = [0.1, -0.2, 0.3];
        assert_eq!(resample(&input, 16_000, 16_000), input);
    }

    #[test]
    fn resampler_keeps_duration_for_fractional_ratios() {
        // One second at 44.1 kHz is exactly one second at 16 kHz, and a constant stays constant.
        let out = resample(&vec![0.5; 44_100], 44_100, 16_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-6));
        // Upsampling holds each value.
        assert_eq!(resample(&[0.2, 0.4], 8_000, 16_000), [0.2, 0.2, 0.4, 0.4]);
    }

    #[test]
    fn resampler_suppresses_content_above_the_new_nyquist() {
        // A 12 kHz tone at 48 kHz cannot be represented at 16 kHz; averaging must not alias it
        // into something loud.
        let tone: Vec<f32> = (0..48_000).map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 12_000.0 / 48_000.0).sin()).collect();
        let out = resample(&tone, 48_000, 16_000);
        let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
        assert!(rms < 0.35, "aliased energy too high: {rms}");
    }

    fn sink(device_rate: u32) -> (Sink, flume::Receiver<Vec<f32>>) {
        let (tx, rx) = flume::bounded(16);
        (Sink::new(device_rate, tx, Arc::new(AtomicU32::new(0))), rx)
    }

    fn queued(rx: &flume::Receiver<Vec<f32>>) -> Vec<f32> {
        rx.try_iter().flatten().collect()
    }

    #[test]
    fn sink_downmixes_stereo_and_reports_level() {
        let (mut s, rx) = sink(16_000);
        s.accept(&[1.0f32, 0.0, 0.5, 0.5, -1.0, -1.0], 2);
        assert_eq!(queued(&rx), vec![0.5, 0.5, -1.0]);
        let level = f32::from_bits(s.level.load(Ordering::Relaxed));
        let expected = ((0.25f32 + 0.25 + 1.0) / 3.0).sqrt();
        assert!((level - expected).abs() < 1e-6, "{level} vs {expected}");
    }

    #[test]
    fn sink_converts_integer_samples() {
        let (mut s, rx) = sink(16_000);
        s.accept(&[i16::MAX, 0i16, i16::MIN], 1);
        let got = queued(&rx);
        assert!((got[0] - 1.0).abs() < 1e-3);
        assert_eq!(got[1], 0.0);
        assert_eq!(got[2], -1.0);
    }

    #[test]
    fn sink_resamples_and_drops_instead_of_growing_when_the_writer_stalls() {
        let (tx, rx) = flume::bounded(1);
        let mut s = Sink::new(48_000, tx, Arc::new(AtomicU32::new(0)));
        s.accept(&[0.3f32; 6], 1);
        s.accept(&[0.9f32; 6], 1);
        // The queue held one callback; the second was dropped, but the meter still moved.
        assert_eq!(queued(&rx), vec![0.3, 0.3]);
        assert!((f32::from_bits(s.level.load(Ordering::Relaxed)) - 0.9).abs() < 1e-6);
    }

    #[test]
    fn writer_streams_a_valid_wav() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rec.wav");
        let mut w = RecordingWriter::create(&path, 1_000).unwrap();
        assert!(w.write(&[0.0, 0.5, -0.5]).is_none());
        assert!(w.write(&[1.0, 2.0, f32::NAN]).is_none());
        let rec = w.finish();
        assert_eq!(rec.samples, 6);
        assert!(!rec.limit_reached && rec.error.is_none());
        assert_eq!(rec.path, path);

        let mut reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!((spec.channels, spec.sample_rate, spec.bits_per_sample), (1, RECORDING_RATE, 16));
        let read: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        assert_eq!(read, [0, 16384, -16384, 32767, 32767, 0]);
    }

    #[test]
    fn writer_stops_at_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cap.wav");
        let mut w = RecordingWriter::create(&path, 4).unwrap();
        w.write(&[0.1; 3]);
        w.write(&[0.2; 3]);
        let rec = w.finish();
        assert_eq!(rec.samples, 4);
        assert!(rec.limit_reached);
        assert_eq!(hound::WavReader::open(&path).unwrap().duration(), 4);
        assert_eq!(rec.duration_ms(), 0);
    }

    #[test]
    fn dropping_an_unfinished_writer_still_leaves_a_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dropped.wav");
        let mut w = RecordingWriter::create(&path, 100).unwrap();
        w.write(&[0.25; 10]);
        drop(w);
        assert_eq!(hound::WavReader::open(&path).unwrap().duration(), 10);
    }

    #[test]
    fn writer_loop_saves_queued_audio_after_stop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("loop.wav");
        let writer = RecordingWriter::create(&path, 1_000).unwrap();
        let (tx, rx) = flume::bounded(8);
        tx.send(vec![0.5; 160]).unwrap();
        tx.send(vec![0.5; 40]).unwrap();
        let signals = AudioRecorder::new().signals();
        report_error(&signals.error, "The microphone was disconnected.".into());
        report_error(&signals.error, "a later error".into());
        // Stop already requested: whatever is queued is still written.
        let rec = write_until_stopped(writer, &rx, &AtomicBool::new(true), &signals);
        assert_eq!(rec.samples, 200);
        assert_eq!(rec.error.as_deref(), Some("The microphone was disconnected."));
        assert_eq!(hound::WavReader::open(&path).unwrap().duration(), 200);
        assert_eq!(Recorded { samples: 16_000, ..rec }.duration_ms(), 1_000);
    }

    #[test]
    fn signals_keep_the_first_error() {
        let r = AudioRecorder::new();
        let s = r.signals();
        assert!(s.error().is_none());
        report_error(&s.error, "gone".into());
        report_error(&s.error, "later".into());
        assert_eq!(s.error().as_deref(), Some("gone"));
    }

    #[test]
    fn only_real_failures_are_reported() {
        assert!(!is_fatal(cpal::ErrorKind::Xrun));
        assert!(!is_fatal(cpal::ErrorKind::DeviceChanged));
        assert!(is_fatal(cpal::ErrorKind::DeviceNotAvailable));
        assert!(is_fatal(cpal::ErrorKind::StreamInvalidated));
    }

    #[test]
    fn stop_without_start_is_an_error() {
        let mut r = AudioRecorder::new();
        assert!(!r.is_recording());
        assert!(r.stop().is_err());
        assert_eq!(r.level(), 0.0);
    }

    #[test]
    fn two_hours_at_16k_fits_in_a_wav() {
        let max = MAX_RECORDING.as_secs() * RECORDING_RATE as u64;
        assert_eq!(max, 115_200_000);
        // 16-bit samples; WAV sizes are 32-bit.
        assert!(max * 2 < u32::MAX as u64);
    }
}
