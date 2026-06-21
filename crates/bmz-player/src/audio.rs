use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use bmz_audio::backend::cpal::{
    CpalBackend, CpalHostId, CpalOutputConfig, CpalOutputSource, CpalSharedOutput,
    SharedAudioEngine,
};
use bmz_audio::clock::AudioClock;
use bmz_audio::engine::AudioEngine;
use bmz_audio::loader::LoadedSampleReport;
use bmz_audio::queue::ScheduledSoundQueue;
use bmz_chart::model::BgaAssetId;
use bmz_core::time::TimeUs;
use bmz_gameplay::session::GameSession;

use crate::config::app_config::{
    AudioBackend, AudioBufferSizeMode, AudioConfig, AudioSampleRateMode,
};
use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::play_session::{AppliedArrange, PreparedPlaySession};
use crate::screens::play_snapshot::{BgaFrameCatalog, PlayRenderSnapshotCache};
use crate::screens::result_model::ResultGraphCollector;
use crate::storage::score_db::ScoreKey;
use crate::video_bga::ActiveVideoBgaDecoder;

pub struct AppAudioOutput {
    pub engine: SharedAudioEngine,
    runtime: AudioRuntime,
    source: CpalOutputSource,
}

#[derive(Clone)]
pub struct AudioRuntime {
    output: CpalSharedOutput,
}

