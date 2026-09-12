use crate::gameplay_runtime::{GameplayClient, RuntimeRenderConfig};
use anyhow::{Context, Result, bail};
use bmz_audio::backend::cpal::{
    CpalBackend, CpalCommandedOutputSource, CpalHostId, CpalOutputConfig, CpalOutputDiagnostics,
    CpalOutputSourceKind, CpalSharedOutput,
};
use bmz_audio::clock::AudioClock;
use bmz_audio::command::{AudioCommandQueueDiagnostics, AudioEngineHandle};
use bmz_audio::engine::AudioEngine;
use bmz_audio::loader::LoadedSampleReport;
use bmz_chart::model::BgaAssetId;
use bmz_core::ids::SoundId;
use bmz_core::time::TimeUs;
use std::collections::{HashMap, HashSet};

use crate::config::app_config::{
    AudioBackend, AudioBufferSizeMode, AudioConfig, AudioOutputMode, AudioSampleRateMode,
};
use crate::ln_policy::ChartLnProfile;
use crate::screens::play_finish::{FinishedPlaySession, PendingFinishedPlaySession};
use crate::screens::play_session::{AppliedArrange, PreparedPlaySession};
use crate::screens::play_snapshot::{BgaFrameCatalog, PlayRenderSnapshotCache};
use crate::screens::result_model::ResultGraphCollector;
use crate::select_options::TargetOption;
use crate::storage::score_db::ScoreKey;
use crate::video_bga::ActiveVideoBgaDecoder;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AudioOutputDiagnostics {
    pub callback_count: u64,
    pub rendered_frames: u64,
    pub timeline_catch_up_count: u64,
    pub timeline_catch_up_frames: u64,
    pub stream_error_count: u64,
    pub source_lock_miss_count: u64,
    pub engine_lock_miss_count: u64,
    pub engine_lock_miss_callback_count: u64,
    pub system_engine_lock_miss_count: u64,
    pub play_engine_lock_miss_count: u64,
    pub draining_engine_lock_miss_count: u64,
    pub other_engine_lock_miss_count: u64,
    pub clipped_sample_count: u64,
    pub peak_abs: f32,
    pub max_callback_ns: u64,
    pub command_submitted_count: u64,
    pub command_dropped_count: u64,
    pub command_drained_count: u64,
    pub command_coalesced_count: u64,
    pub command_drain_lock_miss_count: u64,
    pub command_engine_lock_miss_count: u64,
    pub command_queue_max_depth: u64,
}

impl AudioOutputDiagnostics {
    pub fn from_cpal(snapshot: CpalOutputDiagnostics) -> Self {
        Self {
            callback_count: snapshot.callback_count,
            rendered_frames: snapshot.rendered_frames,
            timeline_catch_up_count: snapshot.timeline_catch_up_count,
            timeline_catch_up_frames: snapshot.timeline_catch_up_frames,
            stream_error_count: snapshot.stream_error_count,
            source_lock_miss_count: snapshot.source_lock_miss_count,
            engine_lock_miss_count: snapshot.engine_lock_miss_count,
            engine_lock_miss_callback_count: snapshot.engine_lock_miss_callback_count,
            system_engine_lock_miss_count: snapshot.system_engine_lock_miss_count,
            play_engine_lock_miss_count: snapshot.play_engine_lock_miss_count,
            draining_engine_lock_miss_count: snapshot.draining_engine_lock_miss_count,
            other_engine_lock_miss_count: snapshot.other_engine_lock_miss_count,
            clipped_sample_count: snapshot.clipped_sample_count,
            peak_abs: snapshot.peak_abs,
            max_callback_ns: snapshot.max_callback_ns,
            ..Default::default()
        }
    }

