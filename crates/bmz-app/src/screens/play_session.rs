use anyhow::{Context, Result, bail};
use bmz_audio::clock::AudioClock;
use bmz_audio::engine::AudioEngine;
use bmz_audio::ffmpeg_loader::FfmpegSampleLoader;
use bmz_audio::loader::{LoadedSampleReport, SampleLoader, load_chart_samples};
use bmz_chart::import::import_bms_chart;
use bmz_chart::model::{NoteEvent, PlayableChart};
use bmz_core::clear::GaugeType;
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use bmz_core::time::TimeUs;
use bmz_gameplay::autoplay::AutoplayController;
use bmz_gameplay::gauge::{GaugeAutoShiftMode, GaugeProperty, GaugeState, gauge_total_for_chart};
use bmz_gameplay::hit_error::HitErrorRing;
use bmz_gameplay::input::backend::{InputBackend, NullInputBackend};
use bmz_gameplay::input::system::InputSystem;
use bmz_gameplay::input::translator::DefaultInputTranslator;
use bmz_gameplay::judge::engine::JudgeEngine;
use bmz_gameplay::judge::window::{
    judge_percent_at_time, judge_window_for_rule_mode, note_judge_window_for_rule_mode,
};
use bmz_gameplay::replay::{ReplayPlayer, ReplayRecorder};
use bmz_gameplay::score::ScoreState;
use bmz_gameplay::session::{BgmScheduler, GameSession, HispeedMode, PlaySkinOffset, PlayState};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::play::{
    audio_mix_from_profile, gauge_auto_shift_from_config, gauge_type_from_config,
    lane_binding_for_chart, lane_unit_to_f32, play_offsets_from_profile,
};
use crate::config::profile_config::{
    BgaExpandConfig, BgaModeConfig, HispeedModeConfig, LaneEffectConfig, ProfileConfig,
};
use crate::screens::practice::{
    PracticeProperty, apply_practice_property, apply_practice_start_gauge,
};
use crate::select_options::{ArrangeOption, TargetOption};
use crate::storage::library_db::LibraryDatabase;

