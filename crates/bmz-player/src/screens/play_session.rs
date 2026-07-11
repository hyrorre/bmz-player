use anyhow::{Context, Result, bail};
use bmz_audio::clock::AudioClock;
use bmz_audio::engine::AudioEngine;
use bmz_audio::ffmpeg_loader::FfmpegSampleLoader;
use bmz_audio::loader::{LoadedSampleReport, SampleLoader, load_chart_samples};
use bmz_audio::loudness::{analyze_chart_loudness, play_normalization_gain_for_loudness};
use bmz_chart::import::import_bms_chart;
use bmz_chart::model::{BgaAssetRef, NoteEvent, NoteKind, PlayableChart, TimingEventKind};
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
use bmz_gameplay::input::system::InputSystem;
use bmz_gameplay::input::translator::DefaultInputTranslator;
use bmz_gameplay::judge::engine::JudgeEngine;
use bmz_gameplay::judge::model::{JudgeAlgorithm, JudgeWindow, JudgeWindows};
use bmz_gameplay::judge::window::{
    judge_percent_at_time_for_keymode, judge_windows_for_keymode_and_rule_mode,
    judge_windows_for_rule_mode_and_keymode,
};
use bmz_gameplay::replay::{ReplayPlayer, ReplayRecorder};
use bmz_gameplay::rule::RuleMode;
use bmz_gameplay::score::{ScoreState, scored_note_count};
use bmz_gameplay::session::{
    BgmScheduler, GameSession, HispeedMode, InputOffsetAutoAdjustState, PlaySkinOffset, PlayState,
};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::play::{
    audio_mix_from_profile, bottom_shiftable_gauge_from_config, gauge_auto_shift_from_config,
    gauge_type_from_config, lane_binding_for_chart_with_slots, lane_unit_to_f32,
    play_offsets_from_profile,
};
use crate::config::profile_config::{
    BgaExpandConfig, BgaModeConfig, HispeedModeConfig, JudgeAlgorithmConfig, LaneEffectConfig,
    ProfileConfig,
};
use crate::input::gilrs::GamepadSlotMap;
use crate::ln_policy::{
    LnPolicySetting, apply_ln_policy_to_chart, force_ln_mode_for_chart, score_ln_policy_for_chart,
};
use crate::screens::practice::{
    PracticeProperty, apply_practice_property, apply_practice_start_gauge,
};
use crate::select_options::{ArrangeOption, DoubleOption, HsFixOption, TargetOption};
use crate::storage::library_db::ChartNormalizationAnalysis;
use crate::storage::library_db::LibraryDatabase;
use crate::storage::score_db::ScoreKey;