    pub fn add_command_queue(&mut self, diagnostics: AudioCommandQueueDiagnostics) {
        self.command_submitted_count =
            self.command_submitted_count.saturating_add(diagnostics.submitted);
        self.command_dropped_count = self.command_dropped_count.saturating_add(diagnostics.dropped);
        self.command_drained_count = self.command_drained_count.saturating_add(diagnostics.drained);
        self.command_coalesced_count =
            self.command_coalesced_count.saturating_add(diagnostics.coalesced);
        self.command_drain_lock_miss_count =
            self.command_drain_lock_miss_count.saturating_add(diagnostics.drain_lock_misses);
        self.command_engine_lock_miss_count =
            self.command_engine_lock_miss_count.saturating_add(diagnostics.engine_lock_misses);
        self.command_queue_max_depth = self.command_queue_max_depth.max(diagnostics.max_depth);
    }
}

pub struct AppAudioOutput {
    pub engine: AudioEngineHandle,
    runtime: AudioRuntime,
    source: CpalCommandedOutputSource,
}

#[derive(Clone)]
pub struct AudioRuntime {
    output: CpalSharedOutput,
    config: AudioConfig,
}

pub struct RunningPlaySession {
    pub gameplay: GameplayClient,
    pub skin_attempt: bmz_render::snapshot::SkinAttemptState,
    pub source_ln_profile: ChartLnProfile,
    /// Duration recorded in `library.db` when this play was preloaded.
    pub chart_length_ms: u64,
    /// Frozen hardware-clock duration from chart time zero to the first terminal state.
    pub play_duration_ms: Option<u64>,
    pub audio: AppAudioOutput,
    /// Decoded sample lengths used to determine which BGM voices survive a
    /// viewer seek. Durations stay valid if the output source is resampled.
    bgm_sample_duration_us: HashMap<SoundId, i64>,
    pub sample_report: Vec<LoadedSampleReport>,
    pub finished: Option<FinishedPlaySession>,
    pub pending_finished: Option<PendingFinishedPlaySession>,
    pub finish_error: Option<String>,
    pub result_graph: ResultGraphCollector,
    pub score_key: ScoreKey,
    /// プレイ開始時に DB から取得したベスト EX スコア。未取得なら None。
    pub best_ex_score: Option<u32>,
    /// プレイ開始時に DB から取得した beatoraja 互換 ghost。
    pub best_ghost: Option<Vec<u8>>,
    /// プレイ開始時のターゲット設定を譜面ノーツ数で解決した EX スコア。
    pub target_ex_score: Option<u32>,
    /// プレイ開始時に確定したターゲット表示名。ライバル名は変換せず保持する。
    pub target_name: String,
    /// 実譜面と実スコアキーが確定してから EX 目標値を解決するための設定値。
    pub target_option: TargetOption,
    pub resolved_target: Option<crate::select_options::ResolvedTarget>,
    pub rival_name: Option<String>,
    pub applied_arrange: AppliedArrange,
    pub practice_mode: bool,
    pub score_save_disabled: bool,
    pub playback_rate_percent: u16,
    pub bga_frames: BgaFrameCatalog,
    pub render_snapshot_cache: PlayRenderSnapshotCache,
    pub video_bga_decoders: HashMap<BgaAssetId, ActiveVideoBgaDecoder>,
    pub failed_video_bga: HashSet<BgaAssetId>,
}

impl std::ops::Deref for RunningPlaySession {
    type Target = GameplayClient;
    fn deref(&self) -> &Self::Target {
        &self.gameplay
    }
}

impl std::ops::DerefMut for RunningPlaySession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.gameplay
    }
}

impl AppAudioOutput {
    pub fn command_diagnostics(&self) -> AudioCommandQueueDiagnostics {
        self.engine.diagnostics()
    }

    pub fn clock(&self) -> AudioClock {
        self.source.clock()
    }

    pub fn set_playback_rate_percent(&mut self, rate: u16) {
        if let Some(change) = self.source.set_playback_rate_percent(rate)
            && !self.engine.apply_playback_rate_change(change)
        {
            tracing::warn!(
                requested_rate_percent = change.new_rate_percent,
                "playback rate change was dropped by the audio command queue"
            );
            let _ = self.source.set_playback_rate_percent(change.old_rate_percent);
        }
    }

