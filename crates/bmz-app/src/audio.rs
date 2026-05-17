use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use bmz_audio::backend::cpal::{CpalBackend, CpalOutput, SharedAudioEngine};
use bmz_audio::clock::AudioClock;
use bmz_audio::engine::AudioEngine;
use bmz_audio::loader::LoadedSampleReport;
use bmz_core::time::TimeUs;
use bmz_gameplay::session::GameSession;

use crate::config::app_config::{AudioBackend, AudioConfig};
use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::play_session::PreparedPlaySession;
use crate::screens::play_snapshot::BgaFrameCatalog;

pub struct AppAudioOutput {
    pub engine: SharedAudioEngine,
    pub output: CpalOutput,
}

pub struct RunningPlaySession {
    pub session: GameSession,
    pub audio: AppAudioOutput,
    pub sample_report: Vec<LoadedSampleReport>,
    pub finished: Option<FinishedPlaySession>,
    pub audio_paused_after_finish: bool,
    /// プレイ開始時に DB から取得したベスト EX スコア。未取得なら None。
    pub best_ex_score: Option<u32>,
    pub bga_frames: BgaFrameCatalog,
}

impl AppAudioOutput {
    pub fn clock(&self) -> AudioClock {
        self.output.clock.clone()
    }

    pub fn pause(&mut self) -> Result<()> {
        self.output.pause().context("failed to pause audio output")
    }

    pub fn play(&mut self, chart_zero_time: TimeUs) -> Result<()> {
        self.output.play(chart_zero_time).context("failed to start audio output")
    }
}

impl RunningPlaySession {
    pub fn start(&mut self, chart_zero_time: TimeUs) -> Result<()> {
        self.audio.play(chart_zero_time)?;
        self.session.audio_clock = self.audio.clock();
        self.audio_paused_after_finish = false;
        Ok(())
    }

    pub fn pause_audio(&mut self) -> Result<()> {
        self.audio.pause()?;
        self.session.audio_clock = self.audio.clock();
        Ok(())
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

    Ok(RunningPlaySession {
        session,
        audio,
        sample_report: prepared.sample_report,
        finished: None,
        audio_paused_after_finish: false,
        best_ex_score: None,
        bga_frames: BgaFrameCatalog::new(),
    })
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