#[derive(Debug, Clone)]
pub struct PlaySessionOptions {
    pub autoplay: bool,
    /// Practice section play: no score / replay persistence (like autoplay).
    pub practice_mode: bool,
    pub replay_player: Option<ReplayPlayer>,
    pub sample_rate: u32,
    pub gauge_override: Option<GaugeType>,
    pub gauge_auto_shift: GaugeAutoShiftMode,
    pub arrange: ArrangeOption,
    pub target: TargetOption,
    pub arrange_seed: Option<i64>,
    pub arrange_pattern: Option<Vec<u8>>,
    /// When set, overrides the gauge's starting value.  Used to carry the
    /// gauge between charts during a course.
    pub initial_gauge_value: Option<f32>,
    /// Course judge constraint forwarded from CourseJudgeConstraint.
    /// `NoGood` zeroes the good window, `NoGreat` zeroes great and good
    /// windows; the next judge band kicks in immediately.
    pub judge_constraint: bmz_core::course::CourseJudgeConstraint,
    /// Course-forced long-note mode (Ln/Cn/Hcn).  `None` keeps the chart's
    /// declared mode.
    pub ln_mode_override: Option<bmz_chart::model::LongNoteMode>,
    /// 段位ゲージ用の `GaugeProperty` 上書き。コース時に
    /// `apply_course_constraints` が `CourseGaugeConstraint::Lr2/Keys5/...` を
    /// 解釈して設定する。`None` の場合はチャートの `KeyMode` から自動推定する。
    pub gauge_property: Option<GaugeProperty>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppliedArrange {
    pub arrange: ArrangeOption,
    pub seed: Option<i64>,
    pub pattern: Option<Vec<u8>>,
}

pub struct PreparedPlaySession {
    pub session: GameSession,
    pub audio: AudioEngine,
    pub sample_report: Vec<LoadedSampleReport>,
    pub applied_arrange: AppliedArrange,
    pub target_ex_score: Option<u32>,
    pub practice_mode: bool,
}

pub struct PreloadedPlaySession {
    pub chart: Arc<PlayableChart>,
    pub audio: AudioEngine,
    pub sample_report: Vec<LoadedSampleReport>,
    pub applied_arrange: AppliedArrange,
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
            arrange: ArrangeOption::Normal,
            target: TargetOption::None,
            arrange_seed: None,
            arrange_pattern: None,
            initial_gauge_value: None,
            judge_constraint: bmz_core::course::CourseJudgeConstraint::Normal,
            ln_mode_override: None,
            gauge_property: None,
        }
    }
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
    let initial_gauge_value = options.initial_gauge_value;
    let autoplay_enabled = profile.play.auto_play || options.autoplay;
    let replay_player = options.replay_player;
    let is_replay = replay_player.is_some();
    let autoplay = autoplay_enabled.then(AutoplayController::default);
    let key_mode = chart.metadata.key_mode;
    let rule_mode = profile.play.rule_mode;
    let input_system = InputSystem {
        backend: input_backend,
        translator: Box::new(DefaultInputTranslator {
            binding: lane_binding_for_chart(&profile.input, key_mode),
        }),
    };

    let timing_map = bmz_chart::timing::TimingMap::from_chart_timing_events(
        chart.metadata.initial_bpm,
        &chart.timing_events,
    );

    // Course judge constraints narrow the judge window so the corresponding
    // judge band is unreachable: NoGood zeroes good_us, NoGreat zeroes both
    // great_us and good_us.  Mirrors beatoraja JudgeManager's *JudgeWindowRate
    // = 0 path.
    let base_judge_window = {
        let mut w = note_judge_window_for_rule_mode(chart.metadata.key_mode, rule_mode);
        match options.judge_constraint {
            bmz_core::course::CourseJudgeConstraint::Normal => {}
            bmz_core::course::CourseJudgeConstraint::NoGood => {
                w.good_us = 0;
            }
            bmz_core::course::CourseJudgeConstraint::NoGreat => {
                w.great_us = 0;
                w.good_us = 0;
            }
        }
        w
    };

    let mut gauge = {
        let gauge_total = gauge_total_for_chart(chart.metadata.total, chart.total_notes);
        // 単曲時はチャートのキーモードから GaugeProperty を導出、コース時は
        // `apply_course_constraints` が CourseGaugeConstraint から決めた値を使う。
        let gauge_property = options
            .gauge_property
            .unwrap_or_else(|| GaugeProperty::from_keymode(chart.metadata.key_mode));
        if gauge_auto_shift != GaugeAutoShiftMode::Off {
            GaugeState::new_with_auto_shift_property_and_rule_mode(
                gauge_type,
                gauge_auto_shift,
                gauge_total,
                chart.total_notes,
                gauge_property,
                rule_mode,
            )
        } else {
            GaugeState::new_with_property_and_rule_mode(
                gauge_type,
                gauge_total,
                chart.total_notes,
                gauge_property,
                rule_mode,
            )
        }
    };
    // Course play carries the previous chart's gauge value over; this overrides
    // the initial value computed by GaugeState::new* above.
    if let Some(initial) = initial_gauge_value {
        gauge.set_initial_value(initial);
    }

    GameSession {
        gauge,
        judge: JudgeEngine::new_with_rule_mode(
            judge_window_for_rule_mode(
                base_judge_window,
                judge_percent_at_time(
                    chart.metadata.judge_rank,
                    &chart.judge_rank_events,
                    TimeUs(0),
                ),
                rule_mode,
            ),
            rule_mode,
        ),
        base_judge_window,
        rule_mode,
        audio_clock: AudioClock::stopped(options.sample_rate),
        chart,
        timing_map,
        input_system,
        score: ScoreState::default(),
        replay_recorder: ReplayRecorder::default(),
        replay_player,
        autoplay,
        recent_inputs: Vec::new(),
        lane_keyon_started_at: Default::default(),
        lane_keyoff_started_at: Default::default(),
        lane_auto_release_at: Default::default(),
        recent_judgements: Vec::new(),
        hit_error_ring: HitErrorRing::default(),
        full_combo_started_at: None,
        bgm_scheduler: BgmScheduler::default(),
        offsets: play_offsets_from_profile(profile),
        audio_mix: audio_mix_from_profile(profile),
        hispeed: clamp_hispeed(profile.lane.hispeed),
        hispeed_mode: hispeed_mode_from_profile(profile.lane.hispeed_mode),
        target_green_number: profile.lane.target_green_number.max(1),
        lift: lane_unit_to_f32(profile.lane.lift),
        lane_cover: lane_unit_to_f32(profile.lane.sudden),
        lane_cover_visible: true,
        lane_cover_changing: false,
        lanecover_enabled: lanecover_enabled_from_profile(profile),
        lift_enabled: true,
        hidden_enabled: hidden_enabled_from_profile(profile),
        hidden_cover: hidden_cover_from_profile(profile),
        skin_offsets: skin_offsets_from_profile(profile),
        bga_enabled: bga_enabled_from_profile(profile, autoplay_enabled, is_replay),
        poor_bga_duration_us: poor_bga_duration_us_from_profile(profile),
        bga_stretch: bga_stretch_from_profile(profile),
        hsfix_index: 0,
        input_timestamp_anchor: None,
        pending_mine_hits: Vec::new(),
        state: PlayState::Ready,
        last_hcn_gauge_at: None,
    }
}

