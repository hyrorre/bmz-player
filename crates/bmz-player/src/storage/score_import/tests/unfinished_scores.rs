use super::*;

fn unfinished_source(kind: ScoreImportKind, sha: [u8; 32], md5: [u8; 16]) -> Connection {
    let source = source(kind, sha, md5);
    let sql = if kind.uses_lr2_schema() {
        "UPDATE score SET clear = 1, totalnotes = 2006, maxcombo = 1,
            perfect = 0, great = 1, good = 0, bad = 0, poor = 2, minbp = 2005"
    } else {
        "UPDATE score SET clear = 1, notes = 2006, combo = 1,
            epg = 0, lpg = 0, egr = 1, lgr = 0, egd = 0, lgd = 0,
            ebd = 0, lbd = 0, epr = 2, lpr = 0, ems = 0, lms = 0, minbp = 2005"
    };
    source.execute_batch(sql).unwrap();
    source
}

fn unfinished_databases() -> (LibraryDatabase, ScoreDatabase, [u8; 32], [u8; 16]) {
    let mut target = chart();
    target.total_notes = 2006;
    open_test_databases_with_chart(target)
}

fn history_metrics(db: &ScoreDatabase) -> (u32, u32, Option<u32>) {
    db.conn()
        .query_row("SELECT bp, cb, source_min_bp FROM score_history ORDER BY id LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap()
}

#[test]
fn issue_105_all_formats_count_unprocessed_notes_and_reimport_once() {
    for kind in KINDS {
        let (mut library, mut db, sha, md5) = unfinished_databases();
        let source = unfinished_source(kind, sha, md5);
        assert_eq!(
            import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard).imported,
            1
        );
        assert_eq!(history_metrics(&db), (2005, 2005, Some(2005)));
        let saved = best(&db, sha, kind);
        assert_eq!((saved.bp, saved.cb, saved.play_count), (2005, 2005, 1));
        assert_eq!(
            import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard).skipped,
            1
        );
        assert_eq!(all_rows(&db, "score_history", &[]).len(), 1);
    }
}

#[test]
fn independent_clear_lamp_does_not_hide_unfinished_ex_judges() {
    for kind in KINDS {
        let (mut library, mut db, sha, md5) = unfinished_databases();
        let source = unfinished_source(kind, sha, md5);
        source.execute("UPDATE score SET clear = 4, minbp = 17", []).unwrap();
        import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
        assert_eq!(history_metrics(&db), (2005, 2005, Some(17)));
        assert_eq!((best(&db, sha, kind).bp, best(&db, sha, kind).cb), (17, 2005));
    }
}

#[test]
fn scoredatalog_empty_poor_does_not_consume_notes() {
    for kind in [ScoreImportKind::Beatoraja, ScoreImportKind::Lr2Oraja, ScoreImportKind::Lr2OrajaDx]
    {
        let (mut library, mut db, sha, md5) = unfinished_databases();
        let source = unfinished_source(kind, sha, md5);
        source
            .execute_batch(
                "UPDATE score SET ems = 3, lms = 4, minbp = 2012;
            ALTER TABLE score RENAME TO scoredatalog;",
            )
            .unwrap();
        import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
        assert_eq!(history_metrics(&db), (2012, 2005, Some(2012)));
        assert_eq!((best(&db, sha, kind).bp, best(&db, sha, kind).cb), (2012, 2005));
        assert_eq!(
            import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard).skipped,
            1
        );
    }
}