#[derive(Debug, Clone)]
pub struct PlaySessionOptions {
    pub autoplay: bool,
    /// Practice section play: no score / replay persistence (like autoplay).
    pub practice_mode: bool,
    pub replay_player: Option<ReplayPlayer>,
    pub sample_rate: u32,
    pub gauge_override: Option<GaugeType>,
    pub gauge_auto_shift: GaugeAutoShiftMode,
    pub bottom_shiftable_gauge: GaugeType,
    pub arrange: ArrangeOption,
    pub arrange_2p: ArrangeOption,
    pub double_option: DoubleOption,
    pub hs_fix: HsFixOption,
    pub target: TargetOption,
    pub arrange_seed: Option<i64>,
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppliedArrange {
    pub arrange: ArrangeOption,
    pub arrange_2p: ArrangeOption,
    pub double_option: DoubleOption,
    pub seed: Option<i64>,
    pub pattern: Option<Vec<u8>>,
}

pub struct PreparedPlaySession {
    pub session: GameSession,
    pub audio: AudioEngine,
    pub sample_report: Vec<LoadedSampleReport>,
    pub applied_arrange: AppliedArrange,
    pub score_key: ScoreKey,
    pub target_option: TargetOption,
    pub target: String,
    pub practice_mode: bool,
}

pub struct PreloadedPlaySession {
    pub chart: Arc<PlayableChart>,
    pub audio: AudioEngine,
    pub sample_report: Vec<LoadedSampleReport>,
    pub normalization_gain: f32,
    pub applied_arrange: AppliedArrange,
    pub score_key: ScoreKey,
}

impl Default for PlaySessionOptions {
    fn default() -> Self {
        Self {
            autoplay: false,
            practice_mode: false,
            replay_player: None,
            sample_rate: 48_000,
            gauge_override: None,
            gauge_auto_shift: GaugeAutoShiftMode::Off,
            bottom_shiftable_gauge: GaugeType::AssistEasy,
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            double_option: DoubleOption::Off,
            hs_fix: HsFixOption::Off,
            target: TargetOption::None,
            arrange_seed: None,
            arrange_pattern: None,
            initial_gauge_value: None,
            initial_gauge_values: None,
            initial_course_combo: None,
            judge_constraint: bmz_core::course::CourseJudgeConstraint::Normal,
            ln_mode_override: None,
            ln_policy_setting: LnPolicySetting::AutoLn,
            rule_mode: RuleMode::Beatoraja,
            gauge_property: None,
            gamepad_slots: GamepadSlotMap::default(),
        }
    }
}

/// Play 入場直後 (preload 完了前) の placeholder snapshot に、
/// セッション開始時と同じ初期ゲージ・レーン設定を反映する。
/// `install_active_play` でフルスナップショットに置き換わるまでの間、
/// グルーブゲージや緑数字が空表示になるのを防ぐ。
/// ゲージ選択ロジックは `build_game_session_with_input_backend` と揃えること。
pub fn apply_placeholder_session_visuals(
    snapshot: &mut bmz_render::snapshot::RenderSnapshot,
    profile: &ProfileConfig,
    key_mode: KeyMode,
    options: &PlaySessionOptions,
) {
    let gauge_type =
        options.gauge_override.unwrap_or_else(|| gauge_type_from_config(profile.play.gauge));
    let gauge_auto_shift = if options.gauge_auto_shift != GaugeAutoShiftMode::Off {
        options.gauge_auto_shift
    } else if options.gauge_override.is_none() {
        gauge_auto_shift_from_config(profile.play.gauge, profile.play.gauge_auto_shift)
    } else {
        GaugeAutoShiftMode::Off
    };
    let bottom_shiftable_gauge = if options.gauge_auto_shift != GaugeAutoShiftMode::Off {
        options.bottom_shiftable_gauge
    } else {
        bottom_shiftable_gauge_from_config(profile.play.bottom_shiftable_gauge)
    };
    let gauge_property =
        options.gauge_property.unwrap_or_else(|| GaugeProperty::from_keymode(key_mode));
    // TOTAL は譜面パース前で不明だが、init/max/border は TOTAL 非依存なので
    // ノーツ数由来のデフォルト TOTAL で代用して問題ない。
    let rule_mode = profile.play.rule_mode;
    let gauge_total = gauge_total_for_chart_and_rule_mode(None, snapshot.total_notes, rule_mode);
    let mut gauge = if gauge_auto_shift != GaugeAutoShiftMode::Off {
        GaugeState::new_with_auto_shift_property_and_rule_mode(
            gauge_type,
            gauge_auto_shift,
            gauge_total,
            snapshot.total_notes,
            gauge_property,
            rule_mode,
        )
    } else {
        GaugeState::new_with_property_and_rule_mode(
            gauge_type,
            gauge_total,
            snapshot.total_notes,
            gauge_property,
            rule_mode,
        )
    };
    gauge.set_bottom_shiftable_gauge(bottom_shiftable_gauge);
    if let Some(values) = &options.initial_gauge_values {
        gauge.set_initial_values(values);
    } else if let Some(initial) = options.initial_gauge_value {
        gauge.set_initial_value(initial);
    }
    let current = gauge.current();
    snapshot.gauge = current.value;
    snapshot.gauge_type = current.definition.gauge_type as i32;
    snapshot.gauge_auto_shift = gauge.auto_shift;
    snapshot.gauge_max = current.definition.max;
    snapshot.gauge_border = current.definition.border;

    snapshot.lift = lane_unit_to_f32(profile.lane.lift);
    snapshot.lane_cover = crate::config::play::clamp_lane_cover_for_lift(
        lane_unit_to_f32(profile.lane.sudden),
        snapshot.lift,
    );
    let hispeed_mode = hispeed_mode_from_profile(profile.lane.hispeed_mode);
    snapshot.hispeed = placeholder_hispeed_for_mode(
        profile,
        hispeed_mode,
        profile.lane.target_green_number.max(1),
        snapshot.lane_cover,
        snapshot.lift,
        snapshot.now_bpm,
    );
    snapshot.lanecover_enabled = lanecover_enabled_from_profile(profile);
    snapshot.lift_enabled = lift_enabled_from_profile(profile);
    snapshot.hidden_enabled = hidden_enabled_from_profile(profile);
    snapshot.hidden_cover = hidden_cover_from_profile(profile);

    snapshot.key_mode = key_mode;
    // session 構築時と同じく基準 BPM = initial_bpm (decide snapshot の now_bpm)。
    snapshot.main_bpm = snapshot.now_bpm;
    snapshot.fs_threshold_ms =
        bmz_render::chart_graph::rm_skin_fs_threshold_ms(snapshot.judge_rank, key_mode);
    snapshot.judge_timing_offset_ms =
        (play_offsets_from_profile(profile).visual_offset_us / 1_000) as i32;
    snapshot.judge_timing_auto_adjust = profile.judge.visual_offset_auto_adjust;
    let replay_playback = options.replay_player.is_some();
    snapshot.autoplay = !replay_playback && (profile.play.auto_play || options.autoplay);
    snapshot.replay_playback = replay_playback;
    snapshot.target_ex_score = options.target.target_ex_score(snapshot.total_notes);
    snapshot.target = options.target.as_string();

    snapshot.note_display_duration_ms =
        crate::screens::play_snapshot::display_duration_ms_for_bpm_hispeed(
            snapshot.now_bpm,
            snapshot.hispeed,
            snapshot.lane_cover,
            snapshot.lift,
            1.0,
        )
        .round()
        .clamp(0.0, i32::MAX as f32) as i32;

    let initial_bpm = snapshot.now_bpm.max(1.0);
    let max_bpm = snapshot.max_bpm.max(initial_bpm);
    snapshot.adjusted_cover_progress = bmz_render::chart_graph::compute_adjusted_cover_progress(
        snapshot.hidden_enabled,
        snapshot.lane_cover,
        snapshot.lift,
        snapshot.hsfix_index,
        initial_bpm,
        max_bpm,
        initial_bpm,
    );
    snapshot.adjusted_rate = bmz_render::chart_graph::compute_adjusted_rate(
        snapshot.hidden_enabled,
        snapshot.lanecover_enabled,
        snapshot.hsfix_index,
        initial_bpm,
        max_bpm,
        initial_bpm,
    );
    snapshot.adjusted_rate_adot = snapshot.adjusted_rate.map(|rate| (rate * 100.0).floor() as i32);

    // プロファイルのスキンオフセット (位置調整)。スクラッチ回転角は session が
    // 必要なので install 後の refresh に任せる。
    let mut offsets = bmz_render::skin_offset::SkinOffsetValues::default();
    for offset in skin_offsets_from_profile(profile) {
        offsets.set(
            offset.id,
            bmz_render::skin_offset::SkinOffsetValue {
                x: offset.x,
                y: offset.y,
                w: offset.w,
                h: offset.h,
                r: offset.r,
                a: offset.a,
            },
        );
    }
    snapshot.skin_offsets = offsets;
}

pub fn build_game_session(
    chart: Arc<PlayableChart>,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
) -> GameSession {
    build_game_session_with_input_backend(chart, profile, options, Box::new(NullInputBackend))
}

pub fn build_game_session_with_input_backend(
    chart: Arc<PlayableChart>,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
    input_backend: Box<dyn InputBackend>,
) -> GameSession {
    let gauge_type =
        options.gauge_override.unwrap_or_else(|| gauge_type_from_config(profile.play.gauge));
    let gauge_auto_shift = if options.gauge_auto_shift != GaugeAutoShiftMode::Off {
        options.gauge_auto_shift
    } else if options.gauge_override.is_none() {
        gauge_auto_shift_from_config(profile.play.gauge, profile.play.gauge_auto_shift)
    } else {
        GaugeAutoShiftMode::Off
    };
    let bottom_shiftable_gauge = if options.gauge_auto_shift != GaugeAutoShiftMode::Off {
        options.bottom_shiftable_gauge
    } else {
        bottom_shiftable_gauge_from_config(profile.play.bottom_shiftable_gauge)
    };
    let initial_gauge_value = options.initial_gauge_value;
    let initial_gauge_values = options.initial_gauge_values.clone();
    let initial_course_combo = options.initial_course_combo.unwrap_or(0);
    let replay_player = options.replay_player;
    let is_replay = replay_player.is_some();
    let autoplay_enabled = !is_replay && (profile.play.auto_play || options.autoplay);
    let autoplay = if autoplay_enabled {
        Some(AutoplayController::default())
    } else if options.double_option == DoubleOption::BattleAutoScratch {
        Some(AutoplayController::for_lanes(&[Lane::Scratch, Lane::Scratch2]))
    } else {
        None
    };
    let input_offset_auto_adjust_enabled = profile.judge.visual_offset_auto_adjust;
    let input_offset_auto_adjust =
        if input_offset_auto_adjust_enabled && !autoplay_enabled && !is_replay {
            Some(InputOffsetAutoAdjustState::default())
        } else {
            None
        };
    let key_mode = chart.metadata.key_mode;
    // `chart` is built from the source file and already has the selected LN
    // policy, course override, and double option applied.  Derive the gameplay
    // denominator here instead of using the policy-independent library count.
    let scored_total_notes = scored_note_count(&chart);
    let rule_mode = profile.play.rule_mode;
    let input_system = InputSystem {
        backend: input_backend,
        translator: Box::new(DefaultInputTranslator {
            binding: lane_binding_for_chart_with_slots(
                &profile.input,
                key_mode,
                options.gamepad_slots,
            ),
        }),
    };

    let timing_map = bmz_chart::timing::TimingMap::from_chart_timing_events(
        chart.metadata.initial_bpm,
        &chart.timing_events,
    );
    let hispeed_mode = hispeed_mode_from_profile(profile.lane.hispeed_mode);
    let target_green_number = profile.lane.target_green_number.max(1);
    let lift = lane_unit_to_f32(profile.lane.lift);
    let lane_cover =
        crate::config::play::clamp_lane_cover_for_lift(lane_unit_to_f32(profile.lane.sudden), lift);
    let hsfix_base_bpm = hsfix_base_bpm_for_chart(&chart, &timing_map, options.hs_fix);
    let hispeed = initial_hispeed_for_mode(
        profile,
        hispeed_mode,
        target_green_number,
        lane_cover,
        lift,
        &chart,
        &timing_map,
        options.hs_fix,
    );

    // Course judge constraints narrow the judge window so the corresponding
    // judge band is unreachable: NoGood zeroes good_us, NoGreat zeroes both
    // great_us and good_us.  Mirrors beatoraja JudgeManager's *JudgeWindowRate
    // = 0 path.
    let base_judge_windows = apply_judge_constraint_to_windows(
        judge_windows_for_keymode_and_rule_mode(chart.metadata.key_mode, rule_mode),
        options.judge_constraint,
    );
    let base_judge_window = base_judge_windows.note;

    let mut gauge = {
        let gauge_total = gauge_total_for_chart_and_rule_mode(
            chart.metadata.total,
            scored_total_notes,
            rule_mode,
        );
        // 単曲時はチャートのキーモードから GaugeProperty を導出、コース時は
        // `apply_course_constraints` が CourseGaugeConstraint から決めた値を使う。
        let gauge_property = options
            .gauge_property
            .unwrap_or_else(|| GaugeProperty::from_keymode(chart.metadata.key_mode));
        if gauge_auto_shift != GaugeAutoShiftMode::Off {
            let mut gauge = GaugeState::new_with_auto_shift_property_and_rule_mode(
                gauge_type,
                gauge_auto_shift,
                gauge_total,
                scored_total_notes,
                gauge_property,
                rule_mode,
            );
            gauge.set_bottom_shiftable_gauge(bottom_shiftable_gauge);
            gauge
        } else {
            GaugeState::new_with_property_and_rule_mode(
                gauge_type,
                gauge_total,
                scored_total_notes,
                gauge_property,
                rule_mode,
            )
        }
    };
    // Course play carries the previous chart's gauge value over; this overrides
    // the initial value computed by GaugeState::new* above.
    if let Some(values) = &initial_gauge_values {
        gauge.set_initial_values(values);
    } else if let Some(initial) = initial_gauge_value {
        gauge.set_initial_value(initial);
    }

    GameSession {
        gauge,
        judge: JudgeEngine::new_with_window_set_algorithm_and_keymode(
            judge_windows_for_rule_mode_and_keymode(
                base_judge_windows,
                judge_percent_at_time_for_keymode(
                    chart.metadata.judge_rank_spec,
                    &chart.judge_rank_events,
                    TimeUs(0),
                    chart.metadata.key_mode,
                    rule_mode,
                ),
                rule_mode,
                chart.metadata.key_mode,
            ),
            rule_mode,
            judge_algorithm_from_config(profile.judge.judge_algorithm),
            chart.metadata.key_mode,
        ),
        base_judge_window,
        base_judge_windows,
        rule_mode,
        audio_clock: AudioClock::stopped(options.sample_rate),
        chart,
        scored_total_notes,
        timing_map,
        input_system,
        score: ScoreState::for_rule_mode(key_mode, rule_mode),
        course_combo_carry: initial_course_combo,
        course_combo_carry_active: initial_course_combo > 0,
        course_max_combo: initial_course_combo,
        replay_recorder: ReplayRecorder::default(),
        replay_player,
        autoplay,
        recent_inputs: Vec::new(),
        lane_keyon_started_at: Default::default(),
        lane_keyoff_started_at: Default::default(),
        lane_scratch_direction: Default::default(),
        lane_scratch_angle_delta_ms: Default::default(),
        scratch_angle_last_render_at: None,
        lane_auto_release_at: Default::default(),
        recent_judgements: Vec::new(),
        pending_skin_events: Vec::new(),
        next_skin_event_sequence: 0,
        result_judgements: Default::default(),
        hit_error_ring: HitErrorRing::default(),
        input_offset_auto_adjust_enabled,
        input_offset_auto_adjust,
        gauge_increase_started_at: None,
        gauge_max_started_at: None,
        full_combo_started_at: None,
        bgm_scheduler: BgmScheduler::default(),
        offsets: play_offsets_from_profile(profile),
        audio_mix: audio_mix_from_profile(profile),
        hispeed,
        hispeed_mode,
        target_green_number,
        hsfix_base_bpm,
        lift,
        lane_cover,
        lane_cover_visible: true,
        lane_cover_changing: false,
        lanecover_enabled: lanecover_enabled_from_profile(profile),
        lift_enabled: lift_enabled_from_profile(profile),
        hidden_enabled: hidden_enabled_from_profile(profile),
        hidden_cover: hidden_cover_from_profile(profile),
        skin_offsets: skin_offsets_from_profile(profile),
        bga_enabled: bga_enabled_from_profile(profile, autoplay_enabled, is_replay),
        poor_bga_duration_us: poor_bga_duration_us_from_profile(profile),
        bga_stretch: bga_stretch_from_profile(profile),
        show_ln_tail_cap: profile.play.show_ln_tail_cap,
        lane_hcn_timer: [None; bmz_core::lane::LANE_COUNT],
        lane_hcn_keysound_muted: [None; bmz_core::lane::LANE_COUNT],
        pending_keysounds: Vec::new(),
        pending_keysound_volumes: Vec::new(),
        hsfix_index: hsfix_index_from_option(options.hs_fix),
        input_timestamp_anchor: None,
        pending_mine_hits: Vec::new(),
        state: PlayState::Ready,
        last_hcn_gauge_at: None,
    }
}

fn clamp_hispeed(hispeed: f32) -> f32 {
    hispeed.clamp(0.5, 10.0)
}

fn hsfix_index_from_option(option: HsFixOption) -> i32 {
    match option {
        HsFixOption::Off => 0,
        HsFixOption::StartBpm => 1,
        HsFixOption::MaxBpm => 2,
        HsFixOption::MainBpm => 3,
        HsFixOption::MinBpm => 4,
    }
}

fn apply_judge_constraint_to_windows(
    windows: JudgeWindows,
    constraint: bmz_core::course::CourseJudgeConstraint,
) -> JudgeWindows {
    JudgeWindows {
        note: apply_judge_constraint_to_window(windows.note, constraint),
        scratch: apply_judge_constraint_to_window(windows.scratch, constraint),
        long_note_end: apply_judge_constraint_to_window(windows.long_note_end, constraint),
        long_scratch_end: apply_judge_constraint_to_window(windows.long_scratch_end, constraint),
    }
}

fn apply_judge_constraint_to_window(
    mut window: JudgeWindow,
    constraint: bmz_core::course::CourseJudgeConstraint,
) -> JudgeWindow {
    match constraint {
        bmz_core::course::CourseJudgeConstraint::Normal => {}
        bmz_core::course::CourseJudgeConstraint::NoGood => {
            window.good_us = 0;
        }
        bmz_core::course::CourseJudgeConstraint::NoGreat => {
            window.great_us = 0;
            window.good_us = 0;
        }
    }
    window
}

fn hispeed_mode_from_profile(mode: HispeedModeConfig) -> HispeedMode {
    match mode {
        HispeedModeConfig::Normal => HispeedMode::Normal,
        HispeedModeConfig::Floating => HispeedMode::Floating,
    }
}

fn initial_hispeed_for_mode(
    profile: &ProfileConfig,
    hispeed_mode: HispeedMode,
    target_green_number: u32,
    lane_cover: f32,
    lift: f32,
    chart: &PlayableChart,
    timing_map: &bmz_chart::timing::TimingMap,
    hs_fix: HsFixOption,
) -> f32 {
    if hispeed_mode == HispeedMode::Normal {
        return clamp_hispeed(profile.lane.hispeed);
    }

    let now_bpm = hsfix_base_bpm_for_chart(chart, timing_map, hs_fix);
    let scroll_multiplier =
        crate::screens::play_snapshot::current_scroll_multiplier(chart, timing_map, TimeUs(0));
    let visible_max = crate::config::play::visible_lane_fraction(lane_cover, lift);
    crate::screens::play_snapshot::hispeed_for_green_number_values(
        target_green_number as f32,
        visible_max,
        now_bpm,
        scroll_multiplier,
    )
    .clamp(0.5, 10.0)
}

fn hsfix_base_bpm_for_chart(
    chart: &PlayableChart,
    timing_map: &bmz_chart::timing::TimingMap,
    hs_fix: HsFixOption,
) -> f64 {
    match hs_fix {
        HsFixOption::Off | HsFixOption::StartBpm => chart.metadata.initial_bpm,
        HsFixOption::MinBpm => chart
            .timing_events
            .iter()
            .filter_map(|event| match event.kind {
                TimingEventKind::BpmChange { bpm } => Some(bpm),
                TimingEventKind::Stop { .. } => None,
            })
            .fold(chart.metadata.initial_bpm, f64::min),
        HsFixOption::MaxBpm => chart
            .timing_events
            .iter()
            .filter_map(|event| match event.kind {
                TimingEventKind::BpmChange { bpm } => Some(bpm),
                TimingEventKind::Stop { .. } => None,
            })
            .fold(chart.metadata.initial_bpm, f64::max),
        HsFixOption::MainBpm => main_bpm_for_chart(chart, timing_map),
    }
    .max(1.0)
}

fn main_bpm_for_chart(chart: &PlayableChart, timing_map: &bmz_chart::timing::TimingMap) -> f64 {
    let mut counted = std::collections::HashSet::new();
    let mut counts: Vec<(f64, u32)> = Vec::new();
    for note in chart.lane_notes.iter().flatten() {
        if note.kind == NoteKind::Mine {
            continue;
        }
        counted.insert(note.id);
        let bpm = timing_map.bpm_at_time(note.time);
        if let Some((_, count)) =
            counts.iter_mut().find(|(value, _)| value.to_bits() == bpm.to_bits())
        {
            *count = count.saturating_add(1);
        } else {
            counts.push((bpm, 1));
        }
    }
    for long in &chart.long_notes {
        if !counted.insert(long.start_note_id) {
            continue;
        }
        let bpm = timing_map.bpm_at_time(long.start_time);
        if let Some((_, count)) =
            counts.iter_mut().find(|(value, _)| value.to_bits() == bpm.to_bits())
        {
            *count = count.saturating_add(1);
        } else {
            counts.push((bpm, 1));
        }
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(bpm, _)| bpm)
        .unwrap_or(chart.metadata.initial_bpm)
}

fn placeholder_hispeed_for_mode(
    profile: &ProfileConfig,
    hispeed_mode: HispeedMode,
    target_green_number: u32,
    lane_cover: f32,
    lift: f32,
    now_bpm: f32,
) -> f32 {
    if hispeed_mode == HispeedMode::Normal {
        return clamp_hispeed(profile.lane.hispeed);
    }

    let visible_max = crate::config::play::visible_lane_fraction(lane_cover, lift);
    crate::screens::play_snapshot::hispeed_for_green_number_values(
        target_green_number as f32,
        visible_max,
        now_bpm.max(1.0) as f64,
        1.0,
    )
    .clamp(0.5, 10.0)
}

fn judge_algorithm_from_config(value: JudgeAlgorithmConfig) -> JudgeAlgorithm {
    match value {
        JudgeAlgorithmConfig::Combo => JudgeAlgorithm::Combo,
        JudgeAlgorithmConfig::Duration => JudgeAlgorithm::Duration,
        JudgeAlgorithmConfig::Lowest => JudgeAlgorithm::Lowest,
        JudgeAlgorithmConfig::Score => JudgeAlgorithm::Score,
    }
}

fn hidden_cover_from_profile(profile: &ProfileConfig) -> f32 {
    match profile.play.lane_effect {
        LaneEffectConfig::Hidden | LaneEffectConfig::HiddenSudden => {
            lane_unit_to_f32(profile.lane.hidden)
        }
        LaneEffectConfig::Off | LaneEffectConfig::Sudden => 0.0,
    }
}

fn lanecover_enabled_from_profile(profile: &ProfileConfig) -> bool {
    let lift = lane_unit_to_f32(profile.lane.lift);
    let lane_cover =
        crate::config::play::clamp_lane_cover_for_lift(lane_unit_to_f32(profile.lane.sudden), lift);
    matches!(profile.play.lane_effect, LaneEffectConfig::Sudden | LaneEffectConfig::HiddenSudden)
        || lane_cover > 0.0
}

fn lift_enabled_from_profile(_profile: &ProfileConfig) -> bool {
    true
}

fn hidden_enabled_from_profile(profile: &ProfileConfig) -> bool {
    matches!(profile.play.lane_effect, LaneEffectConfig::Hidden | LaneEffectConfig::HiddenSudden)
}

fn poor_bga_duration_us_from_profile(profile: &ProfileConfig) -> i64 {
    i64::from(profile.play.misslayer_duration_ms.min(5_000)) * 1_000
}

fn bga_stretch_from_profile(profile: &ProfileConfig) -> i32 {
    match profile.play.bga_expand {
        BgaExpandConfig::Full => 0,
        BgaExpandConfig::KeepAspect => 1,
        BgaExpandConfig::Off => 8,
    }
}

fn bga_enabled_from_profile(profile: &ProfileConfig, autoplay: bool, replay: bool) -> bool {
    match profile.play.bga {
        BgaModeConfig::On => true,
        BgaModeConfig::Auto => autoplay || replay,
        BgaModeConfig::Off => false,
    }
}

fn skin_offsets_from_profile(profile: &ProfileConfig) -> Vec<PlaySkinOffset> {
    profile
        .skin
        .offsets
        .iter()
        .copied()
        .map(|offset| PlaySkinOffset {
            id: offset.id,
            x: offset.x,
            y: offset.y,
            w: offset.w,
            h: offset.h,
            r: offset.r,
            a: offset.a,
        })
        .collect()
}

pub fn load_game_session_for_chart(
    library_db: &LibraryDatabase,
    chart_id: i64,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
) -> Result<GameSession> {
    load_game_session_for_chart_with_input_backend(
        library_db,
        chart_id,
        profile,
        options,
        Box::new(NullInputBackend),
    )
}

pub fn load_game_session_for_chart_with_input_backend(
    library_db: &LibraryDatabase,
    chart_id: i64,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
    input_backend: Box<dyn InputBackend>,
) -> Result<GameSession> {
    let Some(path) = library_db.primary_chart_file_path(chart_id)? else {
        bail!("chart file not found for chart id {chart_id}");
    };
    let import =
        import_bms_chart(std::path::Path::new(&path), random_seed_for_chart(&options), true)
            .with_context(|| format!("failed to import chart file: {path}"))?;
    Ok(build_game_session_with_input_backend(
        Arc::new(import.chart),
        profile,
        options,
        input_backend,
    ))
}

/// `import_bms_chart` に渡す BMS `#RANDOM` / `#IF` 解決用 seed。
/// アレンジ seed (リプレイにも保存される) と同じ値を流用することで、
/// 同じ replay を再生したときに RANDOM が必ず同じ分岐へ落ちることを保証する。
fn random_seed_for_chart(options: &PlaySessionOptions) -> Option<u64> {
    options.arrange_seed.map(|s| s as u64)
}

pub fn build_audio_engine_for_chart(
    chart: &PlayableChart,
    sample_rate: u32,
    loader: &mut dyn SampleLoader,
) -> (AudioEngine, Vec<LoadedSampleReport>) {
    let mut audio = AudioEngine::new(sample_rate);
    let sample_report = load_chart_samples(&mut audio, chart, loader);
    (audio, sample_report)
}

pub fn load_prepared_play_session_for_chart(
    library_db: &LibraryDatabase,
    chart_id: i64,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
) -> Result<PreparedPlaySession> {
    load_prepared_play_session_for_chart_with_input_backend(
        library_db,
        chart_id,
        profile,
        options,
        Box::new(NullInputBackend),
    )
}

pub fn load_prepared_play_session_for_chart_with_input_backend(
    library_db: &LibraryDatabase,
    chart_id: i64,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
    input_backend: Box<dyn InputBackend>,
) -> Result<PreparedPlaySession> {
    let preloaded = preload_play_session_for_chart(
        library_db,
        chart_id,
        PlaySessionOptions { rule_mode: profile.play.rule_mode, ..options.clone() },
        profile.audio_mix.normalize_chart_volume,
    )?;
    Ok(build_prepared_play_session_from_preloaded(preloaded, profile, options, input_backend))
}

pub fn preload_play_session_for_chart(
    library_db: &LibraryDatabase,
    chart_id: i64,
    options: PlaySessionOptions,
    normalize_chart_volume: bool,
) -> Result<PreloadedPlaySession> {
    let imported = load_transformed_chart_for_play(library_db, chart_id, &options)?;
    let chart = Arc::new(imported.chart);
    let mut loader = FfmpegSampleLoader::default();
    let (audio, sample_report) =
        build_audio_engine_for_chart(&chart, options.sample_rate, &mut loader);
    let normalization_gain = load_or_compute_normalization_gain(
        library_db,
        chart_id,
        normalize_chart_volume,
        &chart,
        &audio,
    )?;

    Ok(PreloadedPlaySession {
        chart,
        audio,
        sample_report,
        normalization_gain,
        applied_arrange: imported.applied_arrange,
        score_key: imported.score_key,
    })
}

struct TransformedPlayChart {
    chart: PlayableChart,
    applied_arrange: AppliedArrange,
    score_key: ScoreKey,
}

pub fn load_source_chart_for_chart(
    library_db: &LibraryDatabase,
    chart_id: i64,
    random_seed: Option<u64>,
) -> Result<PlayableChart> {
    let Some(path) = library_db.primary_chart_file_path(chart_id)? else {
        bail!("chart file not found for chart id {chart_id}");
    };
    Ok(import_bms_chart(std::path::Path::new(&path), random_seed, true)
        .with_context(|| format!("failed to import chart file: {path}"))?
        .chart)
}

fn load_transformed_chart_for_play(
    library_db: &LibraryDatabase,
    chart_id: i64,
    options: &PlaySessionOptions,
) -> Result<TransformedPlayChart> {
    let mut chart =
        load_source_chart_for_chart(library_db, chart_id, random_seed_for_chart(options))?;
    let applied_double_option =
        options.double_option.normalize_for_key_mode(chart.metadata.key_mode);
    let score_key = ScoreKey::with_options(
        chart.identity.file_sha256,
        score_ln_policy_for_chart(options.ln_policy_setting, &chart),
        applied_double_option.score_bucket(),
        options.rule_mode,
    );
    apply_ln_policy_to_chart(options.ln_policy_setting, &mut chart);
    // Course constraint may force a specific LN mode (Ln/Cn/Hcn) regardless of
    // what the chart declared. Mirrors beatoraja PlayerConfig.setLnmode().
    if let Some(ln_mode) = options.ln_mode_override {
        force_ln_mode_for_chart(ln_mode, &mut chart);
    }
    apply_double_option(&mut chart, applied_double_option);
    let mut applied_arrange = apply_arrange_pair(
        &mut chart,
        options.arrange,
        options.arrange_2p,
        options.arrange_seed,
        options.arrange_pattern.as_deref(),
    );
    applied_arrange.double_option = applied_double_option;

    Ok(TransformedPlayChart { chart, applied_arrange, score_key })
}

pub fn scored_note_count_for_chart(
    library_db: &LibraryDatabase,
    chart_id: i64,
    options: &PlaySessionOptions,
) -> Result<u32> {
    let imported = load_transformed_chart_for_play(library_db, chart_id, options)?;
    Ok(scored_note_count(&imported.chart))
}

pub fn load_chart_bga_assets_for_chart(
    library_db: &LibraryDatabase,
    chart_id: i64,
    options: &PlaySessionOptions,
) -> Result<Vec<BgaAssetRef>> {
    Ok(load_source_chart_for_chart(library_db, chart_id, random_seed_for_chart(options))?
        .bga_assets)
}

pub fn build_practice_prepared_from_preloaded(
    preloaded: PreloadedPlaySession,
    profile: &ProfileConfig,
    property: &PracticeProperty,
    mut options: PlaySessionOptions,
    input_backend: Box<dyn InputBackend>,
) -> PreparedPlaySession {
    let mut chart = (*preloaded.chart).clone();
    let applied_arrange = apply_practice_property(&mut chart, property);
    options.practice_mode = true;
    options.autoplay = false;
    options.replay_player = None;
    options.gauge_override = Some(gauge_type_from_config(property.gauge));
    options.gauge_auto_shift = GaugeAutoShiftMode::Off;
    options.arrange = property.arrange;
    let target = TargetOption::None.as_string();
    let practice_mode = options.practice_mode;
    let mut session =
        build_game_session_with_input_backend(Arc::new(chart), profile, options, input_backend);
    session.audio_mix.normalization_gain = preloaded.normalization_gain;
    apply_practice_start_gauge(&mut session.gauge, property.start_gauge);
    PreparedPlaySession {
        session,
        audio: preloaded.audio,
        sample_report: preloaded.sample_report,
        applied_arrange,
        score_key: preloaded.score_key,
        target_option: TargetOption::None,
        target,
        practice_mode,
    }
}

pub fn build_prepared_play_session_from_preloaded(
    preloaded: PreloadedPlaySession,
    profile: &ProfileConfig,
    mut options: PlaySessionOptions,
    input_backend: Box<dyn InputBackend>,
) -> PreparedPlaySession {
    options.double_option = preloaded.applied_arrange.double_option;
    let target_option = options.target;
    let target = options.target.as_string();
    let practice_mode = options.practice_mode;
    let session =
        build_game_session_with_input_backend(preloaded.chart, profile, options, input_backend);
    let mut session = session;
    session.audio_mix.normalization_gain = preloaded.normalization_gain;
    PreparedPlaySession {
        session,
        audio: preloaded.audio,
        sample_report: preloaded.sample_report,
        applied_arrange: preloaded.applied_arrange,
        score_key: preloaded.score_key,
        target_option,
        target,
        practice_mode,
    }
}

fn load_or_compute_normalization_gain(
    library_db: &LibraryDatabase,
    chart_id: i64,
    normalize_chart_volume: bool,
    chart: &PlayableChart,
    audio: &AudioEngine,
) -> Result<f32> {
    if !normalize_chart_volume {
        return Ok(1.0);
    }
    if let Some(analysis) = library_db.chart_normalization_analysis_by_chart_id(chart_id)? {
        // DB の normalization_gain は -12 LUFS 基準の互換値。再生は loudness から -6 相当へ再計算する。
        return Ok(play_normalization_gain_for_loudness(analysis.loudness_lufs));
    }

    let Some(analysis) = analyze_chart_loudness(chart, &audio.samples, audio.output_sample_rate())
    else {
        tracing::warn!(chart_id, "failed to analyze chart loudness; using unity gain");
        return Ok(1.0);
    };
    let stored = ChartNormalizationAnalysis {
        loudness_lufs: analysis.loudness_lufs,
        normalization_gain: analysis.normalization_gain,
    };
    library_db.write_chart_normalization_analysis(chart_id, stored)?;
    let play_gain = play_normalization_gain_for_loudness(stored.loudness_lufs);
    tracing::info!(
        chart_id,
        loudness_lufs = stored.loudness_lufs,
        normalization_gain = stored.normalization_gain,
        play_normalization_gain = play_gain,
        "stored chart volume normalization analysis"
    );
    Ok(play_gain)
}

pub fn generate_arrange_seed() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as i64).unwrap_or(12345)
}