    pub fn pause(&mut self) -> Result<()> {
        self.source.pause();
        Ok(())
    }

    pub fn pause_playback_at(&mut self, chart_time: TimeUs) -> Result<()> {
        if !self.engine.set_playback_paused(true) {
            bail!("playback pause was dropped by the audio command queue");
        }
        self.source.pause_at(chart_time);
        Ok(())
    }

    pub fn resume_playback(&mut self, chart_time: TimeUs) -> Result<()> {
        if !self.engine.set_playback_paused(false) {
            bail!("playback resume was dropped by the audio command queue");
        }
        self.source.play(chart_time);
        Ok(())
    }

    pub fn play(&mut self, chart_zero_time: TimeUs) -> Result<()> {
        self.source.play(chart_zero_time);
        self.runtime.play().context("failed to start audio output stream")?;
        Ok(())
    }

    pub fn mark_draining(&mut self) {
        self.source.set_kind(CpalOutputSourceKind::Draining);
    }
}

impl RunningPlaySession {
    pub fn start_gameplay_runtime(
        &mut self,
        effects: Option<crate::system_sound_manager::GameplaySoundOutput>,
    ) -> Result<()> {
        // Asset identity belongs to the immutable timeline. Texture readiness
        // and decoded video dimensions remain renderer-owned and may arrive later.
        let mut bga_frames = self.bga_frames.clone();
        for asset in &self.session.chart.bga_assets {
            bga_frames.entry(asset.id).or_insert_with(|| {
                if asset.kind == bmz_chart::model::BgaAssetKind::Video {
                    crate::screens::play_snapshot::display_video_bga_frame(asset.id, 1, 1)
                } else {
                    crate::screens::play_snapshot::display_bga_frame(asset.id, 1, 1)
                }
            });
        }
        let config = RuntimeRenderConfig {
            #[cfg(test)]
            probe: None,
            effects,
            best_ex_score: self.best_ex_score,
            best_ghost: self.best_ghost.clone(),
            target_ex_score: self.target_ex_score,
            target: self.target_option.as_string(),
            resolved_target_name: self
                .rival_name
                .clone()
                .or_else(|| self.resolved_target.as_ref().map(|target| target.name.clone())),
            applied_arrange: self.applied_arrange.clone(),
            source_ln_profile: self.source_ln_profile,
            skin_attempt: self.skin_attempt,
            score_key: self.score_key,
            practice_mode: self.practice_mode,
            score_save_disabled: self.score_save_disabled,
            bga_frames,
            cache: self.render_snapshot_cache.clone(),
        };
        self.gameplay.start(self.audio.engine.clone(), config)
    }

    fn sync_gameplay_clock(&mut self) {
        let clock = self.audio.clock();
        self.gameplay.edit(move |session| session.audio_clock = clock);
    }
    pub fn set_playback_rate_percent(&mut self, rate: u16) {
        self.audio.set_playback_rate_percent(rate);
        self.sync_gameplay_clock();
        self.playback_rate_percent = self.audio.clock().playback_rate_percent();
    }

    pub fn start(&mut self, chart_zero_time: TimeUs) -> Result<()> {
        self.audio.play(chart_zero_time)?;
        self.sync_gameplay_clock();
        Ok(())
    }

