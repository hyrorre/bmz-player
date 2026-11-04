use super::*;

#[test]
fn import_score_summary_sanity_checks_ex_score_and_combo() {
    assert!(score_summary_is_sane(128, 128, 256));
    assert!(!score_summary_is_sane(0, 0, 0));
    assert!(!score_summary_is_sane(128, 129, 256));
    assert!(!score_summary_is_sane(128, 128, 257));
}

#[test]
fn lr2_import_skips_unregistered_md5() {
    let (mut library_db, mut score_db, _, _) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    create_lr2_source(&source, &[9; 16]);

    let report = import_lr2_scores(
        &source,
        ScoreImportKind::Lr2,
        &mut library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();

    assert_eq!(report.scanned, 1);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.imported, 0);
}

#[test]
fn beatoraja_import_skips_course_scores_without_failing() {
    let (library_db, mut score_db, _, _) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    // A 4-song course key: four 64-char hashes concatenated (256 chars).
    let course_key = "a".repeat(256);
    create_beatoraja_source_with_sha256(&source, &course_key, 1_700_000_001_000, 0);

    let report = import_beatoraja_scores(
        &source,
        ScoreImportKind::Beatoraja,
        &library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();

    assert_eq!(report.scanned, 1);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.failed, 0);
    assert_eq!(report.imported, 0);
}

#[test]
fn lr2_import_skips_course_scores_without_failing() {
    let (mut library_db, mut score_db, _, _) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    // An LR2 course key: a 32-char marker plus four 32-char md5s (160 chars).
    let course_key = "0".repeat(32) + &"a".repeat(128);
    create_lr2_source_with_hash(&source, &course_key);

    let report = import_lr2_scores(
        &source,
        ScoreImportKind::Lr2,
        &mut library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();

    assert_eq!(report.scanned, 1);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.failed, 0);
    assert_eq!(report.imported, 0);
}