fn normal_applied_arrange() -> AppliedArrange {
    AppliedArrange {
        arrange: ArrangeOption::Normal,
        arrange_2p: ArrangeOption::Normal,
        double_option: DoubleOption::Off,
        seed: None,
        pattern: None,
    }
}

pub fn apply_arrange(
    chart: &mut PlayableChart,
    arrange: ArrangeOption,
    seed: Option<i64>,
    pattern: Option<&[u8]>,
) -> AppliedArrange {
    let key_mode = chart.metadata.key_mode;
    if arrange_requires_scratch(arrange) && !key_mode_has_scratch(key_mode) {
        return normal_applied_arrange();
    }

    if let Some(perm) = pattern {
        let perm_usize: Vec<usize> = perm.iter().map(|&i| i as usize).collect();
        apply_lane_permutation(chart, &perm_usize);
        return AppliedArrange {
            arrange,
            arrange_2p: ArrangeOption::Normal,
            double_option: DoubleOption::Off,
            seed,
            pattern: Some(perm.to_vec()),
        };
    }

    match arrange {
        ArrangeOption::Normal => normal_applied_arrange(),
        ArrangeOption::Mirror => {
            let perm = mirror_permutation(key_mode);
            apply_lane_permutation(chart, &perm);
            AppliedArrange {
                arrange: ArrangeOption::Mirror,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                seed: None,
                pattern: Some(perm.iter().map(|&i| i as u8).collect()),
            }
        }
        ArrangeOption::Random => {
            let used_seed = seed.unwrap_or_else(generate_arrange_seed);
            let perm = random_lane_permutation(used_seed, key_mode, false);
            apply_lane_permutation(chart, &perm);
            AppliedArrange {
                arrange: ArrangeOption::Random,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                seed: Some(used_seed),
                pattern: Some(perm.iter().map(|&i| i as u8).collect()),
            }
        }
        ArrangeOption::RRandom => {
            let used_seed = seed.unwrap_or_else(generate_arrange_seed);
            let perm = rotate_lane_permutation(used_seed, key_mode, false);
            apply_lane_permutation(chart, &perm);
            AppliedArrange {
                arrange: ArrangeOption::RRandom,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                seed: Some(used_seed),
                pattern: Some(perm.iter().map(|&i| i as u8).collect()),
            }
        }
        ArrangeOption::RandomEx => {
            let used_seed = seed.unwrap_or_else(generate_arrange_seed);
            let perm = random_lane_permutation(used_seed, key_mode, true);
            apply_lane_permutation(chart, &perm);
            AppliedArrange {
                arrange: ArrangeOption::RandomEx,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                seed: Some(used_seed),
                pattern: Some(perm.iter().map(|&i| i as u8).collect()),
            }
        }
        ArrangeOption::FRandom | ArrangeOption::MFRandom => {
            let used_seed = seed.unwrap_or_else(generate_arrange_seed);
            let perm = f_random_lane_permutation(used_seed, key_mode, arrange);
            apply_lane_permutation(chart, &perm);
            AppliedArrange {
                arrange,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                seed: Some(used_seed),
                pattern: Some(perm.iter().map(|&i| i as u8).collect()),
            }
        }
        ArrangeOption::SRandom
        | ArrangeOption::Spiral
        | ArrangeOption::HRandom
        | ArrangeOption::AllScratch
        | ArrangeOption::SRandomEx => {
            let used_seed = seed.unwrap_or_else(generate_arrange_seed);
            apply_note_arrange(chart, arrange, used_seed);
            AppliedArrange {
                arrange,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                seed: Some(used_seed),
                pattern: None,
            }
        }
    }
}

