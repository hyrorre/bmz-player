use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use bmz_audio::backend::cpal::{CpalBackend, CpalOutput, SharedAudioEngine};
use bmz_audio::clock::AudioClock;
use bmz_audio::engine::AudioEngine;
use bmz_audio::loader::LoadedSampleReport;
use bmz_gameplay::session::GameSession;

use crate::config::app_config::{AudioBackend, AudioConfig};
use crate::screens::play_session::PreparedPlaySession;

pub struct AppAudioOutput {
    pub engine: SharedAudioEngine,
    pub output: CpalOutput,
}

pub struct RunningPlaySession {
    pub session: GameSession,
    pub audio: AppAudioOutput,
    pub sample_report: Vec<LoadedSampleReport>,
}

impl AppAudioOutput {
    pub fn clock(&self) -> AudioClock {
        self.output.clock.clone()
    }
}

pub fn open_app_audio_output(config: &AudioConfig, engine: AudioEngine) -> Result<AppAudioOutput> {
    ensure_default_device_supported(config)?;

    let engine = Arc::new(Mutex::new(engine));
    let output = CpalBackend::open_default(Arc::clone(&engine))?;
    Ok(AppAudioOutput { engine, output })
}

pub fn open_prepared_play_audio(
    config: &AudioConfig,
    prepared: PreparedPlaySession,
) -> Result<RunningPlaySession> {
    let audio = open_app_audio_output(config, prepared.audio)?;
    let mut session = prepared.session;
    session.audio_clock = audio.clock();

    Ok(RunningPlaySession { session, audio, sample_report: prepared.sample_report })
}

fn ensure_default_device_supported(config: &AudioConfig) -> Result<()> {
    if !config.output_device.is_empty() {
        bail!("named output devices are not implemented yet: {}", config.output_device);
    }

    match config.backend {
        AudioBackend::Auto
        | AudioBackend::Wasapi
        | AudioBackend::CoreAudio
        | AudioBackend::Alsa
        | AudioBackend::Pulse => Ok(()),
        AudioBackend::Asio => {
            if config.asio_driver.is_empty() {
                Ok(())
            } else {
                bail!("named ASIO drivers are not implemented yet: {}", config.asio_driver)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::app_config::AppConfig;

    #[test]
    fn default_audio_config_can_use_cpal_default_output() {
        let config = AppConfig::default();

        ensure_default_device_supported(&config.audio).unwrap();
    }

    #[test]
    fn named_output_device_is_rejected_until_device_selection_exists() {
        let mut config = AppConfig::default().audio;
        config.output_device = "External DAC".to_string();

        assert!(ensure_default_device_supported(&config).is_err());
    }
}
