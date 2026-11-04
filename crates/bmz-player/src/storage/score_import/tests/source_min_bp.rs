use super::*;
use rusqlite::types::Value;

fn databases() -> (LibraryDatabase, ScoreDatabase, [u8; 32], [u8; 16]) {
    let mut target = chart();
    target.total_notes = 1100;
    open_test_databases_with_chart(target)
}

fn source(kind: ScoreImportKind, sha: [u8; 32], md5: [u8; 16]) -> Connection {
    let source = Connection::open_in_memory().unwrap();
    if kind.uses_lr2_schema() {
        create_lr2_source_with_score(
            &source,
            &hash_to_hex(&md5),
            Lr2ScoreFixture {
                total_notes: 1100,
                max_combo: 900,
                perfect: 1050,
                great: 0,
                good: 30,
                bad: 12,
                poor: 8,
            },
        );
    } else {
        create_beatoraja_source_with_score(
            &source,
            &hash_to_hex(&sha),
            BeatorajaScoreFixture {
                date: 1_700_000_000,
                mode: 0,
                clear: 7,
                total_notes: 1100,
                judged: 1100,
                max_combo: 900,
            },
        );
        source
            .execute_batch(
                "UPDATE score SET epg = 1050, lpg = 0, egr = 0, lgr = 0,
            egd = 30, lgd = 0, ebd = 12, lbd = 0, epr = 0, lpr = 0, ems = 8, lms = 0;",
            )
            .unwrap();
    }
    source.execute("UPDATE score SET minbp = 5", []).unwrap();
    source
}

fn import(
    source: &Connection,
    kind: ScoreImportKind,
    library: &mut LibraryDatabase,
    db: &mut ScoreDatabase,
    device: InputDeviceKind,
) -> ScoreImportReport {
    if kind.uses_lr2_schema() {
        import_lr2_scores_with_device_type(source, kind, library, db, 1_700_000_100, device)
            .unwrap()
    } else {
        import_beatoraja_scores_with_device_type(source, kind, library, db, 1_700_000_100, device)
            .unwrap()
    }
}

fn best(
    db: &ScoreDatabase,
    sha: [u8; 32],
    kind: ScoreImportKind,
) -> crate::storage::score_db::BestScoreSummary {
    db.best_scores_for_charts(&[
        ScoreKey::new(sha, LnScorePolicy::ForceLn).with_rule_mode(kind.rule_mode_enum())
    ])
    .unwrap()
    .remove(0)
}