fn arrange_requires_scratch(arrange: ArrangeOption) -> bool {
    matches!(
        arrange,
        ArrangeOption::AllScratch | ArrangeOption::RandomEx | ArrangeOption::SRandomEx
    )
}

fn key_mode_has_scratch(key_mode: KeyMode) -> bool {
    key_mode.active_lanes().iter().any(|&lane| matches!(lane, Lane::Scratch | Lane::Scratch2))
}

pub fn apply_arrange_pair(
    chart: &mut PlayableChart,
    arrange_1p: ArrangeOption,
    arrange_2p: ArrangeOption,
    seed: Option<i64>,
    pattern: Option<&[u8]>,
) -> AppliedArrange {
    if let Some(perm) = pattern {
        let perm_usize: Vec<usize> = perm.iter().map(|&i| i as usize).collect();
        apply_lane_permutation(chart, &perm_usize);
        return AppliedArrange {
            arrange: arrange_1p,
            arrange_2p,
            double_option: DoubleOption::Off,
            seed,
            pattern: Some(perm.to_vec()),
        };
    }

    let key_mode = chart.metadata.key_mode;
    if !matches!(key_mode, KeyMode::K10 | KeyMode::K14) {
        return apply_arrange(chart, arrange_1p, seed, None);
    }

    let used_seed = (arrange_1p.uses_seed() || arrange_2p.uses_seed())
        .then(|| seed.unwrap_or_else(generate_arrange_seed));
    let mut combined_perm: Vec<usize> = (0..LANE_COUNT).collect();
    let mut has_perm = false;

    if let Some(perm) = apply_arrange_side(chart, arrange_1p, used_seed, ArrangeSide::P1) {
        merge_lane_permutation(&mut combined_perm, &perm);
        has_perm = true;
    }
    if let Some(perm) = apply_arrange_side(
        chart,
        arrange_2p,
        used_seed.map(|seed| seed.wrapping_add(0x9e37_79b9)),
        ArrangeSide::P2,
    ) {
        merge_lane_permutation(&mut combined_perm, &perm);
        has_perm = true;
    }

    AppliedArrange {
        arrange: arrange_1p,
        arrange_2p,
        double_option: DoubleOption::Off,
        seed: used_seed,
        pattern: has_perm.then(|| combined_perm.iter().map(|&i| i as u8).collect()),
    }
}

fn apply_double_option(chart: &mut PlayableChart, double_option: DoubleOption) {
    match double_option {
        DoubleOption::Off => return,
        DoubleOption::Flip => {
            if !matches!(chart.metadata.key_mode, KeyMode::K10 | KeyMode::K14) {
                return;
            }
        }
        DoubleOption::Battle | DoubleOption::BattleAutoScratch => {
            apply_battle_double_option(chart);
            return;
        }
    }

    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    for (left, right) in [
        (Lane::Scratch, Lane::Scratch2),
        (Lane::Key1, Lane::Key8),
        (Lane::Key2, Lane::Key9),
        (Lane::Key3, Lane::Key10),
        (Lane::Key4, Lane::Key11),
        (Lane::Key5, Lane::Key12),
        (Lane::Key6, Lane::Key13),
        (Lane::Key7, Lane::Key14),
    ] {
        let left = left.index();
        let right = right.index();
        perm[left] = right;
        perm[right] = left;
    }
    apply_lane_permutation(chart, &perm);
}

fn apply_battle_double_option(chart: &mut PlayableChart) {
    let (next_mode, pairs): (KeyMode, &[(Lane, Lane)]) = match chart.metadata.key_mode {
        KeyMode::K5 => (
            KeyMode::K10,
            &[
                (Lane::Scratch, Lane::Scratch2),
                (Lane::Key1, Lane::Key8),
                (Lane::Key2, Lane::Key9),
                (Lane::Key3, Lane::Key10),
                (Lane::Key4, Lane::Key11),
                (Lane::Key5, Lane::Key12),
            ],
        ),
        KeyMode::K7 => (
            KeyMode::K14,
            &[
                (Lane::Scratch, Lane::Scratch2),
                (Lane::Key1, Lane::Key8),
                (Lane::Key2, Lane::Key9),
                (Lane::Key3, Lane::Key10),
                (Lane::Key4, Lane::Key11),
                (Lane::Key5, Lane::Key12),
                (Lane::Key6, Lane::Key13),
                (Lane::Key7, Lane::Key14),
            ],
        ),
        _ => return,
    };

    let mut next_note_id = next_note_id(chart);
    let mut cloned_ids = std::collections::HashMap::new();
    for &(source, dest) in pairs {
        let source_index = source.index();
        let dest_index = dest.index();
        let clones: Vec<NoteEvent> = chart.lane_notes[source_index]
            .iter()
            .cloned()
            .map(|mut note| {
                let new_id = next_note_id;
                next_note_id.0 = next_note_id.0.saturating_add(1);
                cloned_ids.insert(note.id, new_id);
                note.id = new_id;
                note.lane = dest;
                note
            })
            .collect();
        chart.lane_notes[dest_index].extend(clones);
    }

    let source_to_dest: std::collections::HashMap<_, _> = pairs.iter().copied().collect();
    let mut cloned_long_notes = Vec::new();
    for pair in &chart.long_notes {
        let Some(&dest) = source_to_dest.get(&pair.lane) else {
            continue;
        };
        let (Some(&start_note_id), Some(&end_note_id)) =
            (cloned_ids.get(&pair.start_note_id), cloned_ids.get(&pair.end_note_id))
        else {
            continue;
        };
        let mut cloned = pair.clone();
        cloned.lane = dest;
        cloned.start_note_id = start_note_id;
        cloned.end_note_id = end_note_id;
        cloned_long_notes.push(cloned);
    }
    chart.long_notes.extend(cloned_long_notes);
    chart.total_notes = chart.total_notes.saturating_mul(2);
    chart.metadata.key_mode = next_mode;
}

fn next_note_id(chart: &PlayableChart) -> NoteId {
    let lane_max = chart.lane_notes.iter().flatten().map(|note| note.id.0).max().unwrap_or(0);
    let long_max = chart
        .long_notes
        .iter()
        .flat_map(|pair| [pair.start_note_id.0, pair.end_note_id.0])
        .max()
        .unwrap_or(0);
    NoteId(lane_max.max(long_max).saturating_add(1))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArrangeSide {
    P1,
    P2,
}

fn apply_arrange_side(
    chart: &mut PlayableChart,
    arrange: ArrangeOption,
    seed: Option<i64>,
    side: ArrangeSide,
) -> Option<Vec<usize>> {
    if arrange == ArrangeOption::Normal {
        return None;
    }

    let include_scratch = matches!(
        arrange,
        ArrangeOption::AllScratch | ArrangeOption::RandomEx | ArrangeOption::SRandomEx
    );
    let groups = arrange_lane_groups_for_side(chart.metadata.key_mode, include_scratch, side);
    if groups.is_empty() {
        return None;
    }

    match arrange {
        ArrangeOption::Normal => None,
        ArrangeOption::Mirror => {
            let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
            for group in groups {
                reverse_lane_group(&mut perm, &group);
            }
            apply_lane_permutation(chart, &perm);
            Some(perm)
        }
        ArrangeOption::Random => {
            let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
            let mut rng = SplitMix64::new(seed.unwrap_or_else(generate_arrange_seed));
            for group in groups {
                fisher_yates_shuffle(&mut rng, &group, &mut perm);
            }
            apply_lane_permutation(chart, &perm);
            Some(perm)
        }
        ArrangeOption::RRandom => {
            let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
            let mut rng = SplitMix64::new(seed.unwrap_or_else(generate_arrange_seed));
            for group in groups {
                rotate_lane_group(&mut rng, &group, &mut perm);
            }
            apply_lane_permutation(chart, &perm);
            Some(perm)
        }
        ArrangeOption::RandomEx => {
            let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
            let mut rng = SplitMix64::new(seed.unwrap_or_else(generate_arrange_seed));
            for group in groups {
                fisher_yates_shuffle(&mut rng, &group, &mut perm);
            }
            apply_lane_permutation(chart, &perm);
            Some(perm)
        }
        ArrangeOption::FRandom | ArrangeOption::MFRandom => {
            let perm = f_random_lane_permutation_for_side(
                seed.unwrap_or_else(generate_arrange_seed),
                chart.metadata.key_mode,
                arrange,
                side,
            );
            apply_lane_permutation(chart, &perm);
            Some(perm)
        }
        ArrangeOption::SRandom
        | ArrangeOption::Spiral
        | ArrangeOption::HRandom
        | ArrangeOption::AllScratch
        | ArrangeOption::SRandomEx => {
            apply_note_arrange_for_groups(
                chart,
                arrange,
                seed.unwrap_or_else(generate_arrange_seed),
                &groups,
            );
            None
        }
    }
}

fn merge_lane_permutation(target: &mut [usize], source: &[usize]) {
    for (index, &source_lane) in source.iter().enumerate() {
        if source_lane != index {
            target[index] = source_lane;
        }
    }
}

fn mirror_permutation(key_mode: KeyMode) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    for group in arrange_lane_groups(key_mode, false) {
        reverse_lane_group(&mut perm, &group);
    }
    perm
}

fn random_lane_permutation(seed: i64, key_mode: KeyMode, include_scratch: bool) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    let mut rng = SplitMix64::new(seed);
    for group in arrange_lane_groups(key_mode, include_scratch) {
        fisher_yates_shuffle(&mut rng, &group, &mut perm);
    }
    perm
}

fn f_random_lane_permutation(seed: i64, key_mode: KeyMode, arrange: ArrangeOption) -> Vec<usize> {
    let f_random = shuffle_lane_groups(seed, f_random_lane_groups(key_mode));
    if arrange == ArrangeOption::MFRandom {
        compose_lane_permutations(&f_random, &mirror_permutation(key_mode))
    } else {
        f_random
    }
}

fn f_random_lane_permutation_for_side(
    seed: i64,
    key_mode: KeyMode,
    arrange: ArrangeOption,
    side: ArrangeSide,
) -> Vec<usize> {
    let f_random = shuffle_lane_groups(seed, f_random_lane_groups_for_side(key_mode, side));
    if arrange == ArrangeOption::MFRandom {
        let mirror = mirror_lane_permutation_for_side(key_mode, side);
        compose_lane_permutations(&f_random, &mirror)
    } else {
        f_random
    }
}

fn shuffle_lane_groups(seed: i64, groups: Vec<Vec<usize>>) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    let mut rng = SplitMix64::new(seed);
    for group in groups {
        fisher_yates_shuffle(&mut rng, &group, &mut perm);
    }
    perm
}

fn mirror_lane_permutation_for_side(key_mode: KeyMode, side: ArrangeSide) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    for group in arrange_lane_groups_for_side(key_mode, false, side) {
        reverse_lane_group(&mut perm, &group);
    }
    perm
}

fn compose_lane_permutations(first: &[usize], second: &[usize]) -> Vec<usize> {
    second.iter().map(|&source| first[source]).collect()
}

fn rotate_lane_permutation(seed: i64, key_mode: KeyMode, include_scratch: bool) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    let mut rng = SplitMix64::new(seed);
    for group in arrange_lane_groups(key_mode, include_scratch) {
        rotate_lane_group(&mut rng, &group, &mut perm);
    }
    perm
}

fn rotate_lane_group(rng: &mut SplitMix64, group: &[usize], perm: &mut [usize]) {
    if group.len() < 2 {
        return;
    }
    let inc = rng.next_bool();
    let mut index = rng.next_usize(group.len() - 1);
    if inc {
        index += 1;
    }
    for &lane in group {
        perm[lane] = group[index];
        index =
            if inc { (index + 1) % group.len() } else { (index + group.len() - 1) % group.len() };
    }
}

fn arrange_lane_groups_for_side(
    key_mode: KeyMode,
    include_scratch: bool,
    side: ArrangeSide,
) -> Vec<Vec<usize>> {
    let groups = arrange_lane_groups(key_mode, include_scratch);
    match (key_mode, side) {
        (KeyMode::K10 | KeyMode::K14, ArrangeSide::P1) => groups.into_iter().take(1).collect(),
        (KeyMode::K10 | KeyMode::K14, ArrangeSide::P2) => {
            groups.into_iter().skip(1).take(1).collect()
        }
        (_, ArrangeSide::P1) => groups,
        (_, ArrangeSide::P2) => Vec::new(),
    }
}

