use super::*;

#[test]
fn insert_score_persists_enum_strings_and_empty_values() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut record = record(20, ClearType::Normal);
    record.gauge_type = None;
    record.rule_mode = "Dx".to_string();
    record.arrange = "Random".to_string();
    record.arrange_2p = "Mirror".to_string();
    record.applied_double_option = DoubleOption::Flip;
    record.source_kind = ScoreSourceKind::Beatoraja;
    record.seed_scheme = "beatoraja_24bit_v1".to_string();
    record.device_type = InputDeviceKind::Controller;
    db.insert_score(&record).unwrap();

    let (
            clear_type,
            gauge_type,
            gauge_option,
            rule_mode,
            arrange,
            arrange_2p,
            device_type,
            replay_path,
            source_kind,
            applied_double_option,
        ): (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        ) = db
            .conn()
            .query_row(
                "SELECT clear_type, gauge_type, gauge_option, rule_mode, arrange, arrange_2p, device_type, replay_path, source_kind, applied_double_option FROM score_history",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                    ))
                },
            )
            .unwrap();

    assert_eq!(clear_type, "Normal");
    assert_eq!(gauge_type, "");
    assert_eq!(gauge_option, "");
    assert_eq!(rule_mode, "Dx");
    assert_eq!(arrange, "Random");
    assert_eq!(arrange_2p, "Mirror");
    assert_eq!(device_type, "controller");
    assert_eq!(replay_path, "");
    assert_eq!(source_kind, "Beatoraja");
    assert_eq!(applied_double_option, "Flip");
    let seed_scheme: String =
        db.conn().query_row("SELECT seed_scheme FROM score_history", [], |row| row.get(0)).unwrap();
    assert_eq!(seed_scheme, "beatoraja_24bit_v1");
    assert_eq!(db.recent_history(1, 0).unwrap()[0].source_kind, ScoreSourceKind::Beatoraja);
    assert_eq!(db.recent_history(1, 0).unwrap()[0].applied_double_option, DoubleOption::Flip);
}

#[test]
fn same_score_from_source_ignores_time_but_keeps_score_context_distinct() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut imported = record(20, ClearType::Normal);
    imported.source_kind = ScoreSourceKind::Beatoraja;
    imported.random_seed = Some(1234);
    imported.arrange = "Random".to_string();
    imported.arrange_2p = "Mirror".to_string();
    imported.applied_double_option = DoubleOption::Flip;
    imported.rule_mode = "Beatoraja".to_string();
    db.insert_score(&imported).unwrap();

    let mut same = imported.clone();
    same.played_at += 60;
    assert!(db.has_same_score_from_source(&same).unwrap());

    let mut different_source = same.clone();
    different_source.source_kind = ScoreSourceKind::Lr2Oraja;
    assert!(!db.has_same_score_from_source(&different_source).unwrap());

    let mut different_seed = same.clone();
    different_seed.random_seed = Some(1235);
    assert!(!db.has_same_score_from_source(&different_seed).unwrap());

    let mut different_arrange_2p = same.clone();
    different_arrange_2p.arrange_2p = "Random".to_string();
    assert!(!db.has_same_score_from_source(&different_arrange_2p).unwrap());

    let mut different_applied_double_option = same.clone();
    different_applied_double_option.applied_double_option = DoubleOption::Off;
    assert!(!db.has_same_score_from_source(&different_applied_double_option).unwrap());

    let mut different_judges = same;
    different_judges.score.judges.fast_empty_poor = 1;
    assert!(!db.has_same_score_from_source(&different_judges).unwrap());
}

