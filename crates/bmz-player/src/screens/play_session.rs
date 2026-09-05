use anyhow::{Context, Result, bail};
use bmz_audio::clock::AudioClock;
use bmz_audio::engine::AudioEngine;
use bmz_audio::ffmpeg_loader::FfmpegSampleLoader;
use bmz_audio::loader::{
    LoadedSampleReport, SampleLoader, load_chart_samples, load_chart_samples_with_progress,
};
use bmz_audio::loudness::{
    LoudnessAnalysis, analyze_chart_loudness, play_normalization_gain_for_analysis_with_output_gain,
};
use bmz_chart::import::{
    BmsRandomSource, ImportResult, import_bms_chart, import_bms_chart_with_random_source,
};
use bmz_chart::model::{
    LongNoteMode, LongNotePair, NoteEvent, NoteKind, PlayableChart, SoundEvent, TimingEventKind,
};
use bmz_chart::start_margin::apply_start_note_margin;
use bmz_core::clear::GaugeType;
use bmz_core::ids::NoteId;
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use bmz_core::time::TimeUs;
use bmz_gameplay::autoplay::AutoplayController;
use bmz_gameplay::gauge::{
    GaugeAutoShiftMode, GaugeCarryValue, GaugeProperty, GaugeState,
    gauge_total_for_chart_and_rule_mode,
};
use bmz_gameplay::hit_error::HitErrorRing;
use bmz_gameplay::input::backend::{InputBackend, NullInputBackend};
use bmz_gameplay::input::bounce::InputBounceFilter;
use bmz_gameplay::input::system::InputSystem;
use bmz_gameplay::input::translator::DefaultInputTranslator;
use bmz_gameplay::judge::engine::JudgeEngine;
use bmz_gameplay::judge::model::{JudgeAlgorithm, JudgeWindow, JudgeWindows};
use bmz_gameplay::judge::window::{
    judge_percent_at_time_for_keymode, judge_windows_for_keymode_and_rule_mode,
    judge_windows_for_rule_mode_and_keymode, scale_judge_windows_for_playback_rate,
};
use bmz_gameplay::replay::{ReplayPlayer, ReplayRecorder};
use bmz_gameplay::rule::RuleMode;
use bmz_gameplay::score::{ScoreState, scored_note_count};
use bmz_gameplay::session::{
    AssistRuntime, AutoKeysoundScheduler, BgmScheduler, FloatingPolicy, GameSession, HispeedMode,
    InputOffsetAutoAdjustState, PlaySkinOffset, PlayState,
};
use std::sync::Arc;

use crate::config::play::{
    audio_mix_from_profile, bottom_shiftable_gauge_from_config, chart_normalization_output_gain,
    gauge_auto_shift_from_config, gauge_type_from_config, input_bounce_config_from_profile,
    lane_binding_for_chart_with_slots, lane_unit_to_f32, play_offsets_from_profile_for_mode,
};
#[cfg(test)]
use crate::config::profile_config::LaneEffectConfig;
use crate::config::profile_config::{
    AssistOptionConfig, BaseHispeedConfig, BgaExpandConfig, BgaModeConfig, FloatingPolicyConfig,
    JudgeAlgorithmConfig, KeyModeConversionConfig, PlayModeConfig, ProfileConfig,
    SevenToNinePattern, SevenToNineRuleMode, SevenToNineType,
};
use crate::input::gamepad::GamepadSlotMap;
use crate::ln_policy::{
    ChartLnProfile, LnPolicySetting, apply_score_ln_policy_to_chart, course_score_ln_policy,
    played_ln_mode,
};
use crate::random_option_seed::{JavaRandom, RandomOptionSeed, RandomOptionSeeds};
use crate::screens::practice::{
    PracticeGaugeType, PracticeProperty, apply_practice_property, apply_practice_start_gauge,
};
use crate::select_options::{
    ArrangeOption, DoubleOption, HsFixOption, ResolvedTarget, SessionMode, TargetOption,
};
use crate::skin_loader::play_skin_selection_for_session;
use crate::storage::library_db::ChartNormalizationAnalysis;
use crate::storage::library_db::LibraryDatabase;
use crate::storage::score_db::ScoreKey;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SRandomScheme {
    Legacy40MsV1,
    #[default]
    Lm120HzV1,
}