    pub fn start_viewer_seek(
        &mut self,
        chart_zero_time: TimeUs,
        remain_paused: bool,
    ) -> Result<usize> {
        self.gameplay.edit(move |session| {
            bmz_gameplay::session::prepare_viewer_seek(session, chart_zero_time)
        });
        self.start(chart_zero_time)?;
        if remain_paused {
            self.audio.source.pause_at(chart_zero_time);
            self.sync_gameplay_clock();
        }
        let bgm_volume = self.session.audio_mix.master_volume
            * self.session.audio_mix.effective_normalization_gain()
            * self.session.audio_mix.bgm_volume;
        let (scheduler, carryover) =
            bmz_gameplay::session::BgmScheduler::starting_at_with_carryover(
                &self.session.chart,
                chart_zero_time,
                &self.session.audio_clock,
                bgm_volume,
                |sound_id| self.bgm_sample_duration_us.get(&sound_id).copied(),
            );
        self.gameplay.edit(move |session| session.bgm_scheduler = scheduler);
        let carryover_count = carryover.len();
        let replaced = if remain_paused {
            self.audio.engine.replace_playback_paused(carryover)
        } else {
            self.audio.engine.replace_playback(carryover)
        };
        if !replaced {
            bail!(
                "viewer seek audio replacement was dropped ({carryover_count} carry-over voices)"
            );
        }
        Ok(carryover_count)
    }

    pub fn pause_viewer_playback(&mut self, chart_time: TimeUs) -> Result<()> {
        self.audio.pause_playback_at(chart_time)?;
        self.sync_gameplay_clock();
        Ok(())
    }

    pub fn resume_viewer_playback(&mut self) -> Result<()> {
        // The render observation may still contain the running clock while Viewer is
        // paused and no longer polls the worker. Resume from the source's frozen clock.
        let chart_time = self.audio.clock().now();
        self.audio.resume_playback(chart_time)?;
        self.sync_gameplay_clock();
        Ok(())
    }

    pub fn pause_audio(&mut self) -> Result<()> {
        self.audio.pause()?;
        self.sync_gameplay_clock();
        Ok(())
    }

    pub fn finish_play_duration_ms(&mut self) -> u64 {
        if let Some(duration_ms) = self.play_duration_ms {
            return duration_ms;
        }

        let duration_ms = self.gameplay.result.as_ref().map_or_else(
            || (self.audio.clock().elapsed_since(TimeUs(0)).0.max(0) / 1_000) as u64,
            |result| result.play_duration_ms,
        );
        self.play_duration_ms = Some(duration_ms);
        duration_ms
    }
}

impl AudioRuntime {
    pub fn open(config: &AudioConfig) -> Result<Self> {
        let output_config = cpal_output_config(config)?;
        let output = CpalBackend::open_shared(output_config)
            .context("failed to open audio output stream")?;
        Ok(Self { output, config: config.clone() })
    }

    pub fn play(&self) -> Result<()> {
        self.output.play().context("failed to start audio output stream")
    }

    pub fn config(&self) -> &AudioConfig {
        &self.config
    }

    pub fn sample_rate(&self) -> u32 {
        self.output.sample_rate()
    }

    pub fn uses_pulseaudio_host(&self) -> bool {
        self.output.uses_pulseaudio_host()
    }

    pub fn take_diagnostics(&self) -> AudioOutputDiagnostics {
        AudioOutputDiagnostics::from_cpal(self.output.take_diagnostics())
    }

    /// 音声 callback から退避した source を app thread で破棄する。
    pub fn reap_retired_sources(&self) {
        self.output.reap_retired_sources();
    }

    fn add_commanded_source(
        &self,
        handle: AudioEngineHandle,
        kind: CpalOutputSourceKind,
    ) -> CpalCommandedOutputSource {
        self.output.add_commanded_source_with_kind(handle, kind)
    }
}

pub fn open_app_audio_output(runtime: &AudioRuntime, engine: AudioEngine) -> AppAudioOutput {
    let engine = AudioEngineHandle::new(engine);
    let source = runtime.add_commanded_source(engine.clone(), CpalOutputSourceKind::Play);
    AppAudioOutput { engine, runtime: runtime.clone(), source }
}

