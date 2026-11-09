use super::*;

#[test]
fn cleanup_preserves_assisted_and_course_aggregates() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };
    let baseline = record(20, ClearType::NoPlay);
    let removed = db.insert_score(&baseline).unwrap();
    let remaining = db.insert_score(&baseline).unwrap();
    let mut assisted = record(100, ClearType::LightAssistEasy);
    assisted.score.max_combo = 100;
    db.update_score_clear_only(&assisted).unwrap();
    assisted.chart_sha256 = [8; 32];
    db.update_score_clear_only(&assisted).unwrap();
    let mut course = record(40, ClearType::FullCombo);
    course.chart_sha256 = [9; 32];
    let course_history = db.insert_score(&course).unwrap();
    let course_id = insert_test_course_score(&mut db, "cleanup-course");
    db.conn()
        .execute(
            "UPDATE score_history SET course_score_id = ?1 WHERE id = ?2",
            params![course_id, course_history],
        )
        .unwrap();

    db.purge_score_history_ids_and_rebuild(&[removed]).unwrap();
    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!((best.ex_score, best.play_count, best.clear_count), (20, 2, 1));
    assert_eq!(best.clear_type, "LightAssistEasy");
    let untouched = db.best_scores_for_charts(&[key([8; 32])]).unwrap().pop().unwrap();
    assert_eq!((untouched.ex_score, untouched.play_count), (0, 1));
    assert_eq!(untouched.clear_type, "LightAssistEasy");
    let course_best = db.best_scores_for_charts(&[key([9; 32])]).unwrap().pop().unwrap();
    assert_eq!((course_best.ex_score, course_best.play_count), (40, 1));
    let stats = db.player_stats().unwrap();
    assert_eq!((stats.play_count, stats.clear_count, stats.max_combo), (4, 3, 100));
    assert_eq!(stats.slow_pgreat, 130);

    db.purge_score_history_ids_and_rebuild(&[remaining]).unwrap();
    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!((best.ex_score, best.play_count, best.clear_count), (0, 1, 1));
    assert_eq!(best.clear_type, "LightAssistEasy");
    // Rebuilding an affected key must retain its remaining course-stage history.
    let ordinary_course_key = db.insert_score(&course).unwrap();
    db.purge_score_history_ids_and_rebuild(&[ordinary_course_key]).unwrap();
    let best = db.best_scores_for_charts(&[key([9; 32])]).unwrap().pop().unwrap();
    assert_eq!((best.ex_score, best.play_count), (40, 1));
}

#[test]
fn score_best_keeps_independent_bp_cb_and_max_combo_records() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut best_score = record(20, ClearType::Normal);
    best_score.score.max_combo = 30;
    best_score.score.judges.fast_bad = 4;
    db.insert_score(&best_score).unwrap();

    let mut lower_score = record(10, ClearType::Failed);
    lower_score.played_at = 2;
    lower_score.score.max_combo = 80;
    lower_score.score.judges.fast_bad = 2;
    db.insert_score(&lower_score).unwrap();

    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(best.ex_score, 20);
    assert_eq!(best.clear_type, "Normal");
    assert_eq!(best.max_combo, 80);
    assert_eq!(best.bp, 2);
    assert_eq!(best.cb, 2);
    assert_eq!(best.play_count, 2);
    assert_eq!(best.clear_count, 1);
}

#[test]
fn clear_only_updates_lamp_counts_and_player_stats_without_numeric_score_or_history() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut baseline = record(20, ClearType::NoPlay);
    baseline.played_at = 10;
    baseline.playtime_seconds = 30;
    baseline.score.max_combo = 30;
    baseline.score.judges.fast_bad = 4;
    db.insert_score(&baseline).unwrap();

    let mut assisted = record(200, ClearType::LightAssistEasy);
    assisted.played_at = 20;
    assisted.playtime_seconds = 120;
    assisted.score.max_combo = 100;
    assisted.score.judges.slow_good = 7;
    db.update_score_clear_only(&assisted).unwrap();

    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(best.clear_type, "LightAssistEasy");
    assert_eq!(best.ex_score, 20);
    assert_eq!(best.max_combo, 30);
    assert_eq!(best.bp, 4);
    assert_eq!(best.cb, 4);
    assert_eq!(best.play_count, 2);
    assert_eq!(best.clear_count, 1);
    assert_eq!(db.recent_history(10, 0).unwrap().len(), 1);

    let stats = db.player_stats().unwrap();
    assert_eq!(stats.play_count, 2);
    assert_eq!(stats.clear_count, 1);
    assert_eq!(stats.playtime_seconds, 150);
    assert_eq!(stats.max_combo, 100);
    assert_eq!(stats.fast_bad, 4);
    assert_eq!(stats.slow_good, 7);
    assert_eq!(stats.updated_at, 20);

    let mut lower_lamp = record(400, ClearType::AssistEasy);
    lower_lamp.score.max_combo = 200;
    db.update_score_clear_only(&lower_lamp).unwrap();
    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(best.clear_type, "LightAssistEasy");
    assert_eq!(best.ex_score, 20);
    assert_eq!(best.max_combo, 30);
    assert_eq!(best.play_count, 3);
    assert_eq!(best.clear_count, 2);

    let stats = db.player_stats().unwrap();
    assert_eq!(stats.play_count, 3);
    assert_eq!(stats.clear_count, 2);
    assert_eq!(stats.max_combo, 200);
}

