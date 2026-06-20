use anyhow::Result;
use bmz_core::clear::ClearType;
use bmz_core::input::{InputDeviceKind, InputKind};
use bmz_core::replay::ReplayEvent;
use bmz_gameplay::result::PlayResult;

use crate::config::profile_config::{ReplayConfig, ReplaySlotRule};
use crate::ln_policy::LnScorePolicy;
use crate::paths::ProfilePaths;
use crate::select_options::{ArrangeOption, DoubleOptionScoreBucket};

use super::replay::{ReplayFile, replay_file_name, replay_slot_file_name, save_replay};
use super::score_db::{
    ReplaySlotRecord, ScoreDatabase, ScoreInsertMode, ScoreRecord, ScoreRecordMetadata,
};

#[derive(Debug, Clone)]
pub struct StoredPlayResult {
    pub score_history_id: i64,
    pub played_at: i64,
    pub replay_path: String,
    pub slot_paths: [Option<String>; 4],
    pub device_type: InputDeviceKind,
}

#[derive(Debug, Clone)]
pub struct StorePlayResultRequest {
    pub played_at: i64,
    pub playtime_seconds: u32,
    pub ln_policy: LnScorePolicy,
    pub double_option: DoubleOptionScoreBucket,
    pub random_seed: Option<i64>,
    pub gauge_option: String,
    pub rule_mode: String,
    pub assist_mask: u32,
    pub replay_events: Vec<ReplayEvent>,
    pub arrange: ArrangeOption,
    pub arrange_2p: ArrangeOption,
    pub arrange_seed: Option<i64>,
    pub arrange_pattern: Option<Vec<u8>>,
    pub mode: StorePlayResultMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorePlayResultMode {
    Normal,
    CourseStage,
}

impl StorePlayResultMode {
    fn score_insert_mode(self) -> ScoreInsertMode {
        match self {
            Self::Normal => ScoreInsertMode::Full,
            Self::CourseStage => ScoreInsertMode::HistoryOnly,
        }
    }

    fn save_replay_slots(self) -> bool {
        match self {
            Self::Normal => true,
            Self::CourseStage => false,
        }
    }

