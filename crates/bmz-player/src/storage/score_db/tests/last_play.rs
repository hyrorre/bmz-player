use super::*;

fn database() -> ScoreDatabase {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, SCORE_MIGRATIONS).unwrap();
    ScoreDatabase { conn }
}

#[test]
fn last_play_tracks_failed_and_clear_only_attempts_without_changing_best_date() {
    let mut db = database();
    let key = key([7; 32]);
    assert!(db.last_played_times_for_charts(&[key]).unwrap().is_empty());
    let mut best = record(20, ClearType::Normal);
    best.played_at = 100;
    db.insert_score(&best).unwrap();
    let mut failed = record(2, ClearType::Failed);
    failed.played_at = 200;
    let failed_id = db.insert_score(&failed).unwrap();
    assert_eq!(db.last_played_times_for_charts(&[key]).unwrap()[&key], 200);
    assert_eq!(db.best_scores_for_charts(&[key]).unwrap()[0].played_at, 100);
    let mut assist = record(40, ClearType::AssistEasy);
    assist.played_at = 300;
    db.update_score_clear_only(&assist).unwrap();
    assert_eq!(db.last_played_times_for_charts(&[key]).unwrap()[&key], 300);
    assert_eq!(db.best_scores_for_charts(&[key]).unwrap()[0].played_at, 100);

    // History correction/removal cannot erase an unrecorded assisted play or
    // leave the timestamp of a removed imported result behind.
    db.conn.execute("UPDATE score_history SET played_at = 400 WHERE id = ?1", [failed_id]).unwrap();
    assert_eq!(db.last_played_times_for_charts(&[key]).unwrap()[&key], 400);
    db.purge_score_history_ids_and_rebuild(&[failed_id]).unwrap();
    assert_eq!(db.last_played_times_for_charts(&[key]).unwrap()[&key], 300);
    db.conn.execute("DELETE FROM score_history", []).unwrap();
    db.conn.execute("DELETE FROM score_best", []).unwrap();
    assert_eq!(db.last_played_times_for_charts(&[key]).unwrap()[&key], 300);
}

#[test]
fn last_play_cleanup_reverts_to_remaining_history_or_missing() {
    let mut db = database();
    let mut saved = record(20, ClearType::Normal);
    saved.played_at = 100;
    let first = db.insert_score(&saved).unwrap();
    saved.played_at = 200;
    let last = db.insert_score(&saved).unwrap();
    let score_key = key(saved.chart_sha256);
    db.purge_score_history_ids_and_rebuild(&[last]).unwrap();
    assert_eq!(db.last_played_times_for_charts(&[score_key]).unwrap()[&score_key], 100);
    db.purge_score_history_ids_and_rebuild(&[first]).unwrap();
    assert!(db.last_played_times_for_charts(&[score_key]).unwrap().is_empty());
}

#[test]
fn last_play_batches_score_keys_and_ignores_autoplay_and_invalid_dates() {
    let mut db = database();
    let mut keys = Vec::new();
    for index in 0_u32..205 {
        let mut record = record(20, ClearType::Normal);
        record.chart_sha256[..4].copy_from_slice(&index.to_le_bytes());
        record.played_at = 100 + i64::from(index);
        db.insert_score(&record).unwrap();
        keys.push(key(record.chart_sha256));
    }
    assert_eq!(db.last_played_times_for_charts(&keys).unwrap().len(), 205);
    let mut candidate = record(2, ClearType::Failed);
    candidate.chart_sha256 = keys[0].chart_sha256;
    candidate.played_at = 900;
    candidate.autoplay = true;
    db.insert_score(&candidate).unwrap();
    candidate.autoplay = false;
    candidate.played_at = 0;
    db.insert_score(&candidate).unwrap();
    assert_eq!(db.last_played_times_for_charts(&[keys[0]]).unwrap()[&keys[0]], 100);

    candidate.played_at = 1000;
    candidate.ln_policy = LnScorePolicy::ForceCn;
    db.insert_score(&candidate).unwrap();
    let cn = ScoreKey::new(candidate.chart_sha256, candidate.ln_policy);
    candidate.double_option = DoubleOptionScoreBucket::Battle;
    db.update_score_clear_only(&candidate).unwrap();
    let battle = ScoreKey::with_double_option(
        candidate.chart_sha256,
        candidate.ln_policy,
        candidate.double_option,
    );
    candidate.rule_mode = "Dx".into();
    candidate.played_at = 1100;
    db.update_score_clear_only(&candidate).unwrap();
    let dx = battle.with_rule_mode(RuleMode::Dx);
    let times = db.last_played_times_for_charts(&[keys[0], cn, battle, dx]).unwrap();
    assert_eq!(times[&keys[0]], 100);
    assert_eq!(times[&cn], 1000);
    assert_eq!(times[&battle], 1000);
    assert_eq!(times[&dx], 1100);
}

#[test]
fn last_play_migration_recovers_history_and_clear_only_first_date() {
    let mut conn = Connection::open_in_memory().unwrap();
    run_migrations(&mut conn, &SCORE_MIGRATIONS[..30]).unwrap();
    let mut db = ScoreDatabase { conn };
    let mut regular = record(20, ClearType::Normal);
    regular.played_at = 100;
    db.insert_score(&regular).unwrap();
    regular.played_at = 200;
    regular.score = score_with_ex_score(2);
    db.insert_score(&regular).unwrap();
    let mut assist = record(0, ClearType::AssistEasy);
    assist.chart_sha256 = [8; 32];
    assist.played_at = 150;
    // Invoke the old write boundary; the new table does not exist yet.
    upsert_score_best_clear_only(db.conn(), &assist).unwrap();
    run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
    let times = db.last_played_times_for_charts(&[key([7; 32]), key([8; 32])]).unwrap();
    assert_eq!(times[&key([7; 32])], 200);
    assert_eq!(times[&key([8; 32])], 150);
    assert_eq!(db.best_scores_for_charts(&[key([7; 32])]).unwrap()[0].played_at, 100);
}