pub struct RunningPlaySession {
    pub session: GameSession,
    pub audio: AppAudioOutput,
    pub pending_audio: ScheduledSoundQueue,
    pub sample_report: Vec<LoadedSampleReport>,
    pub finished: Option<FinishedPlaySession>,
    pub result_graph: ResultGraphCollector,
    pub score_key: ScoreKey,
    /// プレイ開始時に DB から取得したベスト EX スコア。未取得なら None。
    pub best_ex_score: Option<u32>,
    /// プレイ開始時に DB から取得した beatoraja 互換 ghost。
    pub best_ghost: Option<Vec<u8>>,
    /// プレイ開始時のターゲット設定を譜面ノーツ数で解決した EX スコア。
    pub target_ex_score: Option<u32>,
    pub applied_arrange: AppliedArrange,
    pub practice_mode: bool,
    pub bga_frames: BgaFrameCatalog,
    pub render_snapshot_cache: PlayRenderSnapshotCache,
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
        self.runtime.play().context("failed to start shared audio output stream")?;
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
        let output_config = cpal_output_config(config)?;
        let output = CpalBackend::open_shared(output_config)
            .context("failed to open shared audio output stream")?;
        Ok(Self { output })
    }

    pub fn play(&self) -> Result<()> {
        self.output.play().context("failed to start shared audio output stream")
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
    AppAudioOutput { engine, runtime: runtime.clone(), source }
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
        Self::with_engine(runtime, engine)
    }

    /// 既存のシステムエンジンを別の `AudioRuntime`(新しい cpal ストリーム)へ
    /// 載せ替える。設定変更時に音声出力を開き直しても、`SystemSoundManager` や
    /// `SelectChartPreview` が共有しているエンジン Arc をそのまま使い続けられる。
    pub fn reattach(runtime: &AudioRuntime, engine: SharedAudioEngine) -> Self {
        Self::with_engine(runtime, engine)
    }

    fn with_engine(runtime: &AudioRuntime, engine: SharedAudioEngine) -> Self {
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
    score_key: ScoreKey,
) -> RunningPlaySession {
    let audio = open_app_audio_output(runtime, prepared.audio);
    let mut session = prepared.session;
    session.audio_clock = audio.clock();

    RunningPlaySession {
        render_snapshot_cache: PlayRenderSnapshotCache::from_chart(&session.chart),
        session,
        audio,
        pending_audio: ScheduledSoundQueue::new(),
        sample_report: prepared.sample_report,
        finished: None,
        result_graph: ResultGraphCollector::default(),
        score_key,
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

fn cpal_output_config(config: &AudioConfig) -> Result<CpalOutputConfig> {
    let host = cpal_host_for_backend(&config.backend)?;
    let output_device_name = cpal_output_device_name(config);
    let sample_rate = cpal_sample_rate(config);
    let buffer_size = cpal_buffer_size(config);
    // ペア番号(0=1-2ch, 1=3-4ch …)をインターリーブ先頭チャンネル位置へ変換する。
    let channel_offset = config.output_channel_pair.saturating_mul(2);

    Ok(CpalOutputConfig { host, output_device_name, sample_rate, buffer_size, channel_offset })
}

/// サンプルレートモードが `Fixed` のときだけ Hz を指定する。`Auto` は
/// ドライバ / OS 既定に任せるため `None`。
fn cpal_sample_rate(config: &AudioConfig) -> Option<u32> {
    match config.sample_rate_mode {
        AudioSampleRateMode::Fixed => Some(config.sample_rate),
        AudioSampleRateMode::Auto => None,
    }
}

/// バッファサイズモードが `Fixed` のときだけフレーム数を指定する。`Auto` は
/// デバイス既定に任せるため `None`。
fn cpal_buffer_size(config: &AudioConfig) -> Option<u32> {
    match config.buffer_size_mode {
        AudioBufferSizeMode::Fixed => Some(config.buffer_size),
        AudioBufferSizeMode::Auto => None,
    }
}

/// 設定 UI 用に、選択中バックエンドの出力デバイス名(ASIO ならドライバ名)を列挙する。
/// ホストが利用不可・列挙失敗なら空 Vec を返す。
pub fn list_output_devices(backend: &AudioBackend) -> Vec<String> {
    let Ok(host) = cpal_host_for_backend(backend) else {
        return Vec::new();
    };
    bmz_audio::backend::cpal::list_output_device_names(host)
}

fn cpal_host_for_backend(backend: &AudioBackend) -> Result<Option<CpalHostId>> {
    match backend {
        AudioBackend::Auto => Ok(None),
        AudioBackend::Wasapi => cpal_host_for_platform(CpalHostId::Wasapi, "WASAPI"),
        AudioBackend::Asio => cpal_asio_host(),
        AudioBackend::CoreAudio => cpal_host_for_platform(CpalHostId::CoreAudio, "Core Audio"),
        AudioBackend::Alsa => cpal_host_for_platform(CpalHostId::Alsa, "ALSA"),
        AudioBackend::Pulse => cpal_host_for_platform(CpalHostId::Pulse, "PulseAudio"),
        AudioBackend::PipeWire => cpal_host_for_platform(CpalHostId::PipeWire, "PipeWire"),
    }
}

fn cpal_output_device_name(config: &AudioConfig) -> Option<String> {
    if matches!(config.backend, AudioBackend::Asio) && !config.asio_driver.trim().is_empty() {
        Some(config.asio_driver.trim().to_string())
    } else if !config.output_device.trim().is_empty() {
        Some(config.output_device.trim().to_string())
    } else {
        None
    }
}

fn cpal_host_for_platform(host: CpalHostId, label: &str) -> Result<Option<CpalHostId>> {
    if bmz_audio::backend::cpal::is_host_supported(host) {
        Ok(Some(host))
    } else {
        bail!("{label} audio backend is not available on this platform")
    }
}

#[cfg(all(windows, feature = "asio"))]
fn cpal_asio_host() -> Result<Option<CpalHostId>> {
    Ok(Some(CpalHostId::Asio))
}

#[cfg(not(all(windows, feature = "asio")))]
fn cpal_asio_host() -> Result<Option<CpalHostId>> {
    bail!("ASIO audio backend requires building bmz-player on Windows with the `asio` feature")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::app_config::AppConfig;

    #[test]
    fn default_audio_config_can_use_cpal_default_output() {
        let config = AppConfig::default();

        let output = cpal_output_config(&config.audio).unwrap();

        assert_eq!(output.host, None);
        assert_eq!(output.output_device_name, None);
        // 既定はサンプルレート Auto なので cpal へはレート未指定で渡す。
        assert_eq!(output.sample_rate, None);
    }

    #[test]
    fn auto_sample_rate_mode_leaves_driver_default() {
        let mut config = AppConfig::default().audio;
        config.sample_rate_mode = AudioSampleRateMode::Auto;
        config.sample_rate = 96_000;

        let output = cpal_output_config(&config).unwrap();

        assert_eq!(output.sample_rate, None);
    }

    #[test]
    fn fixed_sample_rate_mode_passes_requested_hz() {
        let mut config = AppConfig::default().audio;
        config.sample_rate_mode = AudioSampleRateMode::Fixed;
        config.sample_rate = 96_000;

        let output = cpal_output_config(&config).unwrap();

        assert_eq!(output.sample_rate, Some(96_000));
    }

    #[test]
    fn named_output_device_is_passed_to_cpal_config() {
        let mut config = AppConfig::default().audio;
        config.output_device = "External DAC".to_string();

        let output = cpal_output_config(&config).unwrap();

        assert_eq!(output.output_device_name.as_deref(), Some("External DAC"));
    }

    #[test]
    fn asio_driver_is_used_as_asio_device_name() {
        let mut config = AppConfig::default().audio;
        config.backend = AudioBackend::Asio;
        config.output_device = "External DAC".to_string();
        config.asio_driver = "ASIO Driver".to_string();

        let output = cpal_output_config(&config);

        #[cfg(all(windows, feature = "asio"))]
        {
            let output = output.unwrap();
            assert_eq!(output.host, Some(CpalHostId::Asio));
            assert_eq!(output.output_device_name.as_deref(), Some("ASIO Driver"));
        }

        #[cfg(not(all(windows, feature = "asio")))]
        assert!(output.is_err());
    }

    #[test]
    fn fixed_buffer_size_mode_passes_frame_count() {
        let mut config = AppConfig::default().audio;
        config.buffer_size_mode = AudioBufferSizeMode::Fixed;
        config.buffer_size = 96;

        let output = cpal_output_config(&config).unwrap();

        assert_eq!(output.buffer_size, Some(96));
    }

    #[test]
    fn auto_buffer_size_mode_leaves_device_default() {
        let mut config = AppConfig::default().audio;
        config.buffer_size_mode = AudioBufferSizeMode::Auto;
        config.buffer_size = 96;

        let output = cpal_output_config(&config).unwrap();

        assert_eq!(output.buffer_size, None);
    }
}