#[test]
fn imported_score_reconciliation_updates_history_and_its_best_device() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut imported = record(20, ClearType::Normal);
    imported.source_kind = ScoreSourceKind::Beatoraja;
    let history_id = db.insert_score(&imported).unwrap();

    let mut corrected = imported.clone();
    corrected.played_at += 60;
    corrected.device_type = InputDeviceKind::Controller;
    assert_eq!(
        db.reconcile_imported_score(&corrected).unwrap(),
        ImportedScoreReconciliation::Corrected
    );
    assert!(db.has_same_score_from_source(&corrected).unwrap());
    assert_eq!(
        db.reconcile_imported_score(&corrected).unwrap(),
        ImportedScoreReconciliation::Unchanged
    );

    let history_device: String = db
        .conn()
        .query_row(
            "SELECT device_type FROM score_history WHERE id = ?1",
            params![history_id],
            |row| row.get(0),
        )
        .unwrap();
    let (best_history_id, best_device): (i64, String) = db
        .conn()
        .query_row("SELECT best_score_history_id, device_type FROM score_best", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(history_device, "controller");
    assert_eq!(best_history_id, history_id);
    assert_eq!(best_device, "controller");
}

#[test]
fn imported_score_reconciliation_backfills_missing_best_ghost() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut imported = record(20, ClearType::Normal);
    imported.source_kind = ScoreSourceKind::Beatoraja;
    let history_id = db.insert_score(&imported).unwrap();
    db.conn_mut().execute("UPDATE score_best SET ghost = ''", []).unwrap();

    let mut reimported = imported.clone();
    reimported.played_at += 60;
    assert_eq!(
        db.reconcile_imported_score(&reimported).unwrap(),
        ImportedScoreReconciliation::Corrected
    );
    assert_eq!(db.best_ghost(key([7; 32]), 10).unwrap(), Some(vec![0; 10]));
    assert_eq!(
        db.reconcile_imported_score(&reimported).unwrap(),
        ImportedScoreReconciliation::Unchanged
    );

    let history_ids: Vec<i64> = db
        .conn()
        .prepare("SELECT id FROM score_history ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(history_ids, vec![history_id]);
}

#[test]
fn imported_score_reconciliation_prefers_matching_best_history_for_ghost_backfill() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut imported = record(20, ClearType::Normal);
    imported.source_kind = ScoreSourceKind::Beatoraja;
    db.insert_score(&imported).unwrap();
    let current_best_history_id = db.insert_score(&imported).unwrap();
    db.conn_mut()
        .execute(
            "UPDATE score_best SET best_score_history_id = ?1, ghost = ''",
            params![current_best_history_id],
        )
        .unwrap();

    assert_eq!(
        db.reconcile_imported_score(&imported).unwrap(),
        ImportedScoreReconciliation::Corrected
    );
    assert_eq!(db.best_ghost(key([7; 32]), 10).unwrap(), Some(vec![0; 10]));
}

#[test]
fn imported_score_reconciliation_does_not_backfill_another_best_history() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut imported = record(20, ClearType::Normal);
    imported.source_kind = ScoreSourceKind::Beatoraja;
    db.insert_score(&imported).unwrap();

    let local = record(40, ClearType::Normal);
    let local_history_id = db.insert_score(&local).unwrap();
    db.conn_mut().execute("UPDATE score_best SET ghost = ''", []).unwrap();

    assert_eq!(
        db.reconcile_imported_score(&imported).unwrap(),
        ImportedScoreReconciliation::Unchanged
    );
    assert_eq!(db.best_ghost(key([7; 32]), 20).unwrap(), None);
    let best_history_id: i64 = db
        .conn()
        .query_row("SELECT best_score_history_id FROM score_best", [], |row| row.get(0))
        .unwrap();
    assert_eq!(best_history_id, local_history_id);
}

#[test]
fn imported_score_reconciliation_does_not_change_local_best_device() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut imported = record(20, ClearType::Normal);
    imported.source_kind = ScoreSourceKind::Beatoraja;
    db.insert_score(&imported).unwrap();

    let local = record(40, ClearType::Normal);
    let local_history_id = db.insert_score(&local).unwrap();

    let mut corrected = imported.clone();
    corrected.device_type = InputDeviceKind::Controller;
    assert_eq!(
        db.reconcile_imported_score(&corrected).unwrap(),
        ImportedScoreReconciliation::Corrected
    );

    let (best_history_id, best_device): (i64, String) = db
        .conn()
        .query_row("SELECT best_score_history_id, device_type FROM score_best", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(best_history_id, local_history_id);
    assert_eq!(best_device, "keyboard");
}

