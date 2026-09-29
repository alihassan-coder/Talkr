use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use crate::error::{AppError, Result};

/// Microphone recorder.
///
/// `cpal::Stream` is `!Send`, so the stream lives on a dedicated thread for the duration of
/// the recording. Captured mono samples are accumulated in a shared buffer and the current
/// RMS input level is published through an atomic so it can be polled from any thread.
pub struct AudioRecorder {
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
    recording: Arc<AtomicBool>,
    sample_rate: u32,
    stop_tx: Option<flume::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            level: Arc::new(AtomicU32::new(0)),
            recording: Arc::new(AtomicBool::new(false)),
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

    pub fn start(&mut self) -> Result<()> {
        if self.is_recording() {
            return Err(AppError::Audio("Already recording".into()));
        }
        self.samples.lock().unwrap_or_else(|e| e.into_inner()).clear();
        self.level.store(0f32.to_bits(), Ordering::Relaxed);

        let (stop_tx, stop_rx) = flume::bounded::<()>(1);
        let (ready_tx, ready_rx) = flume::bounded::<Result<u32>>(1);

        let samples = self.samples.clone();
        let level = self.level.clone();
        let recording = self.recording.clone();

        let thread = std::thread::Builder::new()
            .name("talkr-recorder".into())
            .spawn(move || {
                let stream = match open_input_stream(samples, level) {
                    Ok((stream, rate)) => {
                        if let Err(e) = stream.play() {
                            let _ = ready_tx.send(Err(e.into()));
                            return;
                        }
                        recording.store(true, Ordering::Relaxed);
                        let _ = ready_tx.send(Ok(rate));
                        stream
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                // Block until stop is requested (or the recorder is dropped).
                let _ = stop_rx.recv();
                drop(stream);
                recording.store(false, Ordering::Relaxed);
            })?;

        match ready_rx.recv() {
            Ok(Ok(rate)) => {
                self.sample_rate = rate;
                self.stop_tx = Some(stop_tx);
                self.thread = Some(thread);
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
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
        let samples = std::mem::take(&mut *self.samples.lock().unwrap_or_else(|e| e.into_inner()));
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

fn open_input_stream(samples: Arc<Mutex<Vec<f32>>>, level: Arc<AtomicU32>) -> Result<(cpal::Stream, u32)> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| AppError::Audio("No input device found".into()))?;

    let config = device.default_input_config()?;
    let sample_rate = config.sample_rate().0;
    let format = config.sample_format();
    let stream_config: StreamConfig = config.into();

    let stream = match format {
        SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, samples, level)?,
        SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, samples, level)?,
        SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, samples, level)?,
        SampleFormat::I32 => build_stream::<i32>(&device, &stream_config, samples, level)?,
        _ => return Err(AppError::Audio(format!("Unsupported sample format: {:?}", format))),
    };
    Ok((stream, sample_rate))
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample + Send + 'static,
    f32: cpal::FromSample<T>,
{
    let channels = (config.channels as usize).max(1);
    let err_fn = |err| log::error!("Audio stream error: {}", err);

    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut sum_sq = 0.0f32;
            let mut mono = Vec::with_capacity(data.len() / channels);
            for frame in data.chunks(channels) {
                let s = frame
                    .iter()
                    .map(|&x| <f32 as cpal::FromSample<T>>::from_sample_(x))
                    .sum::<f32>()
                    / frame.len() as f32;
                sum_sq += s * s;
                mono.push(s);
            }
            if !mono.is_empty() {
                let rms = (sum_sq / mono.len() as f32).sqrt();
                level.store(rms.to_bits(), Ordering::Relaxed);
                if let Ok(mut buf) = samples.lock() {
                    buf.extend_from_slice(&mono);
                }
            }
        },
        err_fn,
        None,
    )?;

    Ok(stream)
}
