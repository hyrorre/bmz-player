use super::*;
use rusqlite::types::Value;

fn database_before_repair() -> ScoreDatabase {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, &SCORE_MIGRATIONS[..32]).unwrap();
    ScoreDatabase { conn }
}

fn play(ex: u32, bp: u32, cb: u32, combo: u32) -> ScoreRecord {
    let mut entry = record(ex, ClearType::Normal);
    entry.rule_mode = RuleMode::Beatoraja.as_str().into();
    entry.score.judges.fast_bad = cb;
    entry.score.judges.fast_empty_poor = bp - cb;
    entry.score.max_combo = combo;
    entry
}

fn numeric_best(db: &ScoreDatabase) -> (u32, u32, u32, u32) {
    db.conn
        .query_row("SELECT ex_score, bp, cb, max_combo FROM score_best", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap()
}

fn ir_source() -> ScoreHistorySourceRecord {
    ScoreHistorySourceRecord {
        key: ScoreHistorySourceKey {
            source: "ir_download".into(),
            provider: "rian_ir".into(),
            account_id: "test-account".into(),
            remote_score_id: "test-score".into(),
        },
        verification: "verified".into(),
        server_received_at: 1_700_000_001,
        imported_at: 1_700_000_002,
    }
}

fn rows(conn: &Connection, sql: &str) -> Vec<Vec<Value>> {
    let mut stmt = conn.prepare(sql).unwrap();
    let count = stmt.column_count();
    stmt.query_map([], |row| (0..count).map(|i| row.get(i)).collect())
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn metadata(conn: &Connection) -> Vec<Vec<Value>> {
    let columns: Vec<String> = conn
        .prepare("PRAGMA table_info(score_best)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
        .into_iter()
        .filter(|name| !["bp", "cb", "max_combo"].contains(&name.as_str()))
        .collect();
    rows(
        conn,
        &format!(
            "SELECT {} FROM score_best ORDER BY chart_sha256, ln_policy, double_option, rule_mode",
            columns.join(",")
        ),
    )
}

#[test]
fn independent_bests_cases_a_through_e_and_combo_only() {
    let cases = [
        ((2000, 5, 3, 900), (2100, 20, 12, 800), (2100, 5, 3, 900), true),
        ((2000, 10, 5, 900), (1900, 7, 6, 800), (2000, 7, 5, 900), false),
        ((2000, 8, 5, 900), (1900, 10, 3, 800), (2000, 8, 3, 900), false),
        ((2000, 8, 3, 900), (2000, 7, 5, 800), (2000, 7, 3, 900), true),
        ((2000, 8, 5, 900), (2000, 8, 3, 800), (2000, 8, 3, 900), true),
        ((2000, 8, 5, 800), (1900, 10, 6, 900), (2000, 8, 5, 900), false),
    ];
    for (before, next, expected, changes_representative) in cases {
        let mut db = database_before_repair();
        let first = play(before.0, before.1, before.2, before.3);
        let first_id = db.insert_score(&first).unwrap();
        assert_eq!(numeric_best(&db), before); // Initial play.
        let mut second = play(next.0, next.1, next.2, next.3);
        second.played_at += 1;
        second.replay_path = "replay/second.toml".into();
        second.device_type = InputDeviceKind::Controller;
        second.score.ghost = vec![1; second.total_notes as usize];
        // Preserve EX while making the judgement breakdown visibly different.
        second.score.judges.slow_pgreat -= 1;
        second.score.judges.fast_great = 2;
        let second_id = db.insert_score(&second).unwrap();
        assert_eq!(numeric_best(&db), expected);
        let source = if changes_representative { &second } else { &first };
        let source_id = if changes_representative { second_id } else { first_id };
        let saved = db.best_scores_for_charts(&[key(first.chart_sha256)]).unwrap().remove(0);
        assert_eq!(saved.judge_counts.bad, source.score.judges.fast_bad);
        assert_eq!(saved.judge_counts.great, source.score.judges.fast_great);
        assert_eq!(saved.played_at, source.played_at);
        assert_eq!(saved.device_type, source.device_type);
        let representative =
            rows(db.conn(), "SELECT best_score_history_id, replay_path, ghost FROM score_best");
        assert_eq!(
            representative[0],
            vec![
                Value::Integer(source_id),
                Value::Text(source.replay_path.clone()),
                Value::Text(encode_beatoraja_ghost(&source.score.ghost).unwrap())
            ]
        );

        // Exact ties retain the representative's judgement/ghost/replay/date.
        let mut equal = second.clone();
        equal.replay_path = "must-not-replace".into();
        equal.played_at += 1;
        equal.score.ghost.fill(2);
        db.insert_score(&equal).unwrap();
        assert_eq!(numeric_best(&db), expected);
        assert_eq!(
            rows(db.conn(), "SELECT best_score_history_id, replay_path, ghost FROM score_best"),
            representative
        );
        assert_eq!(db.best_scores_for_charts(&[key(first.chart_sha256)]).unwrap()[0].play_count, 3);
        let before_rebuild = rows(db.conn(), "SELECT * FROM score_best");
        let redundant = db.insert_score(&equal).unwrap();
        db.purge_score_history_ids_and_rebuild(&[redundant]).unwrap();
        assert_eq!(rows(db.conn(), "SELECT * FROM score_best"), before_rebuild);
    }
}

#[test]
fn representative_ties_use_actual_history_not_independent_extrema() {
    let sequences = [
        [play(2000, 10, 5, 900), play(1900, 1, 1, 950), play(2000, 8, 6, 800)],
        [play(2000, 8, 5, 900), play(1900, 8, 1, 950), play(2000, 8, 3, 800)],
        [play(2000, 8, 5, 800), play(1900, 8, 5, 950), play(2000, 8, 5, 900)],
    ];
    for sequence in sequences {
        let mut db = database_before_repair();
        let mut last = 0;
        for entry in &sequence {
            last = db.insert_score(entry).unwrap();
        }
        assert_eq!(
            rows(db.conn(), "SELECT best_score_history_id FROM score_best"),
            vec![vec![Value::Integer(last)]]
        );
        // Exercise the real cleanup/rebuild path, removing only a redundant play.
        let redundant = db.insert_score(&sequence[1]).unwrap();
        let before = rows(db.conn(), "SELECT * FROM score_best");
        db.purge_score_history_ids_and_rebuild(&[redundant]).unwrap();
        // Re-inserting the redundant play restores the counters for a full comparison.
        db.insert_score(&sequence[1]).unwrap();
        assert_eq!(rows(db.conn(), "SELECT * FROM score_best"), before);
    }
}

#[test]
fn first_numeric_play_after_clear_only_initializes_even_zero_ex() {
    for ex in [0, 2000] {
        let mut db = database_before_repair();
        let assisted = record(2200, ClearType::AssistEasy);
        db.update_score_clear_only(&assisted).unwrap();
        assert_eq!(numeric_best(&db), (0, 0, 0, 0));
        let mut first = play(ex, 20, 12, ex / 2);
        first.clear_type = ClearType::Failed;
        first.count_unprocessed_notes = false;
        let id = db.insert_score(&first).unwrap();
        assert_eq!(numeric_best(&db), (ex, 20, 12, ex / 2));
        let saved = db.best_scores_for_charts(&[key(first.chart_sha256)]).unwrap().remove(0);
        assert_eq!(saved.clear_type, "AssistEasy");
        assert_eq!((saved.play_count, saved.clear_count), (2, 1));
        assert_eq!(
            rows(db.conn(), "SELECT best_score_history_id FROM score_best"),
            vec![vec![Value::Integer(id)]]
        );
    }
}

#[test]
fn source_download_and_reimport_keep_independent_bests_and_representative() {
    let mut db = database_before_repair();
    db.insert_score(&play(2000, 5, 3, 900)).unwrap();
    let mut imported = play(2100, 20, 12, 800);
    imported.source_kind = ScoreSourceKind::Beatoraja;
    let source = ir_source();
    let outcome = db.insert_score_with_source(&imported, &source).unwrap();
    assert!(matches!(outcome, ScoreSourceInsertOutcome::Inserted { .. }));
    assert_eq!(numeric_best(&db), (2100, 5, 3, 900));
    let before = rows(db.conn(), "SELECT * FROM score_best");
    assert!(matches!(
        db.insert_score_with_source(&imported, &source).unwrap(),
        ScoreSourceInsertOutcome::Duplicate { .. }
    ));
    assert_eq!(rows(db.conn(), "SELECT * FROM score_best"), before);
    imported.device_type = InputDeviceKind::Controller;
    assert_eq!(
        db.reconcile_imported_score(&imported).unwrap(),
        ImportedScoreReconciliation::Corrected
    );
    assert_eq!(numeric_best(&db), (2100, 5, 3, 900));
    let representative_sql = "SELECT best_score_history_id, ghost, device_type, replay_path,
        played_at, fast_bad FROM score_best";
    let metadata_before = rows(db.conn(), representative_sql);
    let mut lower = play(1900, 1, 1, 950);
    lower.source_kind = ScoreSourceKind::Lr2Oraja;
    db.insert_score(&lower).unwrap();
    lower.score.ghost = vec![2; lower.total_notes as usize];
    lower.device_type = InputDeviceKind::Controller;
    assert_eq!(
        db.reconcile_imported_score(&lower).unwrap(),
        ImportedScoreReconciliation::Corrected
    );
    assert_eq!(numeric_best(&db), (2100, 1, 1, 950));
    // The lower play increments counters, but cannot supply the representative ghost.
    assert_eq!(rows(db.conn(), representative_sql), metadata_before);
}

#[test]
fn missing_legacy_representative_preserves_metadata_on_ties_and_real_extrema() {
    for missing in ["NULL", "999999"] {
        let mut db = database_before_repair();
        db.insert_score(&play(2000, 8, 5, 900)).unwrap();
        db.conn.execute_batch(&format!("UPDATE score_best SET best_score_history_id = {missing}; DELETE FROM score_history;")).unwrap();
        let before = rows(
            db.conn(),
            "SELECT played_at, replay_path, ghost, best_score_history_id, fast_bad FROM score_best",
        );
        db.insert_score(&play(2000, 7, 3, 800)).unwrap();
        assert_eq!(numeric_best(&db), (2000, 7, 3, 900));
        assert_eq!(
            rows(
                db.conn(),
                "SELECT played_at, replay_path, ghost, best_score_history_id, fast_bad FROM score_best"
            ),
            before
        );
        db.insert_score(&play(2100, 20, 12, 800)).unwrap();
        assert_eq!(numeric_best(&db), (2100, 7, 3, 900));
    }
}

#[test]
fn repair_extrema_preserves_every_other_field_and_history() {
    let mut db = database_before_repair();
    db.insert_score(&play(2000, 5, 3, 900)).unwrap();
    let mut second = play(2100, 20, 12, 800);
    second.clear_type = ClearType::Hard;
    second.played_at += 100;
    second.device_type = InputDeviceKind::Controller;
    second.replay_path = "replay/representative.toml".into();
    second.score.ghost = vec![1; 1050];
    let representative_id = db.insert_score(&second).unwrap();
    db.attach_score_history_source(representative_id, &ir_source()).unwrap();
    db.upsert_replay_slot(&sample_slot(0, 2100)).unwrap();
    db.conn.execute("UPDATE score_best SET bp = 20, cb = 12, max_combo = 800", []).unwrap();
    let before = metadata(db.conn());
    let tables = [
        "score_history",
        "score_history_ir_identity",
        "score_history_sources",
        "replay_slots",
        "player_stats",
        "score_unrecorded_plays",
    ];
    let snapshots: Vec<_> = tables
        .iter()
        .map(|table| rows(db.conn(), &format!("SELECT * FROM {table} ORDER BY rowid")))
        .collect();
    run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
    assert_eq!(numeric_best(&db), (2100, 5, 3, 900));
    assert_eq!(metadata(db.conn()), before);
    assert_eq!(
        rows(db.conn(), "SELECT best_score_history_id FROM score_best"),
        vec![vec![Value::Integer(representative_id)]]
    );
    for (table, snapshot) in tables.iter().zip(snapshots) {
        assert_eq!(rows(db.conn(), &format!("SELECT * FROM {table} ORDER BY rowid")), snapshot);
    }
    let repaired = rows(db.conn(), "SELECT * FROM score_best");
    // The SQL is idempotent too, independently of user_version skipping it.
    let tx = db.conn_mut().transaction().unwrap();
    for sql in SCORE_MIGRATIONS.last().unwrap().statements {
        tx.execute_batch(sql).unwrap();
    }
    tx.commit().unwrap();
    assert_eq!(rows(db.conn(), "SELECT * FROM score_best"), repaired);
    db.conn.execute("UPDATE score_best SET bp = 44", []).unwrap();
    run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
    assert_eq!(numeric_best(&db).1, 44); // Subsequent startups do not aggregate again.
}

#[test]
fn repair_extrema_separates_full_score_keys_and_includes_imports() {
    let mut db = database_before_repair();
    let mut expected = Vec::new();
    for sha in [[7; 32], [8; 32]] {
        for ln in [LnScorePolicy::ForceLn, LnScorePolicy::ForceCn] {
            for double in [
                DoubleOptionScoreBucket::Off,
                DoubleOptionScoreBucket::Battle,
                DoubleOptionScoreBucket::BattleAutoScratch,
            ] {
                for rule in [RuleMode::Beatoraja, RuleMode::Lr2Oraja, RuleMode::Dx] {
                    let index = expected.len() as u32;
                    let mut first = play(2000, 5 + index, 3 + index, 900 + index);
                    first.chart_sha256 = sha;
                    first.ln_policy = ln;
                    first.double_option = double;
                    // Empty legacy rule strings share the Beatoraja bucket.
                    first.rule_mode = if rule == RuleMode::Beatoraja {
                        String::new()
                    } else {
                        rule.as_str().into()
                    };
                    first.source_kind = match rule {
                        RuleMode::Beatoraja => ScoreSourceKind::Beatoraja,
                        RuleMode::Lr2Oraja => ScoreSourceKind::Lr2Oraja,
                        _ => ScoreSourceKind::Local,
                    };
                    db.insert_score(&first).unwrap();
                    let mut second = first.clone();
                    second.score = play(2100, 20 + index, 12 + index, 800 + index).score;
                    second.total_notes = 1050;
                    second.rule_mode = rule.as_str().into();
                    db.insert_score(&second).unwrap();
                    expected.push((
                        ScoreKey::with_options(sha, ln, double, rule),
                        5 + index,
                        3 + index,
                        900 + index,
                    ));
                }
            }
        }
    }
    for &(key, bp, cb, combo) in &expected {
        let saved = db.best_scores_for_charts(&[key]).unwrap().remove(0);
        assert_eq!((saved.ex_score, saved.bp, saved.cb, saved.max_combo), (2100, bp, cb, combo));
    }
    db.conn
        .execute("UPDATE score_best SET bp = bp + 15, cb = cb + 9, max_combo = max_combo - 100", [])
        .unwrap();
    let before = metadata(db.conn());
    run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
    assert_eq!(metadata(db.conn()), before);
    for (key, bp, cb, combo) in expected {
        let saved = db.best_scores_for_charts(&[key]).unwrap().remove(0);
        assert_eq!((saved.ex_score, saved.bp, saved.cb, saved.max_combo), (2100, bp, cb, combo));
    }
}

#[test]
fn repair_extrema_keeps_historyless_and_clear_only_keys_and_valid_zeroes() {
    let mut db = database_before_repair();
    db.update_score_clear_only(&record(2000, ClearType::AssistEasy)).unwrap();
    let mut legacy = play(2000, 7, 3, 900);
    legacy.chart_sha256 = [8; 32];
    db.insert_score(&legacy).unwrap();
    db.conn.execute("DELETE FROM score_history", []).unwrap();
    let before = rows(db.conn(), "SELECT * FROM score_best ORDER BY chart_sha256");
    run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
    assert_eq!(rows(db.conn(), "SELECT * FROM score_best ORDER BY chart_sha256"), before);

    let mut zero = play(0, 0, 0, 0);
    zero.chart_sha256 = [9; 32];
    zero.gauge_value = None; // Only gauge_value, not numeric metrics, is nullable.
    zero.source_kind = ScoreSourceKind::Beatoraja;
    db.insert_score(&zero).unwrap();
    db.conn
        .execute(
            "UPDATE score_best SET bp = 999999, cb = 999999 WHERE chart_sha256 = ?1",
            [hash_to_hex(&zero.chart_sha256)],
        )
        .unwrap();
    db.conn.pragma_update(None, "user_version", 32).unwrap();
    run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
    let saved = db.best_scores_for_charts(&[key(zero.chart_sha256)]).unwrap().remove(0);
    assert_eq!((saved.bp, saved.cb, saved.max_combo, saved.gauge_value), (0, 0, 0, None));
}

#[test]
fn repair_extrema_rolls_back_on_failure_including_version() {
    let mut db = database_before_repair();
    db.insert_score(&play(2000, 5, 3, 900)).unwrap();
    db.conn
        .execute_batch(
            "UPDATE score_best SET bp = 20;
        CREATE TRIGGER reject_repair AFTER UPDATE OF bp ON score_best
        BEGIN SELECT RAISE(ABORT, 'test repair failure'); END;",
        )
        .unwrap();
    let before = rows(db.conn(), "SELECT * FROM score_best");
    assert!(run_migrations(db.conn_mut(), SCORE_MIGRATIONS).is_err());
    assert_eq!(rows(db.conn(), "SELECT * FROM score_best"), before);
    assert_eq!(
        db.conn.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0)).unwrap(),
        32
    );
    db.conn.execute_batch("DROP TRIGGER reject_repair;").unwrap();
    run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
    assert_eq!(numeric_best(&db), (2000, 5, 3, 900));
}