#[test]
fn legacy_beatoraja_cleanup_removes_matching_local_history_and_rebuilds_aggregates() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut legacy = record(20, ClearType::Normal);
    legacy.playtime_seconds = 10;
    legacy.random_seed = Some(1234);
    let legacy_first_id = db.insert_score(&legacy).unwrap();
    let legacy_second_id = db.insert_score(&legacy).unwrap();

    let mut imported = legacy.clone();
    imported.playtime_seconds = 20;
    imported.arrange = "Random".to_string();
    imported.source_kind = ScoreSourceKind::Beatoraja;
    imported.device_type = InputDeviceKind::Controller;
    let imported_id = db.insert_score(&imported).unwrap();

    let mut ordinary = record(30, ClearType::Hard);
    ordinary.chart_sha256 = [8; 32];
    ordinary.playtime_seconds = 30;
    ordinary.played_at += 1;
    db.insert_score(&ordinary).unwrap();

    let plan = db.legacy_beatoraja_cleanup_plan().unwrap();
    assert_eq!(plan.legacy_history_ids, vec![legacy_first_id, legacy_second_id]);
    assert_eq!(plan.retained_beatoraja_history_ids, vec![imported_id]);

    let report = db.purge_legacy_beatoraja_imports(&plan).unwrap();
    assert_eq!(report.removed_legacy_history, 2);
    assert_eq!(report.retained_beatoraja_history, 1);
    assert!(db.legacy_beatoraja_cleanup_plan().unwrap().legacy_history_ids.is_empty());

    let history_ids: Vec<i64> = db
        .conn()
        .prepare("SELECT id FROM score_history ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(history_ids, vec![imported_id, imported_id + 1]);

    let imported_best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(imported_best.play_count, 1);
    assert_eq!(imported_best.device_type, InputDeviceKind::Controller);
    let best_history_id: i64 = db
        .conn()
        .query_row(
            "SELECT best_score_history_id FROM score_best WHERE chart_sha256 = ?1",
            params![hash_to_hex(&[7; 32])],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(best_history_id, imported_id);

    let stats = db.player_stats().unwrap();
    assert_eq!(stats.play_count, 2);
    assert_eq!(stats.clear_count, 2);
    assert_eq!(stats.playtime_seconds, 70);
}

#[test]
fn same_source_duplicate_history_ids_match_the_cleanup_fingerprint() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut first = record(20, ClearType::Normal);
    first.random_seed = Some(1234);
    let first_id = db.insert_score(&first).unwrap();
    let duplicate_id = db.insert_score(&first).unwrap();

    let mut different = first;
    different.played_at += 1;
    let different_id = db.insert_score(&different).unwrap();

    assert_eq!(db.same_source_duplicate_history_ids(duplicate_id).unwrap(), vec![first_id]);
    assert!(db.same_source_duplicate_history_ids(different_id).unwrap().is_empty());
}

#[test]
fn best_score_keeps_higher_ex_score() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    db.insert_score(&record(20, ClearType::Normal)).unwrap();
    db.insert_score(&record(10, ClearType::Hard)).unwrap();
    db.insert_score(&record(30, ClearType::Easy)).unwrap();

    assert_eq!(db.best_ex_score(key([7; 32])).unwrap(), Some(30));
}

#[test]
fn score_best_updates_clear_lamp_independently_from_ex_score() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    db.insert_score(&record(40, ClearType::Normal)).unwrap();
    let mut hard = record(20, ClearType::Hard);
    hard.gauge_type = Some(GaugeType::Hard);
    hard.gauge_value = Some(12.0);
    db.insert_score(&hard).unwrap();

    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(best.ex_score, 40);
    assert_eq!(best.clear_type, "Hard");
    assert_eq!(best.gauge_type, "Hard");
    assert_eq!(best.gauge_value, Some(12.0));
}

