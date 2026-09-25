use super::*;

#[test]
fn lr2_import_maps_md5_and_clear_type() {
    let (mut library_db, mut score_db, sha256, md5) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    create_lr2_source(&source, &md5);

    let report = import_lr2_scores_with_device_type(
        &source,
        ScoreImportKind::Lr2,
        &mut library_db,
        &mut score_db,
        1_700_000_000,
        InputDeviceKind::Controller,
    )
    .unwrap();

    assert_eq!(report.imported, 1);
    let best = score_db
        .best_scores_for_charts(&[
            ScoreKey::new(sha256, LnScorePolicy::ForceLn).with_rule_mode(RuleMode::Lr2Oraja)
        ])
        .unwrap();
    assert_eq!(best[0].clear_type, "Hard");
    assert_eq!(best[0].ex_score, 222);
    assert_eq!(best[0].ln_policy, LnScorePolicy::ForceLn);
    assert_eq!(best[0].device_type, InputDeviceKind::Controller);
}

#[test]
fn lr2_import_preserves_supported_op_best_arrangements_and_flip() {
    let (mut library_db, mut score_db, _, md5) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    create_lr2_source(&source, &md5);
    // Gauge=1 (ignored), 1P=RANDOM, 2P=S-RANDOM, DP=FLIP.
    source.execute("UPDATE score SET op_best = 1321", []).unwrap();

    let report = import_lr2_scores(
        &source,
        ScoreImportKind::Lr2,
        &mut library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();

    assert_eq!(report.imported, 1);
    let options: (String, String, String, String) = score_db
        .conn()
        .query_row(
            "SELECT arrange, arrange_2p, double_option, applied_double_option
                 FROM score_history",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        options,
        ("Random".to_string(), "SRandom".to_string(), "Off".to_string(), "Flip".to_string(),)
    );
}

#[test]
fn lr2_import_skips_unsupported_op_best_arrangements() {
    for op_best in [40, 500, 60, 2_000] {
        let (mut library_db, mut score_db, _, md5) = open_test_databases();
        let source = Connection::open_in_memory().unwrap();
        create_lr2_source(&source, &md5);
        source.execute("UPDATE score SET op_best = ?1", [op_best]).unwrap();

        let report = import_lr2_scores(
            &source,
            ScoreImportKind::Lr2,
            &mut library_db,
            &mut score_db,
            1_700_000_000,
        )
        .unwrap();

        assert_eq!(report.scanned, 1, "op_best={op_best}");
        assert_eq!(report.skipped, 1, "op_best={op_best}");
        assert_eq!(report.imported, 0, "op_best={op_best}");
        assert_eq!(report.failed, 0, "op_best={op_best}");
    }
}

#[test]
fn beatoraja_import_preserves_fast_slow_counts_and_current_schema_fields() {
    let (library_db, mut score_db, sha256, _) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    create_beatoraja_source(&source, &sha256, 1_700_000_001_000, 0);
    // 1P=ROTATE, 2P=MIRROR, double=FLIP.  FLIP shares the Off score
    // bucket, but is retained as the applied option in history.
    source.execute("UPDATE score SET option = 113", []).unwrap();

    let report = import_beatoraja_scores_with_device_type(
        &source,
        ScoreImportKind::Beatoraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
        InputDeviceKind::Controller,
    )
    .unwrap();

    assert_eq!(report.imported, 1);
    type ScoreFields = (String, u32, u32, u32, String, String, String, String);
    type ContextFields = (String, String, String, String, i64);
    let row: (ScoreFields, ContextFields) = score_db
        .conn()
        .query_row(
            "SELECT clear_type, fast_pgreat, slow_pgreat, slow_empty_poor,
                    rule_mode, ln_policy, double_option, applied_double_option, arrange, arrange_2p,
                    device_type, source_kind, played_at
                 FROM score_history",
            [],
            |row| {
                Ok((
                    (
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ),
                    (row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?, row.get(12)?),
                ))
            },
        )
        .unwrap();
    assert_eq!(
        row,
        (
            (
                "ExHard".to_string(),
                100,
                10,
                1,
                "Beatoraja".to_string(),
                "ForceLn".to_string(),
                "Off".to_string(),
                "Flip".to_string(),
            ),
            (
                "RRandom".to_string(),
                "Mirror".to_string(),
                "controller".to_string(),
                "Beatoraja".to_string(),
                1_700_000_001,
            ),
        )
    );
}

#[test]
fn beatoraja_option_maps_both_arrange_slots_and_double_bucket() {
    assert_eq!(
        beatoraja_arrange_options(213, "7K"),
        (ArrangeOption::RRandom, ArrangeOption::Mirror)
    );
    assert_eq!(beatoraja_double_option(213).score_bucket(), DoubleOptionScoreBucket::Battle);
    assert_eq!(beatoraja_double_option(100), DoubleOption::Flip);
    // FLIP is intentionally in the existing Off score bucket, while its
    // actual option is retained by `ScoreRecord::applied_double_option`.
    assert_eq!(beatoraja_double_option(100).score_bucket(), DoubleOptionScoreBucket::Off);
    assert_eq!(beatoraja_double_option(200), DoubleOption::Battle);
    assert_eq!(
        beatoraja_double_option(300).score_bucket(),
        DoubleOptionScoreBucket::BattleAutoScratch
    );
    assert_eq!(beatoraja_arrange_options(7, "9K"), (ArrangeOption::Normal, ArrangeOption::Normal));
    assert_eq!(beatoraja_arrange_options(8, "9K"), (ArrangeOption::Random, ArrangeOption::Normal));
    assert_eq!(beatoraja_arrange_options(9, "9K"), (ArrangeOption::SRandom, ArrangeOption::Normal));
}

#[test]
fn beatoraja_import_skips_identical_scores_from_same_source_kind() {
    let (library_db, mut score_db, sha256, _) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    create_beatoraja_source(&source, &sha256, 1_700_000_001_000, 0);

    let first = import_beatoraja_scores(
        &source,
        ScoreImportKind::Beatoraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();
    assert_eq!(first.imported, 1);

    // A timestamp change alone does not make an external score a new play.
    source.execute("UPDATE score SET date = ?1", params![1_700_000_002_000_i64]).unwrap();
    let duplicate = import_beatoraja_scores(
        &source,
        ScoreImportKind::Beatoraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();
    assert_eq!(duplicate.imported, 0);
    assert_eq!(duplicate.skipped, 1);

    // Provenance is part of the duplicate key: the same score imported from
    // LR2oraja remains a separate history entry.
    let distinct_source = import_beatoraja_scores(
        &source,
        ScoreImportKind::Lr2Oraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();
    assert_eq!(distinct_source.imported, 1);
    let source_kinds: Vec<String> = score_db
        .conn()
        .prepare("SELECT source_kind FROM score_history ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<std::result::Result<_, _>>()
        .unwrap();
    assert_eq!(source_kinds, vec!["Beatoraja", "Lr2Oraja"]);
}

#[test]
fn beatoraja_reimport_corrects_device_type_without_adding_history() {
    let (library_db, mut score_db, sha256, _) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    create_beatoraja_source(&source, &sha256, 1_700_000_001_000, 0);

    let first = import_beatoraja_scores_with_device_type(
        &source,
        ScoreImportKind::Beatoraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
        InputDeviceKind::Keyboard,
    )
    .unwrap();
    assert_eq!(first.imported, 1);

    let corrected = import_beatoraja_scores_with_device_type(
        &source,
        ScoreImportKind::Beatoraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
        InputDeviceKind::Controller,
    )
    .unwrap();
    assert_eq!(corrected.imported, 0);
    assert_eq!(corrected.corrected, 1);
    assert_eq!(corrected.skipped, 0);

    let history_count: u32 = score_db
        .conn()
        .query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get(0))
        .unwrap();
    let history_device: String = score_db
        .conn()
        .query_row("SELECT device_type FROM score_history", [], |row| row.get(0))
        .unwrap();
    let best = score_db
        .best_scores_for_charts(&[ScoreKey::new(sha256, LnScorePolicy::ForceLn)])
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(history_count, 1);
    assert_eq!(history_device, "controller");
    assert_eq!(best.device_type, InputDeviceKind::Controller);
}

#[test]
fn beatoraja_reimport_backfills_missing_best_ghost_without_adding_history() {
    let (library_db, mut score_db, sha256, _) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    create_beatoraja_source(&source, &sha256, 1_700_000_001_000, 0);
    let mut ghost = vec![0; 110];
    ghost.extend(vec![1; 8]);
    ghost.extend(vec![2; 3]);
    ghost.extend(vec![3; 3]);
    ghost.extend(vec![4; 4]);
    let encoded = crate::storage::score_db::encode_beatoraja_ghost(&ghost).unwrap();
    source.execute("UPDATE score SET ghost = ?1", params![encoded]).unwrap();

    let first = import_beatoraja_scores(
        &source,
        ScoreImportKind::Beatoraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();
    assert_eq!(first.imported, 1);
    score_db.conn_mut().execute("UPDATE score_best SET ghost = ''", []).unwrap();

    let backfilled = import_beatoraja_scores(
        &source,
        ScoreImportKind::Beatoraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();
    assert_eq!(backfilled.imported, 0);
    assert_eq!(backfilled.corrected, 1);
    assert_eq!(backfilled.skipped, 0);
    assert_eq!(
        score_db.best_ghost(ScoreKey::new(sha256, LnScorePolicy::ForceLn), 128).unwrap(),
        Some(ghost)
    );
    let history_count: u32 = score_db
        .conn()
        .query_row("SELECT COUNT(*) FROM score_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(history_count, 1);
}

#[test]
fn lr2oraja_dx_import_sets_dx_rule_mode() {
    let (library_db, mut score_db, sha256, _) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    create_beatoraja_source(&source, &sha256, 1_700_000_001_000, 0);

    import_beatoraja_scores(
        &source,
        ScoreImportKind::Lr2OrajaDx,
        &library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();

    let rule_mode: String = score_db
        .conn()
        .query_row("SELECT rule_mode FROM score_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rule_mode, "Dx");
}

#[test]
fn lr2oraja_import_uses_beatoraja_schema_and_rule_mode() {
    let (library_db, mut score_db, sha256, _) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    create_beatoraja_source(&source, &sha256, 1_700_000_001_000, 0);

    let report = import_beatoraja_scores(
        &source,
        ScoreImportKind::Lr2Oraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();
    assert_eq!(report.imported, 1);
    let rule_mode: String = score_db
        .conn()
        .query_row("SELECT rule_mode FROM score_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rule_mode, "Lr2Oraja");
}

#[test]
fn beatoraja_import_mode_cn_on_undefined_ln_sets_force_cn() {
    let (mut library_db, mut score_db, sha256, _) =
        open_test_databases_with_chart(undefined_ln_chart(2, 2));
    let source = Connection::open_in_memory().unwrap();
    // ForceCn expected = 2 base + 2 CN ends = 4
    create_beatoraja_source_with_score(
        &source,
        &hash_to_hex(&sha256),
        BeatorajaScoreFixture {
            date: 1_700_000_001_000,
            mode: 1,
            clear: 7,
            total_notes: 4,
            judged: 4,
            max_combo: 4,
        },
    );

    let report = import_beatoraja_scores(
        &source,
        ScoreImportKind::Beatoraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();
    assert_eq!(report.imported, 1);
    let ln_policy: String = score_db
        .conn()
        .query_row("SELECT ln_policy FROM score_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(ln_policy, "ForceCn");
    let _ = &mut library_db;
}

#[test]
fn beatoraja_import_falls_back_to_force_ln_when_only_ln_expected_matches() {
    let (library_db, mut score_db, sha256, _) =
        open_test_databases_with_chart(undefined_ln_chart(2, 2));
    let source = Connection::open_in_memory().unwrap();
    // mode=1 -> ForceCn expects 4, but source notes=2 match ForceLn only.
    create_beatoraja_source_with_score(
        &source,
        &hash_to_hex(&sha256),
        BeatorajaScoreFixture {
            date: 1_700_000_001_000,
            mode: 1,
            clear: 7,
            total_notes: 2,
            judged: 2,
            max_combo: 2,
        },
    );

    let report = import_beatoraja_scores(
        &source,
        ScoreImportKind::Beatoraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();
    assert_eq!(report.imported, 1);
    let ln_policy: String = score_db
        .conn()
        .query_row("SELECT ln_policy FROM score_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(ln_policy, "ForceLn");
}

#[test]
fn beatoraja_import_fails_when_source_note_count_mismatches_all_policies() {
    let (library_db, mut score_db, sha256, _) =
        open_test_databases_with_chart(undefined_ln_chart(2, 2));
    let source = Connection::open_in_memory().unwrap();
    create_beatoraja_source_with_score(
        &source,
        &hash_to_hex(&sha256),
        BeatorajaScoreFixture {
            date: 1_700_000_001_000,
            mode: 1,
            clear: 7,
            total_notes: 3,
            judged: 3,
            max_combo: 3,
        },
    );

    let report = import_beatoraja_scores(
        &source,
        ScoreImportKind::Beatoraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();
    assert_eq!(report.failed, 1);
    assert_eq!(report.imported, 0);
}

#[test]
fn beatoraja_import_accepts_failed_row_with_fewer_judgements() {
    let (library_db, mut score_db, sha256, _) =
        open_test_databases_with_chart(undefined_ln_chart(2, 2));
    let source = Connection::open_in_memory().unwrap();
    create_beatoraja_source_with_score(
        &source,
        &hash_to_hex(&sha256),
        BeatorajaScoreFixture {
            date: 1_700_000_001_000,
            mode: 1,
            clear: 1,
            total_notes: 4,
            judged: 3,
            max_combo: 3,
        },
    );

    let report = import_beatoraja_scores(
        &source,
        ScoreImportKind::Beatoraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();
    assert_eq!(report.imported, 1);
    assert_eq!(report.failed, 0);
    let clear_type: String = score_db
        .conn()
        .query_row("SELECT clear_type FROM score_history", [], |row| row.get(0))
        .unwrap();
    assert_eq!(clear_type, "Failed");
}

#[test]
fn beatoraja_import_accepts_more_judgements_than_source_notes() {
    let (library_db, mut score_db, sha256, _) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    create_beatoraja_source_with_score(
        &source,
        &hash_to_hex(&sha256),
        BeatorajaScoreFixture {
            date: 1_700_000_001_000,
            mode: 0,
            clear: 7,
            total_notes: 128,
            judged: 129,
            max_combo: 80,
        },
    );

    let report = import_beatoraja_scores(
        &source,
        ScoreImportKind::Beatoraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();
    assert_eq!(report.imported, 1);
    assert_eq!(report.failed, 0);
}

#[test]
fn lr2_import_accepts_empty_poor_in_judge_total() {
    let (mut library_db, mut score_db, _, md5) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    create_lr2_source_with_score(
        &source,
        &hash_to_hex(&md5),
        Lr2ScoreFixture {
            total_notes: 128,
            max_combo: 64,
            perfect: 100,
            great: 22,
            good: 3,
            bad: 2,
            poor: 20,
        },
    );

    let report = import_lr2_scores(
        &source,
        ScoreImportKind::Lr2,
        &mut library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();
    assert_eq!(report.failed, 0);
    assert_eq!(report.imported, 1);
}

#[test]
fn lr2_import_fails_when_source_note_count_mismatches() {
    let (mut library_db, mut score_db, _, md5) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    create_lr2_source_with_score(
        &source,
        &hash_to_hex(&md5),
        Lr2ScoreFixture {
            total_notes: 127,
            max_combo: 64,
            perfect: 100,
            great: 20,
            good: 3,
            bad: 2,
            poor: 1,
        },
    );

    let report = import_lr2_scores(
        &source,
        ScoreImportKind::Lr2,
        &mut library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();
    assert_eq!(report.failed, 1);
    assert_eq!(report.imported, 0);
}