#[test]
fn clear_only_update_creates_neutral_best_row() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let assisted = record(200, ClearType::AssistEasy);
    db.update_score_clear_only(&assisted).unwrap();

    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(best.clear_type, "AssistEasy");
    assert_eq!(best.ex_score, 0);
    assert_eq!(best.max_combo, 0);
    assert_eq!(best.bp, 0);
    assert_eq!(best.cb, 0);
    assert_eq!(best.play_count, 1);
    assert_eq!(best.clear_count, 1);
    assert!(db.recent_history(10, 0).unwrap().is_empty());
}

#[test]
fn failed_score_counts_unprocessed_notes_for_bp_and_cb_records() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut failed = record(0, ClearType::Failed);
    failed.total_notes = 100;
    db.insert_score(&failed).unwrap();

    let history = db.recent_history(10, 0).unwrap();
    assert_eq!(history[0].bp, 100);
    assert_eq!(history[0].cb, 100);

    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(best.clear_type, "Failed");
    assert_eq!(best.bp, 100);
    assert_eq!(best.cb, 100);
}

#[test]
fn score_best_is_separate_per_ln_policy() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut ln = record(20, ClearType::Normal);
    ln.ln_policy = LnScorePolicy::ForceLn;
    let mut cn = record(40, ClearType::Hard);
    cn.ln_policy = LnScorePolicy::ForceCn;

    db.insert_score(&ln).unwrap();
    db.insert_score(&cn).unwrap();

    assert_eq!(db.best_ex_score(key([7; 32])).unwrap(), Some(20));
    assert_eq!(db.best_ex_score(ScoreKey::new([7; 32], LnScorePolicy::ForceCn)).unwrap(), Some(40));
}

#[test]
fn player_info_is_created_and_display_name_updates() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let info = db.player_info().unwrap();
    assert_eq!(info.player_uuid.len(), 32);
    assert!(info.player_uuid.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(info.display_name, "");

    db.set_player_display_name("hyrorre", 1_700_000_099).unwrap();

    let info = db.player_info().unwrap();
    assert_eq!(info.display_name, "hyrorre");
    assert_eq!(info.updated_at, 1_700_000_099);
}

#[test]
fn player_stats_accumulates_profile_wide_scores() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut first = record(20, ClearType::Normal);
    first.played_at = 10;
    first.playtime_seconds = 120;
    first.score.judges.fast_great = 3;
    first.score.judges.slow_bad = 2;
    let mut failed = record(10, ClearType::Failed);
    failed.played_at = 20;
    failed.playtime_seconds = 30;
    failed.score.max_combo = 99;
    failed.score.judges.fast_empty_poor = 4;

    db.insert_score(&first).unwrap();
    db.insert_score(&failed).unwrap();

    let stats = db.player_stats().unwrap();
    assert_eq!(stats.play_count, 2);
    assert_eq!(stats.clear_count, 1);
    assert_eq!(stats.playtime_seconds, 150);
    assert_eq!(stats.max_combo, 99);
    assert_eq!(stats.fast_pgreat, 0);
    assert_eq!(stats.slow_pgreat, 15);
    assert_eq!(stats.fast_great, 3);
    assert_eq!(stats.slow_bad, 2);
    assert_eq!(stats.fast_empty_poor, 4);
    assert_eq!(stats.updated_at, 20);
}

