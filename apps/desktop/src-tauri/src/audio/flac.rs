//! A small FLAC encoder for exports, with no dependencies beyond `hound` (already used for WAV).
//!
//! FLAC is lossless: the exported file decodes to exactly the samples of the WAV it came from,
//! at about half the size for speech. Each block of 4096 samples is coded per channel as a
//! constant, a fixed polynomial predictor (orders 0-4) with partitioned Rice residuals, or
//! verbatim, whichever is smallest. That is FLAC's "fast" subset: no LPC, no stereo decorrelation,
//! which costs a few percent of size and keeps the code short enough to read in one sitting.
//!
//! Encoding streams block by block, so a long recording costs no more memory than a short one.

use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use crate::error::{AppError, Result};

/// Samples per channel in every frame but the last.
pub const BLOCK_SIZE: usize = 4096;
const MAX_FIXED_ORDER: usize = 4;
const MAX_PARTITION_ORDER: u32 = 8;
/// Rice parameters are written in 4 bits, and 15 is reserved as the escape code.
const MAX_RICE_PARAM: u32 = 14;
/// "fLaC" plus the 4-byte metadata block header: where STREAMINFO's 34 bytes start.
const STREAMINFO_OFFSET: u64 = 8;

/// Convert a WAV file to FLAC, writing it to `out`. 8, 16 and 24-bit integer WAVs keep their
/// depth; 32-bit integer WAVs are stored at 24 bits and float WAVs at 16 bits.
pub fn wav_to_flac<W: Write + Seek>(src: &Path, out: W) -> Result<()> {
    let mut reader = hound::WavReader::open(src)?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    if channels == 0 || channels > 8 {
        return Err(AppError::Audio(format!("FLAC supports 1 to 8 channels, this file has {}", channels)));
    }
    if spec.sample_rate == 0 || spec.sample_rate >= 1 << 20 {
        return Err(AppError::Audio(format!("Unsupported sample rate for FLAC: {} Hz", spec.sample_rate)));
    }

    match spec.sample_format {
        hound::SampleFormat::Int => {
            let (bits, shift) = match spec.bits_per_sample {
                0..=8 => (8, 0),
                9..=16 => (16, 0),
                17..=24 => (24, 0),
                b => (24, b as u32 - 24),
            };
            let samples = reader.samples::<i32>().map(|s| s.map(|v| v >> shift));
            encode(samples, channels, bits, spec.sample_rate, out)
        }
        hound::SampleFormat::Float => {
            let samples = reader.samples::<f32>().map(|s| {
                s.map(|v| if v.is_finite() { (v.clamp(-1.0, 1.0) * 32767.0).round() as i32 } else { 0 })
            });
            encode(samples, channels, 16, spec.sample_rate, out)
        }
    }
}

fn encode<W, I>(samples: I, channels: usize, bits: u32, rate: u32, out: W) -> Result<()>
where
    W: Write + Seek,
    I: Iterator<Item = std::result::Result<i32, hound::Error>>,
{
    let mut encoder = Encoder::new(out, channels, bits, rate)?;
    let mut interleaved = Vec::with_capacity(BLOCK_SIZE * channels);
    for sample in samples {
        interleaved.push(sample?);
        if interleaved.len() == BLOCK_SIZE * channels {
            encoder.write_block(&interleaved)?;
            interleaved.clear();
        }
    }
    // A truncated WAV can end mid-frame: keep whole frames only.
    interleaved.truncate(interleaved.len() / channels * channels);
    if !interleaved.is_empty() {
        encoder.write_block(&interleaved)?;
    }
    encoder.finish()
}

/// Streaming FLAC writer. STREAMINFO is written first and patched by [`Encoder::finish`] with
/// the totals, which is why the output must be seekable.
pub struct Encoder<W: Write + Seek> {
    out: W,
    channels: usize,
    bits: u32,
    rate: u32,
    frames: u64,
    total_samples: u64,
    min_frame: u32,
    max_frame: u32,
    // Reused between blocks.
    channel: Vec<i32>,
    residual: Vec<u32>,
}