fn clamp_hispeed(hispeed: f32) -> f32 {
    hispeed.clamp(0.5, 10.0)
}

fn hispeed_mode_from_profile(mode: HispeedModeConfig) -> HispeedMode {
    match mode {
        HispeedModeConfig::Normal => HispeedMode::Normal,
        HispeedModeConfig::Floating => HispeedMode::Floating,
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
    matches!(profile.play.lane_effect, LaneEffectConfig::Sudden | LaneEffectConfig::HiddenSudden)
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
    let preloaded = preload_play_session_for_chart(library_db, chart_id, options.clone())?;
    Ok(build_prepared_play_session_from_preloaded(preloaded, profile, options, input_backend))
}

pub fn preload_play_session_for_chart(
    library_db: &LibraryDatabase,
    chart_id: i64,
    options: PlaySessionOptions,
) -> Result<PreloadedPlaySession> {
    let Some(path) = library_db.primary_chart_file_path(chart_id)? else {
        bail!("chart file not found for chart id {chart_id}");
    };
    let import =
        import_bms_chart(std::path::Path::new(&path), random_seed_for_chart(&options), true)
            .with_context(|| format!("failed to import chart file: {path}"))?;
    let mut chart = import.chart;
    // Course constraint may force a specific LN mode (Ln/Cn/Hcn) regardless of
    // what the chart declared. Mirrors beatoraja PlayerConfig.setLnmode().
    if let Some(ln_mode) = options.ln_mode_override {
        chart.metadata.long_note_mode = ln_mode;
    }
    let applied_arrange = apply_arrange(
        &mut chart,
        options.arrange,
        options.arrange_seed,
        options.arrange_pattern.as_deref(),
    );
    let chart = Arc::new(chart);
    let mut loader = FfmpegSampleLoader;
    let (audio, sample_report) =
        build_audio_engine_for_chart(&chart, options.sample_rate, &mut loader);

    Ok(PreloadedPlaySession { chart, audio, sample_report, applied_arrange })
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
    let target_ex_score = None;
    let practice_mode = options.practice_mode;
    let mut session =
        build_game_session_with_input_backend(Arc::new(chart), profile, options, input_backend);
    apply_practice_start_gauge(&mut session.gauge, property.start_gauge);
    PreparedPlaySession {
        session,
        audio: preloaded.audio,
        sample_report: preloaded.sample_report,
        applied_arrange,
        target_ex_score,
        practice_mode,
    }
}

pub fn build_prepared_play_session_from_preloaded(
    preloaded: PreloadedPlaySession,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
    input_backend: Box<dyn InputBackend>,
) -> PreparedPlaySession {
    let target_ex_score = options.target.target_ex_score(preloaded.chart.total_notes);
    let practice_mode = options.practice_mode;
    let session =
        build_game_session_with_input_backend(preloaded.chart, profile, options, input_backend);
    PreparedPlaySession {
        session,
        audio: preloaded.audio,
        sample_report: preloaded.sample_report,
        applied_arrange: preloaded.applied_arrange,
        target_ex_score,
        practice_mode,
    }
}

pub fn generate_arrange_seed() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as i64).unwrap_or(12345)
}

