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
    let decisions =
        vec![bmz_core::replay::BranchDecision { block: 0, branch: 1, time: TimeUs(10) }];
    let root = make_temp_dir("store-result");
    let paths = ProfilePaths {
        root_dir: root.clone(),
        profile_toml: root.join("profile.toml"),
        collection_db: root.join("collection.db"),
        score_db: root.join("score.db"),
        network_db: root.join("network.db"),
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
            applied_double_option: DoubleOption::Flip,
            played_at: 1_700_000_060,
            playtime_seconds: 0,
            random_seed: Some(77),
            gauge_option: String::new(),
            rule_mode: String::new(),
            assist_mask: 0,
            branch_decisions: Some(decisions.clone()),
            replay_events: vec![ReplayEvent {
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: TimeUs(10),
                device_kind: InputDeviceKind::Keyboard,
                scratch_direction: None,
            }],
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            arrange_seed: None,
            arrange_seed_2p: None,
            bms_random_choices: Vec::new(),
            bms_switch_choices: Vec::new(),
            seed_scheme: String::new(),
            s_random_scheme: crate::screens::play_session::SRandomScheme::Lm120HzV1,
            s_random_scheme_2p: None,
            h_random_threshold_ms: None,
            arrange_pattern: None,
            update_score: true,
            mode: StorePlayResultMode::Normal,
        },
    )
    .unwrap();

    assert!(stored.score_history_id > 0);
    assert!(!stored.replay_path.is_empty());
    assert!(root.join(&stored.replay_path).exists());
    let replay = crate::storage::replay::load_replay(&root.join(&stored.replay_path)).unwrap();
    assert_eq!(replay.branch_decisions, Some(decisions));
    assert_eq!(
        replay.effective_s_random_scheme().unwrap(),
        crate::screens::play_session::SRandomScheme::Lm120HzV1
    );
    assert_eq!(replay.double_option(), DoubleOption::Flip);
    assert_eq!(replay.recorded_gauge_type(), Some(result.gauge_type));
    assert_eq!(score_db.recent_history(1, 0).unwrap()[0].applied_double_option, DoubleOption::Flip);
    assert_eq!(
        score_db
            .best_ex_score(super::super::score_db::ScoreKey::new([4; 32], LnScorePolicy::ForceLn))
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
        collection_db: root.join("collection.db"),
        score_db: root.join("score.db"),
        network_db: root.join("network.db"),
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
            applied_double_option: DoubleOption::Off,
            played_at: 1_700_000_061,
            playtime_seconds: 0,
            random_seed: None,
            gauge_option: String::new(),
            rule_mode: String::new(),
            assist_mask: 0,
            branch_decisions: None,
            replay_events: Vec::new(),
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            arrange_seed: None,
            arrange_seed_2p: None,
            bms_random_choices: Vec::new(),
            bms_switch_choices: Vec::new(),
            seed_scheme: String::new(),
            s_random_scheme: crate::screens::play_session::SRandomScheme::Lm120HzV1,
            s_random_scheme_2p: None,
            h_random_threshold_ms: None,
            arrange_pattern: None,
            update_score: true,
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
        collection_db: root.join("collection.db"),
        score_db: root.join("score.db"),
        network_db: root.join("network.db"),
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
            applied_double_option: DoubleOption::Off,
            played_at: 1_700_000_070,
            playtime_seconds: 0,
            random_seed: Some(7),
            gauge_option: String::new(),
            rule_mode: String::new(),
            assist_mask: 0,
            branch_decisions: None,
            replay_events: vec![ReplayEvent {
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: TimeUs(10),
                device_kind: InputDeviceKind::Keyboard,
                scratch_direction: None,
            }],
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            arrange_seed: None,
            arrange_seed_2p: None,
            bms_random_choices: Vec::new(),
            bms_switch_choices: Vec::new(),
            seed_scheme: String::new(),
            s_random_scheme: crate::screens::play_session::SRandomScheme::Lm120HzV1,
            s_random_scheme_2p: None,
            h_random_threshold_ms: None,
            arrange_pattern: None,
            update_score: true,
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
        collection_db: root.join("collection.db"),
        score_db: root.join("score.db"),
        network_db: root.join("network.db"),
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
            applied_double_option: DoubleOption::Off,
            played_at: 1_700_000_062,
            playtime_seconds: 0,
            random_seed: None,
            gauge_option: String::new(),
            rule_mode: String::new(),
            assist_mask: 0,
            branch_decisions: None,
            replay_events: Vec::new(),
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            arrange_seed: None,
            arrange_seed_2p: None,
            bms_random_choices: Vec::new(),
            bms_switch_choices: Vec::new(),
            seed_scheme: String::new(),
            s_random_scheme: crate::screens::play_session::SRandomScheme::Lm120HzV1,
            s_random_scheme_2p: None,
            h_random_threshold_ms: None,
            arrange_pattern: None,
            update_score: true,
            mode: StorePlayResultMode::Normal,
        },
    )
    .unwrap();

    assert!(!stored.replay_path.is_empty());
    assert!(root.join(&stored.replay_path).exists());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn store_play_result_course_stage_updates_single_best_with_rounded_clear() {
    let root = make_temp_dir("store-course-stage-result");
    let paths = ProfilePaths {
        root_dir: root.clone(),
        profile_toml: root.join("profile.toml"),
        collection_db: root.join("collection.db"),
        score_db: root.join("score.db"),
        network_db: root.join("network.db"),
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
            applied_double_option: DoubleOption::Off,
            played_at: 1_700_000_063,
            playtime_seconds: 0,
            random_seed: None,
            gauge_option: String::new(),
            rule_mode: String::new(),
            assist_mask: 0,
            branch_decisions: None,
            replay_events: Vec::new(),
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            arrange_seed: None,
            arrange_seed_2p: None,
            bms_random_choices: Vec::new(),
            bms_switch_choices: Vec::new(),
            seed_scheme: String::new(),
            s_random_scheme: crate::screens::play_session::SRandomScheme::Lm120HzV1,
            s_random_scheme_2p: None,
            h_random_threshold_ms: None,
            arrange_pattern: None,
            update_score: true,
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
    let bests = score_db
        .best_scores_for_charts(&[super::super::score_db::ScoreKey::new(
            [4; 32],
            LnScorePolicy::ForceLn,
        )])
        .unwrap();
    assert_eq!(bests.len(), 1);
    assert_eq!(bests[0].clear_type, "NoPlay");
    assert_eq!(bests[0].ex_score, result.score.ex_score());
    assert_eq!(bests[0].max_combo, result.score.max_combo);
    assert_eq!(bests[0].bp, result.total_notes);
    assert!(
        score_db
            .replay_slot(super::super::score_db::ScoreKey::new([4; 32], LnScorePolicy::ForceLn), 0,)
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
        collection_db: root.join("collection.db"),
        score_db: root.join("score.db"),
        network_db: root.join("network.db"),
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
            applied_double_option: DoubleOption::Off,
            played_at: 1_700_000_100,
            playtime_seconds: 0,
            random_seed: None,
            gauge_option: String::new(),
            rule_mode: String::new(),
            assist_mask: 0,
            branch_decisions: None,
            replay_events: Vec::new(),
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            arrange_seed: None,
            arrange_seed_2p: None,
            bms_random_choices: Vec::new(),
            bms_switch_choices: Vec::new(),
            seed_scheme: String::new(),
            s_random_scheme: crate::screens::play_session::SRandomScheme::Lm120HzV1,
            s_random_scheme_2p: None,
            h_random_threshold_ms: None,
            arrange_pattern: None,
            update_score: true,
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
            applied_double_option: DoubleOption::Off,
            played_at: 1_700_000_101,
            playtime_seconds: 0,
            random_seed: None,
            gauge_option: String::new(),
            rule_mode: String::new(),
            assist_mask: 0,
            branch_decisions: None,
            replay_events: Vec::new(),
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            arrange_seed: None,
            arrange_seed_2p: None,
            bms_random_choices: Vec::new(),
            bms_switch_choices: Vec::new(),
            seed_scheme: String::new(),
            s_random_scheme: crate::screens::play_session::SRandomScheme::Lm120HzV1,
            s_random_scheme_2p: None,
            h_random_threshold_ms: None,
            arrange_pattern: None,
            update_score: true,
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
        collection_db: root.join("collection.db"),
        score_db: root.join("score.db"),
        network_db: root.join("network.db"),
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
            applied_double_option: DoubleOption::Off,
            played_at: 1_700_000_110,
            playtime_seconds: 0,
            random_seed: None,
            gauge_option: String::new(),
            rule_mode: String::new(),
            assist_mask: 0,
            branch_decisions: None,
            replay_events: Vec::new(),
            arrange: ArrangeOption::Normal,
            arrange_2p: ArrangeOption::Normal,
            arrange_seed: None,
            arrange_seed_2p: None,
            bms_random_choices: Vec::new(),
            bms_switch_choices: Vec::new(),
            seed_scheme: String::new(),
            s_random_scheme: crate::screens::play_session::SRandomScheme::Lm120HzV1,
            s_random_scheme_2p: None,
            h_random_threshold_ms: None,
            arrange_pattern: None,
            update_score: true,
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
        ex_score: Some(100),
        bp: Some(10),
        cb: Some(10),
        max_combo: Some(50),
        clear_rank: Some(ClearType::Normal as u8),
        source_kind: crate::storage::score_db::ScoreSourceKind::Local,
        source_path: String::new(),
        source_fingerprint: String::new(),
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
fn comparison_rule_replaces_imported_slot_with_unknown_metrics() {
    let prev = ReplaySlotRecord {
        chart_sha256: [0; 32],
        slot: 0,
        rule: ReplaySlotRule::ScoreUpdate,
        replay_path: String::new(),
        ln_policy: LnScorePolicy::ForceLn,
        double_option: DoubleOptionScoreBucket::Off,
        rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
        played_at: 0,
        ex_score: None,
        bp: None,
        cb: None,
        max_combo: None,
        clear_rank: None,
        source_kind: crate::storage::score_db::ScoreSourceKind::Beatoraja,
        source_path: "source.brd".to_string(),
        source_fingerprint: String::new(),
    };

    assert!(evaluate_slot_update(
        ReplaySlotRule::ScoreUpdate,
        Some(&prev),
        &CandidateMetrics { ex_score: 1, bp: 1, cb: 1, max_combo: 1, clear_rank: 1 }
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
        ex_score: Some(100),
        bp: Some(10),
        cb: Some(10),
        max_combo: Some(50),
        clear_rank: Some(ClearType::Normal as u8),
        source_kind: crate::storage::score_db::ScoreSourceKind::Local,
        source_path: String::new(),
        source_fingerprint: String::new(),
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
        ex_score: Some(100),
        bp: Some(10),
        cb: Some(10),
        max_combo: Some(50),
        clear_rank: Some(ClearType::Normal as u8),
        source_kind: crate::storage::score_db::ScoreSourceKind::Local,
        source_path: String::new(),
        source_fingerprint: String::new(),
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
        ex_score: Some(10_000),
        bp: Some(0),
        cb: Some(0),
        max_combo: Some(9_999),
        clear_rank: Some(ClearType::Perfect as u8),
        source_kind: crate::storage::score_db::ScoreSourceKind::Local,
        source_path: String::new(),
        source_fingerprint: String::new(),
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
            scratch_direction: None,
        },
        ReplayEvent {
            lane: Lane::Key2,
            kind: InputKind::Press,
            time: TimeUs(20),
            device_kind: InputDeviceKind::Controller,
            scratch_direction: None,
        },
        ReplayEvent {
            lane: Lane::Scratch,
            kind: InputKind::Press,
            time: TimeUs(30),
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
        },
        ReplayEvent {
            lane: Lane::Key1,
            kind: InputKind::Release,
            time: TimeUs(40),
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
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
            scratch_direction: None,
        },
        ReplayEvent {
            lane: Lane::Key2,
            kind: InputKind::Press,
            time: TimeUs(20),
            device_kind: InputDeviceKind::Keyboard,
            scratch_direction: None,
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