/// アプリ全体で常時 ON のシステム SE / BGM 出力。
///
/// プレイセッションの [`AppAudioOutput`] と同じ shared cpal stream に source
/// として登録される。ASIO のようにデバイス側で複数 stream を開けない環境でも、
/// BMZ 側で system / preview / play 音を 1 本に mix する。
pub struct SystemAudio {
    engine: AudioEngineHandle,
    _runtime: AudioRuntime,
    _source: CpalCommandedOutputSource,
}

impl SystemAudio {
    pub fn command_diagnostics(&self) -> AudioCommandQueueDiagnostics {
        self.engine.diagnostics()
    }

    /// クロックを開始してストリームを走らせ、`play_now` / `stop_sound` を即座に
    /// 反映できる状態にする。`chart_zero_time` 引数はシステム音のスケジューリング
    /// (`start_frame = 0`)には影響しないため `TimeUs(0)` 固定で良い。
    pub fn open(runtime: &AudioRuntime) -> Self {
        let engine = AudioEngineHandle::new(AudioEngine::default());
        Self::with_engine(runtime, engine)
    }

    /// 既存のシステムエンジンを別の `AudioRuntime`(新しい cpal ストリーム)へ
    /// 載せ替える。設定変更時に音声出力を開き直しても、`SystemSoundManager`
    /// や `SelectChartPreview` が共有している command handle をそのまま使い続けられる。
    pub fn reattach(runtime: &AudioRuntime, engine: AudioEngineHandle) -> Self {
        Self::with_engine(runtime, engine)
    }

    fn with_engine(runtime: &AudioRuntime, engine: AudioEngineHandle) -> Self {
        let mut source = runtime.add_commanded_source(engine.clone(), CpalOutputSourceKind::System);
        source.play(TimeUs(0));
        Self { engine, _runtime: runtime.clone(), _source: source }
    }

    pub fn engine(&self) -> AudioEngineHandle {
        self.engine.clone()
    }
}