#[test]
fn daily_player_stats_aggregates_only_local_history_inside_range() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut played = record(0, ClearType::Normal);
    played.played_at = 110;
    played.score = ScoreState::default();
    played.score.judges.fast_pgreat = 2;
    played.score.judges.slow_great = 3;
    played.score.judges.fast_good = 4;
    played.score.judges.slow_bad = 5;
    played.score.judges.fast_poor = 6;
    played.score.judges.slow_empty_poor = 7;
    db.insert_score(&played).unwrap();

    let mut failed = record(0, ClearType::Failed);
    failed.chart_sha256 = [8; 32];
    failed.played_at = 120;
    failed.score = ScoreState::default();
    failed.score.judges.slow_pgreat = 11;
    db.insert_score(&failed).unwrap();

    let mut outside = record(0, ClearType::Normal);
    outside.chart_sha256 = [9; 32];
    outside.played_at = 99;
    outside.score = ScoreState::default();
    outside.score.judges.fast_pgreat = 100;
    db.insert_score(&outside).unwrap();

    let mut imported = record(0, ClearType::Normal);
    imported.chart_sha256 = [10; 32];
    imported.played_at = 130;
    imported.source_kind = ScoreSourceKind::Beatoraja;
    imported.score = ScoreState::default();
    imported.score.judges.fast_pgreat = 200;
    db.insert_score(&imported).unwrap();

    let stats = db.daily_player_stats_between(100, 200).unwrap();
    assert_eq!(
        stats,
        DailyPlayerStats {
            play_count: 2,
            clear_count: 1,
            pgreat: 13,
            great: 3,
            good: 4,
            bad: 5,
            poor: 6,
            empty_poor: 7,
            score_update_count: 2,
            clear_update_count: 2,
            miss_count_update_count: 2,
        }
    );
    assert_eq!(
        db.daily_recent_chart_sha256s_between(100, 200, 10).unwrap(),
        vec![[8; 32], [7; 32]]
    );
    db.reset_daily_statistics(i64::MAX).unwrap();
    let (reset_start, reset_end) = db.current_daily_statistics_range(0).unwrap();
    assert_eq!(reset_start, reset_end);
    assert_eq!(db.current_local_day_player_stats().unwrap(), DailyPlayerStats::default());
}

#[test]
fn player_stats_migration_backfills_existing_history() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, &SCORE_MIGRATIONS[..7]).unwrap();
    conn.execute(
        "INSERT INTO score_history (
                chart_sha256, ln_policy, played_at, clear_type, gauge_type, gauge_value,
                total_notes, ex_score, bp, cb, max_combo,
                fast_pgreat, slow_pgreat, fast_great, slow_great,
                fast_good, slow_good, fast_bad, slow_bad,
                fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
                random_seed, gauge_option, rule_mode, assist_mask, autoplay,
                replay_path, ghost
            ) VALUES (
                ?1, 'ForceLn', 10, 'Normal', 'Normal', 80.0,
                10, 20, 1, 1, 8,
                1, 2, 3, 4,
                5, 6, 7, 8,
                9, 10, 11, 12,
                NULL, '', 'Beatoraja', 0, 0,
                '', ''
            )",
        params![hash_to_hex(&[1; 32])],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO score_history (
                chart_sha256, ln_policy, played_at, clear_type, gauge_type, gauge_value,
                total_notes, ex_score, bp, cb, max_combo,
                fast_pgreat, slow_pgreat, fast_great, slow_great,
                fast_good, slow_good, fast_bad, slow_bad,
                fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
                random_seed, gauge_option, rule_mode, assist_mask, autoplay,
                replay_path, ghost
            ) VALUES (
                ?1, 'ForceLn', 20, 'Failed', 'Normal', 20.0,
                10, 10, 5, 5, 12,
                2, 3, 4, 5,
                6, 7, 8, 9,
                10, 11, 12, 13,
                NULL, '', 'Beatoraja', 0, 0,
                '', ''
            )",
        params![hash_to_hex(&[2; 32])],
    )
    .unwrap();

    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let db = ScoreDatabase { conn };

    let stats = db.player_stats().unwrap();
    assert_eq!(stats.play_count, 2);
    assert_eq!(stats.clear_count, 1);
    assert_eq!(stats.playtime_seconds, 0);
    assert_eq!(stats.max_combo, 12);
    assert_eq!(stats.fast_pgreat, 3);
    assert_eq!(stats.slow_empty_poor, 25);
    assert_eq!(stats.updated_at, 20);
}