#[test]
fn no_judgements_and_unknown_source_bp_do_not_become_zero_bests() {
    for kind in KINDS {
        for unknown in [None, Some(-1), Some(i64::from(i32::MAX))] {
            if kind.uses_lr2_schema() && unknown == Some(i64::from(i32::MAX)) {
                continue; // LR2 has no Integer.MAX_VALUE sentinel.
            }
            let (mut library, mut db, sha, md5) = unfinished_databases();
            let source = unfinished_source(kind, sha, md5);
            source.execute("UPDATE score SET minbp = ?1", [unknown]).unwrap();
            let sql = if kind.uses_lr2_schema() {
                "UPDATE score SET great = 0, poor = 0, maxcombo = 0"
            } else {
                "UPDATE score SET egr = 0, epr = 0, combo = 0"
            };
            source.execute_batch(sql).unwrap();
            import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
            assert_eq!(history_metrics(&db), (2006, 2006, None));
            assert_eq!((best(&db, sha, kind).bp, best(&db, sha, kind).cb), (2006, 2006));
        }
    }
}

#[test]
fn lr2_known_minbp_overrides_ambiguous_poor_and_survives_cleanup() {
    let kind = ScoreImportKind::Lr2;
    let (mut library, mut db, sha, md5) = unfinished_databases();
    let source = unfinished_source(kind, sha, md5);
    source.execute("UPDATE score SET poor = 9, minbp = NULL", []).unwrap();
    import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
    assert_eq!(history_metrics(&db), (2005, 2005, None));
    source.execute("UPDATE score SET minbp = 2012", []).unwrap();
    assert_eq!(
        import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard).corrected,
        1
    );
    assert_eq!(best(&db, sha, kind).bp, 2012);

    let mut local = imported_score_record(
        sha,
        1_700_000_200,
        ClearType::Normal,
        2006,
        ScoreState {
            judges: JudgeCounts { fast_great: 1996, fast_bad: 10, ..Default::default() },
            past_notes: 2006,
            ..Default::default()
        },
        None,
        kind.rule_mode(),
        LnScorePolicy::ForceLn,
    );
    local.source_kind = ScoreSourceKind::Local;
    let local_id = db.insert_score(&local).unwrap();
    assert_eq!(best(&db, sha, kind).bp, 10);
    db.purge_score_history_ids_and_rebuild(&[local_id]).unwrap();
    assert_eq!((best(&db, sha, kind).bp, best(&db, sha, kind).cb), (2012, 2005));
}

#[test]
fn lr2_completed_score_excludes_empty_poor_from_cb() {
    let (mut library, mut db, sha, md5) = open_test_databases();
    let kind = ScoreImportKind::Lr2;
    let source = Connection::open_in_memory().unwrap();
    // 125 successful notes, 2 BAD and 1 note POOR; the other 9 POOR are empty.
    create_lr2_source(&source, &md5);
    import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
    assert_eq!(history_metrics(&db), (12, 3, Some(3)));
    assert_eq!((best(&db, sha, kind).bp, best(&db, sha, kind).cb), (3, 3));
    db.conn_mut()
        .execute_batch(
            "UPDATE score_history SET cb = 12, fast_poor = 10, fast_empty_poor = 0;
        UPDATE score_best SET cb = 12; PRAGMA user_version = 36;",
        )
        .unwrap();
    run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
    assert_eq!(history_metrics(&db), (12, 3, Some(3)));
    assert_eq!(best(&db, sha, kind).cb, 3);
    assert_eq!(import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard).skipped, 1);
}

#[test]
fn migration_repairs_old_imports_without_duplicates_or_metadata_changes() {
    for kind in KINDS {
        let (mut library, mut db, sha, md5) = unfinished_databases();
        let source = unfinished_source(kind, sha, md5);
        import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
        db.conn_mut()
            .execute_batch(
                "UPDATE score_history SET bp = 2, cb = 2;
            UPDATE score_best SET bp = 2, cb = 2; PRAGMA user_version = 36;",
            )
            .unwrap();
        let history_before = all_rows(&db, "score_history", &["bp", "cb"]);
        let best_before = all_rows(&db, "score_best", &["bp", "cb"]);
        let identities_before = all_rows(&db, "score_history_ir_identity", &[]);
        let stats_before = all_rows(&db, "player_stats", &[]);
        run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
        assert_eq!(history_metrics(&db), (2005, 2005, Some(2005)));
        assert_eq!((best(&db, sha, kind).bp, best(&db, sha, kind).cb), (2005, 2005));
        assert_eq!(all_rows(&db, "score_history", &["bp", "cb"]), history_before);
        assert_eq!(all_rows(&db, "score_best", &["bp", "cb"]), best_before);
        assert_eq!(all_rows(&db, "score_history_ir_identity", &[]), identities_before);
        assert_eq!(all_rows(&db, "player_stats", &[]), stats_before);
        run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
        assert_eq!(
            import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard).skipped,
            1
        );
        assert_eq!(best(&db, sha, kind).play_count, 1);
    }
}