pub fn apply_arrange(
    chart: &mut PlayableChart,
    arrange: ArrangeOption,
    seed: Option<i64>,
    pattern: Option<&[u8]>,
) -> AppliedArrange {
    if let Some(perm) = pattern {
        let perm_usize: Vec<usize> = perm.iter().map(|&i| i as usize).collect();
        apply_lane_permutation(chart, &perm_usize);
        return AppliedArrange { arrange, seed, pattern: Some(perm.to_vec()) };
    }

    let key_mode = chart.metadata.key_mode;
    match arrange {
        ArrangeOption::Normal => {
            AppliedArrange { arrange: ArrangeOption::Normal, seed: None, pattern: None }
        }
        ArrangeOption::Mirror => {
            let perm = mirror_permutation(key_mode);
            apply_lane_permutation(chart, &perm);
            AppliedArrange {
                arrange: ArrangeOption::Mirror,
                seed: None,
                pattern: Some(perm.iter().map(|&i| i as u8).collect()),
            }
        }
        ArrangeOption::Random => {
            let used_seed = seed.unwrap_or_else(generate_arrange_seed);
            let perm = random_lane_permutation(used_seed as u64, key_mode);
            apply_lane_permutation(chart, &perm);
            AppliedArrange {
                arrange: ArrangeOption::Random,
                seed: Some(used_seed),
                pattern: Some(perm.iter().map(|&i| i as u8).collect()),
            }
        }
    }
}

fn mirror_permutation(key_mode: KeyMode) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    for group in arrange_lane_groups(key_mode) {
        reverse_lane_group(&mut perm, &group);
    }
    perm
}

fn random_lane_permutation(seed: u64, key_mode: KeyMode) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    let mut rng = seed;
    for group in arrange_lane_groups(key_mode) {
        fisher_yates_shuffle(&mut rng, &group, &mut perm);
    }
    perm
}

fn arrange_lane_groups(key_mode: KeyMode) -> Vec<Vec<usize>> {
    let active = key_mode.active_lanes();
    match key_mode {
        KeyMode::K4 | KeyMode::K6 | KeyMode::K9 => {
            vec![active.iter().map(|&lane| lane as usize).collect()]
        }
        KeyMode::K5 | KeyMode::K7 | KeyMode::K8 => {
            vec![
                active
                    .iter()
                    .filter(|&&lane| lane != Lane::Scratch)
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
                        Lane::Key1
                            | Lane::Key2
                            | Lane::Key3
                            | Lane::Key4
                            | Lane::Key5
                            | Lane::Key6
                            | Lane::Key7
                    )
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
                    )
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

fn fisher_yates_shuffle(rng: &mut u64, lanes: &[usize], perm: &mut [usize]) {
    if lanes.len() < 2 {
        return;
    }
    let mut indices: Vec<usize> = lanes.to_vec();
    for i in (1..indices.len()).rev() {
        *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let j = (*rng >> 33) as usize % (i + 1);
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
    use bmz_core::ids::SoundId;
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
        assert!((session.audio_mix.master_volume - 0.2).abs() < 1e-6);
        assert_eq!(session.audio_clock.sample_rate, 48_000);
        assert_eq!(session.hispeed, 2.0);
        assert_eq!(session.hidden_cover, 0.0);
        assert!(session.bga_enabled);
        assert_eq!(session.poor_bga_duration_us, 500_000);
        assert_eq!(session.bga_stretch, 1);
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
    fn mirror_permutation_k9_reverses_all_nine_keys() {
        let perm = mirror_permutation(KeyMode::K9);
        assert_eq!(perm[Lane::Key1 as usize], Lane::Key9 as usize);
        assert_eq!(perm[Lane::Key9 as usize], Lane::Key1 as usize);
        assert_eq!(perm[Lane::Key5 as usize], Lane::Key5 as usize);
    }

    #[test]
    fn random_lane_permutation_k9_preserves_active_lanes() {
        let perm = random_lane_permutation(42, KeyMode::K9);
        let active: HashSet<_> =
            KeyMode::K9.active_lanes().iter().map(|&lane| lane as usize).collect();
        let mapped: HashSet<_> =
            KeyMode::K9.active_lanes().iter().map(|&lane| perm[lane as usize]).collect();
        assert_eq!(active, mapped);
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
        assert_eq!(session.gauge.selected, GaugeType::ExHard);
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