impl<W: Write + Seek> Encoder<W> {
    pub fn new(mut out: W, channels: usize, bits: u32, rate: u32) -> Result<Self> {
        if !(1..=8).contains(&channels) || !matches!(bits, 8 | 16 | 24) || rate == 0 || rate >= 1 << 20 {
            return Err(AppError::Audio(format!(
                "Unsupported FLAC format: {} channel(s), {} bits, {} Hz",
                channels, bits, rate
            )));
        }
        out.write_all(b"fLaC")?;
        // Last metadata block (1), type STREAMINFO (0), 34 bytes long.
        out.write_all(&[0x80, 0, 0, 34])?;
        out.write_all(&streaminfo(channels, bits, rate, 0, 0, 0))?;
        Ok(Self {
            out,
            channels,
            bits,
            rate,
            frames: 0,
            total_samples: 0,
            min_frame: 0,
            max_frame: 0,
            channel: Vec::with_capacity(BLOCK_SIZE),
            residual: Vec::with_capacity(BLOCK_SIZE),
        })
    }

    /// Encode one frame from interleaved samples: at most [`BLOCK_SIZE`] per channel, and only
    /// the last block of a stream may be shorter.
    pub fn write_block(&mut self, interleaved: &[i32]) -> Result<()> {
        let n = interleaved.len() / self.channels;
        if n == 0 || n > BLOCK_SIZE || n * self.channels != interleaved.len() {
            return Err(AppError::Audio("A FLAC block must hold whole frames, at most 4096 per channel".into()));
        }
        let mut bw = BitWriter::with_capacity(n * self.channels * self.bits as usize / 8 + 64);

        // Frame header: sync code, fixed block size, then the coded parameters.
        bw.write(0b11_1111_1111_1110, 14);
        bw.write(0, 1);
        bw.write(0, 1);
        bw.write(0b0111, 4); // block size: 16-bit (n - 1) at the end of the header
        let (rate_code, rate_extra) = rate_code(self.rate);
        bw.write(rate_code, 4);
        bw.write(self.channels as u64 - 1, 4); // independent channels
        bw.write(sample_size_code(self.bits), 3);
        bw.write(0, 1);
        for byte in utf8_number(self.frames) {
            bw.write(byte as u64, 8);
        }
        bw.write(n as u64 - 1, 16);
        if let Some((value, len)) = rate_extra {
            bw.write(value, len);
        }
        let crc = crc8(&bw.bytes);
        bw.write(crc as u64, 8);

        for ch in 0..self.channels {
            self.channel.clear();
            self.channel.extend(interleaved.iter().skip(ch).step_by(self.channels));
            write_subframe(&mut bw, &self.channel, self.bits, &mut self.residual);
        }
        bw.align();
        let crc = crc16(&bw.bytes);
        bw.write(crc as u64, 16);

        self.out.write_all(&bw.bytes)?;
        let size = bw.bytes.len() as u32;
        self.min_frame = if self.frames == 0 { size } else { self.min_frame.min(size) };
        self.max_frame = self.max_frame.max(size);
        self.frames += 1;
        self.total_samples += n as u64;
        Ok(())
    }

    /// Patch STREAMINFO with the totals and flush.
    pub fn finish(mut self) -> Result<()> {
        let info = streaminfo(self.channels, self.bits, self.rate, self.total_samples, self.min_frame, self.max_frame);
        self.out.seek(SeekFrom::Start(STREAMINFO_OFFSET))?;
        self.out.write_all(&info)?;
        self.out.seek(SeekFrom::End(0))?;
        self.out.flush()?;
        Ok(())
    }
}

/// Convenience for writing to a file path.
pub fn wav_file_to_flac_file(src: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::create(dest)?;
    let out = std::io::BufWriter::new(file);
    wav_to_flac(src, out).inspect_err(|_| {
        let _ = std::fs::remove_file(dest);
    })
}

/// Encode a whole FLAC export in memory.
#[cfg(test)]
pub fn wav_to_flac_bytes(src: &Path) -> Result<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    wav_to_flac(src, &mut out)?;
    Ok(out.into_inner())
}

