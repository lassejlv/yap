pub mod encode;
pub mod levels;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use parking_lot::Mutex;
use std::sync::Arc;

pub use encode::{wav_bytes, TARGET_SAMPLE_RATE};
pub use levels::{AudioLevels, BARS};

pub struct AudioRecorder {
    shared: Arc<Shared>,
    _stream: Stream,
}

struct Shared {
    samples: Mutex<Vec<i16>>,
    levels: Arc<AudioLevels>,
    input_sample_rate: u32,
    channels: u16,
}

impl AudioRecorder {
    pub fn start(levels: Arc<AudioLevels>) -> Result<Self> {
        levels.reset();

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("no default input device"))?;
        let supported = device
            .default_input_config()
            .context("default input config")?;

        let input_sample_rate = supported.sample_rate().0;
        let channels = supported.channels();
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();

        let shared = Arc::new(Shared {
            samples: Mutex::new(Vec::with_capacity(input_sample_rate as usize * 8)),
            levels,
            input_sample_rate,
            channels,
        });

        let err_fn = |e| tracing::error!(error = ?e, "audio stream error");

        let stream = match sample_format {
            SampleFormat::F32 => {
                let shared = shared.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _| append_f32(&shared, data),
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I16 => {
                let shared = shared.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| append_i16(&shared, data),
                    err_fn,
                    None,
                )?
            }
            SampleFormat::U16 => {
                let shared = shared.clone();
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| append_u16(&shared, data),
                    err_fn,
                    None,
                )?
            }
            other => return Err(anyhow!("unsupported sample format: {other:?}")),
        };

        stream.play()?;
        tracing::debug!(input_sample_rate, channels, "recording started");

        Ok(Self {
            shared,
            _stream: stream,
        })
    }

    pub fn stop(self) -> Result<Vec<i16>> {
        let samples = std::mem::take(&mut *self.shared.samples.lock());
        Ok(resample_linear(
            &samples,
            self.shared.input_sample_rate,
            TARGET_SAMPLE_RATE,
        ))
    }
}

fn append_f32(shared: &Shared, data: &[f32]) {
    let step = shared.channels.max(1) as usize;
    let mut chunk: Vec<i16> = Vec::with_capacity(data.len() / step);
    for frame in data.chunks_exact(step) {
        let mono = frame.iter().copied().sum::<f32>() / step as f32;
        chunk.push((mono.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
    }
    shared.levels.ingest(&chunk);
    shared.samples.lock().extend_from_slice(&chunk);
}

fn append_i16(shared: &Shared, data: &[i16]) {
    let step = shared.channels.max(1) as usize;
    let mut chunk: Vec<i16> = Vec::with_capacity(data.len() / step);
    for frame in data.chunks_exact(step) {
        let sum: i32 = frame.iter().map(|s| *s as i32).sum();
        chunk.push((sum / step as i32) as i16);
    }
    shared.levels.ingest(&chunk);
    shared.samples.lock().extend_from_slice(&chunk);
}

fn append_u16(shared: &Shared, data: &[u16]) {
    let step = shared.channels.max(1) as usize;
    let mut chunk: Vec<i16> = Vec::with_capacity(data.len() / step);
    for frame in data.chunks_exact(step) {
        let sum: i32 = frame
            .iter()
            .map(|s| *s as i32 - i16::MAX as i32 - 1)
            .sum();
        chunk.push((sum / step as i32) as i16);
    }
    shared.levels.ingest(&chunk);
    shared.samples.lock().extend_from_slice(&chunk);
}

fn resample_linear(input: &[i16], from_hz: u32, to_hz: u32) -> Vec<i16> {
    if from_hz == to_hz || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from_hz as f64 / to_hz as f64;
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 * ratio;
        let idx = src.floor() as usize;
        let frac = src - idx as f64;
        let a = input[idx.min(input.len() - 1)] as f64;
        let b = input[(idx + 1).min(input.len() - 1)] as f64;
        out.push((a + (b - a) * frac).round() as i16);
    }
    out
}
