use super::*;

fn local_score(sha: [u8; 32], kind: ScoreImportKind) -> ScoreRecord {
    imported_score_record(
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
    )
}

#[test]
fn imported_combo_lamps_supply_only_independent_cb_and_cleanup_removes_the_evidence() {
    for kind in KINDS {
        let lamps: &[i64] = if kind.uses_lr2_schema() { &[5, 6] } else { &[8, 9, 10] };
        for &lamp in lamps {
            let (mut library, mut db, sha, md5) = unfinished_databases();
            let source = unfinished_source(kind, sha, md5);
            source.execute("UPDATE score SET clear = ?1", [lamp]).unwrap();
            assert_eq!(
                import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard).imported,
                1
            );
            let imported_id: i64 =
                db.conn().query_row("SELECT id FROM score_history", [], |r| r.get(0)).unwrap();
            // The lamp is a separate play's evidence, not a replacement for EX judges.
            assert_eq!(history_metrics(&db), (2005, 2005, Some(2005)));
            assert_eq!((best(&db, sha, kind).bp, best(&db, sha, kind).cb), (2005, 0));
            assert_eq!(
                import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard).skipped,
                1
            );

            let local = local_score(sha, kind);
            let local_id = db.insert_score(&local).unwrap();
            assert_eq!(
                (best(&db, sha, kind).ex_score, best(&db, sha, kind).bp, best(&db, sha, kind).cb),
                (1996, 10, 0)
            );
            db.purge_score_history_ids_and_rebuild(&[local_id]).unwrap();
            assert_eq!((best(&db, sha, kind).bp, best(&db, sha, kind).cb), (2005, 0));
            db.insert_score(&local).unwrap();
            db.purge_score_history_ids_and_rebuild(&[imported_id]).unwrap();
            assert_eq!((best(&db, sha, kind).bp, best(&db, sha, kind).cb), (10, 10));
        }
    }
}

#[test]
fn imported_combo_lamp_improves_cb_without_replacing_a_better_ex_representative() {
    for kind in KINDS {
        let (mut library, mut db, sha, md5) = unfinished_databases();
        let local_id = db.insert_score(&local_score(sha, kind)).unwrap();
        let source = unfinished_source(kind, sha, md5);
        let lamp = if kind.uses_lr2_schema() { 5 } else { 8 };
        source.execute("UPDATE score SET clear = ?1", [lamp]).unwrap();
        import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
        assert_eq!((best(&db, sha, kind).bp, best(&db, sha, kind).cb), (10, 0));
        let representative: i64 = db
            .conn()
            .query_row("SELECT best_score_history_id FROM score_best", [], |r| r.get(0))
            .unwrap();
        assert_eq!(representative, local_id);
    }
}

#[test]
fn combo_cb_migration_repairs_both_pre_v37_and_already_migrated_imports() {
    for kind in KINDS {
        for version in [36, 37] {
            let (mut library, mut db, sha, md5) = unfinished_databases();
            let source = unfinished_source(kind, sha, md5);
            let lamp = if kind.uses_lr2_schema() { 5 } else { 8 };
            source.execute("UPDATE score SET clear = ?1", [lamp]).unwrap();
            import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard);
            if version == 36 {
                db.conn_mut()
                    .execute_batch(
                        "UPDATE score_history SET bp = 2, cb = 2;
                    UPDATE score_best SET bp = 2, cb = 2;",
                    )
                    .unwrap();
            } else {
                db.conn_mut().execute("UPDATE score_best SET cb = 2005", []).unwrap();
            }
            db.conn_mut().pragma_update(None, "user_version", version).unwrap();
            let identities = all_rows(&db, "score_history_ir_identity", &[]);
            run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
            assert_eq!(history_metrics(&db), (2005, 2005, Some(2005)));
            assert_eq!((best(&db, sha, kind).bp, best(&db, sha, kind).cb), (2005, 0));
            assert_eq!(all_rows(&db, "score_history_ir_identity", &[]), identities);
            let history = all_rows(&db, "score_history", &[]);
            let saved = all_rows(&db, "score_best", &[]);
            run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
            assert_eq!(all_rows(&db, "score_history", &[]), history);
            assert_eq!(all_rows(&db, "score_best", &[]), saved);
            assert_eq!(
                import(&source, kind, &mut library, &mut db, InputDeviceKind::Keyboard).skipped,
                1
            );
        }
    }
}

#[test]
fn combo_cb_migration_keeps_key_partitions_local_and_ineligible_history() {
    for rule_mode in ["Lr2Oraja", "Dx", "legacy"] {
        let (_, mut db, sha, _) = unfinished_databases();
        let mut source = local_score(sha, ScoreImportKind::Lr2Oraja);
        source.source_kind = ScoreSourceKind::Beatoraja;
        source.rule_mode = rule_mode.into();
        source.clear_type = ClearType::FullCombo;
        db.insert_score(&source).unwrap();
        // Unrelated keys still have positive CB, even if their aggregate lamp is FC.
        let mut unrelated = source.clone();
        unrelated.source_kind = ScoreSourceKind::Local;
        for dimension in 0..4 {
            let mut record = unrelated.clone();
            match dimension {
                0 => record.chart_sha256 = [3; 32],
                1 => record.ln_policy = LnScorePolicy::ForceCn,
                2 => record.double_option = DoubleOptionScoreBucket::Battle,
                _ => record.rule_mode = if rule_mode == "Dx" { "Lr2Oraja" } else { "Dx" }.into(),
            }
            db.insert_score(&record).unwrap();
        }
        unrelated.chart_sha256 = [4; 32];
        db.insert_score(&unrelated).unwrap();
        let mut ineligible = unrelated.clone();
        ineligible.source_kind = ScoreSourceKind::Beatoraja;
        db.insert_score_with_mode(
            &ineligible,
            crate::storage::score_db::ScoreInsertMode::HistoryOnly,
        )
        .unwrap();
        db.conn_mut()
            .execute_batch("UPDATE score_best SET cb = 10; PRAGMA user_version = 37;")
            .unwrap();
        let histories = all_rows(&db, "score_history", &[]);
        run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
        let zeros: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM score_best WHERE cb = 0", [], |r| r.get(0))
            .unwrap();
        assert_eq!(zeros, 1);
        assert_eq!(all_rows(&db, "score_history", &[]), histories);
    }
}