impl SRandomScheme {
    pub const LEGACY_40MS_V1: &'static str = "legacy_40ms_v1";
    pub const LM_120HZ_V1: &'static str = "lm_120hz_v1";

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Legacy40MsV1 => Self::LEGACY_40MS_V1,
            Self::Lm120HzV1 => Self::LM_120HZ_V1,
        }
    }

    pub fn from_persistent_str(value: &str) -> Result<Self> {
        match value {
            Self::LEGACY_40MS_V1 => Ok(Self::Legacy40MsV1),
            Self::LM_120HZ_V1 => Ok(Self::Lm120HzV1),
            _ => bail!("unsupported S-RANDOM scheme: {value}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlaySessionOptions {
    /// Per-key presentation settings use the source chart mode. This differs
    /// from `chart.metadata.key_mode` after BATTLE expands 5K/7K to 10K/14K.
    pub play_config_key_mode: Option<KeyMode>,
    /// The authoritative launch mode. Practice-specific runtime state is
    /// derived from this value instead of being configured independently.
    pub session_mode: SessionMode,
    pub autoplay: bool,
    /// Requested key-mode conversion. It becomes effective only for a
    /// compatible source chart and is mutually exclusive with battle modes.
    pub key_mode_conversion: KeyModeConversionConfig,
    pub seven_to_nine_pattern: SevenToNinePattern,
    pub seven_to_nine_type: SevenToNineType,
    pub seven_to_nine_rule_mode: SevenToNineRuleMode,
    /// Explicitly disables score/lamp/replay/IR persistence without presenting
    /// the session as practice or autoplay.
    pub score_save_disabled: bool,
    /// Fixed chart/audio rate for this session. Restricted to non-scoring modes
    /// by the app flow and clamped by `bmz-audio` to 50..=200.
    pub playback_rate_percent: u16,
    pub assist: AssistOptionConfig,
    /// 譜面変換時に確定した実効 assist。preload 後に内部で設定する。
    pub assist_runtime: AssistRuntime,
    pub replay_player: Option<ReplayPlayer>,
    /// `SessionMode::GBattle` opponent. Preload builds a separately arranged
    /// opponent chart and gameplay advances this replay without taking over
    /// the primary input lanes.
    pub battle_opponent: Option<BattleOpponentOptions>,
    /// Preload-only output consumed by `build_game_session*`.
    pub opponent_chart: Option<Arc<PlayableChart>>,
    pub sample_rate: u32,
    pub gauge_override: Option<GaugeType>,
    /// G-BATTLE opponent gauge reconstructed from the selected IR score.
    pub opponent_gauge_override: Option<GaugeType>,
    pub gauge_auto_shift: GaugeAutoShiftMode,
    pub bottom_shiftable_gauge: GaugeType,
    pub arrange: ArrangeOption,
    pub arrange_2p: ArrangeOption,
    pub double_option: DoubleOption,
    pub hs_fix: HsFixOption,
    pub target: TargetOption,
    pub resolved_target: Option<ResolvedTarget>,
    /// beatoraja-compatible 24-bit RANDOM option seed for the 1P side.
    pub arrange_seed: Option<i64>,
    /// beatoraja-compatible 24-bit RANDOM option seed for the 2P side.
    pub arrange_seed_2p: Option<i64>,
    /// Fresh play 用 Random Trainer seed。7K の通常 RANDOM だけで 1P seed より優先する。
    pub random_trainer_seed: Option<i64>,
    /// Replay v3 and older used one unrestricted i64 seed with SplitMix64.
    pub legacy_arrange_seed: bool,
    /// S-RANDOM algorithm generation, independent from the arrange RNG scheme.
    pub s_random_scheme: SRandomScheme,
    /// Optional 2P S-RANDOM generation for mixed ghost/replay arrangements.
    pub s_random_scheme_2p: Option<SRandomScheme>,
    /// beatoraja H-RANDOM/ALL-SCR key-lane threshold recovered for a replay.
    /// None keeps BMZ's historical 100 ms behavior.
    pub h_random_threshold_ms: Option<u32>,
    /// Independent seed used only while selecting BMS `#RANDOM` branches.
    pub bms_random_seed: Option<u64>,
    /// Recorded `#RANDOM` decisions, in source order, for exact replay.
    pub bms_random_choices: Option<Vec<i32>>,
    /// Recorded `#SWITCH` decisions, in source order, for exact replay.
    pub bms_switch_choices: Option<Vec<u64>>,
    pub arrange_pattern: Option<Vec<u8>>,
    /// When set, overrides the gauge's starting value.  Used to carry the
    /// gauge between charts during a course.
    pub initial_gauge_value: Option<f32>,
    /// Per-gauge starting values for course carry.  This preserves auto-shift
    /// gauges independently, so depleted higher gauges stay depleted.
    pub initial_gauge_values: Option<Vec<GaugeCarryValue>>,
    /// Course-mode combo carried from the previous chart. Score storage still
    /// starts from zero; this affects rendered combo/max combo only.
    pub initial_course_combo: Option<u32>,
    /// Course judge constraint forwarded from CourseJudgeConstraint.
    /// `NoGood` zeroes the good window, `NoGreat` zeroes great and good
    /// windows; the next judge band kicks in immediately.
    pub judge_constraint: bmz_core::course::CourseJudgeConstraint,
    /// Course speed constraint. `NoSpeed` overrides the session-only lane
    /// presentation while preserving the player's saved lane settings.
    pub speed_constraint: bmz_core::course::CourseSpeedConstraint,
    /// Course-forced long-note mode (Ln/Cn/Hcn).  `None` keeps the chart's
    /// declared mode.
    pub ln_mode_override: Option<bmz_chart::model::LongNoteMode>,
    pub ln_policy_setting: LnPolicySetting,
    pub rule_mode: RuleMode,
    /// 段位ゲージ用の `GaugeProperty` 上書き。コース時に
    /// `apply_course_constraints` が `CourseGaugeConstraint::Lr2/Keys5/...` を
    /// 解釈して設定する。`None` の場合はチャートの `KeyMode` から自動推定する。
    pub gauge_property: Option<GaugeProperty>,
    /// 論理 `gamepad1`/`gamepad2` → 物理 gilrs id の対応。プレイ開始時に固定する。
    pub gamepad_slots: GamepadSlotMap,
}

#[derive(Debug, Clone)]
pub struct BattleOpponentOptions {
    pub replay_player: Option<ReplayPlayer>,
    pub gauge: Option<GaugeType>,
    pub arrange: ArrangeOption,
    pub arrange_2p: ArrangeOption,
    pub double_option: DoubleOption,
    pub arrange_seed: Option<i64>,
    pub arrange_seed_2p: Option<i64>,
    pub legacy_arrange_seed: bool,
    /// rianIR-compatible packed side seeds. Expanded after the source key mode
    /// is known during preload.
    pub packed_seed: Option<i64>,
    pub bms_random_choices: Option<Vec<i32>>,
    pub bms_switch_choices: Option<Vec<u64>>,
    pub arrange_pattern: Option<Vec<u8>>,
    pub s_random_scheme: SRandomScheme,
    pub s_random_scheme_2p: Option<SRandomScheme>,
    pub h_random_threshold_ms: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppliedArrange {
    pub arrange: ArrangeOption,
    pub arrange_2p: ArrangeOption,
    pub double_option: DoubleOption,
    /// 1P option seed. New plays always use the beatoraja 24-bit range.
    pub seed: Option<i64>,
    /// Independent 2P option seed for DP charts.
    pub seed_2p: Option<i64>,
    /// True only when replaying the pre-v4 SplitMix64 seed format.
    pub legacy_seed: bool,
    /// S-RANDOM algorithm generation actually used for this chart.
    pub s_random_scheme: SRandomScheme,
    /// 2P generation for DP charts. `None` means this is an SP chart.
    pub s_random_scheme_2p: Option<SRandomScheme>,
    pub h_random_threshold_ms: Option<u32>,
    /// BMS `#RANDOM` decisions applied before the arrange modifier.
    pub bms_random_choices: Vec<i32>,
    /// BMS `#SWITCH` decisions applied before the arrange modifier.
    pub bms_switch_choices: Vec<u64>,
    pub pattern: Option<Vec<u8>>,
    /// Key-mode conversion actually applied to the source chart.
    pub key_mode_conversion: KeyModeConversionConfig,
    pub seven_to_nine_pattern: SevenToNinePattern,
    pub seven_to_nine_type: SevenToNineType,
    pub seven_to_nine_rule_mode: SevenToNineRuleMode,
}

impl AppliedArrange {
    pub const fn key_mode_converted(&self) -> bool {
        !matches!(self.key_mode_conversion, KeyModeConversionConfig::Off)
    }

    pub const fn score_persistence_disabled(&self) -> bool {
        self.key_mode_conversion.score_persistence_disabled(self.seven_to_nine_rule_mode)
    }

    pub const fn seven_to_six(&self) -> bool {
        matches!(self.key_mode_conversion, KeyModeConversionConfig::SevenToSix)
    }
}

impl AppliedArrange {
    pub fn packed_beatoraja_seed_from_sides(&self) -> Option<i64> {
        if self.legacy_seed {
            return None;
        }
        let p1 = RandomOptionSeed::new(u32::try_from(self.seed?).ok()?)?;
        let seeds = if let Some(seed_2p) = self.seed_2p {
            let p2 = RandomOptionSeed::new(u32::try_from(seed_2p).ok()?)?;
            RandomOptionSeeds::double(p1, p2)
        } else {
            RandomOptionSeeds::single(p1)
        };
        i64::try_from(seeds.pack()).ok()
    }

    pub fn packed_beatoraja_seed(&self, key_mode: KeyMode) -> Option<i64> {
        if self.legacy_seed {
            return None;
        }
        let packed = self.packed_beatoraja_seed_from_sides()?;
        let has_p2 = self.seed_2p.is_some();
        (has_p2 == matches!(key_mode, KeyMode::K10 | KeyMode::K14)).then_some(packed)
    }
}

pub struct PreparedPlaySession {
    pub session: GameSession,
    pub skin_attempt: bmz_render::snapshot::SkinAttemptState,
    pub source_ln_profile: ChartLnProfile,
    pub chart_length_ms: u64,
    pub audio: AudioEngine,
    pub sample_report: Vec<LoadedSampleReport>,
    pub render_snapshot_cache: crate::screens::play_snapshot::PlayRenderSnapshotCache,
    pub applied_arrange: AppliedArrange,
    pub score_key: ScoreKey,
    pub target_option: TargetOption,
    pub target: String,
    pub resolved_target: Option<ResolvedTarget>,
    pub practice_mode: bool,
    pub score_save_disabled: bool,
    pub playback_rate_percent: u16,
}

/// 譜面 parse・オプション適用までが完了し、WAV/BMP ロードを開始できる状態。
///
/// beatoraja の `BMSModel` と同様、メディアロード完了前から Play skin へ
/// 静的な譜面情報を供給するために main thread へ先行公開する。
#[derive(Debug, Clone)]
pub struct PreparedPlayChart {
    pub chart: Arc<PlayableChart>,
    pub skin_attempt: bmz_render::snapshot::SkinAttemptState,
    pub source_ln_profile: ChartLnProfile,
    pub chart_length_ms: u64,
    pub render_snapshot_cache: crate::screens::play_snapshot::PlayRenderSnapshotCache,
    pub applied_arrange: AppliedArrange,
    pub score_key: ScoreKey,
    pub assist_runtime: AssistRuntime,
    pub score_save_disabled: bool,
    pub opponent_chart: Option<Arc<PlayableChart>>,
}

pub struct PreloadedPlaySession {
    pub chart: Arc<PlayableChart>,
    pub skin_attempt: bmz_render::snapshot::SkinAttemptState,
    pub source_ln_profile: ChartLnProfile,
    pub chart_length_ms: u64,
    pub audio: AudioEngine,
    pub sample_report: Vec<LoadedSampleReport>,
    pub chart_normalization_gain: f32,
    pub render_snapshot_cache: crate::screens::play_snapshot::PlayRenderSnapshotCache,
    pub applied_arrange: AppliedArrange,
    pub score_key: ScoreKey,
    pub assist_runtime: AssistRuntime,
    pub score_save_disabled: bool,
    pub opponent_chart: Option<Arc<PlayableChart>>,
}

impl PreloadedPlaySession {
    pub fn prepared_chart(&self) -> PreparedPlayChart {
        PreparedPlayChart {
            chart: Arc::clone(&self.chart),
            skin_attempt: self.skin_attempt,
            source_ln_profile: self.source_ln_profile,
            chart_length_ms: self.chart_length_ms,
            render_snapshot_cache: self.render_snapshot_cache.clone(),
            applied_arrange: self.applied_arrange.clone(),
            score_key: self.score_key,
            assist_runtime: self.assist_runtime,
            score_save_disabled: self.score_save_disabled,
            opponent_chart: self.opponent_chart.clone(),
        }
    }

    /// デコード済み PCM と変換済み譜面を共有し、再生状態だけを空にした
    /// 新しい preload を作る。Practice の反復開始で filesystem / FFmpeg の
    /// 再ロードを行わないための beatoraja `BMSResource` 相当の経路。
    pub fn clone_loaded_resources(&self) -> Self {
        Self {
            chart: Arc::clone(&self.chart),
            skin_attempt: self.skin_attempt,
            source_ln_profile: self.source_ln_profile,
            chart_length_ms: self.chart_length_ms,
            audio: AudioEngine::with_sample_bank(
                self.audio.output_sample_rate(),
                self.audio.samples.clone(),
            ),
            sample_report: self.sample_report.clone(),
            chart_normalization_gain: self.chart_normalization_gain,
            render_snapshot_cache: self.render_snapshot_cache.clone(),
            applied_arrange: self.applied_arrange.clone(),
            score_key: self.score_key,
            assist_runtime: self.assist_runtime,
            score_save_disabled: self.score_save_disabled,
            opponent_chart: self.opponent_chart.clone(),
        }
    }
}

impl Default for PlaySessionOptions {
    fn default() -> Self {
        Self {
            play_config_key_mode: None,
            session_mode: SessionMode::Normal,
            autoplay: false,
            key_mode_conversion: KeyModeConversionConfig::Off,
            seven_to_nine_pattern: SevenToNinePattern::default(),
            seven_to_nine_type: SevenToNineType::default(),
            seven_to_nine_rule_mode: SevenToNineRuleMode::default(),
            score_save_disabled: false,
            playback_rate_percent: 100,
            assist: AssistOptionConfig::default(),
            assist_runtime: AssistRuntime::default(),
            replay_player: None,
            battle_opponent: None,
            opponent_chart: None,
            sample_rate: 48_000,
            gauge_override: None,
            opponent_gauge_override: None,
            gauge_auto_shift: GaugeAutoShiftMode::Off,
            bottom_shiftable_gauge: GaugeType::AssistEasy,
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            double_option: DoubleOption::Off,
            hs_fix: HsFixOption::Off,
            target: TargetOption::None,
            resolved_target: None,
            arrange_seed: None,
            arrange_seed_2p: None,
            random_trainer_seed: None,
            legacy_arrange_seed: false,
            s_random_scheme: SRandomScheme::default(),
            s_random_scheme_2p: None,
            h_random_threshold_ms: None,
            bms_random_seed: None,
            bms_random_choices: None,
            bms_switch_choices: None,
            arrange_pattern: None,
            initial_gauge_value: None,
            initial_gauge_values: None,
            initial_course_combo: None,
            judge_constraint: bmz_core::course::CourseJudgeConstraint::Normal,
            speed_constraint: bmz_core::course::CourseSpeedConstraint::Free,
            ln_mode_override: None,
            ln_policy_setting: LnPolicySetting::AutoLn,
            rule_mode: RuleMode::Beatoraja,
            gauge_property: None,
            gamepad_slots: GamepadSlotMap::default(),
        }
    }
}

#[path = "play_session/arrange/algorithm.rs"]
mod arrange_algorithm;
#[path = "play_session/arrange/permutation.rs"]
mod arrange_permutation;
#[path = "play_session/arrange/pipeline.rs"]
mod arrange_pipeline;
#[path = "play_session/arrange/rng.rs"]
mod arrange_rng;
mod build;
mod preload;
#[path = "play_session/seven_to_nine.rs"]
mod seven_to_nine;
#[path = "play_session/seven_to_six.rs"]
mod seven_to_six;
#[path = "play_session/sp_to_dp.rs"]
mod sp_to_dp;

pub(crate) use arrange_pipeline::second_player_lane_mask;
pub use arrange_pipeline::{apply_arrange, apply_arrange_pair, generate_arrange_seed};
pub(crate) use build::judge_algorithm_from_config;
pub use build::{
    apply_placeholder_session_visuals, build_game_session, build_game_session_with_input_backend,
};
pub use preload::{
    ScoredChartMetrics, build_audio_engine_for_chart, build_practice_prepared_from_preloaded,
    build_prepared_play_session_from_preloaded, load_game_session_for_chart,
    load_game_session_for_chart_with_input_backend, load_prepared_play_session_for_chart,
    load_prepared_play_session_for_chart_with_input_backend, load_source_chart_for_chart,
    preload_play_session_for_chart, preload_play_session_for_chart_with_callbacks,
    preload_play_session_for_chart_with_progress,
    preload_play_session_reloading_audio_with_progress, scored_chart_metrics_for_chart,
    scored_chart_metrics_from_prepared, scored_note_count_for_chart,
};
pub use seven_to_nine::{apply_seven_to_nine, seven_to_nine_replay_lane_projection};
pub use seven_to_six::{apply_seven_to_six, normalize_arrange_for_seven_to_six};
pub use sp_to_dp::apply_sp_to_dp;

use arrange_algorithm::*;
use arrange_permutation::*;
use arrange_pipeline::*;
use arrange_rng::*;
#[cfg(test)]
use build::*;
#[cfg(test)]
use preload::*;

#[cfg(test)]
#[path = "play_session/tests.rs"]
mod tests;
