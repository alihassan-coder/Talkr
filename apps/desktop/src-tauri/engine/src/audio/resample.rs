//! Streaming conversion of mono audio to the 16 kHz whisper.cpp expects.
//!
//! [`StreamResampler`] takes decoded audio in blocks of any size and resamples it in fixed-size
//! chunks, so memory stays at one chunk plus the 16 kHz output however long the source is.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async, Fft, FixedAsync, FixedSync, Indexing, Resampler, SincInterpolationParameters, WindowFunction,
};
use crate::{EngineError, Result};

/// The sample rate whisper.cpp works at.
pub const TARGET_RATE: u32 = 16_000;

/// Input frames per resampler call: about 0.1 s at 44.1/48 kHz.
const CHUNK_FRAMES: usize = 4096;

/// The FFT resampler's block is `rate / gcd(rate, 16000)` input frames. Common rates give small
/// blocks (44.1 kHz: 441, 48 kHz: 3); an odd rate such as 44 101 Hz would need a block as long
/// as a second of audio, so those go to the sinc resampler instead.
const MAX_FFT_BLOCK: u32 = 4096;

/// Resamples a mono stream to [`TARGET_RATE`], appending to a caller-owned output.
///
/// Call [`push`](Self::push) with each decoded block and [`finish`](Self::finish) once at the end;
/// `finish` flushes the resampler's delay line, so the output has `ceil(input * 16000 / rate)`
/// samples with no leading silence.
pub struct StreamResampler {
    from_rate: u32,
    /// `None` when the input is already at 16 kHz.
    engine: Option<Box<dyn Resampler<f32>>>,
    /// Input frames waiting for a full chunk.
    pending: Vec<f32>,
    /// One chunk of resampler output.
    scratch: Vec<f32>,
    /// Output frames still to drop: the resampler's start-up delay.
    to_trim: usize,
    frames_in: u64,
    frames_out: u64,
}

impl StreamResampler {
    pub fn new(from_rate: u32) -> Result<Self> {
        if from_rate == 0 {
            return Err(EngineError::Invalid("The audio has a sample rate of 0 Hz".into()));
        }
        let engine: Option<Box<dyn Resampler<f32>>> = if from_rate == TARGET_RATE {
            None
        } else if from_rate / gcd(from_rate, TARGET_RATE) <= MAX_FFT_BLOCK {
            Some(Box::new(Fft::<f32>::new(
                from_rate as usize,
                TARGET_RATE as usize,
                CHUNK_FRAMES,
                1,
                FixedSync::Input,
            )?))
        } else {
            let params = SincInterpolationParameters::new(256, WindowFunction::BlackmanHarris2);
            Some(Box::new(Async::<f32>::new_sinc(
                TARGET_RATE as f64 / from_rate as f64,
                1.0,
                &params,
                CHUNK_FRAMES,
                1,
                FixedAsync::Input,
            )?))
        };
        let (to_trim, pending, scratch) = match &engine {
            Some(e) => (e.output_delay(), Vec::with_capacity(e.input_frames_max()), vec![0.0; e.output_frames_max()]),
            None => (0, Vec::new(), Vec::new()),
        };
        Ok(Self { from_rate, engine, pending, scratch, to_trim, frames_in: 0, frames_out: 0 })
    }

    /// The input sample rate.
    pub fn from_rate(&self) -> u32 {
        self.from_rate
    }

    /// How many output samples `input_frames` of input become.
    pub fn output_len_for(&self, input_frames: u64) -> u64 {
        (input_frames * TARGET_RATE as u64).div_ceil(self.from_rate as u64)
    }

    /// Resample `input`, appending whatever output is ready to `out`.
    pub fn push(&mut self, mut input: &[f32], out: &mut Vec<f32>) -> Result<()> {
        self.frames_in += input.len() as u64;
        let Some(engine) = self.engine.as_mut() else {
            out.extend_from_slice(input);
            self.frames_out += input.len() as u64;
            return Ok(());
        };
        while !input.is_empty() {
            let need = engine.input_frames_next();
            if self.pending.is_empty() && input.len() >= need {
                // A whole chunk is available: resample it in place, without copying.
                let (chunk, rest) = input.split_at(need);
                self.frames_out += run(engine.as_mut(), chunk, None, &mut self.scratch, &mut self.to_trim, out)?;
                input = rest;
            } else {
                let take = (need - self.pending.len()).min(input.len());
                self.pending.extend_from_slice(&input[..take]);
                input = &input[take..];
                if self.pending.len() == need {
                    self.frames_out +=
                        run(engine.as_mut(), &self.pending, None, &mut self.scratch, &mut self.to_trim, out)?;
                    self.pending.clear();
                }
            }
        }
        Ok(())
    }