fn f_random_lane_groups(key_mode: KeyMode) -> Vec<Vec<usize>> {
    arrange_lane_groups(key_mode, false).into_iter().flat_map(split_f_random_group).collect()
}

fn f_random_lane_groups_for_side(key_mode: KeyMode, side: ArrangeSide) -> Vec<Vec<usize>> {
    arrange_lane_groups_for_side(key_mode, false, side)
        .into_iter()
        .flat_map(split_f_random_group)
        .collect()
}

fn split_f_random_group(group: Vec<usize>) -> Vec<Vec<usize>> {
    let len = group.len();
    if len < 2 {
        return Vec::new();
    }

    let mid = len / 2;
    let mut groups = Vec::with_capacity(2);
    let left = group[..mid].to_vec();
    if left.len() >= 2 {
        groups.push(left);
    }
    let right_start = if len.is_multiple_of(2) { mid } else { mid + 1 };
    let right = group[right_start..].to_vec();
    if right.len() >= 2 {
        groups.push(right);
    }
    groups
}

fn arrange_lane_groups(key_mode: KeyMode, include_scratch: bool) -> Vec<Vec<usize>> {
    let active = key_mode.active_lanes();
    match key_mode {
        KeyMode::K4 | KeyMode::K6 | KeyMode::K9 => {
            vec![active.iter().map(|&lane| lane as usize).collect()]
        }
        KeyMode::K5 | KeyMode::K7 | KeyMode::K8 => {
            vec![
                active
                    .iter()
                    .filter(|&&lane| include_scratch || lane != Lane::Scratch)
                    .map(|&lane| lane as usize)
                    .collect(),
            ]
        }
        KeyMode::K10 | KeyMode::K14 => {
            let p1 = active
                .iter()
                .filter(|&&lane| {
                    matches!(
                        lane,
                        Lane::Scratch
                            | Lane::Key1
                            | Lane::Key2
                            | Lane::Key3
                            | Lane::Key4
                            | Lane::Key5
                            | Lane::Key6
                            | Lane::Key7
                    ) && (include_scratch || lane != Lane::Scratch)
                })
                .map(|&lane| lane as usize)
                .collect();
            let p2 = active
                .iter()
                .filter(|&&lane| {
                    matches!(
                        lane,
                        Lane::Key8
                            | Lane::Key9
                            | Lane::Key10
                            | Lane::Key11
                            | Lane::Key12
                            | Lane::Key13
                            | Lane::Key14
                            | Lane::Scratch2
                    ) && (include_scratch || lane != Lane::Scratch2)
                })
                .map(|&lane| lane as usize)
                .collect();
            vec![p1, p2]
        }
    }
}

fn reverse_lane_group(perm: &mut [usize], lanes: &[usize]) {
    if lanes.len() < 2 {
        return;
    }
    let reversed: Vec<usize> = lanes.iter().rev().copied().collect();
    for (orig, rev) in lanes.iter().zip(reversed.iter()) {
        perm[*orig] = *rev;
    }
}

fn apply_lane_permutation(chart: &mut PlayableChart, perm: &[usize]) {
    let mut old_notes: Vec<Option<Vec<NoteEvent>>> =
        (0..LANE_COUNT).map(|i| Some(std::mem::take(&mut chart.lane_notes[i]))).collect();
    for (new_idx, &old_idx) in perm.iter().enumerate() {
        let new_lane = Lane::ALL[new_idx];
        let notes = old_notes[old_idx].take().unwrap_or_default();
        chart.lane_notes[new_idx] = notes
            .into_iter()
            .map(|mut n| {
                n.lane = new_lane;
                n
            })
            .collect();
    }

    let mut reverse = [0usize; LANE_COUNT];
    for (new_idx, &old_idx) in perm.iter().enumerate() {
        reverse[old_idx] = new_idx;
    }
    for ln in &mut chart.long_notes {
        ln.lane = Lane::ALL[reverse[ln.lane as usize]];
    }
}

fn apply_note_arrange(chart: &mut PlayableChart, arrange: ArrangeOption, seed: i64) {
    let include_scratch = matches!(arrange, ArrangeOption::AllScratch | ArrangeOption::SRandomEx);
    let groups = arrange_lane_groups(chart.metadata.key_mode, include_scratch);
    apply_note_arrange_for_groups(chart, arrange, seed, &groups);
}

fn apply_note_arrange_for_groups(
    chart: &mut PlayableChart,
    arrange: ArrangeOption,
    seed: i64,
    groups: &[Vec<usize>],
) {
    let mut engine = NoteArrangeEngine::new(arrange, seed, groups);
    let mut notes: Vec<NoteEvent> = chart.lane_notes.iter_mut().flat_map(std::mem::take).collect();
    notes.sort_by_key(|note| (note.tick, note.time, note.lane as u8, note.id));

    let mut start_to_end = std::collections::HashMap::new();
    let mut end_to_start = std::collections::HashMap::new();
    for ln in &chart.long_notes {
        start_to_end.insert(ln.start_note_id, ln.end_note_id);
        end_to_start.insert(ln.end_note_id, ln.start_note_id);
    }

    let mut arranged = Vec::with_capacity(notes.len());
    let mut index = 0;
    while index < notes.len() {
        let tick = notes[index].tick;
        let mut end = index + 1;
        while end < notes.len() && notes[end].tick == tick {
            end += 1;
        }
        let mut group_notes = notes[index..end].to_vec();
        engine.arrange_timeline(&mut group_notes, &start_to_end, &end_to_start);
        arranged.extend(group_notes);
        index = end;
    }

    for lane_notes in &mut chart.lane_notes {
        lane_notes.clear();
    }
    let mut start_lane = std::collections::HashMap::new();
    for note in arranged {
        if note.kind == NoteKind::LongStart {
            start_lane.insert(note.id, note.lane);
        }
        chart.lane_notes[note.lane.index()].push(note);
    }
    for ln in &mut chart.long_notes {
        if let Some(&lane) = start_lane.get(&ln.start_note_id) {
            ln.lane = lane;
        }
    }
}

struct NoteArrangeEngine {
    arrange: ArrangeOption,
    rng: SplitMix64,
    groups: Vec<NoteArrangeGroup>,
}

impl NoteArrangeEngine {
    fn new(arrange: ArrangeOption, seed: i64, groups: &[Vec<usize>]) -> Self {
        Self {
            arrange,
            rng: SplitMix64::new(seed),
            groups: groups.iter().map(|lanes| NoteArrangeGroup::new(lanes)).collect(),
        }
    }

    fn arrange_timeline(
        &mut self,
        notes: &mut [NoteEvent],
        start_to_end: &std::collections::HashMap<bmz_core::ids::NoteId, bmz_core::ids::NoteId>,
        end_to_start: &std::collections::HashMap<bmz_core::ids::NoteId, bmz_core::ids::NoteId>,
    ) {
        let time = notes.first().map(|note| note.time).unwrap_or(TimeUs(0));
        for group in &mut self.groups {
            let map = group.randomize(notes, time, self.arrange, &mut self.rng);
            for note in notes.iter_mut() {
                let source = note.lane.index();
                let Some(&dest) = map.get(&source) else {
                    continue;
                };
                note.lane = Lane::ALL[dest];
                if note.kind == NoteKind::LongStart {
                    if start_to_end.contains_key(&note.id) {
                        group.active_ln.insert(source, dest);
                    }
                } else if note.kind == NoteKind::LongEnd && end_to_start.contains_key(&note.id) {
                    group.active_ln.remove(&source);
                }
            }
        }
    }
}

struct NoteArrangeGroup {
    lanes: Vec<usize>,
    last_note_time: std::collections::HashMap<usize, TimeUs>,
    active_ln: std::collections::HashMap<usize, usize>,
    spiral_increment: usize,
    spiral_head: usize,
    scratch_lanes: Vec<usize>,
    scratch_index: usize,
}

impl NoteArrangeGroup {
    fn new(lanes: &[usize]) -> Self {
        let scratch_lanes: Vec<usize> = lanes
            .iter()
            .copied()
            .filter(|&lane| lane == Lane::Scratch.index() || lane == Lane::Scratch2.index())
            .collect();
        Self {
            lanes: lanes.to_vec(),
            last_note_time: lanes.iter().copied().map(|lane| (lane, TimeUs(-10_000_000))).collect(),
            active_ln: std::collections::HashMap::new(),
            spiral_increment: 0,
            spiral_head: 0,
            scratch_lanes,
            scratch_index: 0,
        }
    }

    fn randomize(
        &mut self,
        notes: &[NoteEvent],
        time: TimeUs,
        arrange: ArrangeOption,
        rng: &mut SplitMix64,
    ) -> std::collections::HashMap<usize, usize> {
        if self.lanes.is_empty() {
            return std::collections::HashMap::new();
        }
        if arrange == ArrangeOption::Spiral {
            return self.spiral_map(rng);
        }

        let mut changeable = self.changeable_lanes();
        let mut assignable = self.assignable_lanes();
        let mut map = std::collections::HashMap::new();
        map.extend(self.active_ln.iter().map(|(&source, &dest)| (source, dest)));

        if arrange == ArrangeOption::AllScratch {
            self.assign_all_scratch(notes, time, rng, &mut changeable, &mut assignable, &mut map);
        }

        let threshold = match arrange {
            ArrangeOption::SRandom => TimeUs(40_000),
            ArrangeOption::SRandomEx => TimeUs(40_000),
            ArrangeOption::HRandom | ArrangeOption::AllScratch => TimeUs(100_000),
            _ => TimeUs(40_000),
        };
        map.extend(self.time_based_shuffle(notes, time, threshold, rng, changeable, assignable));
        map
    }

    fn changeable_lanes(&self) -> Vec<usize> {
        self.lanes.iter().copied().filter(|lane| !self.active_ln.contains_key(lane)).collect()
    }

    fn assignable_lanes(&self) -> Vec<usize> {
        self.lanes
            .iter()
            .copied()
            .filter(|lane| !self.active_ln.values().any(|active| active == lane))
            .collect()
    }

    fn time_based_shuffle(
        &mut self,
        notes: &[NoteEvent],
        time: TimeUs,
        threshold: TimeUs,
        rng: &mut SplitMix64,
        changeable: Vec<usize>,
        assignable: Vec<usize>,
    ) -> std::collections::HashMap<usize, usize> {
        let mut note_lane = Vec::new();
        let mut empty_lane = Vec::new();
        for lane in changeable {
            if notes.iter().any(|note| note.lane.index() == lane && note.kind != NoteKind::Mine) {
                note_lane.push(lane);
            } else {
                empty_lane.push(lane);
            }
        }

        let mut primary_lane = Vec::new();
        let mut inferior_lane = Vec::new();
        for lane in assignable {
            let last = self.last_note_time.get(&lane).copied().unwrap_or(TimeUs(-10_000_000));
            if time.0 - last.0 > threshold.0 {
                primary_lane.push(lane);
            } else {
                inferior_lane.push(lane);
            }
        }

        let mut map = std::collections::HashMap::new();
        while !note_lane.is_empty() && !primary_lane.is_empty() {
            let index = rng.next_usize(primary_lane.len());
            map.insert(note_lane.remove(0), primary_lane.remove(index));
        }
        while !note_lane.is_empty() && !inferior_lane.is_empty() {
            let min_time = inferior_lane
                .iter()
                .filter_map(|lane| self.last_note_time.get(lane))
                .map(|time| time.0)
                .min()
                .unwrap_or(-10_000_000);
            let candidates: Vec<usize> = inferior_lane
                .iter()
                .copied()
                .filter(|lane| {
                    self.last_note_time.get(lane).map(|time| time.0).unwrap_or(-10_000_000)
                        == min_time
                })
                .collect();
            let dest = candidates[rng.next_usize(candidates.len())];
            map.insert(note_lane.remove(0), dest);
            inferior_lane.retain(|&lane| lane != dest);
        }

        primary_lane.extend(inferior_lane);
        while !empty_lane.is_empty() && !primary_lane.is_empty() {
            let index = rng.next_usize(primary_lane.len());
            map.insert(empty_lane.remove(0), primary_lane.remove(index));
        }

        for (&source, &dest) in &map {
            if notes.iter().any(|note| note.lane.index() == source && note.kind != NoteKind::Mine) {
                self.last_note_time.insert(dest, time);
            }
        }
        map
    }

    fn spiral_map(&mut self, rng: &mut SplitMix64) -> std::collections::HashMap<usize, usize> {
        if self.lanes.len() < 2 {
            return std::collections::HashMap::new();
        }
        if self.spiral_increment == 0 {
            self.spiral_increment = rng.next_usize(self.lanes.len() - 1) + 1;
        }
        let changeable = self.changeable_lanes();
        if changeable.len() == self.lanes.len() {
            self.spiral_head = (self.spiral_head + self.spiral_increment) % self.lanes.len();
        }
        let mut map = std::collections::HashMap::new();
        map.extend(self.active_ln.iter().map(|(&source, &dest)| (source, dest)));
        for (index, &lane) in self.lanes.iter().enumerate() {
            if changeable.contains(&lane) {
                map.insert(lane, self.lanes[(index + self.spiral_head) % self.lanes.len()]);
            }
        }
        map
    }