fn streaminfo(channels: usize, bits: u32, rate: u32, total: u64, min_frame: u32, max_frame: u32) -> [u8; 34] {
    let mut bw = BitWriter::with_capacity(34);
    bw.write(BLOCK_SIZE as u64, 16); // min block size (the last block may be shorter)
    bw.write(BLOCK_SIZE as u64, 16); // max block size
    bw.write(min_frame as u64, 24);
    bw.write(max_frame as u64, 24);
    bw.write(rate as u64, 20);
    bw.write(channels as u64 - 1, 3);
    bw.write(bits as u64 - 1, 5);
    bw.write(total >> 32, 4);
    bw.write(total & 0xFFFF_FFFF, 32);
    // MD5 of the audio: all zeros means "not computed", which every decoder accepts.
    for _ in 0..4 {
        bw.write(0, 32);
    }
    let mut out = [0u8; 34];
    out.copy_from_slice(&bw.bytes);
    out
}

/// The header's sample-rate code, and the extra bits some codes need after the block size.
fn rate_code(rate: u32) -> (u64, Option<(u64, u32)>) {
    match rate {
        88_200 => (0b0001, None),
        176_400 => (0b0010, None),
        192_000 => (0b0011, None),
        8_000 => (0b0100, None),
        16_000 => (0b0101, None),
        22_050 => (0b0110, None),
        24_000 => (0b0111, None),
        32_000 => (0b1000, None),
        44_100 => (0b1001, None),
        48_000 => (0b1010, None),
        96_000 => (0b1011, None),
        r if r.is_multiple_of(1000) && r / 1000 <= 255 => (0b1100, Some(((r / 1000) as u64, 8))),
        r if r <= 0xFFFF => (0b1101, Some((r as u64, 16))),
        r if r.is_multiple_of(10) && r / 10 <= 0xFFFF => (0b1110, Some(((r / 10) as u64, 16))),
        _ => (0b0000, None), // from STREAMINFO
    }
}

fn sample_size_code(bits: u32) -> u64 {
    match bits {
        8 => 0b001,
        16 => 0b100,
        _ => 0b110, // 24
    }
}

/// FLAC codes frame numbers like UTF-8, extended to 36 bits.
fn utf8_number(v: u64) -> Vec<u8> {
    if v < 0x80 {
        return vec![v as u8];
    }
    let len = match v {
        _ if v < 0x800 => 2,
        _ if v < 0x1_0000 => 3,
        _ if v < 0x20_0000 => 4,
        _ if v < 0x400_0000 => 5,
        _ if v < 0x8000_0000 => 6,
        _ => 7,
    };
    let mut out = vec![0u8; len];
    let mut rest = v;
    for byte in out.iter_mut().skip(1).rev() {
        *byte = 0x80 | (rest & 0x3F) as u8;
        rest >>= 6;
    }
    out[0] = if len == 7 { 0xFE } else { !(0xFFu8 >> len) | rest as u8 };
    out
}

fn write_subframe(bw: &mut BitWriter, x: &[i32], bits: u32, residual: &mut Vec<u32>) {
    let n = x.len();

    // Silence (and any other constant run) costs a few bytes.
    if x.iter().all(|&s| s == x[0]) {
        bw.write(0, 8); // padding bit, type CONSTANT, no wasted bits
        bw.write_signed(x[0] as i64, bits);
        return;
    }

    // Pick the fixed predictor whose residual is smallest in absolute sum (libFLAC's heuristic).
    let max_order = MAX_FIXED_ORDER.min(n - 1);
    let mut sums = [0u64; MAX_FIXED_ORDER + 1];
    for i in 0..n {
        for (order, sum) in sums.iter_mut().enumerate().take(max_order + 1) {
            if i >= order {
                *sum += fixed_residual(x, i, order).unsigned_abs();
            }
        }
    }
    let order = (0..=max_order).min_by_key(|&o| sums[o]).unwrap_or(0);

    residual.clear();
    residual.extend((order..n).map(|i| zigzag(fixed_residual(x, i, order))));
    let (partition_order, params, residual_bits) = best_partitioning(residual, n, order);

    let fixed_bits = 8 + order as u64 * bits as u64 + 6 + residual_bits;
    let verbatim_bits = 8 + n as u64 * bits as u64;
    if verbatim_bits <= fixed_bits {
        bw.write(0b0000_0010, 8); // type VERBATIM
        for &s in x {
            bw.write_signed(s as i64, bits);
        }
        return;
    }

    bw.write((0b001000 | order as u64) << 1, 8); // type FIXED with this order
    for &s in &x[..order] {
        bw.write_signed(s as i64, bits);
    }
    bw.write(0b00, 2); // residual coding: Rice with 4-bit parameters
    bw.write(partition_order as u64, 4);
    let partitions = 1usize << partition_order;
    let per = n >> partition_order;
    let mut start = 0;
    for (p, &k) in params.iter().enumerate().take(partitions) {
        let len = if p == 0 { per - order } else { per };
        bw.write(k as u64, 4);
        for &u in &residual[start..start + len] {
            bw.write_rice(u, k);
        }
        start += len;
    }
}

