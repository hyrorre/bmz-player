use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use ::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ::cpal::{SampleFormat, StreamConfig};
use bmz_core::time::TimeUs;
use thiserror::Error;

use crate::clock::AudioClock;
use crate::engine::AudioEngine;

pub type SharedAudioEngine = Arc<Mutex<AudioEngine>>;

#[derive(Debug, Default)]
pub struct CpalBackend;

pub struct CpalOutput {
    stream: ::cpal::Stream,
    pub clock: AudioClock,
}

#[derive(Debug, Error)]
pub enum CpalBackendError {
    #[error("no default output device is available")]
    MissingDefaultOutputDevice,

    #[error("failed to query default output config")]
    DefaultOutputConfig(#[from] ::cpal::DefaultStreamConfigError),

    #[error("failed to build output stream")]
    BuildStream(#[from] ::cpal::BuildStreamError),

    #[error("failed to play output stream")]
    PlayStream(#[from] ::cpal::PlayStreamError),
}

impl CpalBackend {
    pub fn open_default(engine: SharedAudioEngine) -> Result<CpalOutput, CpalBackendError> {
        let host = ::cpal::default_host();
        let device =
            host.default_output_device().ok_or(CpalBackendError::MissingDefaultOutputDevice)?;
        let supported_config = device.default_output_config()?;
        let sample_format = supported_config.sample_format();
        let config = supported_config.config();
        let sample_rate = config.sample_rate.0;
        let current_frame = Arc::new(AtomicU64::new(0));

        if let Ok(mut engine) = engine.lock() {
            engine.mixer.output_sample_rate = sample_rate;
        }

        let stream = match sample_format {
            SampleFormat::F32 => build_output_stream::<f32>(
                &device,
                &config,
                Arc::clone(&engine),
                Arc::clone(&current_frame),
            )?,
            SampleFormat::I16 => build_output_stream::<i16>(
                &device,
                &config,
                Arc::clone(&engine),
                Arc::clone(&current_frame),
            )?,
            SampleFormat::U16 => build_output_stream::<u16>(
                &device,
                &config,
                Arc::clone(&engine),
                Arc::clone(&current_frame),
            )?,
            _ => build_output_stream::<f32>(&device, &config, engine, Arc::clone(&current_frame))?,
        };

        let clock = AudioClock::with_position(sample_rate, 0, 0, current_frame, false);

        Ok(CpalOutput { stream, clock })
    }
}

impl CpalOutput {
    pub fn play(&mut self, chart_zero_time: TimeUs) -> Result<(), CpalBackendError> {
        self.clock.chart_zero_time_us = chart_zero_time.0;
        self.clock.start_output_frame = self.clock.current_frame.load(Ordering::Relaxed);
        self.clock.running = true;
        self.stream.play()?;
        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), ::cpal::PauseStreamError> {
        self.clock.running = false;
        self.stream.pause()
    }
}

fn build_output_stream<T>(
    device: &::cpal::Device,
    config: &StreamConfig,
    engine: SharedAudioEngine,
    current_frame: Arc<AtomicU64>,
) -> Result<::cpal::Stream, ::cpal::BuildStreamError>
where
    T: ::cpal::SizedSample + OutputSample,
{
    let channels = config.channels as usize;
    device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            if channels == 0 {
                data.fill(T::from_f32(0.0));
                return;
            }

            let start_frame = current_frame.load(Ordering::Relaxed);
            render_output(data, channels, start_frame, &engine);
            current_frame.fetch_add((data.len() / channels) as u64, Ordering::Relaxed);
        },
        move |error| {
            tracing::warn!(%error, "cpal output stream error");
        },
        None,
    )
}

fn render_output<T: OutputSample>(
    data: &mut [T],
    channels: usize,
    output_start_frame: u64,
    engine: &SharedAudioEngine,
) {
    if channels == 0 {
        return;
    }

    let frames = data.len() / channels;
    let mut stereo = vec![0.0; frames * 2];
    if let Ok(mut engine) = engine.lock() {
        engine.render_stereo(output_start_frame, &mut stereo);
    }

    write_interleaved_output(data, channels, &stereo);
}

fn write_interleaved_output<T: OutputSample>(data: &mut [T], channels: usize, stereo: &[f32]) {
    if channels == 0 {
        return;
    }

    for (frame_index, frame) in data.chunks_mut(channels).enumerate() {
        let left = stereo.get(frame_index * 2).copied().unwrap_or(0.0);
        let right = stereo.get(frame_index * 2 + 1).copied().unwrap_or(0.0);
        match frame {
            [mono] => *mono = T::from_f32((left + right) * 0.5),
            [left_out, right_out, rest @ ..] => {
                *left_out = T::from_f32(left);
                *right_out = T::from_f32(right);
                for sample in rest {
                    *sample = T::from_f32(0.0);
                }
            }
            [] => {}
        }
    }
}

trait OutputSample: Copy {
    fn from_f32(value: f32) -> Self;
}

impl OutputSample for f32 {
    fn from_f32(value: f32) -> Self {
        value.clamp(-1.0, 1.0)
    }
}

impl OutputSample for i16 {
    fn from_f32(value: f32) -> Self {
        (value.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
    }
}

impl OutputSample for u16 {
    fn from_f32(value: f32) -> Self {
        ((value.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_interleaved_output_downmixes_mono() {
        let mut output = vec![0.0_f32; 2];

        write_interleaved_output(&mut output, 1, &[0.25, 0.75, -0.5, 0.25]);

        assert_eq!(output, vec![0.5, -0.125]);
    }

    #[test]
    fn write_interleaved_output_fills_extra_channels_with_silence() {
        let mut output = vec![1.0_f32; 6];

        write_interleaved_output(&mut output, 3, &[0.25, 0.75, -0.5, 0.25]);

        assert_eq!(output, vec![0.25, 0.75, 0.0, -0.5, 0.25, 0.0]);
    }
}