#[test]
fn score_history_migration_drops_history_ghost_and_sanitizes_course_links() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, &SCORE_MIGRATIONS[..18]).unwrap();
    conn.execute(
        "INSERT INTO score_history (
                chart_sha256, played_at, clear_type, gauge_type, gauge_value,
                total_notes, ex_score, bp, cb, max_combo,
                fast_pgreat, slow_pgreat, fast_great, slow_great,
                fast_good, slow_good, fast_bad, slow_bad,
                fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
                random_seed, gauge_option, assist_mask, autoplay,
                replay_path, ghost, course_score_id
            ) VALUES (
                ?1, 10, 'Normal', 'Normal', 80.0,
                10, 20, 1, 1, 8,
                1, 2, 3, 4,
                5, 6, 7, 8,
                9, 10, 11, 12,
                NULL, '', 0, 0,
                '', 'legacy-ghost', 9999
            )",
        params![hash_to_hex(&[1; 32])],
    )
    .unwrap();

    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();

    let columns: Vec<String> = conn
        .prepare("PRAGMA table_info(score_history)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert!(!columns.iter().any(|column| column == "ghost"));

    let course_score_id: Option<i64> =
        conn.query_row("SELECT course_score_id FROM score_history", [], |row| row.get(0)).unwrap();
    assert_eq!(course_score_id, None);

    let (source_kind, arrange_2p, applied_double_option): (String, String, String) = conn
        .query_row(
            "SELECT source_kind, arrange_2p, applied_double_option FROM score_history",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(source_kind, ScoreSourceKind::Local.as_str());
    assert_eq!(arrange_2p, "Normal");
    assert_eq!(applied_double_option, DoubleOption::Off.to_persistent_str());
}

#[test]
fn score_history_records_previous_best_snapshot() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut first = record(20, ClearType::Normal);
    first.played_at = 10;
    first.score.max_combo = 15;
    first.score.judges.fast_bad = 2;
    db.insert_score(&first).unwrap();

    let mut second = record(30, ClearType::Hard);
    second.played_at = 20;
    db.insert_score(&second).unwrap();

    let history = db.recent_history(10, 0).unwrap();
    assert_eq!(history[1].previous_best, None);
    assert_eq!(
        history[0].previous_best,
        Some(PreviousBestSnapshot {
            clear_type: "Normal".to_string(),
            ex_score: 20,
            max_combo: 15,
            bp: 2,
            cb: 2,
        })
    );
}

#[test]
fn chart_update_times_separates_lamp_and_ex_score_improvements() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut first = record(20, ClearType::Normal);
    first.played_at = 10;
    db.insert_score(&first).unwrap();
    let mut lamp_only = record(10, ClearType::Hard);
    lamp_only.played_at = 20;
    db.insert_score(&lamp_only).unwrap();
    let mut score_only = record(30, ClearType::Easy);
    score_only.played_at = 30;
    db.insert_score(&score_only).unwrap();

    let updates = db.chart_update_times_since(&[key([7; 32])], 0).unwrap();
    let updates = updates.get(&key([7; 32])).unwrap();
    assert_eq!(updates.lamp, [10, 20]);
    assert_eq!(updates.score, [10, 30]);
}

#[test]
fn score_history_previous_best_is_separate_per_ln_policy() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut ln = record(20, ClearType::Normal);
    ln.ln_policy = LnScorePolicy::ForceLn;
    ln.played_at = 10;
    let mut cn_first = record(40, ClearType::Hard);
    cn_first.ln_policy = LnScorePolicy::ForceCn;
    cn_first.played_at = 20;
    let mut cn_second = record(10, ClearType::Easy);
    cn_second.ln_policy = LnScorePolicy::ForceCn;
    cn_second.played_at = 30;

    db.insert_score(&ln).unwrap();
    db.insert_score(&cn_first).unwrap();
    db.insert_score(&cn_second).unwrap();

    let history = db.recent_history(10, 0).unwrap();
    assert_eq!(
        history[0].previous_best.as_ref().map(|best| (best.clear_type.as_str(), best.ex_score)),
        Some(("Hard", 40))
    );
    assert_eq!(history[1].previous_best, None);
    assert_eq!(history[2].previous_best, None);
}

#[test]
fn beatoraja_ghost_round_trips_as_gzip_urlsafe_base64() {
    let ghost = vec![0, 1, 2, 3, 4];

    let encoded = encode_beatoraja_ghost(&ghost).unwrap();
    let decoded = decode_beatoraja_ghost(&encoded, ghost.len() as u32).unwrap();

    assert_eq!(decoded, ghost);
}