fn fixed_residual(x: &[i32], i: usize, order: usize) -> i64 {
    let s = |k: usize| x[i - k] as i64;
    match order {
        0 => s(0),
        1 => s(0) - s(1),
        2 => s(0) - 2 * s(1) + s(2),
        3 => s(0) - 3 * s(1) + 3 * s(2) - s(3),
        _ => s(0) - 4 * s(1) + 6 * s(2) - 4 * s(3) + s(4),
    }
}

fn zigzag(r: i64) -> u32 {
    ((r << 1) ^ (r >> 63)) as u32
}

/// Choose the partition order and per-partition Rice parameters with the smallest estimated
/// size. Returns (partition order, parameters, residual bits including parameter fields).
fn best_partitioning(residual: &[u32], n: usize, order: usize) -> (u32, Vec<u32>, u64) {
    let mut best: Option<(u32, Vec<u32>, u64)> = None;
    for p in 0..=MAX_PARTITION_ORDER {
        let partitions = 1usize << p;
        if !n.is_multiple_of(partitions) || (n >> p) <= order {
            break;
        }
        let per = n >> p;
        let mut params = Vec::with_capacity(partitions);
        let mut total = 0u64;
        let mut start = 0;
        for i in 0..partitions {
            let len = if i == 0 { per - order } else { per };
            let slice = &residual[start..start + len];
            start += len;
            let (k, cost) = rice_param(slice);
            params.push(k);
            total += 4 + cost;
        }
        if best.as_ref().is_none_or(|(_, _, b)| total < *b) {
            best = Some((p, params, total));
        }
    }
    best.unwrap_or((0, vec![0], 4))
}

/// The best Rice parameter for a partition and its exact cost in bits.
fn rice_param(values: &[u32]) -> (u32, u64) {
    if values.is_empty() {
        return (0, 0);
    }
    let sum: u64 = values.iter().map(|&u| u as u64).sum();
    let mean = sum / values.len() as u64;
    let guess = if mean == 0 { 0 } else { 63 - mean.leading_zeros() };
    let cost = |k: u32| values.iter().map(|&u| (u >> k) as u64 + 1 + k as u64).sum::<u64>();
    // Loud 24-bit audio can call for more than the largest 4-bit parameter: clamp both ends,
    // so the range is never empty (an empty one left the cost at u64::MAX).
    let low = guess.saturating_sub(1).min(MAX_RICE_PARAM);
    let high = (guess + 1).min(MAX_RICE_PARAM);
    let mut best = (low, u64::MAX);
    for k in low..=high {
        let c = cost(k);
        if c < best.1 {
            best = (k, c);
        }
    }
    best
}

struct BitWriter {
    bytes: Vec<u8>,
    acc: u64,
    filled: u32,
}

impl BitWriter {
    fn with_capacity(bytes: usize) -> Self {
        Self { bytes: Vec::with_capacity(bytes), acc: 0, filled: 0 }
    }

    /// Append the low `bits` bits of `value` (at most 32 at a time), most significant first.
    fn write(&mut self, value: u64, bits: u32) {
        debug_assert!(bits <= 32);
        if bits == 0 {
            return;
        }
        self.acc = (self.acc << bits) | (value & ((1u64 << bits) - 1));
        self.filled += bits;
        while self.filled >= 8 {
            self.filled -= 8;
            self.bytes.push((self.acc >> self.filled) as u8);
        }
        self.acc &= (1u64 << self.filled) - 1;
    }