#[test]
fn score_best_does_not_downgrade_clear_lamp_on_higher_ex_score() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut hard = record(20, ClearType::Hard);
    hard.gauge_type = Some(GaugeType::Hard);
    hard.gauge_value = Some(12.0);
    db.insert_score(&hard).unwrap();
    let mut normal = record(40, ClearType::Normal);
    normal.gauge_type = Some(GaugeType::Normal);
    normal.gauge_value = Some(82.0);
    db.insert_score(&normal).unwrap();

    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(best.ex_score, 40);
    assert_eq!(best.clear_type, "Hard");
    assert_eq!(best.gauge_type, "Hard");
    assert_eq!(best.gauge_value, Some(12.0));
}

#[test]
fn score_best_updates_only_max_combo_when_lower_ex_score_improves_combo() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut initial = record(40, ClearType::Normal);
    initial.score.judges.fast_bad = 3;
    initial.score.judges.fast_empty_poor = 2;
    db.insert_score(&initial).unwrap();

    let mut combo = record(20, ClearType::Easy);
    combo.score.max_combo = 50;
    combo.score.judges.fast_bad = 4;
    combo.score.judges.fast_empty_poor = 2;
    db.insert_score(&combo).unwrap();

    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(best.ex_score, 40);
    assert_eq!(best.clear_type, "Normal");
    assert_eq!(best.bp, 5);
    assert_eq!(best.cb, 3);
    assert_eq!(best.max_combo, 50);
    assert_eq!(best.judge_counts.pgreat, 20);
    assert_eq!(best.judge_counts.bad, 3);
    assert_eq!(best.judge_counts.empty_poor, 2);
}

#[test]
fn score_best_updates_only_bp_when_lower_ex_score_improves_bp() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut initial = record(40, ClearType::Normal);
    initial.score.judges.fast_bad = 3;
    initial.score.judges.fast_empty_poor = 3;
    db.insert_score(&initial).unwrap();

    let mut lower_bp = record(20, ClearType::Easy);
    lower_bp.score.judges.fast_bad = 3;
    lower_bp.score.judges.fast_empty_poor = 1;
    db.insert_score(&lower_bp).unwrap();

    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(best.ex_score, 40);
    assert_eq!(best.clear_type, "Normal");
    assert_eq!(best.bp, 4);
    assert_eq!(best.cb, 3);
    assert_eq!(best.max_combo, 20);
    assert_eq!(best.judge_counts.pgreat, 20);
    assert_eq!(best.judge_counts.bad, 3);
    assert_eq!(best.judge_counts.empty_poor, 3);
}

#[test]
fn score_best_updates_only_cb_when_lower_ex_score_improves_cb() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut initial = record(40, ClearType::Normal);
    initial.score.judges.fast_bad = 4;
    initial.score.judges.fast_empty_poor = 1;
    db.insert_score(&initial).unwrap();

    let mut lower_cb = record(20, ClearType::Easy);
    lower_cb.score.judges.fast_bad = 2;
    lower_cb.score.judges.fast_empty_poor = 3;
    db.insert_score(&lower_cb).unwrap();

    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(best.ex_score, 40);
    assert_eq!(best.clear_type, "Normal");
    assert_eq!(best.bp, 5);
    assert_eq!(best.cb, 2);
    assert_eq!(best.max_combo, 20);
    assert_eq!(best.judge_counts.pgreat, 20);
    assert_eq!(best.judge_counts.bad, 4);
    assert_eq!(best.judge_counts.empty_poor, 1);
}

#[test]
fn score_best_is_separate_per_double_option() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    db.insert_score(&record(20, ClearType::Normal)).unwrap();
    let mut battle = record(60, ClearType::Hard);
    battle.double_option = DoubleOptionScoreBucket::Battle;
    db.insert_score(&battle).unwrap();

    let off_key = key([7; 32]);
    let battle_key = ScoreKey::with_double_option(
        [7; 32],
        LnScorePolicy::ForceLn,
        DoubleOptionScoreBucket::Battle,
    );

    assert_eq!(db.best_ex_score(off_key).unwrap(), Some(20));
    assert_eq!(db.best_ex_score(battle_key).unwrap(), Some(60));

    let summaries = db.best_scores_for_charts(&[off_key, battle_key]).unwrap();
    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0].double_option, DoubleOptionScoreBucket::Off);
    assert_eq!(summaries[0].ex_score, 20);
    assert_eq!(summaries[1].double_option, DoubleOptionScoreBucket::Battle);
    assert_eq!(summaries[1].ex_score, 60);
}