pub fn open_prepared_play_audio(
    runtime: &AudioRuntime,
    prepared: PreparedPlaySession,
    score_key: ScoreKey,
) -> RunningPlaySession {
    let bgm_sample_duration_us = prepared
        .session
        .chart
        .bgm_events
        .iter()
        .filter_map(|event| {
            let sample = prepared.audio.samples.get(event.sound)?;
            let sample_rate = sample.sample_rate();
            if sample_rate == 0 {
                return None;
            }
            let sample_rate = u128::from(sample_rate);
            let duration_us = (sample.frame_count() as u128 * 1_000_000)
                .saturating_add(sample_rate - 1)
                / sample_rate;
            let duration_us = duration_us.min(i64::MAX as u128) as i64;
            Some((event.sound, duration_us))
        })
        .collect();
    let mut audio = open_app_audio_output(runtime, prepared.audio);
    audio.set_playback_rate_percent(prepared.playback_rate_percent);
    let mut session = prepared.session;
    session.audio_clock = audio.clock();
    let target_ex_score = prepared
        .resolved_target
        .as_ref()
        .map(|target| target.ex_score)
        .or_else(|| prepared.target_option.target_ex_score(session.scored_total_notes));

    RunningPlaySession {
        render_snapshot_cache: prepared.render_snapshot_cache,
        gameplay: GameplayClient::new(session),
        skin_attempt: prepared.skin_attempt,
        source_ln_profile: prepared.source_ln_profile,
        chart_length_ms: prepared.chart_length_ms,
        play_duration_ms: None,
        audio,
        bgm_sample_duration_us,
        sample_report: prepared.sample_report,
        finished: None,
        pending_finished: None,
        finish_error: None,
        result_graph: ResultGraphCollector::default(),
        score_key,
        best_ex_score: None,
        best_ghost: None,
        target_ex_score,
        target_name: prepared.target_name,
        target_option: prepared.target_option,
        resolved_target: prepared.resolved_target,
        rival_name: prepared.rival_name,
        applied_arrange: prepared.applied_arrange,
        practice_mode: prepared.practice_mode,
        score_save_disabled: prepared.score_save_disabled,
        playback_rate_percent: prepared.playback_rate_percent,
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
    // 出力モードは WASAPI 専用。バックエンドを切り替えても保存値は保持し、
    // ASIO などには渡さない。Windows の Auto は既定の WASAPI を使う。
    let uses_wasapi = matches!(host, Some(CpalHostId::Wasapi)) || (cfg!(windows) && host.is_none());
    let low_latency_shared = uses_wasapi && config.output_mode == AudioOutputMode::SharedLowLatency;
    let exclusive = uses_wasapi && config.output_mode == AudioOutputMode::Exclusive;
    // ペア番号(0=1-2ch, 1=3-4ch …)をインターリーブ先頭チャンネル位置へ変換する。
    let channel_offset = config.output_channel_pair.saturating_mul(2);

    Ok(CpalOutputConfig {
        host,
        output_device_name,
        sample_rate,
        buffer_size,
        low_latency_shared,
        exclusive,
        channel_offset,
    })
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

/// 設定 UI に表示できる音声バックエンドを、現在の OS / feature 構成から返す。
///
/// `Auto` は cpal の既定ホストを使うため常に候補に含める。明示的なホストは
/// cpal が現在のビルドで提供している場合だけ表示する。
pub fn available_audio_backends() -> Vec<AudioBackend> {
    [
        AudioBackend::Auto,
        AudioBackend::Wasapi,
        AudioBackend::Asio,
        AudioBackend::CoreAudio,
        AudioBackend::Alsa,
        AudioBackend::Pulse,
        AudioBackend::PipeWire,
    ]
    .into_iter()
    .filter(|backend| *backend == AudioBackend::Auto || cpal_host_for_backend(backend).is_ok())
    .collect()
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

    #[test]
    fn low_latency_shared_mode_is_passed_to_cpal_config() {
        let mut config = AppConfig::default().audio;
        config.output_mode = AudioOutputMode::SharedLowLatency;

        let output = cpal_output_config(&config).unwrap();

        assert_eq!(output.low_latency_shared, cfg!(windows));
        assert!(!output.exclusive);
    }

    #[test]
    fn exclusive_mode_is_passed_to_native_wasapi_config() {
        let mut config = AppConfig::default().audio;
        config.output_mode = AudioOutputMode::Exclusive;

        let output = cpal_output_config(&config).unwrap();

        assert!(!output.low_latency_shared);
        assert_eq!(output.exclusive, cfg!(windows));
    }

    #[cfg(all(windows, feature = "asio"))]
    #[test]
    fn switching_from_wasapi_to_asio_ignores_and_preserves_wasapi_output_mode() {
        let mut config = AppConfig::default().audio;
        config.asio_driver = "ASIO4ALL v2".to_string();
        for mode in [AudioOutputMode::SharedLowLatency, AudioOutputMode::Exclusive] {
            config.backend = AudioBackend::Wasapi;
            config.output_mode = mode.clone();
            let wasapi = cpal_output_config(&config).unwrap();
            assert_eq!(wasapi.low_latency_shared, mode == AudioOutputMode::SharedLowLatency);
            assert_eq!(wasapi.exclusive, mode == AudioOutputMode::Exclusive);

            config.backend = AudioBackend::Asio;
            let asio = cpal_output_config(&config).unwrap();
            assert_eq!(asio.host, Some(CpalHostId::Asio));
            assert_eq!(asio.output_device_name.as_deref(), Some("ASIO4ALL v2"));
            assert!(!asio.low_latency_shared);
            assert!(!asio.exclusive);
            assert_eq!(config.output_mode, mode);

            config.backend = AudioBackend::Wasapi;
            let restored = cpal_output_config(&config).unwrap();
            assert_eq!(restored.low_latency_shared, wasapi.low_latency_shared);
            assert_eq!(restored.exclusive, wasapi.exclusive);
        }
    }
}
