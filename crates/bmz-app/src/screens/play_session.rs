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
use bmz_gameplay::gauge::{GaugeState, gauge_total_for_chart};
use bmz_gameplay::input::backend::{InputBackend, NullInputBackend};
use bmz_gameplay::input::system::InputSystem;
use bmz_gameplay::input::translator::DefaultInputTranslator;
use bmz_gameplay::judge::engine::JudgeEngine;
use bmz_gameplay::judge::window::{judge_percent_at_time, judge_window_for_rank};
use bmz_gameplay::replay::{ReplayPlayer, ReplayRecorder};
use bmz_gameplay::score::ScoreState;
use bmz_gameplay::session::{BgmScheduler, GameSession, PlaySkinOffset, PlayState};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::play::{
    DEFAULT_JUDGE_WINDOW, audio_mix_from_profile, gauge_type_from_config,
    lane_binding_from_profile_input, play_offsets_from_profile,
};
use crate::config::profile_config::{
    BgaExpandConfig, BgaModeConfig, LaneEffectConfig, ProfileConfig,
};
use crate::select_options::ArrangeOption;
use crate::storage::library_db::LibraryDatabase;

#[derive(Debug, Clone)]
pub struct PlaySessionOptions {
    pub autoplay: bool,
    pub replay_player: Option<ReplayPlayer>,
    pub sample_rate: u32,
    pub gauge_override: Option<GaugeType>,
    pub arrange: ArrangeOption,
    pub arrange_seed: Option<i64>,
    pub arrange_pattern: Option<Vec<u8>>,
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
            replay_player: None,
            sample_rate: 48_000,
            gauge_override: None,
            arrange: ArrangeOption::Normal,
            arrange_seed: None,
            arrange_pattern: None,
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
    let autoplay_enabled = profile.play.auto_play || options.autoplay;
    let replay_player = options.replay_player;
    let is_replay = replay_player.is_some();
    let autoplay = autoplay_enabled.then(AutoplayController::default);
    let input_system = InputSystem {
        backend: input_backend,
        translator: Box::new(DefaultInputTranslator {
            binding: lane_binding_from_profile_input(&profile.input),
        }),
    };

    let timing_map = bmz_chart::timing::TimingMap::from_chart_timing_events(
        chart.metadata.initial_bpm,
        &chart.timing_events,
    );

    let base_judge_window = DEFAULT_JUDGE_WINDOW;

    GameSession {
        gauge: GaugeState::new(
            gauge_type,
            gauge_total_for_chart(chart.metadata.total, chart.total_notes),
            chart.total_notes,
        ),
        judge: JudgeEngine::new(judge_window_for_rank(
            base_judge_window,
            judge_percent_at_time(chart.metadata.judge_rank, &chart.judge_rank_events, TimeUs(0)),
        )),
        base_judge_window,
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
        full_combo_started_at: None,
        bgm_scheduler: BgmScheduler::default(),
        offsets: play_offsets_from_profile(profile),
        audio_mix: audio_mix_from_profile(profile),
        hispeed: clamp_hispeed(profile.lane.hispeed),
        lift: profile.lane.lift.clamp(0.0, 1.0),
        lane_cover: profile.lane.lane_cover.clamp(0.0, 1.0),
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
        input_timestamp_anchor: None,
        pending_mine_hits: Vec::new(),
        state: PlayState::Ready,
    }
}

fn clamp_hispeed(hispeed: f32) -> f32 {
    hispeed.clamp(0.5, 10.0)
}

fn hidden_cover_from_profile(profile: &ProfileConfig) -> f32 {
    match profile.play.lane_effect {
        LaneEffectConfig::Hidden | LaneEffectConfig::HiddenSudden => profile.lane.hidden,
        LaneEffectConfig::Off | LaneEffectConfig::Sudden => 0.0,
    }
    .clamp(0.0, 1.0)
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

pub fn build_prepared_play_session_from_preloaded(
    preloaded: PreloadedPlaySession,
    profile: &ProfileConfig,
    options: PlaySessionOptions,
    input_backend: Box<dyn InputBackend>,
) -> PreparedPlaySession {
    let session =
        build_game_session_with_input_backend(preloaded.chart, profile, options, input_backend);
    PreparedPlaySession {
        session,
        audio: preloaded.audio,
        sample_report: preloaded.sample_report,
        applied_arrange: preloaded.applied_arrange,
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
    let active = key_mode.active_lanes();
    // P1 keys (skip Scratch at index 0)
    let p1_keys: Vec<usize> =
        active.iter().skip(1).take_while(|&&l| (l as usize) < 8).map(|&l| l as usize).collect();
    let p1_reversed: Vec<usize> = p1_keys.iter().rev().copied().collect();
    for (orig, rev) in p1_keys.iter().zip(p1_reversed.iter()) {
        perm[*orig] = *rev;
    }
    // P2 keys (skip P2 Scratch)
    let p2_keys: Vec<usize> = active
        .iter()
        .filter(|&&l| (l as usize) >= 8 && l != Lane::Scratch2)
        .map(|&l| l as usize)
        .collect();
    let p2_reversed: Vec<usize> = p2_keys.iter().rev().copied().collect();
    for (orig, rev) in p2_keys.iter().zip(p2_reversed.iter()) {
        perm[*orig] = *rev;
    }
    perm
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

fn random_lane_permutation(seed: u64, key_mode: KeyMode) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..LANE_COUNT).collect();
    let mut rng = seed;

    let active = key_mode.active_lanes();
    // P1 keys: active lanes with index < 8, skip Scratch (index 0)
    let p1_keys: Vec<usize> =
        active.iter().skip(1).take_while(|&&l| (l as usize) < 8).map(|&l| l as usize).collect();
    fisher_yates_shuffle(&mut rng, &p1_keys, &mut perm);

    // P2 keys: active lanes with index >= 8, skip Scratch2
    let p2_keys: Vec<usize> = active
        .iter()
        .filter(|&&l| (l as usize) >= 8 && l != Lane::Scratch2)
        .map(|&l| l as usize)
        .collect();
    fisher_yates_shuffle(&mut rng, &p2_keys, &mut perm);

    perm
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
    use std::sync::Arc;

    use bmz_audio::loader::LoadedSampleStatus;
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, PlayableChart, SoundAssetRef};
    use bmz_core::clear::GaugeType;
    use bmz_core::ids::SoundId;
    use bmz_core::input::InputKind;
    use bmz_core::lane::Lane;
    use bmz_core::time::TimeUs;
    use bmz_gameplay::input::backend::{
        BufferedInputBackend, DeviceId, DeviceInputEvent, DeviceTimestamp, PhysicalControl,
    };
    use bmz_gameplay::input::translator::InputTimingContext;
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
        assert_eq!(session.audio_mix.master_volume, 1.0);
        assert_eq!(session.audio_clock.sample_rate, 48_000);
        assert_eq!(session.hispeed, 2.0);
        assert_eq!(session.hidden_cover, 0.0);
        assert!(session.bga_enabled);
        assert_eq!(session.poor_bga_duration_us, 500_000);
        assert_eq!(session.bga_stretch, 1);
    }

    #[test]
    fn build_game_session_uses_hidden_cover_only_for_hidden_effects() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);
        profile.lane.hidden = 0.4;
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