#[test]
fn migration_excludes_local_and_ir_histories_and_rolls_back_atomically() {
    for provenance in ["local", "ir", "import"] {
        let kind = ScoreImportKind::Lr2Oraja;
        let (mut library, mut db, sha, md5) = unfinished_databases();
        let source = unfinished_source(kind, sha, md5);
        import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
        db.conn_mut()
            .execute_batch(
                "UPDATE score_history SET bp = 2, cb = 2;
            UPDATE score_best SET bp = 2, cb = 2; PRAGMA user_version = 36;",
            )
            .unwrap();
        match provenance {
            "local" => {
                db.conn_mut()
                    .execute("UPDATE score_history SET source_kind = 'Local'", [])
                    .unwrap();
            }
            "ir" => {
                db.conn_mut()
                    .execute("UPDATE score_history_sources SET source = 'bmz_ir'", [])
                    .unwrap();
            }
            _ => {
                db.conn_mut()
                    .execute_batch(
                        "CREATE TRIGGER reject_repair BEFORE UPDATE ON score_best
                BEGIN SELECT RAISE(ABORT, 'repair failure'); END;",
                    )
                    .unwrap();
            }
        }
        let before = all_rows(&db, "score_history", &[]);
        let result = run_migrations(db.conn_mut(), SCORE_MIGRATIONS);
        assert_eq!(result.is_err(), provenance == "import");
        assert_eq!(all_rows(&db, "score_history", &[]), before);
        assert_eq!(best(&db, sha, kind).bp, 2);
        if provenance == "import" {
            let version: i32 =
                db.conn().pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
            assert_eq!(version, 36);
            db.conn_mut().execute_batch("DROP TRIGGER reject_repair;").unwrap();
            run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
            assert_eq!(best(&db, sha, kind).bp, 2005);
        }
    }
}

#[test]
fn migrated_bests_merge_local_scores_and_rebuild_after_their_deletion() {
    let kind = ScoreImportKind::Lr2Oraja;
    let (mut library, mut db, sha, md5) = unfinished_databases();
    let source = unfinished_source(kind, sha, md5);
    import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
    let local = imported_score_record(
        sha,
        1_700_000_200,
        ClearType::Normal,
        2006,
        ScoreState {
            judges: JudgeCounts { fast_great: 1996, fast_bad: 10, ..Default::default() },
            past_notes: 2006,
            ..Default::default()
        },
        None,
        kind.rule_mode(),
        LnScorePolicy::ForceLn,
    );
    let id = db.insert_score(&local).unwrap();
    db.conn_mut()
        .execute_batch(
            "UPDATE score_history SET bp = 2, cb = 2 WHERE source_kind != 'Local';
        UPDATE score_best SET bp = 2, cb = 2; PRAGMA user_version = 36;",
        )
        .unwrap();
    run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
    assert_eq!((best(&db, sha, kind).bp, best(&db, sha, kind).cb), (10, 10));
    assert_eq!(best(&db, sha, kind).play_count, 2);
    db.purge_score_history_ids_and_rebuild(&[id]).unwrap();
    assert_eq!((best(&db, sha, kind).bp, best(&db, sha, kind).cb), (2005, 2005));
    assert_eq!(best(&db, sha, kind).play_count, 1);
}