    fn stored_clear_type(self, clear_type: ClearType) -> ClearType {
        match self {
            Self::Normal => clear_type,
            Self::CourseStage => course_stage_clear_type(clear_type),
        }
    }
}

pub fn course_stage_clear_type(clear_type: ClearType) -> ClearType {
    match clear_type {
        ClearType::FullCombo | ClearType::Perfect | ClearType::Max => clear_type,
        _ => ClearType::NoPlay,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CandidateMetrics {
    pub ex_score: u32,
    pub bp: u32,
    pub cb: u32,
    pub max_combo: u32,
    pub clear_rank: u8,
}

pub fn store_play_result(
    score_db: &mut ScoreDatabase,
    profile_paths: &ProfilePaths,
    replay_config: &ReplayConfig,
    result: &PlayResult,
    request: StorePlayResultRequest,
) -> Result<StoredPlayResult> {
    let arrange = request.arrange;
    let arrange_2p = request.arrange_2p;
    let arrange_seed = request.arrange_seed;
    let arrange_pattern = request.arrange_pattern.clone();
    let replay_events = request.replay_events.clone();
    let rule_mode = bmz_gameplay::rule::RuleMode::from_str_opt(&request.rule_mode)
        .unwrap_or(bmz_gameplay::rule::RuleMode::Beatoraja);
    let device_type = classify_replay_device_type(&replay_events);

    let replay_path = if should_save_replay(replay_config, result) {
        let file_name = replay_file_name(result.chart_sha256, request.played_at);
        let path = profile_paths.replay_dir.join(&file_name);
        let replay = ReplayFile::new_with_policy(
            result.chart_sha256,
            request.ln_policy,
            request.double_option,
            request.played_at,
            request.random_seed,
            arrange,
            arrange_2p,
            arrange_seed,
            arrange_pattern.clone(),
            replay_events.clone(),
        );
        save_replay(&path, &replay)?;
        format!("replay/{file_name}")
    } else {
        String::new()
    };

    let mut record = ScoreRecord::from_play_result(
        result,
        ScoreRecordMetadata::new(
            request.ln_policy,
            request.double_option,
            request.played_at,
            request.random_seed,
            arrange.to_persistent_str(),
            request.gauge_option,
            request.rule_mode.clone(),
            request.assist_mask,
            device_type,
            replay_path.clone(),
        )
        .with_playtime_seconds(request.playtime_seconds),
    );
    record.clear_type = request.mode.stored_clear_type(result.clear_type);
    let score_history_id =
        score_db.insert_score_with_mode(&record, request.mode.score_insert_mode())?;

    let mut slot_paths: [Option<String>; 4] = [None, None, None, None];
    if request.mode.save_replay_slots() && should_save_replay(replay_config, result) {
        let candidate = candidate_metrics(result);
        for (slot_index, &rule) in replay_config.slot_rules.iter().enumerate() {
            let slot = slot_index as u8;
            let key = super::score_db::ScoreKey::with_options(
                result.chart_sha256,
                request.ln_policy,
                request.double_option,
                rule_mode,
            );
            let prev = score_db.replay_slot(key, slot)?;
            if !evaluate_slot_update(rule, prev.as_ref(), &candidate) {
                continue;
            }
            let file_name = replay_slot_file_name(
                result.chart_sha256,
                request.ln_policy,
                request.double_option,
                rule_mode,
                slot,
            );
            let path = profile_paths.replay_dir.join(&file_name);
            let replay = ReplayFile::new_with_policy(
                result.chart_sha256,
                request.ln_policy,
                request.double_option,
                request.played_at,
                request.random_seed,
                arrange,
                arrange_2p,
                arrange_seed,
                arrange_pattern.clone(),
                replay_events.clone(),
            );
            save_replay(&path, &replay)?;
            let rel_path = format!("replay/{file_name}");
            score_db.upsert_replay_slot(&ReplaySlotRecord {
                chart_sha256: result.chart_sha256,
                ln_policy: request.ln_policy,
                double_option: request.double_option,
                rule_mode,
                slot,
                rule,
                replay_path: rel_path.clone(),
                played_at: request.played_at,
                ex_score: candidate.ex_score,
                bp: candidate.bp,
                cb: candidate.cb,
                max_combo: candidate.max_combo,
                clear_rank: candidate.clear_rank,
            })?;
            slot_paths[slot_index] = Some(rel_path);
        }
    }

    Ok(StoredPlayResult {
        score_history_id,
        played_at: request.played_at,
        replay_path,
        slot_paths,
        device_type,
    })
}

pub fn save_existing_replay_to_slot(
    score_db: &mut ScoreDatabase,
    profile_paths: &ProfilePaths,
    result: &PlayResult,
    stored: &StoredPlayResult,
    ln_policy: LnScorePolicy,
    double_option: DoubleOptionScoreBucket,
    rule_mode: bmz_gameplay::rule::RuleMode,
    slot: u8,
) -> Result<Option<String>> {
    if slot > 3 || stored.replay_path.is_empty() || result.autoplay {
        return Ok(None);
    }
    let source = profile_paths.root_dir.join(&stored.replay_path);
    if !source.is_file() {
        return Ok(None);
    }
    std::fs::create_dir_all(&profile_paths.replay_dir)?;
    let file_name =
        replay_slot_file_name(result.chart_sha256, ln_policy, double_option, rule_mode, slot);
    let path = profile_paths.replay_dir.join(&file_name);
    std::fs::copy(&source, &path)?;
    let rel_path = format!("replay/{file_name}");
    let candidate = candidate_metrics(result);
    score_db.upsert_replay_slot(&ReplaySlotRecord {
        chart_sha256: result.chart_sha256,
        ln_policy,
        double_option,
        rule_mode,
        slot,
        rule: ReplaySlotRule::Always,
        replay_path: rel_path.clone(),
        played_at: stored.played_at,
        ex_score: candidate.ex_score,
        bp: candidate.bp,
        cb: candidate.cb,
        max_combo: candidate.max_combo,
        clear_rank: candidate.clear_rank,
    })?;
    Ok(Some(rel_path))
}

pub fn candidate_metrics(result: &PlayResult) -> CandidateMetrics {
    CandidateMetrics {
        ex_score: result.score.ex_score(),
        bp: result.record_bp(),
        cb: result.record_cb(),
        max_combo: result.score.max_combo,
        clear_rank: result.clear_type as u8,
    }
}

pub fn classify_replay_device_type(events: &[ReplayEvent]) -> InputDeviceKind {
    let (keyboard, controller) = events.iter().filter(|event| event.kind == InputKind::Press).fold(
        (0_u32, 0_u32),
        |(keyboard, controller), event| match event.device_kind {
            InputDeviceKind::Keyboard => (keyboard + 1, controller),
            InputDeviceKind::Controller => (keyboard, controller + 1),
        },
    );
    if controller > keyboard { InputDeviceKind::Controller } else { InputDeviceKind::Keyboard }
}

fn evaluate_slot_update(
    rule: ReplaySlotRule,
    prev: Option<&ReplaySlotRecord>,
    next: &CandidateMetrics,
) -> bool {
    let prev_metrics = prev.map(|p| (p.ex_score, p.bp, p.max_combo, p.clear_rank));
    slot_rule_passes(rule, prev_metrics, next)
}

/// Rule-only comparison shared by per-chart `replay_slots` and per-course
/// `course_replay_slots`.  `prev` is `(ex_score, bp, max_combo,
/// clear_rank)` of the row currently in the slot, or `None` if the slot is
/// empty (in which case any enabled rule passes — the first record always wins).
pub fn slot_rule_passes(
    rule: ReplaySlotRule,
    prev: Option<(u32, u32, u32, u8)>,
    next: &CandidateMetrics,
) -> bool {
    if matches!(rule, ReplaySlotRule::Disabled) {
        return false;
    }
    if matches!(rule, ReplaySlotRule::Always) {
        return true;
    }
    let Some((prev_ex, prev_bp, prev_combo, prev_clear)) = prev else {
        return true;
    };
    match rule {
        ReplaySlotRule::Disabled => false,
        ReplaySlotRule::Always => true,
        ReplaySlotRule::ScoreUpdate => next.ex_score > prev_ex,
        ReplaySlotRule::BpUpdate => next.bp < prev_bp,
        ReplaySlotRule::MaxComboUpdate => next.max_combo > prev_combo,
        ReplaySlotRule::ClearUpdate => next.clear_rank > prev_clear,
    }
}

fn should_save_replay(config: &ReplayConfig, result: &PlayResult) -> bool {
    // オートプレイの記録は保存しない (save_autoplay_runs は廃止: 常に false)
    // 失敗ランは保存する (save_failed_runs は廃止: 常に true)
    config.auto_save && !result.autoplay
}

#[cfg(test)]
mod tests {
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::input::{InputDeviceKind, InputKind};
    use bmz_core::lane::Lane;
    use bmz_core::replay::ReplayEvent;
    use bmz_core::time::TimeUs;
    use bmz_gameplay::result::PlayResult;
    use bmz_gameplay::score::ScoreState;
    use rusqlite::Connection;

    use super::*;
    use crate::storage::common::configure_connection;
    use crate::storage::migration::{SCORE_MIGRATIONS, run_migrations};

    #[test]
    fn store_play_result_writes_replay_and_score() {
        let root = make_temp_dir("store-result");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let config = ReplayConfig {
            auto_save: true,
            compress: false,
            slot_rules: crate::config::profile_config::default_slot_rules(),
        };
        let result = play_result(false);

        let stored = store_play_result(
            &mut score_db,
            &paths,
            &config,
            &result,
            StorePlayResultRequest {
                ln_policy: LnScorePolicy::ForceLn,
                double_option: DoubleOptionScoreBucket::Off,
                played_at: 1_700_000_060,
                playtime_seconds: 0,
                random_seed: Some(77),
                gauge_option: String::new(),
                rule_mode: String::new(),
                assist_mask: 0,
                replay_events: vec![ReplayEvent {
                    lane: Lane::Key1,
                    kind: InputKind::Press,
                    time: TimeUs(10),
                    device_kind: InputDeviceKind::Keyboard,
                }],
                arrange: ArrangeOption::Normal,
                arrange_2p: ArrangeOption::Normal,
                arrange_seed: None,
                arrange_pattern: None,
                mode: StorePlayResultMode::Normal,
            },
        )
        .unwrap();

        assert!(stored.score_history_id > 0);
        assert!(!stored.replay_path.is_empty());
        assert!(root.join(&stored.replay_path).exists());
        assert_eq!(
            score_db
                .best_ex_score(super::super::score_db::ScoreKey::new(
                    [4; 32],
                    LnScorePolicy::ForceLn
                ))
                .unwrap(),
            Some(0)
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn store_play_result_skips_autoplay_replay_by_default() {
        let root = make_temp_dir("store-autoplay-result");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let config = ReplayConfig {
            auto_save: true,
            compress: false,
            slot_rules: crate::config::profile_config::default_slot_rules(),
        };
        let result = play_result(true);

        let stored = store_play_result(
            &mut score_db,
            &paths,
            &config,
            &result,
            StorePlayResultRequest {
                ln_policy: LnScorePolicy::ForceLn,
                double_option: DoubleOptionScoreBucket::Off,
                played_at: 1_700_000_061,
                playtime_seconds: 0,
                random_seed: None,
                gauge_option: String::new(),
                rule_mode: String::new(),
                assist_mask: 0,
                replay_events: Vec::new(),
                arrange: ArrangeOption::Normal,
                arrange_2p: ArrangeOption::Normal,
                arrange_seed: None,
                arrange_pattern: None,
                mode: StorePlayResultMode::Normal,
            },
        )
        .unwrap();

        assert_eq!(stored.replay_path, "");
        assert!(!paths.replay_dir.exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_existing_replay_to_slot_overwrites_requested_slot() {
        let root = make_temp_dir("manual-replay-slot");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let config = ReplayConfig {
            auto_save: true,
            compress: false,
            slot_rules: [ReplaySlotRule::Disabled; 4],
        };
        let result = play_result(false);
        let stored = store_play_result(
            &mut score_db,
            &paths,
            &config,
            &result,
            StorePlayResultRequest {
                ln_policy: LnScorePolicy::ForceLn,
                double_option: DoubleOptionScoreBucket::Off,
                played_at: 1_700_000_070,
                playtime_seconds: 0,
                random_seed: Some(7),
                gauge_option: String::new(),
                rule_mode: String::new(),
                assist_mask: 0,
                replay_events: vec![ReplayEvent {
                    lane: Lane::Key1,
                    kind: InputKind::Press,
                    time: TimeUs(10),
                    device_kind: InputDeviceKind::Keyboard,
                }],
                arrange: ArrangeOption::Normal,
                arrange_2p: ArrangeOption::Normal,
                arrange_seed: None,
                arrange_pattern: None,
                mode: StorePlayResultMode::Normal,
            },
        )
        .unwrap();

        let saved = save_existing_replay_to_slot(
            &mut score_db,
            &paths,
            &result,
            &stored,
            LnScorePolicy::ForceLn,
            DoubleOptionScoreBucket::Off,
            bmz_gameplay::rule::RuleMode::Beatoraja,
            2,
        )
        .unwrap()
        .expect("manual slot path");

        assert!(root.join(&saved).exists());
        let key = super::super::score_db::ScoreKey::new([4; 32], LnScorePolicy::ForceLn);
        let slot = score_db.replay_slot(key, 2).unwrap().expect("slot record");
        assert_eq!(slot.rule, ReplaySlotRule::Always);
        assert_eq!(slot.replay_path, saved);
        assert_eq!(slot.played_at, 1_700_000_070);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn store_play_result_saves_failed_replay_for_non_autoplay() {
        // save_failed_runs は廃止 — 失敗ランは常に保存される (オートプレイ除く)
        let root = make_temp_dir("store-failed-result");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let config = ReplayConfig {
            auto_save: true,
            compress: false,
            slot_rules: crate::config::profile_config::default_slot_rules(),
        };
        let mut result = play_result(false);
        result.clear_type = ClearType::Failed;

        let stored = store_play_result(
            &mut score_db,
            &paths,
            &config,
            &result,
            StorePlayResultRequest {
                ln_policy: LnScorePolicy::ForceLn,
                double_option: DoubleOptionScoreBucket::Off,
                played_at: 1_700_000_062,
                playtime_seconds: 0,
                random_seed: None,
                gauge_option: String::new(),
                rule_mode: String::new(),
                assist_mask: 0,
                replay_events: Vec::new(),
                arrange: ArrangeOption::Normal,
                arrange_2p: ArrangeOption::Normal,
                arrange_seed: None,
                arrange_pattern: None,
                mode: StorePlayResultMode::Normal,
            },
        )
        .unwrap();

        assert!(!stored.replay_path.is_empty());
        assert!(root.join(&stored.replay_path).exists());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn store_play_result_course_stage_writes_history_without_single_best_updates() {
        let root = make_temp_dir("store-course-stage-result");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let config = ReplayConfig {
            auto_save: true,
            compress: false,
            slot_rules: crate::config::profile_config::default_slot_rules(),
        };
        let mut result = play_result(false);
        result.clear_type = ClearType::Failed;

        let stored = store_play_result(
            &mut score_db,
            &paths,
            &config,
            &result,
            StorePlayResultRequest {
                ln_policy: LnScorePolicy::ForceLn,
                double_option: DoubleOptionScoreBucket::Off,
                played_at: 1_700_000_063,
                playtime_seconds: 0,
                random_seed: None,
                gauge_option: String::new(),
                rule_mode: String::new(),
                assist_mask: 0,
                replay_events: Vec::new(),
                arrange: ArrangeOption::Normal,
                arrange_2p: ArrangeOption::Normal,
                arrange_seed: None,
                arrange_pattern: None,
                mode: StorePlayResultMode::CourseStage,
            },
        )
        .unwrap();

        assert!(stored.score_history_id > 0);
        assert!(!stored.replay_path.is_empty());
        assert!(stored.slot_paths.iter().all(Option::is_none));

        let history = score_db.recent_history(10, 0).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].clear_type, "NoPlay");
        assert_eq!(history[0].bp, result.total_notes);
        assert!(
            score_db
                .best_scores_for_charts(&[super::super::score_db::ScoreKey::new(
                    [4; 32],
                    LnScorePolicy::ForceLn,
                )])
                .unwrap()
                .is_empty()
        );
        assert!(
            score_db
                .replay_slot(
                    super::super::score_db::ScoreKey::new([4; 32], LnScorePolicy::ForceLn),
                    0,
                )
                .unwrap()
                .is_none()
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn course_stage_clear_type_keeps_only_combo_lamps() {
        assert_eq!(course_stage_clear_type(ClearType::NoPlay), ClearType::NoPlay);
        assert_eq!(course_stage_clear_type(ClearType::Failed), ClearType::NoPlay);
        assert_eq!(course_stage_clear_type(ClearType::Normal), ClearType::NoPlay);
        assert_eq!(course_stage_clear_type(ClearType::FullCombo), ClearType::FullCombo);
        assert_eq!(course_stage_clear_type(ClearType::Perfect), ClearType::Perfect);
        assert_eq!(course_stage_clear_type(ClearType::Max), ClearType::Max);
    }

    #[test]
    fn store_play_result_writes_history_and_default_slot_files() {
        let root = make_temp_dir("store-slot-files");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let config = ReplayConfig {
            auto_save: true,
            compress: false,
            slot_rules: crate::config::profile_config::default_slot_rules(),
        };
        let result = play_result(false);

        let stored = store_play_result(
            &mut score_db,
            &paths,
            &config,
            &result,
            StorePlayResultRequest {
                ln_policy: LnScorePolicy::ForceLn,
                double_option: DoubleOptionScoreBucket::Off,
                played_at: 1_700_000_100,
                playtime_seconds: 0,
                random_seed: None,
                gauge_option: String::new(),
                rule_mode: String::new(),
                assist_mask: 0,
                replay_events: Vec::new(),
                arrange: ArrangeOption::Normal,
                arrange_2p: ArrangeOption::Normal,
                arrange_seed: None,
                arrange_pattern: None,
                mode: StorePlayResultMode::Normal,
            },
        )
        .unwrap();

        // First play with empty slot table -> enabled default slots are populated.
        assert!(stored.slot_paths[..3].iter().all(|p| p.is_some()));
        assert!(stored.slot_paths[3].is_none());
        for path in stored.slot_paths.iter().flatten() {
            assert!(root.join(path).exists());
        }

        // Second play with same score: Always slot updates, but score/miss/combo rules do not
        let stored2 = store_play_result(
            &mut score_db,
            &paths,
            &config,
            &result,
            StorePlayResultRequest {
                ln_policy: LnScorePolicy::ForceLn,
                double_option: DoubleOptionScoreBucket::Off,
                played_at: 1_700_000_101,
                playtime_seconds: 0,
                random_seed: None,
                gauge_option: String::new(),
                rule_mode: String::new(),
                assist_mask: 0,
                replay_events: Vec::new(),
                arrange: ArrangeOption::Normal,
                arrange_2p: ArrangeOption::Normal,
                arrange_seed: None,
                arrange_pattern: None,
                mode: StorePlayResultMode::Normal,
            },
        )
        .unwrap();

        // Default slot 0 = Always (always overwrites)
        assert!(stored2.slot_paths[0].is_some());
        // Slot 1..2 use Score/Bp which require strict improvement; slot 3 is disabled.
        assert!(stored2.slot_paths[1].is_none());
        assert!(stored2.slot_paths[2].is_none());
        assert!(stored2.slot_paths[3].is_none());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn store_play_result_skips_slots_for_autoplay_when_disabled() {
        let root = make_temp_dir("store-slot-autoplay-skip");
        let paths = ProfilePaths {
            root_dir: root.clone(),
            profile_toml: root.join("profile.toml"),
            score_db: root.join("score.db"),
            replay_dir: root.join("replay"),
        };
        let mut conn = Connection::open_in_memory().unwrap();
        configure_connection(&conn).unwrap();
        run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(conn);
        let config = ReplayConfig {
            auto_save: true,
            compress: false,
            slot_rules: crate::config::profile_config::default_slot_rules(),
        };
        let result = play_result(true);

        let stored = store_play_result(
            &mut score_db,
            &paths,
            &config,
            &result,
            StorePlayResultRequest {
                ln_policy: LnScorePolicy::ForceLn,
                double_option: DoubleOptionScoreBucket::Off,
                played_at: 1_700_000_110,
                playtime_seconds: 0,
                random_seed: None,
                gauge_option: String::new(),
                rule_mode: String::new(),
                assist_mask: 0,
                replay_events: Vec::new(),
                arrange: ArrangeOption::Normal,
                arrange_2p: ArrangeOption::Normal,
                arrange_seed: None,
                arrange_pattern: None,
                mode: StorePlayResultMode::Normal,
            },
        )
        .unwrap();

        assert_eq!(stored.replay_path, "");
        assert!(stored.slot_paths.iter().all(|p| p.is_none()));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn slot_rule_score_update_only_when_strictly_better() {
        let prev = ReplaySlotRecord {
            chart_sha256: [0; 32],
            slot: 0,
            rule: ReplaySlotRule::ScoreUpdate,
            replay_path: String::new(),
            ln_policy: LnScorePolicy::ForceLn,
            double_option: DoubleOptionScoreBucket::Off,
            rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
            played_at: 0,
            ex_score: 100,
            bp: 10,
            cb: 10,
            max_combo: 50,
            clear_rank: ClearType::Normal as u8,
        };

        assert!(evaluate_slot_update(
            ReplaySlotRule::ScoreUpdate,
            Some(&prev),
            &CandidateMetrics { ex_score: 101, bp: 10, cb: 10, max_combo: 50, clear_rank: 5 }
        ));
        assert!(!evaluate_slot_update(
            ReplaySlotRule::ScoreUpdate,
            Some(&prev),
            &CandidateMetrics { ex_score: 100, bp: 10, cb: 10, max_combo: 50, clear_rank: 5 }
        ));
        assert!(!evaluate_slot_update(
            ReplaySlotRule::ScoreUpdate,
            Some(&prev),
            &CandidateMetrics { ex_score: 50, bp: 0, cb: 0, max_combo: 100, clear_rank: 6 }
        ));
    }

    #[test]
    fn candidate_metrics_uses_bp_and_cb_helpers() {
        let mut result = play_result(false);
        result.score.judges.fast_bad = 1;
        result.score.judges.slow_poor = 2;
        result.score.judges.fast_empty_poor = 3;

        let metrics = candidate_metrics(&result);

        assert_eq!(metrics.cb, 3);
        assert_eq!(metrics.bp, 6);
    }

    #[test]
    fn candidate_metrics_counts_unprocessed_notes_for_failed_runs() {
        let mut result = play_result(false);
        result.clear_type = ClearType::Failed;
        result.total_notes = 10;

        let metrics = candidate_metrics(&result);

        assert_eq!(metrics.cb, 10);
        assert_eq!(metrics.bp, 10);
    }

    #[test]
    fn slot_rule_bp_update_only_when_strictly_smaller() {
        let prev = ReplaySlotRecord {
            chart_sha256: [0; 32],
            slot: 0,
            rule: ReplaySlotRule::BpUpdate,
            replay_path: String::new(),
            ln_policy: LnScorePolicy::ForceLn,
            double_option: DoubleOptionScoreBucket::Off,
            rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
            played_at: 0,
            ex_score: 100,
            bp: 10,
            cb: 10,
            max_combo: 50,
            clear_rank: ClearType::Normal as u8,
        };

        assert!(evaluate_slot_update(
            ReplaySlotRule::BpUpdate,
            Some(&prev),
            &CandidateMetrics { ex_score: 90, bp: 9, cb: 9, max_combo: 30, clear_rank: 5 }
        ));
        assert!(!evaluate_slot_update(
            ReplaySlotRule::BpUpdate,
            Some(&prev),
            &CandidateMetrics { ex_score: 90, bp: 10, cb: 10, max_combo: 30, clear_rank: 5 }
        ));
    }

    #[test]
    fn slot_rule_clear_update_only_when_higher_rank() {
        let prev = ReplaySlotRecord {
            chart_sha256: [0; 32],
            slot: 0,
            rule: ReplaySlotRule::ClearUpdate,
            replay_path: String::new(),
            ln_policy: LnScorePolicy::ForceLn,
            double_option: DoubleOptionScoreBucket::Off,
            rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
            played_at: 0,
            ex_score: 100,
            bp: 10,
            cb: 10,
            max_combo: 50,
            clear_rank: ClearType::Normal as u8,
        };

        assert!(evaluate_slot_update(
            ReplaySlotRule::ClearUpdate,
            Some(&prev),
            &CandidateMetrics {
                ex_score: 90,
                bp: 9,
                cb: 9,
                max_combo: 30,
                clear_rank: ClearType::Hard as u8,
            }
        ));
        assert!(!evaluate_slot_update(
            ReplaySlotRule::ClearUpdate,
            Some(&prev),
            &CandidateMetrics {
                ex_score: 90,
                bp: 9,
                cb: 9,
                max_combo: 30,
                clear_rank: ClearType::Failed as u8,
            }
        ));
    }

    #[test]
    fn slot_rule_always_overwrites_unconditionally() {
        let prev = ReplaySlotRecord {
            chart_sha256: [0; 32],
            slot: 0,
            rule: ReplaySlotRule::Always,
            replay_path: String::new(),
            ln_policy: LnScorePolicy::ForceLn,
            double_option: DoubleOptionScoreBucket::Off,
            rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
            played_at: 0,
            ex_score: 10_000,
            bp: 0,
            cb: 0,
            max_combo: 9_999,
            clear_rank: ClearType::Perfect as u8,
        };

        assert!(evaluate_slot_update(
            ReplaySlotRule::Always,
            Some(&prev),
            &CandidateMetrics {
                ex_score: 0,
                bp: 9_999,
                cb: 9_999,
                max_combo: 0,
                clear_rank: ClearType::Failed as u8,
            }
        ));
    }

    #[test]
    fn slot_rule_disabled_never_writes() {
        let candidate = CandidateMetrics {
            ex_score: 10,
            bp: 1,
            cb: 1,
            max_combo: 10,
            clear_rank: ClearType::AssistEasy as u8,
        };

        assert!(!evaluate_slot_update(ReplaySlotRule::Disabled, None, &candidate));
        assert!(!slot_rule_passes(ReplaySlotRule::Disabled, Some((0, 999, 0, 0)), &candidate));
    }

    #[test]
    fn slot_rule_first_record_always_written() {
        let candidate = CandidateMetrics {
            ex_score: 0,
            bp: 0,
            cb: 0,
            max_combo: 0,
            clear_rank: ClearType::Failed as u8,
        };
        for &rule in &[
            ReplaySlotRule::Always,
            ReplaySlotRule::ScoreUpdate,
            ReplaySlotRule::BpUpdate,
            ReplaySlotRule::MaxComboUpdate,
            ReplaySlotRule::ClearUpdate,
        ] {
            assert!(
                evaluate_slot_update(rule, None, &candidate),
                "first record must be written for rule {rule:?}"
            );
        }
    }

    #[test]
    fn classify_replay_device_type_uses_controller_majority() {
        let events = vec![
            ReplayEvent {
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: TimeUs(10),
                device_kind: InputDeviceKind::Controller,
            },
            ReplayEvent {
                lane: Lane::Key2,
                kind: InputKind::Press,
                time: TimeUs(20),
                device_kind: InputDeviceKind::Controller,
            },
            ReplayEvent {
                lane: Lane::Scratch,
                kind: InputKind::Press,
                time: TimeUs(30),
                device_kind: InputDeviceKind::Keyboard,
            },
            ReplayEvent {
                lane: Lane::Key1,
                kind: InputKind::Release,
                time: TimeUs(40),
                device_kind: InputDeviceKind::Keyboard,
            },
        ];

        assert_eq!(classify_replay_device_type(&events), InputDeviceKind::Controller);
    }

    #[test]
    fn classify_replay_device_type_defaults_keyboard_for_ties() {
        let events = vec![
            ReplayEvent {
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: TimeUs(10),
                device_kind: InputDeviceKind::Controller,
            },
            ReplayEvent {
                lane: Lane::Key2,
                kind: InputKind::Press,
                time: TimeUs(20),
                device_kind: InputDeviceKind::Keyboard,
            },
        ];

        assert_eq!(classify_replay_device_type(&events), InputDeviceKind::Keyboard);
    }

    fn play_result(autoplay: bool) -> PlayResult {
        PlayResult {
            chart_sha256: [4; 32],
            clear_type: ClearType::Normal,
            gauge_type: GaugeType::Normal,
            gauge_value: 80.0,
            total_notes: 1,
            score: ScoreState::default(),
            autoplay,
        }
    }

    fn make_temp_dir(label: &str) -> std::path::PathBuf {
        let stamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path =
            std::env::temp_dir().join(format!("bmz-player-{label}-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}