    fn write_signed(&mut self, value: i64, bits: u32) {
        self.write(value as u64, bits);
    }

    fn write_rice(&mut self, u: u32, k: u32) {
        let mut q = u >> k;
        while q >= 32 {
            self.write(0, 32);
            q -= 32;
        }
        self.write(1, q + 1); // q zeros, then a one
        self.write(u as u64, k);
    }

    fn align(&mut self) {
        if self.filled > 0 {
            self.write(0, 8 - self.filled);
        }
    }
}

fn crc8(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |crc, &b| {
        let mut c = crc ^ b;
        for _ in 0..8 {
            c = if c & 0x80 != 0 { (c << 1) ^ 0x07 } else { c << 1 };
        }
        c
    })
}

fn crc16(data: &[u8]) -> u16 {
    data.iter().fold(0u16, |crc, &b| {
        let mut c = crc ^ ((b as u16) << 8);
        for _ in 0..8 {
            c = if c & 0x8000 != 0 { (c << 1) ^ 0x8005 } else { c << 1 };
        }
        c
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;

    /// Decode FLAC bytes with symphonia, an independent decoder: (rate, channels, interleaved).
    fn decode(bytes: Vec<u8>) -> (u32, usize, Vec<i32>) {
        let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("flac");
        let mut format = symphonia::default::get_probe()
            .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
            .expect("probe");
        let track = format.default_track(TrackType::Audio).expect("track");
        let track_id = track.id;
        let params = track.codec_params.as_ref().and_then(|p| p.audio()).expect("audio params").clone();
        let rate = params.sample_rate.expect("rate");
        let mut decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
            .expect("decoder");
        let mut out = Vec::new();
        let mut channels = 0;
        let mut buf: Vec<i32> = Vec::new();
        while let Some(packet) = format.next_packet().expect("packet") {
            if packet.track_id != track_id {
                continue;
            }
            let decoded = decoder.decode(&packet).expect("decode");
            channels = decoded.spec().channels().count();
            decoded.copy_to_vec_interleaved(&mut buf);
            out.extend_from_slice(&buf);
        }
        (rate, channels, out)
    }

    fn write_wav(path: &Path, channels: u16, bits: u16, rate: u32, samples: &[i32]) {
        let spec = hound::WavSpec { channels, sample_rate: rate, bits_per_sample: bits, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for &s in samples {
            match bits {
                8 => w.write_sample(s as i8).unwrap(),
                16 => w.write_sample(s as i16).unwrap(),
                _ => w.write_sample(s).unwrap(),
            }
        }
        w.finalize().unwrap();
    }

    /// Speech-like test signal: tones, noise, a silent stretch and full-scale peaks.
    fn signal(len: usize, amplitude: f64, seed: u64) -> Vec<i32> {
        let mut state = seed;
        (0..len)
            .map(|i| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let noise = ((state >> 33) as f64 / (1u64 << 31) as f64 - 0.5) * 0.004;
                let t = i as f64 / 24_000.0;
                let v = if (5_000..9_000).contains(&i) {
                    0.0
                } else if i % 7919 == 0 {
                    1.0
                } else {
                    0.6 * (t * 2.0 * std::f64::consts::PI * 220.0).sin() + 0.2 * (t * 2.0 * std::f64::consts::PI * 1375.0).sin() + noise
                };
                (v.clamp(-1.0, 1.0) * amplitude).round() as i32
            })
            .collect()
    }

    fn round_trip(channels: u16, bits: u16, rate: u32, samples: Vec<i32>) -> Vec<u8> {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("in.wav");
        write_wav(&wav, channels, bits, rate, &samples);
        let bytes = wav_to_flac_bytes(&wav).unwrap();
        assert_eq!(&bytes[..4], b"fLaC");
        let (got_rate, got_channels, decoded) = decode(bytes.clone());
        assert_eq!(got_rate, rate);
        assert_eq!(got_channels, channels as usize);
        // symphonia hands back samples scaled to i32's full range.
        let shift = 32 - bits as u32;
        let decoded: Vec<i32> = decoded.iter().map(|&s| s >> shift).collect();
        assert_eq!(decoded.len(), samples.len());
        assert!(decoded == samples, "decoded samples differ from the source");
        bytes
    }

    #[test]
    fn mono_16_bit_is_lossless_and_smaller() {
        let samples = signal(24_000 * 3 + 123, 32767.0, 1);
        let flac = round_trip(1, 16, 24_000, samples.clone());
        let wav_bytes = samples.len() * 2;
        assert!(flac.len() < wav_bytes * 3 / 4, "{} vs {}", flac.len(), wav_bytes);
    }

    #[test]
    fn stereo_24_bit_and_unusual_rates() {
        let left = signal(10_000, 8_388_607.0, 2);
        let right = signal(10_000, 4_000_000.0, 3);
        let interleaved: Vec<i32> = left.iter().zip(&right).flat_map(|(&l, &r)| [l, r]).collect();
        round_trip(2, 24, 44_100, interleaved.clone());
        round_trip(2, 24, 11_025, interleaved.clone()); // 16-bit Hz code
        round_trip(2, 24, 37_000, interleaved); // kHz code
    }

    #[test]
    fn eight_bit_and_tiny_inputs() {
        round_trip(1, 8, 8_000, signal(3_000, 127.0, 4));
        round_trip(1, 16, 16_000, vec![12_345]);
        round_trip(1, 16, 16_000, vec![-1, 1]);
        round_trip(1, 16, 16_000, vec![0; BLOCK_SIZE]); // all constant
        round_trip(1, 16, 16_000, signal(BLOCK_SIZE * 2, 30_000.0, 5)); // exact block multiple
    }

    #[test]
    fn white_noise_falls_back_to_verbatim_and_stays_exact() {
        let mut state = 9u64;
        let noise: Vec<i32> = (0..20_000)
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                ((state >> 48) as i64 - 32_768) as i32
            })
            .collect();
        round_trip(1, 16, 22_050, noise);
    }

    #[test]
    fn many_frames_need_multi_byte_frame_numbers() {
        // More than 128 frames, so frame numbers take two UTF-8 style bytes.
        round_trip(1, 16, 8_000, signal(BLOCK_SIZE * 130 + 7, 20_000.0, 6));
    }

    #[test]
    fn float_wavs_become_16_bit() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("f.wav");
        let spec = hound::WavSpec { channels: 1, sample_rate: 24_000, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
        let mut w = hound::WavWriter::create(&wav, spec).unwrap();
        for s in [0.0f32, 0.5, -0.5, 1.5, f32::NAN, -1.0] {
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();
        let (_, _, decoded) = decode(wav_to_flac_bytes(&wav).unwrap());
        let decoded: Vec<i32> = decoded.iter().map(|&s| s >> 16).collect();
        assert_eq!(decoded, vec![0, 16_384, -16_384, 32_767, 0, -32_767]);
    }

    #[test]
    fn frame_numbers_and_crcs() {
        assert_eq!(utf8_number(0), vec![0]);
        assert_eq!(utf8_number(0x7F), vec![0x7F]);
        assert_eq!(utf8_number(0x80), vec![0xC2, 0x80]);
        assert_eq!(utf8_number(0x7FF), vec![0xDF, 0xBF]);
        assert_eq!(utf8_number(0x800), vec![0xE0, 0xA0, 0x80]);
        assert_eq!(utf8_number(0xF_FFFF_FFFF).len(), 7);
        // Check values from the FLAC test suite: CRC-8 of "123456789" is 0xF4, CRC-16 0xFEE8.
        assert_eq!(crc8(b"123456789"), 0xF4);
        assert_eq!(crc16(b"123456789"), 0xFEE8);
    }

    #[test]
    fn rejects_what_flac_cannot_hold() {
        assert!(Encoder::new(Cursor::new(Vec::new()), 9, 16, 44_100).is_err());
        assert!(Encoder::new(Cursor::new(Vec::new()), 1, 12, 44_100).is_err());
        assert!(Encoder::new(Cursor::new(Vec::new()), 1, 16, 0).is_err());
        let mut e = Encoder::new(Cursor::new(Vec::new()), 2, 16, 44_100).unwrap();
        assert!(e.write_block(&[1, 2, 3]).is_err());
        assert!(e.write_block(&[]).is_err());
    }
}