#[test]
fn insert_score_persists_best_ghost_for_current_best() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    db.insert_score(&record(20, ClearType::Normal)).unwrap();
    db.insert_score(&record(10, ClearType::Hard)).unwrap();

    assert_eq!(db.best_ghost(key([7; 32]), 10).unwrap(), Some(vec![0; 10]));
}

#[test]
fn class_gauge_types_round_trip_via_score_history_and_best() {
    // 段位ゲージで終わったプレイが score_history / score_best 経由で
    // `"Class" / "ExClass" / "ExHardClass"` の文字列として正しく永続化・
    // 復元されることを担保する。
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let cases = [
        ([10u8; 32], GaugeType::Class, "Class"),
        ([11u8; 32], GaugeType::ExClass, "ExClass"),
        ([12u8; 32], GaugeType::ExHardClass, "ExHardClass"),
    ];

    // ex_score は (sha[0], 段位ごと) で順に上げ、score_best が上書きされて
    // 残ることを保証する。
    for (i, (sha, gauge, _)) in cases.iter().enumerate() {
        let mut rec = record(20 + i as u32 * 10, ClearType::Hard);
        rec.chart_sha256 = *sha;
        rec.gauge_type = Some(*gauge);
        rec.gauge_value = Some(42.0 + i as f32);
        db.insert_score(&rec).unwrap();
    }

    // score_history: GaugeType::as_str() の文字列で素直に入る。
    let history = db.recent_history(10, 0).unwrap();
    let mut history_map: std::collections::HashMap<[u8; 32], String> =
        history.into_iter().map(|entry| (entry.chart_sha256, entry.gauge_type)).collect();
    for (sha, _, expected) in &cases {
        assert_eq!(history_map.remove(sha).as_deref(), Some(*expected), "history {sha:?}");
    }

    // score_best: 同じく文字列でラウンドトリップ、gauge_value も保持される。
    let keys: Vec<_> = cases.iter().map(|(sha, _, _)| key(*sha)).collect();
    let best = db.best_scores_for_charts(&keys).unwrap();
    assert_eq!(best.len(), 3);
    let mut by_sha: std::collections::HashMap<_, _> =
        best.into_iter().map(|s| (s.chart_sha256, s)).collect();
    for (i, (sha, _, expected_label)) in cases.iter().enumerate() {
        let summary = by_sha.remove(sha).expect("best entry exists");
        assert_eq!(summary.gauge_type, *expected_label);
        assert_eq!(summary.gauge_value, Some(42.0 + i as f32));
    }
}

#[test]
fn gauge_type_str_matches_enum_display_for_class_gauges() {
    assert_eq!(gauge_type_str(Some(GaugeType::Class)), "Class");
    assert_eq!(gauge_type_str(Some(GaugeType::ExClass)), "ExClass");
    assert_eq!(gauge_type_str(Some(GaugeType::ExHardClass)), "ExHardClass");
    // sanity: 非段位ゲージも従来通り。
    assert_eq!(gauge_type_str(Some(GaugeType::Normal)), "Normal");
    assert_eq!(gauge_type_str(None), "");
}

#[test]
fn best_scores_for_charts_returns_existing_scores() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };
    let mut first = record(20, ClearType::Normal);
    first.chart_sha256 = [1; 32];
    first.replay_path = "replay/one.bzr".to_string();
    let mut second = record(10, ClearType::Easy);
    second.chart_sha256 = [2; 32];
    second.gauge_type = None;

    db.insert_score(&first).unwrap();
    db.insert_score(&second).unwrap();

    let scores = db.best_scores_for_charts(&[key([2; 32]), key([3; 32]), key([1; 32])]).unwrap();

    assert_eq!(scores.len(), 2);
    assert_eq!(scores[0].chart_sha256, [2; 32]);
    assert_eq!(scores[0].gauge_type, "");
    assert_eq!(scores[1].chart_sha256, [1; 32]);
    assert_eq!(scores[1].replay_path, "replay/one.bzr");
}

#[test]
fn replay_slots_for_charts_reports_slot_presence_from_new_table() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };
    db.upsert_replay_slot(&sample_slot(0, 10)).unwrap();
    db.upsert_replay_slot(&sample_slot(2, 30)).unwrap();

    let slots = db.replay_slots_for_charts(&[key([2; 32]), key([1; 32])]).unwrap();

    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].chart_sha256, [1; 32]);
    assert_eq!(slots[0].replay_slots, [true, false, true, false]);
}
