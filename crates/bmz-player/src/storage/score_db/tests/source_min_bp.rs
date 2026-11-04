use super::*;
use rusqlite::types::Value;

fn database(version: usize) -> ScoreDatabase {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, &SCORE_MIGRATIONS[..version]).unwrap();
    ScoreDatabase { conn }
}

fn play(ex: u32, bp: u32) -> ScoreRecord {
    let mut entry = record(ex, ClearType::Normal);
    entry.rule_mode = RuleMode::Beatoraja.as_str().into();
    entry.score.judges.fast_bad = bp / 2;
    entry.score.judges.fast_empty_poor = bp - bp / 2;
    entry.score.max_combo = 900;
    entry
}

fn imported(ex: u32, bp: u32, source_min_bp: Option<u32>) -> ScoreRecord {
    let mut entry = play(ex, bp);
    entry.source_kind = ScoreSourceKind::Beatoraja;
    entry.source_min_bp = source_min_bp;
    entry
}

fn best(db: &ScoreDatabase, entry: &ScoreRecord) -> BestScoreSummary {
    db.best_scores_for_charts(&[ScoreKey::with_options(
        entry.chart_sha256,
        entry.ln_policy,
        entry.double_option,
        record_rule_mode(entry),
    )])
    .unwrap()
    .remove(0)
}