    fn assign_all_scratch(
        &mut self,
        notes: &[NoteEvent],
        time: TimeUs,
        _rng: &mut SplitMix64,
        changeable: &mut Vec<usize>,
        assignable: &mut Vec<usize>,
        map: &mut std::collections::HashMap<usize, usize>,
    ) {
        if self.scratch_lanes.is_empty() {
            return;
        }
        let scratch = self.scratch_lanes[self.scratch_index];
        let last = self.last_note_time.get(&scratch).copied().unwrap_or(TimeUs(-10_000_000));
        if !assignable.contains(&scratch) || time.0 - last.0 <= 40_000 {
            return;
        }
        let Some(source) = changeable.iter().copied().find(|&lane| {
            notes.iter().any(|note| note.lane.index() == lane && note.kind != NoteKind::Mine)
        }) else {
            return;
        };
        map.insert(source, scratch);
        changeable.retain(|&lane| lane != source);
        assignable.retain(|&lane| lane != scratch);
        self.last_note_time.insert(scratch, time);
        self.scratch_index = (self.scratch_index + 1) % self.scratch_lanes.len();
    }
}

#[derive(Debug, Clone)]
struct SplitMix64 {
    seed: u64,
}

impl SplitMix64 {
    fn new(seed: i64) -> Self {
        Self { seed: seed as u64 }
    }

    fn next_u64(&mut self) -> u64 {
        self.seed = self.seed.wrapping_add(0x9E3779B97F4A7C15);
        let mut value = self.seed;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D049BB133111EB);
        value ^ (value >> 31)
    }

    fn next_bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    fn next_usize(&mut self, bound: usize) -> usize {
        assert!(bound > 0);
        let bound = bound as u128;
        let zone = ((1u128 << 64) / bound) * bound;
        loop {
            let value = self.next_u64() as u128;
            if value < zone {
                return (value % bound) as usize;
            }
        }
    }
}