#[test]
fn score_best_is_separate_per_rule_mode() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut beatoraja = record(20, ClearType::Normal);
    beatoraja.rule_mode = RuleMode::Beatoraja.as_str().to_string();
    let mut dx = record(80, ClearType::Hard);
    dx.rule_mode = RuleMode::Dx.as_str().to_string();

    db.insert_score(&beatoraja).unwrap();
    db.insert_score(&dx).unwrap();

    let beatoraja_key = key([7; 32]).with_rule_mode(RuleMode::Beatoraja);
    let dx_key = key([7; 32]).with_rule_mode(RuleMode::Dx);

    assert_eq!(db.best_ex_score(beatoraja_key).unwrap(), Some(20));
    assert_eq!(db.best_ex_score(dx_key).unwrap(), Some(80));

    let summaries = db.best_scores_for_charts(&[beatoraja_key, dx_key]).unwrap();
    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0].rule_mode, RuleMode::Beatoraja);
    assert_eq!(summaries[0].ex_score, 20);
    assert_eq!(summaries[1].rule_mode, RuleMode::Dx);
    assert_eq!(summaries[1].ex_score, 80);
}

#[test]
fn best_score_tiebreaks_by_lower_bp_then_lower_cb() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    let mut high_bp = record(20, ClearType::Normal);
    high_bp.score.judges.fast_bad = 3;
    high_bp.score.judges.fast_empty_poor = 2;
    db.insert_score(&high_bp).unwrap();

    let mut lower_bp = record(20, ClearType::Normal);
    lower_bp.score.judges.fast_bad = 2;
    lower_bp.score.judges.fast_empty_poor = 2;
    db.insert_score(&lower_bp).unwrap();

    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(best.bp, 4);
    assert_eq!(best.cb, 2);

    let mut higher_cb = record(20, ClearType::Normal);
    higher_cb.score.judges.fast_bad = 4;
    higher_cb.score.judges.fast_empty_poor = 1;
    db.insert_score(&higher_cb).unwrap();

    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(best.bp, 4);
    assert_eq!(best.cb, 2);

    let mut lower_cb = record(20, ClearType::Normal);
    lower_cb.score.judges.fast_great = 6;
    lower_cb.score.judges.slow_good = 5;
    lower_cb.score.judges.fast_bad = 1;
    lower_cb.score.judges.fast_empty_poor = 3;
    db.insert_score(&lower_cb).unwrap();

    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(best.bp, 4);
    assert_eq!(best.cb, 1);
    assert_eq!(best.judge_counts.pgreat, 10);
    assert_eq!(best.judge_counts.great, 6);
    assert_eq!(best.judge_counts.good, 5);
    assert_eq!(best.judge_counts.bad, 1);
    assert_eq!(best.judge_counts.empty_poor, 3);
    assert_eq!(best.fast_slow_counts.fast_great, 6);
    assert_eq!(best.fast_slow_counts.slow_good, 5);
    assert_eq!(best.fast_slow_counts.fast_empty_poor, 3);
    assert_eq!(best.play_count, 4);
    assert_eq!(best.clear_count, 4);
}

#[test]
fn score_best_counts_every_play_but_only_clear_results() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    let mut db = ScoreDatabase { conn };

    db.insert_score(&record(20, ClearType::Normal)).unwrap();
    let mut failed = record(10, ClearType::Failed);
    failed.played_at = 2;
    db.insert_score(&failed).unwrap();
    let mut clear = record(10, ClearType::Easy);
    clear.played_at = 3;
    db.insert_score(&clear).unwrap();

    let best = db.best_scores_for_charts(&[key([7; 32])]).unwrap().pop().unwrap();
    assert_eq!(best.ex_score, 20);
    assert_eq!(best.clear_type, "Normal");
    assert_eq!(best.play_count, 3);
    assert_eq!(best.clear_count, 2);
}