fn rows(conn: &Connection, sql: &str) -> Vec<Vec<Value>> {
    let mut statement = conn.prepare(sql).unwrap();
    let count = statement.column_count();
    statement
        .query_map([], |row| (0..count).map(|index| row.get(index)).collect())
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn snapshot(db: &ScoreDatabase) -> Vec<Vec<Vec<Value>>> {
    [
        "score_best",
        "score_history",
        "player_stats",
        "score_history_ir_identity",
        "score_history_sources",
        "replay_slots",
    ]
    .map(|table| rows(db.conn(), &format!("SELECT * FROM {table} ORDER BY rowid")))
    .into()
}

#[test]
fn source_min_bp_initializes_after_clear_only_but_never_changes_local_history() {
    for source_bp in [None, Some(0), Some(5), Some(30)] {
        let mut db = database(SCORE_MIGRATIONS.len());
        let mut entry = imported(2100, 20, source_bp);
        let mut assist = entry.clone();
        assist.clear_type = ClearType::AssistEasy;
        db.update_score_clear_only(&assist).unwrap();
        let id = db.insert_score(&entry).unwrap();
        let saved = best(&db, &entry);
        assert_eq!(
            (saved.ex_score, saved.bp, saved.cb, saved.max_combo),
            (2100, source_bp.unwrap_or(20).min(20), 10, 900)
        );
        assert_eq!((saved.play_count, saved.clear_count), (2, 2));
        assert_eq!(
            rows(db.conn(), "SELECT bp, source_min_bp FROM score_history"),
            vec![vec![
                Value::Integer(20),
                source_bp.map_or(Value::Null, |v| Value::Integer(v.into()))
            ]]
        );
        // Local records cannot introduce source evidence, even if a caller sets it.
        entry.source_kind = ScoreSourceKind::Local;
        entry.source_min_bp = Some(0);
        let local_id = db.insert_score(&entry).unwrap();
        let source: Option<u32> = db
            .conn
            .query_row("SELECT source_min_bp FROM score_history WHERE id = ?1", [local_id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(source, None);
        db.purge_score_history_ids_and_rebuild(&[id]).unwrap();
        assert_eq!(best(&db, &entry).bp, 20);
    }
}

#[test]
fn source_min_bp_never_supplies_representative_tiebreaks() {
    let mut db = database(SCORE_MIGRATIONS.len());
    let initial = play(2100, 8);
    let first_id = db.insert_score(&initial).unwrap();
    let mut source = imported(2100, 20, Some(0));
    source.score.max_combo = 950;
    source.score.ghost = vec![2; source.total_notes as usize];
    db.insert_score(&source).unwrap();
    assert_eq!(
        rows(db.conn(), "SELECT best_score_history_id FROM score_best"),
        vec![vec![Value::Integer(first_id)]]
    );
    let mut next = play(2100, 7);
    next.score.max_combo = 800;
    next.replay_path = "replay/actual-bp-wins.toml".into();
    next.score.ghost = vec![1; next.total_notes as usize];
    let next_id = db.insert_score(&next).unwrap();
    let saved = best(&db, &next);
    assert_eq!((saved.bp, saved.cb, saved.max_combo), (0, 3, 950));
    assert_eq!(saved.replay_path, next.replay_path);
    assert_eq!(saved.judge_counts.bad, 3);
    assert_eq!(
        rows(db.conn(), "SELECT best_score_history_id FROM score_best"),
        vec![vec![Value::Integer(next_id)]]
    );
    let duplicate_id = db.insert_score(&source).unwrap();
    let before = rows(db.conn(), "SELECT * FROM score_best");
    db.purge_score_history_ids_and_rebuild(&[duplicate_id]).unwrap();
    db.insert_score(&source).unwrap();
    assert_eq!(rows(db.conn(), "SELECT * FROM score_best"), before);
}

#[test]
fn cleanup_retains_surviving_source_evidence_and_removes_deleted_contributions() {
    let mut db = database(SCORE_MIGRATIONS.len());
    let mut representative = imported(2100, 20, Some(5));
    representative.replay_path = "replay/import.toml".into();
    representative.score.ghost = vec![1; representative.total_notes as usize];
    let imported_id = db.insert_score(&representative).unwrap();
    let local = play(2000, 3);
    let local_id = db.insert_score(&local).unwrap();
    assert_eq!(best(&db, &local).bp, 3);
    let identity =
        rows(db.conn(), "SELECT best_score_history_id, ghost, replay_path FROM score_best");
    db.purge_score_history_ids_and_rebuild(&[local_id]).unwrap();
    assert_eq!(best(&db, &local).bp, 5);
    assert_eq!(
        rows(db.conn(), "SELECT best_score_history_id, ghost, replay_path FROM score_best"),
        identity
    );
    let local_id = db.insert_score(&play(2200, 10)).unwrap();
    db.purge_score_history_ids_and_rebuild(&[imported_id]).unwrap();
    assert_eq!(best(&db, &local).bp, 10);
    assert_eq!(
        rows(db.conn(), "SELECT best_score_history_id FROM score_best"),
        vec![vec![Value::Integer(local_id)]]
    );
}

#[test]
fn reimport_source_bp_only_improves_and_does_not_change_other_fields_or_counts() {
    let mut db = database(SCORE_MIGRATIONS.len());
    let mut source = imported(2100, 20, None);
    db.insert_score(&source).unwrap();
    let best_before = best(&db, &source);
    let identities_before = rows(db.conn(), "SELECT * FROM score_history_ir_identity");
    for value in [10, 5, 0] {
        source.source_min_bp = Some(value);
        assert_eq!(
            db.reconcile_imported_score(&source).unwrap(),
            ImportedScoreReconciliation::Corrected
        );
        let mut after = best(&db, &source);
        assert_eq!(after.bp, value);
        after.bp = best_before.bp;
        assert_eq!(after, best_before);
        let before = snapshot(&db);
        assert_eq!(
            db.reconcile_imported_score(&source).unwrap(),
            ImportedScoreReconciliation::Unchanged
        );
        assert_eq!(snapshot(&db), before);
    }
    for worse in [Some(15), None] {
        source.source_min_bp = worse;
        let before = snapshot(&db);
        assert_eq!(
            db.reconcile_imported_score(&source).unwrap(),
            ImportedScoreReconciliation::Unchanged
        );
        assert_eq!(snapshot(&db), before);
    }
    assert_eq!(rows(db.conn(), "SELECT * FROM score_history_ir_identity"), identities_before);
    let redundant = db.insert_score(&play(1900, 30)).unwrap();
    db.purge_score_history_ids_and_rebuild(&[redundant]).unwrap();
    assert_eq!(best(&db, &source).bp, 0);
}

#[test]
fn source_bp_and_device_corrections_are_one_transaction() {
    let mut db = database(SCORE_MIGRATIONS.len());
    let mut source = imported(2100, 20, None);
    db.insert_score(&source).unwrap();
    db.conn
        .execute_batch(
            "CREATE TRIGGER fail_device BEFORE UPDATE OF device_type ON score_history
        BEGIN SELECT RAISE(ABORT, 'test correction failure'); END;",
        )
        .unwrap();
    source.source_min_bp = Some(5);
    source.device_type = InputDeviceKind::Controller;
    let before = snapshot(&db);
    assert!(db.reconcile_imported_score(&source).is_err());
    assert_eq!(snapshot(&db), before);
    db.conn.execute_batch("DROP TRIGGER fail_device;").unwrap();
    assert_eq!(
        db.reconcile_imported_score(&source).unwrap(),
        ImportedScoreReconciliation::Corrected
    );
    let saved = best(&db, &source);
    assert_eq!(
        (saved.bp, saved.device_type, saved.play_count),
        (5, InputDeviceKind::Controller, 1)
    );
    assert_eq!(
        rows(db.conn(), "SELECT bp, source_min_bp FROM score_history"),
        vec![vec![Value::Integer(20), Value::Integer(5)]]
    );
}

#[test]
fn source_min_bp_remains_separate_in_every_score_key_dimension() {
    let mut db = database(SCORE_MIGRATIONS.len());
    let mut expected = Vec::new();
    let mut redundant = Vec::new();
    for sha in [[7; 32], [8; 32]] {
        for ln in [LnScorePolicy::ForceLn, LnScorePolicy::ForceCn, LnScorePolicy::ForceHcn] {
            for double in [
                DoubleOptionScoreBucket::Off,
                DoubleOptionScoreBucket::Battle,
                DoubleOptionScoreBucket::BattleAutoScratch,
            ] {
                for rule in [RuleMode::Beatoraja, RuleMode::Lr2Oraja, RuleMode::Dx] {
                    let bp = 5 + expected.len() as u32;
                    let mut entry = imported(2100, bp + 20, Some(bp));
                    entry.chart_sha256 = sha;
                    entry.ln_policy = ln;
                    entry.double_option = double;
                    entry.rule_mode = rule.as_str().into();
                    db.insert_score(&entry).unwrap();
                    redundant.push(db.insert_score(&entry).unwrap());
                    expected.push((entry, bp));
                }
            }
        }
    }
    db.purge_score_history_ids_and_rebuild(&redundant).unwrap();
    for (entry, bp) in expected {
        assert_eq!(best(&db, &entry).bp, bp);
    }
}

#[test]
fn migration_v34_preserves_existing_records_and_reimport_backfills_without_guessing() {
    let mut db = database(33);
    let mut source = imported(2100, 20, None);
    source.score.ghost = vec![2; source.total_notes as usize];
    source.replay_path = "replay/legacy-import.toml".into();
    db.insert_score(&source).unwrap();
    let before = snapshot(&db);
    run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
    let mut after = snapshot(&db);
    for row in &mut after[1] {
        assert_eq!(row.pop(), Some(Value::Null));
    }
    assert_eq!(after, before);
    let migrated = snapshot(&db);
    run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
    assert_eq!(snapshot(&db), migrated);
    source.source_min_bp = Some(5);
    assert_eq!(
        db.reconcile_imported_score(&source).unwrap(),
        ImportedScoreReconciliation::Corrected
    );
    let mut after = snapshot(&db);
    for row in &mut after[1] {
        assert_eq!(row.pop(), Some(Value::Integer(5)));
    }
    // Only best.bp differs; every old history column, IR identity, ghost and count survives.
    let bp_index = db.conn.prepare("SELECT * FROM score_best").unwrap().column_index("bp").unwrap();
    after[0][0][bp_index] = before[0][0][bp_index].clone();
    assert_eq!(after, before);
}

#[test]
fn migration_v34_failure_rolls_back_schema_data_and_version() {
    let mut db = database(33);
    db.insert_score(&play(2100, 20)).unwrap();
    let before = snapshot(&db);
    let migration = crate::storage::migration::Migration {
        version: 34,
        statements: &[
            "ALTER TABLE score_history ADD COLUMN source_min_bp INTEGER
                CHECK (source_min_bp IS NULL OR source_min_bp >= 0);",
            "INSERT INTO nonexistent_table VALUES (1);",
        ],
    };
    // Exercise the real transaction runner with a failure after the v34 ALTER.
    assert!(run_migrations(db.conn_mut(), &[migration]).is_err());
    assert_eq!(snapshot(&db), before);
    assert_eq!(
        db.conn.pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0)).unwrap(),
        33
    );
    run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
    assert_eq!(best(&db, &play(2100, 20)).bp, 20);
}
