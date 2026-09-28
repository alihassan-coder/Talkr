use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use flume::{Sender, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use crate::error::{AppError, Result};

pub struct AudioRecorder {
    stream: Option<cpal::Stream>,
    sender: Sender<f32>,
    receiver: Receiver<f32>,
    level_sender: Sender<f32>,
    sample_rate: u32,
    is_recording: Arc<Mutex<bool>>,
}

impl AudioRecorder {
    pub fn new() -> Result<Self> {
        let (sender, receiver) = flume::unbounded();
        let (level_sender, _) = flume::unbounded();

        Ok(Self {
            stream: None,
            sender,
            receiver,
            level_sender,
            sample_rate: 16000,
            is_recording: Arc::new(Mutex::new(false)),
        })
    }

    pub fn start(&mut self) -> Result<()> {
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or_else(|| AppError::Audio("No input device found".into()))?;

        let config = device.default_input_config()?;
        self.sample_rate = config.sample_rate().0;

        let sender = self.sender.clone();
        let level_sender = self.level_sender.clone();
        let is_recording = self.is_recording.clone();
        *is_recording.lock().unwrap() = true;

        let stream = match config.sample_format() {
            SampleFormat::F32 => self.build_stream::<f32>(&device, &config.into(), sender, level_sender, is_recording)?,
            SampleFormat::I16 => self.build_stream::<i16>(&device, &config.into(), sender, level_sender, is_recording)?,
            SampleFormat::U16 => self.build_stream::<u16>(&device, &config.into(), sender, level_sender, is_recording)?,
            _ => return Err(AppError::Audio("Unsupported sample format".into())),
        };

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    fn build_stream<T>(
        &self,
        device: &cpal::Device,
        config: &StreamConfig,
        sender: Sender<f32>,
        level_sender: Sender<f32>,
        is_recording: Arc<Mutex<bool>>,
    ) -> Result<cpal::Stream>
    where
        T: cpal::Sample + cpal::SizedSample + Send + 'static,
    {
        let channels = config.channels as usize;
        let err_fn = |err| eprintln!("Audio stream error: {}", err);

        let stream = device.build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if !*is_recording.lock().unwrap() {
                    return;
                }

                let mut sum_sq = 0.0f32;
                let mut count = 0;

                for frame in data.chunks(channels) {
                    let sample = frame[0].to_sample::<f32>();
                    let _ = sender.send(sample);
                    sum_sq += sample * sample;
                    count += 1;
                }

                if count > 0 {
                    let rms = (sum_sq / count as f32).sqrt();
                    let _ = level_sender.send(rms);
                }
            },
            err_fn,
            None,
        )?;

        Ok(stream)
    }

    pub fn stop(&mut self) -> Result<Vec<f32>> {
        *self.is_recording.lock().unwrap() = false;
        self.stream = None;

        let mut samples = Vec::new();
        while let Ok(sample) = self.receiver.try_recv() {
            samples.push(sample);
        }

        Ok(samples)
    }

    pub fn get_level_receiver(&self) -> Receiver<f32> {
        self.level_sender.clone()
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}