use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use bmz_audio::backend::cpal::{
    CpalBackend, CpalOutputSource, CpalSharedOutput, SharedAudioEngine,
};
use bmz_audio::clock::AudioClock;
use bmz_audio::engine::AudioEngine;
use bmz_audio::loader::LoadedSampleReport;
use bmz_chart::model::BgaAssetId;
use bmz_core::time::TimeUs;
use bmz_gameplay::session::GameSession;

use crate::config::app_config::{AudioBackend, AudioConfig};
use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::play_session::{AppliedArrange, PreparedPlaySession};
use crate::screens::play_snapshot::BgaFrameCatalog;
use crate::screens::result_model::ResultGraphCollector;
use crate::video_bga::ActiveVideoBgaDecoder;

pub struct AppAudioOutput {
    pub engine: SharedAudioEngine,
    _runtime: AudioRuntime,
    source: CpalOutputSource,
}

#[derive(Clone)]
pub struct AudioRuntime {
    output: CpalSharedOutput,
}

pub struct RunningPlaySession {
    pub session: GameSession,
    pub audio: AppAudioOutput,
    pub sample_report: Vec<LoadedSampleReport>,
    pub finished: Option<FinishedPlaySession>,
    pub result_graph: ResultGraphCollector,
    /// プレイ開始時に DB から取得したベスト EX スコア。未取得なら None。
    pub best_ex_score: Option<u32>,
    /// プレイ開始時に DB から取得した beatoraja 互換 ghost。
    pub best_ghost: Option<Vec<u8>>,
    /// プレイ開始時のターゲット設定を譜面ノーツ数で解決した EX スコア。
    pub target_ex_score: Option<u32>,
    pub applied_arrange: AppliedArrange,
    pub practice_mode: bool,
    pub bga_frames: BgaFrameCatalog,
    pub video_bga_decoders: HashMap<BgaAssetId, ActiveVideoBgaDecoder>,
    pub failed_video_bga: HashSet<BgaAssetId>,
}

impl AppAudioOutput {
    pub fn clock(&self) -> AudioClock {
        self.source.clock()
    }

    pub fn pause(&mut self) -> Result<()> {
        self.source.pause();
        Ok(())
    }

    pub fn play(&mut self, chart_zero_time: TimeUs) -> Result<()> {
        self.source.play(chart_zero_time);
        Ok(())
    }
}

impl RunningPlaySession {
    pub fn start(&mut self, chart_zero_time: TimeUs) -> Result<()> {
        self.audio.play(chart_zero_time)?;
        self.session.audio_clock = self.audio.clock();
        Ok(())
    }

    pub fn pause_audio(&mut self) -> Result<()> {
        self.audio.pause()?;
        self.session.audio_clock = self.audio.clock();
        Ok(())
    }
}

impl AudioRuntime {
    pub fn open(config: &AudioConfig) -> Result<Self> {
        ensure_default_device_supported(config)?;
        let mut output = CpalBackend::open_shared_default()
            .context("failed to open shared audio output stream")?;
        output.play().context("failed to start shared audio output stream")?;
        Ok(Self { output })
    }

    pub fn sample_rate(&self) -> u32 {
        self.output.sample_rate()
    }

    fn add_source(&self, engine: SharedAudioEngine) -> CpalOutputSource {
        self.output.add_source(engine)
    }
}

pub fn open_app_audio_output(runtime: &AudioRuntime, engine: AudioEngine) -> AppAudioOutput {
    let engine = Arc::new(Mutex::new(engine));
    let source = runtime.add_source(Arc::clone(&engine));
    AppAudioOutput { engine, _runtime: runtime.clone(), source }
}

/// アプリ全体で常時 ON のシステム SE / BGM 出力。
///
/// プレイセッションの [`AppAudioOutput`] と同じ shared cpal stream に source
/// として登録される。ASIO のようにデバイス側で複数 stream を開けない環境でも、
/// BMZ 側で system / preview / play 音を 1 本に mix する。
pub struct SystemAudio {
    pub engine: SharedAudioEngine,
    _runtime: AudioRuntime,
    _source: CpalOutputSource,
}

impl SystemAudio {
    /// クロックを開始してストリームを走らせ、`play_now` / `stop_sound` を即座に
    /// 反映できる状態にする。`chart_zero_time` 引数はシステム音のスケジューリング
    /// (`start_frame = 0`)には影響しないため `TimeUs(0)` 固定で良い。
    pub fn open(runtime: &AudioRuntime) -> Self {
        let engine = Arc::new(Mutex::new(AudioEngine::default()));
        let mut source = runtime.add_source(Arc::clone(&engine));
        source.play(TimeUs(0));
        Self { engine, _runtime: runtime.clone(), _source: source }
    }

    pub fn engine(&self) -> SharedAudioEngine {
        Arc::clone(&self.engine)
    }
}

pub fn open_prepared_play_audio(
    runtime: &AudioRuntime,
    prepared: PreparedPlaySession,
) -> RunningPlaySession {
    let audio = open_app_audio_output(runtime, prepared.audio);
    let mut session = prepared.session;
    session.audio_clock = audio.clock();

    RunningPlaySession {
        session,
        audio,
        sample_report: prepared.sample_report,
        finished: None,
        result_graph: ResultGraphCollector::default(),
        best_ex_score: None,
        best_ghost: None,
        target_ex_score: prepared.target_ex_score,
        applied_arrange: prepared.applied_arrange,
        practice_mode: prepared.practice_mode,
        bga_frames: BgaFrameCatalog::new(),
        video_bga_decoders: HashMap::new(),
        failed_video_bga: HashSet::new(),
    }
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