fn fisher_yates_shuffle(rng: &mut SplitMix64, lanes: &[usize], perm: &mut [usize]) {
    if lanes.len() < 2 {
        return;
    }
    let mut indices: Vec<usize> = lanes.to_vec();
    for i in (1..indices.len()).rev() {
        let j = rng.next_usize(i + 1);
        indices.swap(i, j);
    }
    for (orig, new_target) in lanes.iter().zip(indices.iter()) {
        perm[*orig] = *new_target;
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use bmz_audio::loader::LoadedSampleStatus;
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, PlayableChart, SoundAssetRef};
    use bmz_core::clear::GaugeType;
    use bmz_core::ids::{NoteId, SoundId};
    use bmz_core::input::InputKind;
    use bmz_core::lane::{KeyMode, Lane};
    use bmz_core::time::TimeUs;
    use bmz_gameplay::input::backend::{
        BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
    };
    use bmz_gameplay::input::translator::InputTimingContext;
    use bmz_gameplay::rule::RuleMode;
    use rusqlite::Connection;

    use super::*;
    use crate::storage::common::configure_connection;
    use crate::storage::library_db::{ChartImportRecord, LibraryDatabase};
    use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};

    #[test]
    fn build_game_session_uses_profile_play_settings() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.play.auto_play = true;
        profile.judge.input_offset_us = 123;
        let chart = Arc::new(chart());

        let session = build_game_session(chart, &profile, PlaySessionOptions::default());

        assert_eq!(session.state, PlayState::Ready);
        assert_eq!(session.gauge.selected, GaugeType::Normal);
        assert!(session.autoplay.is_some());
        assert_eq!(session.offsets.input_offset_us, 123);
        assert!((session.audio_mix.master_volume - 0.5).abs() < 1e-6);
        assert_eq!(session.audio_clock.sample_rate, 48_000);
        assert_eq!(session.hispeed, 2.0);
        assert_eq!(session.hidden_cover, 0.0);
        assert!(session.bga_enabled);
        assert_eq!(session.poor_bga_duration_us, 500_000);
        assert_eq!(session.bga_stretch, 1);
    }

    #[test]
    fn build_game_session_uses_visual_offset_auto_adjust_from_profile() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.judge.visual_offset_auto_adjust = true;
        let session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        assert!(session.input_offset_auto_adjust_enabled);
        assert!(session.input_offset_auto_adjust.is_some());
    }

    #[test]
    fn build_game_session_applies_judge_algorithm_from_profile() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.judge.judge_algorithm = JudgeAlgorithmConfig::Duration;

        let duration =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        assert_eq!(duration.judge.algorithm, JudgeAlgorithm::Duration);

        profile.judge.judge_algorithm = JudgeAlgorithmConfig::Score;
        let score = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        assert_eq!(score.judge.algorithm, JudgeAlgorithm::Score);
    }

    #[test]
    fn placeholder_session_visuals_use_visual_offset_for_skin_judge_timing() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.judge.input_offset_us = 3_000;
        profile.judge.visual_offset_us = 4_000;
        profile.judge.visual_offset_auto_adjust = true;
        let options = PlaySessionOptions::default();
        let mut snapshot = bmz_render::snapshot::RenderSnapshot::default();

        apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);

        assert_eq!(snapshot.judge_timing_offset_ms, 4);
        assert!(snapshot.judge_timing_auto_adjust);
    }

    #[test]
    fn placeholder_session_visuals_initialize_floating_hispeed_for_ready_display() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.lane.hispeed_mode = HispeedModeConfig::Floating;
        profile.lane.target_green_number = 300;
        // Stale value from a different BPM should not leak into READY display.
        profile.lane.hispeed = 4.0;
        let options = PlaySessionOptions::default();
        let mut snapshot =
            bmz_render::snapshot::RenderSnapshot { now_bpm: 240.0, ..Default::default() };

        apply_placeholder_session_visuals(&mut snapshot, &profile, KeyMode::K7, &options);

        assert!((snapshot.hispeed - 2.0).abs() < f32::EPSILON);
        assert_eq!(snapshot.note_display_duration_ms, 500);
    }

    fn class_gauge_values(session: &GameSession) -> [f32; 6] {
        session
            .gauge
            .gauges
            .iter()
            .find(|g| g.definition.gauge_type == GaugeType::Class)
            .map(|g| g.definition.values)
            .expect("Class gauge present")
    }

    #[test]
    fn build_game_session_picks_gauge_property_from_chart_keymode() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut chart_k5 = chart();
        chart_k5.metadata.key_mode = KeyMode::K5;
        let mut chart_k7 = chart();
        chart_k7.metadata.key_mode = KeyMode::K7;

        let session_k5 =
            build_game_session(Arc::new(chart_k5), &profile, PlaySessionOptions::default());
        let session_k7 =
            build_game_session(Arc::new(chart_k7), &profile, PlaySessionOptions::default());

        // FIVEKEYS CLASS: PG/GR=0.01, BAD=-0.5。SEVENKEYS CLASS: PG=0.15, BAD=-1.5。
        assert_eq!(class_gauge_values(&session_k5)[0], 0.01);
        assert_eq!(class_gauge_values(&session_k5)[3], -0.5);
        assert_eq!(class_gauge_values(&session_k7)[0], 0.15);
        assert_eq!(class_gauge_values(&session_k7)[3], -1.5);
    }

    #[test]
    fn build_game_session_uses_gauge_property_override() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        // チャートは K7 だが、option で LR2 を強制する。
        let options =
            PlaySessionOptions { gauge_property: Some(GaugeProperty::Lr2), ..Default::default() };
        let session = build_game_session(Arc::new(chart()), &profile, options);

        // LR2 CLASS: BAD=-2.0、PG=0.10。
        assert_eq!(class_gauge_values(&session)[3], -2.0);
        assert_eq!(class_gauge_values(&session)[0], 0.10);
    }

    #[test]
    fn build_game_session_applies_lr2oraja_rule_mode() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.play.rule_mode = RuleMode::Lr2Oraja;

        let session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        assert_eq!(session.rule_mode, RuleMode::Lr2Oraja);
        assert_eq!(session.base_judge_window.pgreat_us, 21_000);
        assert_eq!(session.base_judge_window.empty_poor_slow_us, 0);
        let hard = session
            .gauge
            .gauges
            .iter()
            .find(|g| g.definition.gauge_type == GaugeType::Hard)
            .expect("Hard gauge present");
        assert_eq!(hard.definition.guts, &[(32.0, 0.6)]);
        assert_eq!(hard.definition.death, 2.0);
    }

    #[test]
    fn build_game_session_applies_dx_rule_mode() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.play.rule_mode = RuleMode::Dx;

        let session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        assert_eq!(session.rule_mode, RuleMode::Dx);
        assert_eq!(session.base_judge_window.pgreat_us, 16_666);
        assert_eq!(session.judge.windows.pgreat_us, 16_666);
        let hard = session
            .gauge
            .gauges
            .iter()
            .find(|g| g.definition.gauge_type == GaugeType::Hard)
            .expect("Hard gauge present");
        assert_eq!(hard.definition.values, [0.16, 0.16, 0.0, -4.5, -9.0, -4.5]);
    }

    #[test]
    fn build_game_session_sets_empty_poor_combo_policy_from_keymode() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut chart_k5 = chart();
        chart_k5.metadata.key_mode = KeyMode::K5;
        let mut chart_k7 = chart();
        chart_k7.metadata.key_mode = KeyMode::K7;

        let session_k5 =
            build_game_session(Arc::new(chart_k5), &profile, PlaySessionOptions::default());
        let session_k7 =
            build_game_session(Arc::new(chart_k7), &profile, PlaySessionOptions::default());

        assert!(session_k5.score.empty_poor_breaks_combo);
        assert!(!session_k7.score.empty_poor_breaks_combo);
    }

    #[test]
    fn mirror_permutation_k9_reverses_all_nine_keys() {
        let perm = mirror_permutation(KeyMode::K9);
        assert_eq!(perm[Lane::Key1 as usize], Lane::Key9 as usize);
        assert_eq!(perm[Lane::Key9 as usize], Lane::Key1 as usize);
        assert_eq!(perm[Lane::Key5 as usize], Lane::Key5 as usize);
    }

    #[test]
    fn arrange_lane_groups_cover_no_scratch_keymodes() {
        for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
            let expected: Vec<usize> =
                key_mode.active_lanes().iter().map(|&lane| lane.index()).collect();

            assert_eq!(arrange_lane_groups(key_mode, false), vec![expected.clone()]);
            assert_eq!(arrange_lane_groups(key_mode, true), vec![expected]);
        }
    }

    #[test]
    fn mirror_permutation_reverses_no_scratch_keymodes() {
        for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
            let perm = mirror_permutation(key_mode);
            let active = key_mode.active_lanes();

            for (source, dest) in active.iter().zip(active.iter().rev()) {
                assert_eq!(
                    perm[source.index()],
                    dest.index(),
                    "mirror should reverse {} lane {:?}",
                    key_mode.as_str(),
                    source
                );
            }
        }
    }

    #[test]
    fn random_lane_permutation_k9_preserves_active_lanes() {
        let perm = random_lane_permutation(42, KeyMode::K9, false);
        let active: HashSet<_> =
            KeyMode::K9.active_lanes().iter().map(|&lane| lane as usize).collect();
        let mapped: HashSet<_> =
            KeyMode::K9.active_lanes().iter().map(|&lane| perm[lane as usize]).collect();
        assert_eq!(active, mapped);
    }

    #[test]
    fn random_permutations_preserve_no_scratch_active_lanes() {
        for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
            let active: HashSet<_> =
                key_mode.active_lanes().iter().map(|&lane| lane.index()).collect();
            for perm in [
                random_lane_permutation(42, key_mode, false),
                random_lane_permutation(42, key_mode, true),
                rotate_lane_permutation(42, key_mode, false),
                rotate_lane_permutation(42, key_mode, true),
            ] {
                let mapped: HashSet<_> =
                    key_mode.active_lanes().iter().map(|&lane| perm[lane.index()]).collect();
                assert_eq!(
                    active,
                    mapped,
                    "random permutation should stay inside {} active lanes",
                    key_mode.as_str()
                );
            }
        }
    }

    #[test]
    fn f_random_groups_keep_odd_center_lane_fixed() {
        assert_eq!(
            f_random_lane_groups(KeyMode::K7),
            vec![
                vec![Lane::Key1.index(), Lane::Key2.index(), Lane::Key3.index()],
                vec![Lane::Key5.index(), Lane::Key6.index(), Lane::Key7.index()],
            ]
        );
        assert_eq!(
            f_random_lane_groups(KeyMode::K5),
            vec![
                vec![Lane::Key1.index(), Lane::Key2.index()],
                vec![Lane::Key4.index(), Lane::Key5.index()],
            ]
        );
        assert_eq!(
            f_random_lane_groups(KeyMode::K9),
            vec![
                vec![
                    Lane::Key1.index(),
                    Lane::Key2.index(),
                    Lane::Key3.index(),
                    Lane::Key4.index(),
                ],
                vec![
                    Lane::Key6.index(),
                    Lane::Key7.index(),
                    Lane::Key8.index(),
                    Lane::Key9.index(),
                ],
            ]
        );
    }

    #[test]
    fn f_random_groups_split_even_key_modes_into_halves() {
        assert_eq!(
            f_random_lane_groups(KeyMode::K4),
            vec![
                vec![Lane::Key1.index(), Lane::Key2.index()],
                vec![Lane::Key3.index(), Lane::Key4.index()],
            ]
        );
        assert_eq!(
            f_random_lane_groups(KeyMode::K8),
            vec![
                vec![
                    Lane::Key1.index(),
                    Lane::Key2.index(),
                    Lane::Key3.index(),
                    Lane::Key4.index(),
                ],
                vec![
                    Lane::Key5.index(),
                    Lane::Key6.index(),
                    Lane::Key7.index(),
                    Lane::Key8.index(),
                ],
            ]
        );
    }

    #[test]
    fn f_random_keeps_7k_center_lane_in_place() {
        let mut chart = chart();
        chart.metadata.key_mode = KeyMode::K7;
        chart.lane_notes[Lane::Key4.index()].push(note(1, Lane::Key4, 1_000_000));

        let applied = apply_arrange(&mut chart, ArrangeOption::FRandom, Some(42), None);

        assert_eq!(applied.arrange, ArrangeOption::FRandom);
        assert_eq!(applied.seed, Some(42));
        assert_eq!(chart.lane_notes[Lane::Key4.index()][0].lane, Lane::Key4);
        assert_eq!(chart.lane_notes[Lane::Key4.index()][0].id, NoteId(1));
    }

    #[test]
    fn mf_random_applies_mirror_after_f_random() {
        let f_random = f_random_lane_permutation(42, KeyMode::K7, ArrangeOption::FRandom);
        let mf_random = f_random_lane_permutation(42, KeyMode::K7, ArrangeOption::MFRandom);
        let mirror = mirror_permutation(KeyMode::K7);

        assert_eq!(mf_random, compose_lane_permutations(&f_random, &mirror));
        assert_eq!(mf_random[Lane::Key4.index()], Lane::Key4.index());
    }

    #[test]
    fn scratch_required_arrange_falls_back_to_normal_without_scratch_lane() {
        for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
            for arrange in
                [ArrangeOption::AllScratch, ArrangeOption::RandomEx, ArrangeOption::SRandomEx]
            {
                let mut chart = chart();
                chart.metadata.key_mode = key_mode;
                chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
                let before = lanes_for_notes(&chart);

                let applied = apply_arrange(&mut chart, arrange, Some(7), None);

                assert_eq!(applied.arrange, ArrangeOption::Normal);
                assert_eq!(applied.seed, None);
                assert_eq!(applied.pattern, None);
                assert_eq!(lanes_for_notes(&chart), before);
            }
        }
    }

    #[test]
    fn scratch_required_arrange_ignores_replay_pattern_without_scratch_lane() {
        for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
            let mut chart = chart();
            chart.metadata.key_mode = key_mode;
            chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
            let before = lanes_for_notes(&chart);

            let mut pattern: Vec<u8> = (0u8..LANE_COUNT as u8).collect();
            pattern[Lane::Key1.index()] = Lane::Key2.index() as u8;
            pattern[Lane::Key2.index()] = Lane::Key1.index() as u8;

            let applied =
                apply_arrange(&mut chart, ArrangeOption::RandomEx, Some(7), Some(&pattern));

            assert_eq!(applied.arrange, ArrangeOption::Normal);
            assert_eq!(applied.seed, None);
            assert_eq!(applied.pattern, None);
            assert_eq!(lanes_for_notes(&chart), before);
        }
    }

    #[test]
    fn note_arrange_keeps_no_scratch_modes_inside_active_lanes() {
        for key_mode in [KeyMode::K4, KeyMode::K6, KeyMode::K8, KeyMode::K9] {
            for arrange in [ArrangeOption::SRandom, ArrangeOption::Spiral, ArrangeOption::HRandom] {
                let mut chart = chart();
                chart.metadata.key_mode = key_mode;
                for (index, &lane) in key_mode.active_lanes().iter().enumerate() {
                    chart.lane_notes[lane.index()].push(note(
                        (index + 1) as u32,
                        lane,
                        1_000_000 + index as i64 * 1_000,
                    ));
                }

                apply_arrange(&mut chart, arrange, Some(7), None);

                let active: HashSet<_> =
                    key_mode.active_lanes().iter().map(|&lane| lane.index()).collect();
                for note in chart.lane_notes.iter().flatten() {
                    assert!(
                        active.contains(&note.lane.index()),
                        "{arrange:?} should keep {} note {:?} inside active lanes",
                        key_mode.as_str(),
                        note.id
                    );
                }
            }
        }
    }

    #[test]
    fn splitmix64_matches_known_seed_zero_outputs() {
        let mut rng = SplitMix64::new(0);

        assert_eq!(rng.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(rng.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(rng.next_u64(), 0x06C4_5D18_8009_454F);
    }

    #[test]
    fn apply_arrange_random_moves_notes_between_lanes() {
        use bmz_chart::model::{NoteEvent, NoteKind};
        use bmz_core::time::ChartTick;

        let mut chart = chart();
        chart.metadata.key_mode = KeyMode::K7;
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(1_000_000),
            sound: None,
            damage: None,
        });

        let applied = apply_arrange(&mut chart, ArrangeOption::Random, Some(42), None);

        assert_eq!(applied.arrange, ArrangeOption::Random);
        assert_ne!(applied.pattern, Some((0u8..LANE_COUNT as u8).collect()));
        assert!(chart.lane_notes[Lane::Key1.index()].is_empty());
        assert!(chart.lane_notes.iter().enumerate().any(|(lane_index, notes)| lane_index
            != Lane::Key1.index()
            && notes.iter().any(|note| note.id == NoteId(1) && note.lane.index() == lane_index)));
    }

    #[test]
    fn rotate_random_uses_non_identity_lane_rotation() {
        let perm = rotate_lane_permutation(7, KeyMode::K7, false);
        let key_lanes: Vec<usize> = (Lane::Key1.index()..=Lane::Key7.index()).collect();
        let mapped: HashSet<_> = key_lanes.iter().map(|&lane| perm[lane]).collect();

        assert_eq!(mapped, key_lanes.iter().copied().collect());
        assert!(key_lanes.iter().any(|&lane| perm[lane] != lane));
        assert_eq!(perm[Lane::Scratch.index()], Lane::Scratch.index());
    }

    #[test]
    fn random_ex_includes_scratch_lane() {
        let mut chart = chart();
        chart.metadata.key_mode = KeyMode::K7;
        chart.lane_notes[Lane::Scratch.index()].push(note(1, Lane::Scratch, 1_000_000));

        let applied = apply_arrange(&mut chart, ArrangeOption::RandomEx, Some(1), None);

        assert_eq!(applied.arrange, ArrangeOption::RandomEx);
        assert!(chart.lane_notes.iter().enumerate().any(|(lane_index, notes)| lane_index
            != Lane::Scratch.index()
            && notes.iter().any(|note| note.id == NoteId(1) && note.lane.index() == lane_index)));
    }

    #[test]
    fn random2_arranges_only_dp_second_player_lanes() {
        let mut chart = chart();
        chart.metadata.key_mode = KeyMode::K14;
        chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
        chart.lane_notes[Lane::Key8.index()].push(note(2, Lane::Key8, 1_000_000));

        let applied = apply_arrange_pair(
            &mut chart,
            ArrangeOption::Normal,
            ArrangeOption::Mirror,
            None,
            None,
        );

        assert_eq!(applied.arrange, ArrangeOption::Normal);
        assert_eq!(chart.lane_notes[Lane::Key1.index()][0].id, NoteId(1));
        assert!(chart.lane_notes[Lane::Key8.index()].is_empty());
        assert!(
            chart.lane_notes[Lane::Key14.index()]
                .iter()
                .any(|note| note.id == NoteId(2) && note.lane == Lane::Key14)
        );
    }

    #[test]
    fn double_option_flip_swaps_dp_player_lanes() {
        let mut chart = chart();
        chart.metadata.key_mode = KeyMode::K14;
        chart.lane_notes[Lane::Scratch.index()].push(note(1, Lane::Scratch, 1_000_000));
        chart.lane_notes[Lane::Key1.index()].push(note(2, Lane::Key1, 1_000_000));
        chart.lane_notes[Lane::Scratch2.index()].push(note(3, Lane::Scratch2, 1_000_000));
        chart.lane_notes[Lane::Key8.index()].push(note(4, Lane::Key8, 1_000_000));

        apply_double_option(&mut chart, DoubleOption::Flip);

        assert!(
            chart.lane_notes[Lane::Scratch2.index()]
                .iter()
                .any(|note| note.id == NoteId(1) && note.lane == Lane::Scratch2)
        );
        assert!(
            chart.lane_notes[Lane::Key8.index()]
                .iter()
                .any(|note| note.id == NoteId(2) && note.lane == Lane::Key8)
        );
        assert!(
            chart.lane_notes[Lane::Scratch.index()]
                .iter()
                .any(|note| note.id == NoteId(3) && note.lane == Lane::Scratch)
        );
        assert!(
            chart.lane_notes[Lane::Key1.index()]
                .iter()
                .any(|note| note.id == NoteId(4) && note.lane == Lane::Key1)
        );
    }

    #[test]
    fn double_option_battle_duplicates_sp_lanes_as_dp() {
        let mut chart = chart();
        chart.metadata.key_mode = KeyMode::K7;
        chart.total_notes = 2;
        chart.lane_notes[Lane::Scratch.index()].push(note(1, Lane::Scratch, 1_000_000));
        chart.lane_notes[Lane::Key1.index()].push(note(2, Lane::Key1, 1_010_000));

        apply_double_option(&mut chart, DoubleOption::Battle);

        assert_eq!(chart.metadata.key_mode, KeyMode::K14);
        assert_eq!(chart.total_notes, 4);
        assert!(
            chart.lane_notes[Lane::Scratch.index()]
                .iter()
                .any(|note| note.id == NoteId(1) && note.lane == Lane::Scratch)
        );
        assert!(
            chart.lane_notes[Lane::Scratch2.index()]
                .iter()
                .any(|note| note.id != NoteId(1) && note.lane == Lane::Scratch2)
        );
        assert!(
            chart.lane_notes[Lane::Key1.index()]
                .iter()
                .any(|note| note.id == NoteId(2) && note.lane == Lane::Key1)
        );
        assert!(
            chart.lane_notes[Lane::Key8.index()]
                .iter()
                .any(|note| note.id != NoteId(2) && note.lane == Lane::Key8)
        );
    }

    #[test]
    fn s_random_is_reproducible_from_seed() {
        let mut first = chart_with_two_notes_same_lane();
        let mut second = chart_with_two_notes_same_lane();

        let first_applied = apply_arrange(&mut first, ArrangeOption::SRandom, Some(99), None);
        let _second_applied = apply_arrange(&mut second, ArrangeOption::SRandom, Some(99), None);

        assert_eq!(first_applied.pattern, None);
        assert_eq!(lanes_for_notes(&first), lanes_for_notes(&second));
    }

    #[test]
    fn s_random_keeps_long_note_end_on_start_lane() {
        use bmz_chart::model::{LongNoteMode, LongNotePair, LongNoteStyle};
        use bmz_core::time::ChartTick;

        let mut chart = chart();
        chart.metadata.key_mode = KeyMode::K7;
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            kind: NoteKind::LongStart,
            tick: ChartTick(0),
            ..note(1, Lane::Key1, 1_000_000)
        });
        chart.lane_notes[Lane::Key1.index()].push(NoteEvent {
            kind: NoteKind::LongEnd,
            tick: ChartTick(48),
            ..note(2, Lane::Key1, 2_000_000)
        });
        chart.long_notes.push(LongNotePair {
            lane: Lane::Key1,
            style: LongNoteStyle::ChannelPair,
            mode: Some(LongNoteMode::Cn),
            start_note_id: NoteId(1),
            end_note_id: NoteId(2),
            start_tick: ChartTick(0),
            end_tick: ChartTick(48),
            start_time: TimeUs(1_000_000),
            end_time: TimeUs(2_000_000),
            sound: None,
        });

        apply_arrange(&mut chart, ArrangeOption::SRandom, Some(5), None);

        let start_lane = chart
            .lane_notes
            .iter()
            .flatten()
            .find(|note| note.id == NoteId(1))
            .map(|note| note.lane)
            .expect("start note");
        let end_lane = chart
            .lane_notes
            .iter()
            .flatten()
            .find(|note| note.id == NoteId(2))
            .map(|note| note.lane)
            .expect("end note");
        assert_eq!(start_lane, end_lane);
        assert_eq!(chart.long_notes[0].lane, start_lane);
    }

    #[test]
    fn build_game_session_enables_gauge_auto_shift_from_profile() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.play.gauge_auto_shift =
            crate::config::profile_config::GaugeAutoShiftConfig::BestClear;
        let chart = Arc::new(chart());

        let session = build_game_session(chart, &profile, PlaySessionOptions::default());

        assert!(session.gauge.auto_shift);
        assert_eq!(session.gauge.auto_shift_mode, GaugeAutoShiftMode::BestClear);
        assert_eq!(session.gauge.selected, GaugeType::Hazard);
    }

    #[test]
    fn build_game_session_uses_hidden_cover_only_for_hidden_effects() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.lane.hidden = 400;
        profile.play.lane_effect = LaneEffectConfig::Off;
        let off = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        profile.play.lane_effect = LaneEffectConfig::Hidden;
        let hidden = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        assert_eq!(off.hidden_cover, 0.0);
        assert_eq!(hidden.hidden_cover, 0.4);
    }

    #[test]
    fn build_game_session_maps_lane_cover_and_lift_skin_options_from_values() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.play.lane_effect = LaneEffectConfig::Off;
        profile.lane.sudden = 290;
        profile.lane.lift = 222;

        let session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        assert!(session.lanecover_enabled);
        assert!(session.lift_enabled);

        profile.lane.sudden = 0;
        profile.lane.lift = 0;
        let disabled =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        assert!(!disabled.lanecover_enabled);
        assert!(disabled.lift_enabled);

        profile.play.lane_effect = LaneEffectConfig::Sudden;
        let sudden_option =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        assert!(sudden_option.lanecover_enabled);
    }

    #[test]
    fn build_game_session_clamps_lane_cover_to_remaining_lift_range() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.lane.sudden = 900;
        profile.lane.lift = 200;

        let session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        assert!((session.lane_cover - 0.8).abs() < 0.000_01);
        assert!((session.lift - 0.2).abs() < 0.000_01);
        assert!(session.lanecover_enabled);
    }

    #[test]
    fn build_game_session_clamps_profile_misslayer_duration() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.play.misslayer_duration_ms = 12_000;

        let session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        assert_eq!(session.poor_bga_duration_us, 5_000_000);
    }

    #[test]
    fn build_game_session_maps_profile_bga_expand() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);

        profile.play.bga_expand = BgaExpandConfig::Full;
        let full = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        profile.play.bga_expand = BgaExpandConfig::KeepAspect;
        let keep = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        profile.play.bga_expand = BgaExpandConfig::Off;
        let off = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        assert_eq!(full.bga_stretch, 0);
        assert_eq!(keep.bga_stretch, 1);
        assert_eq!(off.bga_stretch, 8);
    }

    #[test]
    fn build_game_session_maps_profile_bga_mode() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);

        profile.play.bga = BgaModeConfig::Off;
        let off = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        profile.play.bga = BgaModeConfig::Auto;
        let auto_human =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        let auto_autoplay = build_game_session(
            Arc::new(chart()),
            &profile,
            PlaySessionOptions { autoplay: true, ..PlaySessionOptions::default() },
        );
        let auto_replay = build_game_session(
            Arc::new(chart()),
            &profile,
            PlaySessionOptions {
                replay_player: Some(ReplayPlayer::default()),
                ..PlaySessionOptions::default()
            },
        );

        assert!(!off.bga_enabled);
        assert!(!auto_human.bga_enabled);
        assert!(auto_autoplay.bga_enabled);
        assert!(auto_replay.bga_enabled);
    }

    #[test]
    fn build_game_session_copies_profile_skin_offsets() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.skin.offsets.push(crate::config::profile_config::SkinOffsetConfig {
            id: 42,
            x: 1,
            y: 2,
            w: 3,
            h: 4,
            r: 5,
            a: -6,
        });

        let session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        assert_eq!(
            session.skin_offsets,
            vec![PlaySkinOffset { id: 42, x: 1, y: 2, w: 3, h: 4, r: 5, a: -6 }]
        );
    }

    #[test]
    fn build_game_session_clamps_profile_hispeed() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.lane.hispeed = 11.0;
        let high = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        profile.lane.hispeed = 0.25;
        let low = build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());

        assert_eq!(high.hispeed, 10.0);
        assert_eq!(low.hispeed, 0.5);
    }

    #[test]
    fn build_game_session_initializes_floating_hispeed_for_chart_bpm() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.lane.hispeed_mode = HispeedModeConfig::Floating;
        profile.lane.target_green_number = 300;
        // Stale value from a 120 BPM chart with green number 300.
        profile.lane.hispeed = 4.0;
        let mut fast_chart = chart();
        fast_chart.metadata.initial_bpm = 240.0;

        let session =
            build_game_session(Arc::new(fast_chart), &profile, PlaySessionOptions::default());

        assert_eq!(session.hispeed_mode, HispeedMode::Floating);
        assert_eq!(session.target_green_number, 300);
        assert!((session.hispeed - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn build_game_session_initializes_floating_hispeed_for_hsfix_base_bpm() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.lane.hispeed_mode = HispeedModeConfig::Floating;
        profile.lane.target_green_number = 300;
        let mut bpm_chart = chart();
        bpm_chart.metadata.initial_bpm = 120.0;
        bpm_chart.timing_events.push(bmz_chart::model::TimingEvent {
            tick: bmz_core::time::ChartTick(48),
            time: TimeUs(1_000_000),
            kind: TimingEventKind::BpmChange { bpm: 240.0 },
        });

        let session = build_game_session(
            Arc::new(bpm_chart),
            &profile,
            PlaySessionOptions { hs_fix: HsFixOption::MaxBpm, ..PlaySessionOptions::default() },
        );

        assert_eq!(session.hsfix_base_bpm, 240.0);
        assert_eq!(session.hsfix_index, 2);
        assert!((session.hispeed - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn main_bpm_uses_bpm_with_most_notes() {
        let mut bpm_chart = chart();
        bpm_chart.timing_events.push(bmz_chart::model::TimingEvent {
            tick: bmz_core::time::ChartTick(48),
            time: TimeUs(1_000_000),
            kind: TimingEventKind::BpmChange { bpm: 180.0 },
        });
        bpm_chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 0));
        bpm_chart.lane_notes[Lane::Key2.index()].push(note(2, Lane::Key2, 1_100_000));
        bpm_chart.lane_notes[Lane::Key3.index()].push(note(3, Lane::Key3, 1_200_000));
        let timing_map = bmz_chart::timing::TimingMap::from_chart_timing_events(
            bpm_chart.metadata.initial_bpm,
            &bpm_chart.timing_events,
        );

        assert_eq!(hsfix_base_bpm_for_chart(&bpm_chart, &timing_map, HsFixOption::MainBpm), 180.0);
    }

    #[test]
    fn build_game_session_accepts_custom_input_backend() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut backend = BufferedInputBackend::default();
        backend.push(DeviceInputEvent {
            device: DeviceId(1),
            control: PhysicalControl::KeyboardKey("Z".to_string()),
            kind: InputKind::Press,
            timestamp: DeviceTimestamp::Unknown,
        });
        let chart = Arc::new(chart());
        let mut session = build_game_session_with_input_backend(
            chart,
            &profile,
            PlaySessionOptions::default(),
            Box::new(backend),
        );
        let ctx = InputTimingContext {
            audio_clock: &session.audio_clock,
            offsets: session.offsets,
            timestamp_anchor: None,
        };

        let inputs = session.input_system.collect_game_inputs(&ctx);

        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].lane, Lane::Key1);
    }

    #[test]
    fn load_game_session_for_chart_imports_linked_file() {
        let path = write_temp_bms(
            "\
#TITLE Linked
#BPM 120
#00011:01
",
        );
        let imported = import_bms_chart(&path, None, true).unwrap();
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut library_db = LibraryDatabase::from_connection(conn);
        let chart_id = library_db
            .upsert_chart_import(&ChartImportRecord {
                root_id: None,
                file_path: &path,
                file_size: 1,
                modified_at: 1,
                scanned_at: 1,
                chart: &imported.chart,
            })
            .unwrap();
        let profile = ProfileConfig::new_default("default", "Default", 1);

        let session = load_game_session_for_chart(
            &library_db,
            chart_id,
            &profile,
            PlaySessionOptions::default(),
        )
        .unwrap();

        assert_eq!(session.chart.metadata.title, "Linked");

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn load_game_session_counts_cn_ends_from_source_chart() {
        let path = write_temp_bms(
            "\
#TITLE Source CN
#BPM 120
#LNMODE 2
#LNOBJ ZZ
#00011:01ZZ
",
        );
        let imported = import_bms_chart(&path, None, true).unwrap();
        assert_eq!(imported.chart.total_notes, 1);
        assert_eq!(imported.chart.long_notes.len(), 1);
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut library_db = LibraryDatabase::from_connection(conn);
        let chart_id = library_db
            .upsert_chart_import(&ChartImportRecord {
                root_id: None,
                file_path: &path,
                file_size: 1,
                modified_at: 1,
                scanned_at: 1,
                chart: &imported.chart,
            })
            .unwrap();
        let stored = library_db.list_charts_by_ids(&[chart_id]).unwrap().remove(0);
        assert_eq!(stored.total_notes, 1);
        assert_eq!(stored.ln_counts.defined_cn_pairs, 1);
        assert_eq!(stored.scored_total_notes_for_setting(LnPolicySetting::AutoLn), 2);
        library_db
            .conn()
            .execute(
                "UPDATE charts SET total_notes = 999, mode = '14K' WHERE id = ?1",
                rusqlite::params![chart_id],
            )
            .unwrap();
        let source_chart = load_source_chart_for_chart(&library_db, chart_id, None).unwrap();
        assert_eq!(source_chart.metadata.key_mode, KeyMode::K5);
        assert_eq!(source_chart.identity.file_sha256, imported.chart.identity.file_sha256);
        assert_eq!(
            scored_note_count_for_chart(&library_db, chart_id, &PlaySessionOptions::default())
                .unwrap(),
            2,
            "course pre-count must ignore stale library totals"
        );
        let force_ln = PlaySessionOptions {
            ln_mode_override: Some(bmz_chart::model::LongNoteMode::Ln),
            ..Default::default()
        };
        assert_eq!(scored_note_count_for_chart(&library_db, chart_id, &force_ln).unwrap(), 1);
        let battle =
            PlaySessionOptions { double_option: DoubleOption::Battle, ..Default::default() };
        assert_eq!(scored_note_count_for_chart(&library_db, chart_id, &battle).unwrap(), 4);
        let profile = ProfileConfig::new_default("default", "Default", 1);

        let session = load_game_session_for_chart(
            &library_db,
            chart_id,
            &profile,
            PlaySessionOptions::default(),
        )
        .unwrap();

        assert_eq!(session.chart.total_notes, 1);
        assert_eq!(session.scored_total_notes, 2);

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn load_prepared_play_session_for_chart_loads_audio_samples() {
        let (path, wav_path) = write_temp_bms_with_wav(
            "\
#TITLE Prepared
#BPM 120
#WAV01 test.wav
#00011:01
",
        );
        let imported = import_bms_chart(&path, None, true).unwrap();
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
        let mut library_db = LibraryDatabase::from_connection(conn);
        let chart_id = library_db
            .upsert_chart_import(&ChartImportRecord {
                root_id: None,
                file_path: &path,
                file_size: 1,
                modified_at: 1,
                scanned_at: 1,
                chart: &imported.chart,
            })
            .unwrap();
        let profile = ProfileConfig::new_default("default", "Default", 1);

        let prepared = load_prepared_play_session_for_chart(
            &library_db,
            chart_id,
            &profile,
            PlaySessionOptions::default(),
        )
        .unwrap();

        assert_eq!(prepared.session.chart.metadata.title, "Prepared");
        assert_eq!(prepared.audio.mixer.output_sample_rate, 48_000);
        assert!(matches!(prepared.sample_report[0].status, LoadedSampleStatus::Loaded));
        assert!(prepared.audio.samples.get(SoundId(0)).is_some());

        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(wav_path).unwrap();
    }

    fn chart() -> PlayableChart {
        PlayableChart {
            identity: compute_chart_identity(b"session"),
            metadata: ChartMetadata {
                title: "session".to_string(),
                initial_bpm: 120.0,
                total: Some(160.0),
                ..Default::default()
            },
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),

            scroll_events: Vec::new(),

            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: std::collections::HashMap::new(),
            bar_lines: Vec::new(),
            sounds: Vec::<SoundAssetRef>::new(),
            bga_assets: Vec::new(),
            total_notes: 1,
            end_time: TimeUs(0),
        }
    }

    fn note(id: u32, lane: Lane, time_us: i64) -> NoteEvent {
        use bmz_core::time::ChartTick;

        NoteEvent {
            id: NoteId(id),
            lane,
            kind: NoteKind::Tap,
            tick: ChartTick((time_us / 1_000) as u64),
            time: TimeUs(time_us),
            sound: None,
            damage: None,
        }
    }

    fn chart_with_two_notes_same_lane() -> PlayableChart {
        let mut chart = chart();
        chart.metadata.key_mode = KeyMode::K7;
        chart.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
        chart.lane_notes[Lane::Key1.index()].push(note(2, Lane::Key1, 1_020_000));
        chart
    }

    fn lanes_for_notes(chart: &PlayableChart) -> Vec<(NoteId, Lane)> {
        let mut lanes: Vec<_> =
            chart.lane_notes.iter().flatten().map(|note| (note.id, note.lane)).collect();
        lanes.sort_by_key(|(id, _)| *id);
        lanes
    }

    fn write_temp_bms(text: &str) -> std::path::PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir()
            .join(format!("bmz-play-session-{}-{stamp}.bms", std::process::id()));
        std::fs::write(&path, text).unwrap();
        path
    }

    fn write_temp_bms_with_wav(text: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir();
        let bms_path = dir.join(format!("bmz-prepared-session-{}-{stamp}.bms", std::process::id()));
        let wav_name = format!("bmz-prepared-session-{}-{stamp}.wav", std::process::id());
        let wav_path = dir.join(&wav_name);
        std::fs::write(&bms_path, text.replace("test.wav", &wav_name)).unwrap();
        std::fs::write(
            &wav_path,
            [wav_header(1, 1, 48_000, 16, 2).as_slice(), &[0x00, 0x40]].concat(),
        )
        .unwrap();
        (bms_path, wav_path)
    }

    fn wav_header(
        format: u16,
        channels: u16,
        sample_rate: u32,
        bits: u16,
        data_len: u32,
    ) -> Vec<u8> {
        let byte_rate = sample_rate * channels as u32 * bits as u32 / 8;
        let block_align = channels * bits / 8;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16_u32.to_le_bytes());
        out.extend_from_slice(&format.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&bits.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        out
    }
}