#[test]
fn lr2_course_import_resolves_canonical_and_fans_out_ln_variants() {
    use bmz_core::course::{
        CourseConstraints, CourseDefinition, CourseEntry, CourseKind, CourseLnConstraint,
    };

    let (mut library_db, mut score_db, _, _) = open_test_databases();
    let stage_md5s = [
        "11111111111111111111111111111111",
        "22222222222222222222222222222222",
        "33333333333333333333333333333333",
        "44444444444444444444444444444444",
    ];
    let stage_sha256s = ["11".repeat(32), "22".repeat(32), "33".repeat(32), "44".repeat(32)];
    let entries: Vec<CourseEntry> = stage_md5s
        .iter()
        .enumerate()
        .map(|(i, m)| CourseEntry {
            title_hint: format!("stage{i}"),
            md5: Some(m.to_string()),
            sha256: Some(stage_sha256s[i].clone()),
            chart_id: None,
        })
        .collect();
    let course =
        |key: &str, judge: CourseJudgeConstraint, ln: CourseLnConstraint| CourseDefinition {
            key: key.to_string(),
            title: key.to_string(),
            kind: CourseKind::Dan,
            entries: entries.clone(),
            constraints: CourseConstraints {
                class: CourseClassConstraint::GradeMirrorAllowed,
                speed: CourseSpeedConstraint::Free,
                judge,
                gauge: CourseGaugeConstraint::Lr2,
                ln,
                source_constraints: Vec::new(),
            },
            trophies: Vec::new(),
            release: true,
        };
    // Two canonical variants differing only by LN -> both receive the score.
    library_db
        .upsert_course(
            "table:x",
            &course("dan_default", CourseJudgeConstraint::Normal, CourseLnConstraint::Default),
            0,
            1,
        )
        .unwrap();
    library_db
        .upsert_course(
            "table:x",
            &course("dan_ln", CourseJudgeConstraint::Normal, CourseLnConstraint::Ln),
            1,
            1,
        )
        .unwrap();
    // Non-canonical (no_good judge) sharing the same songs -> must be ignored.
    library_db
        .upsert_course(
            "table:x",
            &course("dan_nogood", CourseJudgeConstraint::NoGood, CourseLnConstraint::Default),
            2,
            1,
        )
        .unwrap();

    // LR2 course record: 32-char marker + the four stage md5s (160 chars).
    let hash = "0".repeat(32) + &stage_md5s.concat();
    let source = Connection::open_in_memory().unwrap();
    create_lr2_source_with_hash(&source, &hash);

    let report = import_lr2_scores(
        &source,
        ScoreImportKind::Lr2,
        &mut library_db,
        &mut score_db,
        1_700_000_000,
    )
    .unwrap();

    assert_eq!(report.scanned, 1);
    assert_eq!(report.matched, 1);
    // Fanned out into the two canonical LN variants, not the no_good course.
    assert_eq!(report.imported, 2);
    let count: i64 =
        score_db.conn().query_row("SELECT COUNT(*) FROM course_scores", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 2);
    let distinct_hashes: i64 = score_db
        .conn()
        .query_row("SELECT COUNT(DISTINCT course_hash) FROM course_scores", [], |r| r.get(0))
        .unwrap();
    assert_eq!(distinct_hashes, 2);
    // The imported course score reflects the LR2 aggregate row (clear=4 -> Hard,
    // ex = perfect*2 + great = 222).
    let (clear, ex): (String, u32) = score_db
        .conn()
        .query_row("SELECT clear_type, ex_score FROM course_scores LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(clear, "Hard");
    assert_eq!(ex, 222);

    // Course rows share LR2's `score` table: do not create another course
    // history entry when its best score used SCATTER.
    source.execute("UPDATE score SET op_best = 40", []).unwrap();
    let skipped = import_lr2_scores(
        &source,
        ScoreImportKind::Lr2,
        &mut library_db,
        &mut score_db,
        1_700_000_001,
    )
    .unwrap();
    assert_eq!(skipped.skipped, 1);
    assert_eq!(skipped.imported, 0);
    let count_after_skip: i64 =
        score_db.conn().query_row("SELECT COUNT(*) FROM course_scores", [], |r| r.get(0)).unwrap();
    assert_eq!(count_after_skip, 2);
}

#[test]
fn decode_lr2_ghost_handles_plain_symbols() {
    // No dictionary tokens, no run counts: B A E D -> Bad, Poor, PGreat, Great.
    assert_eq!(decode_lr2_ghost("BAED", 4), vec![3, 4, 0, 1]);
    // Single PGreat.
    assert_eq!(decode_lr2_ghost("E", 1), vec![0]);
}

#[test]
fn decode_lr2_ghost_expands_dictionary_and_runs() {
    // Real LR2 ghost captured from a player DB.  Exercises both dictionary
    // layers (m,c,k,S,c,b,Z tokens), a run count (`@2`, `8`) and the leading
    // empty-poor (`@`) that must be dropped.  Validated against the LR2 score
    // row's judge counts.
    let ghost = decode_lr2_ghost("@2mBckScb8Z", 20);
    assert_eq!(ghost, vec![4, 1, 3, 2, 2, 4, 3, 1, 1, 2, 2, 1, 4, 4, 4, 4, 4, 4, 4, 4]);
}

#[test]
fn decode_lr2_ghost_pads_and_truncates_to_total_notes() {
    // Aborted play: decoded ghost shorter than the chart -> pad with Poor (4).
    let padded = decode_lr2_ghost("E", 4);
    assert_eq!(padded, vec![0, 4, 4, 4]);
    // Over-long ghost is truncated to the note count.
    let truncated = decode_lr2_ghost("E", 0);
    assert_eq!(truncated, vec![0]); // total_notes 0 leaves the decode untouched
    let truncated = decode_lr2_ghost("BAED", 2);
    assert_eq!(truncated, vec![3, 4]);
}

#[test]
fn lr2_score_state_decodes_ghost() {
    let row = Lr2ScoreRow {
        md5: "0".repeat(32),
        clear: 4,
        perfect: 100,
        great: 21,
        good: 3,
        bad: 2,
        poor: 1,
        total_notes: 4,
        max_combo: 64,
        min_bp: Some(3),
        play_count: 2,
        clear_count: 1,
        ghost: "BAED".to_string(),
        random_seed: Some(123),
        op_best: 0,
    };
    let state = score_state_from_lr2(&row, 4);
    assert_eq!(state.ghost, vec![3, 4, 0, 1]);
    assert_eq!(state.judges.fast_pgreat, 100);
}

#[test]
fn is_course_hash_classifies_by_length() {
    // beatoraja sha256 width.
    assert!(!is_course_hash(&"a".repeat(64), 64));
    assert!(is_course_hash(&"a".repeat(128), 64));
    assert!(is_course_hash(&"a".repeat(256), 64));
    // LR2 md5 width.
    assert!(!is_course_hash(&"a".repeat(32), 32));
    assert!(is_course_hash(&"a".repeat(160), 32));
    // Genuinely malformed (not a multiple of the width) stays a hard failure.
    assert!(!is_course_hash(&"a".repeat(100), 64));
    assert!(!is_course_hash("", 64));
}