fn all_rows(db: &ScoreDatabase, table: &str, omit: &[&str]) -> Vec<Vec<Value>> {
    let columns = db
        .conn()
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
        .into_iter()
        .filter(|name| !omit.contains(&name.as_str()))
        .collect::<Vec<_>>();
    let mut statement = db
        .conn()
        .prepare(&format!("SELECT {} FROM {table} ORDER BY rowid", columns.join(",")))
        .unwrap();
    let count = statement.column_count();
    statement
        .query_map([], |row| (0..count).map(|i| row.get(i)).collect())
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn metadata(db: &ScoreDatabase) -> Vec<Vec<Vec<Value>>> {
    vec![
        all_rows(db, "score_history", &["source_min_bp"]),
        all_rows(db, "score_best", &["bp"]),
        all_rows(db, "player_stats", &[]),
        all_rows(db, "score_history_ir_identity", &[]),
    ]
}

const KINDS: [ScoreImportKind; 4] = [
    ScoreImportKind::Beatoraja,
    ScoreImportKind::Lr2,
    ScoreImportKind::Lr2Oraja,
    ScoreImportKind::Lr2OrajaDx,
];

#[test]
fn imports_source_min_bp_for_every_format_without_rewriting_judges() {
    for kind in KINDS {
        let (mut library, mut db, sha, md5) = databases();
        let source = source(kind, sha, md5);
        let report = import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
        assert_eq!((report.imported, report.corrected, report.failed), (1, 0, 0));
        let saved = best(&db, sha, kind);
        assert_eq!((saved.ex_score, saved.bp, saved.max_combo), (2100, 5, 900));
        // LR2 has no distinct empty-Poor count; preserve its existing conversion.
        let cb = if kind.uses_lr2_schema() { 20 } else { 12 };
        assert_eq!(saved.cb, cb);
        assert_eq!(all_rows(&db, "score_history", &[]).len(), 1);
        let metrics: (u32, Option<u32>, u32, u32, u32) = db
            .conn()
            .query_row(
                "SELECT bp, source_min_bp, cb, fast_pgreat, fast_bad FROM score_history",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(metrics, (20, Some(5), cb, 1050, 12));
    }
}

#[test]
fn source_min_bp_merges_with_better_local_bp_independently_of_ex_and_combo() {
    for kind in KINDS {
        let (mut library, mut db, sha, md5) = databases();
        let source = source(kind, sha, md5);
        let local = imported_score_record(
            sha,
            1_700_000_000,
            ClearType::Normal,
            1100,
            ScoreState {
                judges: JudgeCounts { fast_pgreat: 1000, fast_bad: 3, ..Default::default() },
                max_combo: 800,
                past_notes: 1100,
                ..Default::default()
            },
            None,
            kind.rule_mode(),
            LnScorePolicy::ForceLn,
        );
        db.insert_score(&local).unwrap();
        assert_eq!(
            import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard).imported,
            1
        );
        let saved = best(&db, sha, kind);
        assert_eq!((saved.ex_score, saved.bp, saved.cb, saved.max_combo), (2100, 3, 3, 900));
        assert_eq!(saved.judge_counts.pgreat, 1050);
    }
}

#[test]
fn reimport_improves_bp_without_duplicate_history_or_metadata_changes() {
    for kind in KINDS {
        let (mut library, mut db, sha, md5) = databases();
        let source = source(kind, sha, md5);
        source.execute("UPDATE score SET minbp = 10", []).unwrap();
        import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
        let before = metadata(&db);
        source.execute("UPDATE score SET minbp = 5", []).unwrap();
        let report = import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
        assert_eq!((report.imported, report.corrected, report.skipped), (0, 1, 0));
        assert_eq!(best(&db, sha, kind).bp, 5);
        assert_eq!(metadata(&db), before);
        for value in [Some(5), Some(12), None] {
            source.execute("UPDATE score SET minbp = ?1", [value]).unwrap();
            let report = import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
            assert_eq!((report.imported, report.corrected, report.skipped), (0, 0, 1));
            assert_eq!(best(&db, sha, kind).bp, 5);
            assert_eq!(metadata(&db), before);
        }
        source.execute("UPDATE score SET minbp = 0", []).unwrap();
        let report = import(&source, kind, &mut library, &mut db, InputDeviceKind::Controller);
        assert_eq!((report.imported, report.corrected, report.skipped), (0, 1, 0));
        let saved = best(&db, sha, kind);
        assert_eq!(
            (saved.bp, saved.device_type, saved.play_count, saved.clear_count),
            (0, InputDeviceKind::Controller, 1, 1)
        );
    }
}

#[test]
fn reimport_repairs_v33_import_without_inventing_missing_bp_during_migration() {
    let kind = ScoreImportKind::Beatoraja;
    let (mut library, mut db, sha, md5) = databases();
    let source = source(kind, sha, md5);
    source.execute("UPDATE score SET minbp = NULL", []).unwrap();
    let ghost = crate::storage::score_db::encode_beatoraja_ghost(&vec![1; 1100]).unwrap();
    source.execute("UPDATE score SET ghost = ?1", [ghost]).unwrap();
    import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
    // Remove only the new column to construct the actual pre-v34 schema.
    db.conn_mut()
        .execute_batch(
            "ALTER TABLE score_history DROP COLUMN source_min_bp;
        PRAGMA user_version = 33;",
        )
        .unwrap();
    let before = metadata(&db);
    run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
    assert_eq!(best(&db, sha, kind).bp, 20);
    assert_eq!(metadata(&db), before);
    source.execute("UPDATE score SET minbp = 5", []).unwrap();
    let report = import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
    assert_eq!((report.imported, report.corrected), (0, 1));
    assert_eq!(best(&db, sha, kind).bp, 5);
    assert_eq!(metadata(&db), before);
    assert_eq!(all_rows(&db, "score_history", &[]).len(), 1);
}

#[test]
fn zero_unknown_and_large_bp_follow_source_semantics() {
    for kind in KINDS {
        for raw in [Some(0_i64), None, Some(-1), Some(i64::from(i32::MAX)), Some(1200)] {
            let (mut library, mut db, sha, md5) = databases();
            let source = source(kind, sha, md5);
            source.execute("UPDATE score SET minbp = ?1", [raw]).unwrap();
            assert_eq!(
                import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard).imported,
                1
            );
            let expected = match raw {
                Some(v) if v >= 0 && (kind.uses_lr2_schema() || v != i64::from(i32::MAX)) => {
                    Some(v as u32)
                }
                _ => None,
            };
            let stored: Option<u32> = db
                .conn()
                .query_row("SELECT source_min_bp FROM score_history", [], |r| r.get(0))
                .unwrap();
            assert_eq!(stored, expected);
            assert_eq!(best(&db, sha, kind).bp, expected.unwrap_or(20).min(20));
        }
    }
}

#[test]
fn scoredatalog_only_uses_each_rows_minbp_and_keeps_judgement_bp_separate() {
    let (mut library, mut db, sha, md5) = databases();
    let kind = ScoreImportKind::Beatoraja;
    let source = source(kind, sha, md5);
    source
        .execute_batch(
            "UPDATE score SET minbp = 20;
        ALTER TABLE score RENAME TO scoredatalog;",
        )
        .unwrap();
    let report = import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
    assert_eq!(report.imported, 1);
    assert_eq!(best(&db, sha, kind).bp, 20);
    let history: (u32, Option<u32>) = db
        .conn()
        .query_row("SELECT bp, source_min_bp FROM score_history", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(history, (20, Some(20)));
    assert_eq!(import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard).skipped, 1);
}