    /// Resample the buffered tail and flush the delay line. `out` ends up holding exactly
    /// [`output_len_for`](Self::output_len_for) samples from this resampler.
    pub fn finish(mut self, out: &mut Vec<f32>) -> Result<()> {
        let expected = self.output_len_for(self.frames_in);
        let Some(engine) = self.engine.as_mut() else { return Ok(()) };
        if !self.pending.is_empty() {
            let partial = Indexing::new().partial_len(self.pending.len());
            self.frames_out +=
                run(engine.as_mut(), &self.pending, Some(&partial), &mut self.scratch, &mut self.to_trim, out)?;
        }
        // Feed silence until the delayed tail of the real input has come out.
        let silence = Indexing::new().partial_len(0);
        let mut rounds = 0;
        while self.frames_out < expected {
            let produced = run(engine.as_mut(), &[], Some(&silence), &mut self.scratch, &mut self.to_trim, out)?;
            self.frames_out += produced;
            rounds += 1;
            if rounds > 1024 || (produced == 0 && self.to_trim == 0) {
                return Err(EngineError::Engine("The resampler stopped producing audio".into()));
            }
        }
        let extra = (self.frames_out - expected) as usize;
        out.truncate(out.len() - extra);
        Ok(())
    }
}

/// Resample one chunk into `scratch`, drop any start-up delay still owed, and append the rest to
/// `out`. Returns the number of samples appended.
fn run(
    engine: &mut dyn Resampler<f32>,
    input: &[f32],
    indexing: Option<&Indexing>,
    scratch: &mut [f32],
    to_trim: &mut usize,
    out: &mut Vec<f32>,
) -> Result<u64> {
    let adapter_err = |e| EngineError::Engine(format!("Resampler buffer error: {}", e));
    let input = InterleavedSlice::new(input, 1, input.len()).map_err(adapter_err)?;
    let frames = scratch.len();
    let mut output = InterleavedSlice::new_mut(scratch, 1, frames).map_err(adapter_err)?;
    let (_, produced) = engine.process_into_buffer(&input, &mut output, indexing)?;
    let skip = (*to_trim).min(produced);
    *to_trim -= skip;
    out.extend_from_slice(&scratch[skip..produced]);
    Ok((produced - skip) as u64)
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// Resample a whole in-memory mono clip to 16 kHz.
pub fn resample_to_16k_mono(samples: &[f32], from_rate: u32) -> Result<Vec<f32>> {
    let mut resampler = StreamResampler::new(from_rate)?;
    let mut out = Vec::with_capacity(resampler.output_len_for(samples.len() as u64) as usize);
    resampler.push(samples, &mut out)?;
    resampler.finish(&mut out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::test_util::{frequency, sine};

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn common_rates_resample_to_the_right_length_and_pitch() {
        for rate in [8_000, 11_025, 22_050, 32_000, 44_100, 48_000, 96_000] {
            let input = sine(440.0, rate, 2.0);
            let out = resample_to_16k_mono(&input, rate).unwrap();
            let expected = (input.len() as u64 * 16_000).div_ceil(rate as u64) as usize;
            assert_eq!(out.len(), expected, "{rate} Hz");
            let f = frequency(&out, TARGET_RATE);
            assert!((f - 440.0).abs() < 5.0, "{rate} Hz came out at {f} Hz");
            // The start-up delay is trimmed: the signal starts at once, at full level.
            assert!(rms(&out[..800]) > 0.3, "{rate} Hz: leading silence");
            assert!((rms(&out) - 0.5 / 2f32.sqrt()).abs() < 0.02, "{rate} Hz: level {}", rms(&out));
        }
    }

    #[test]
    fn odd_rates_use_the_sinc_resampler() {
        let rate = 44_101; // gcd with 16000 is 1
        assert!(rate / gcd(rate, TARGET_RATE) > MAX_FFT_BLOCK);
        let input = sine(1000.0, rate, 1.5);
        let out = resample_to_16k_mono(&input, rate).unwrap();
        assert_eq!(out.len(), (input.len() as u64 * 16_000).div_ceil(rate as u64) as usize);
        let f = frequency(&out, TARGET_RATE);
        assert!((f - 1000.0).abs() < 10.0, "came out at {f} Hz");
    }

    #[test]
    fn sixteen_khz_passes_through_untouched() {
        let input = sine(300.0, 16_000, 0.5);
        assert_eq!(resample_to_16k_mono(&input, 16_000).unwrap(), input);
    }

    #[test]
    fn block_size_does_not_change_the_result() {
        let input = sine(523.0, 44_100, 1.0);
        let whole = resample_to_16k_mono(&input, 44_100).unwrap();
        for block in [1, 7, 1000, 4096, 5000] {
            let mut r = StreamResampler::new(44_100).unwrap();
            let mut out = Vec::new();
            for piece in input.chunks(block) {
                r.push(piece, &mut out).unwrap();
            }
            r.finish(&mut out).unwrap();
            assert_eq!(out.len(), whole.len(), "block {block}");
            let max_diff = out.iter().zip(&whole).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
            assert!(max_diff < 1e-4, "block {block}: differs by {max_diff}");
        }
    }

    #[test]
    fn tiny_and_empty_inputs() {
        assert!(resample_to_16k_mono(&[], 48_000).unwrap().is_empty());
        assert_eq!(resample_to_16k_mono(&[0.1; 3], 48_000).unwrap().len(), 1);
        assert_eq!(resample_to_16k_mono(&[0.1; 5], 44_100).unwrap().len(), 2);
    }

    #[test]
    fn a_zero_rate_is_invalid() {
        assert!(matches!(StreamResampler::new(0), Err(EngineError::Invalid(_))));
    }

    #[test]
    fn gcd_works() {
        assert_eq!(gcd(44_100, 16_000), 100);
        assert_eq!(gcd(48_000, 16_000), 16_000);
        assert_eq!(gcd(44_101, 16_000), 1);
    }
}
