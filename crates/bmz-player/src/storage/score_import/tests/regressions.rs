use super::*;

#[test]
fn lr2oraja_ln_mode_with_cn_notes_remains_rejected_with_diagnostic() {
    let (library, mut scores, sha, _) = open_test_databases_with_chart(undefined_ln_chart(2, 2));
    let source = Connection::open_in_memory().unwrap();
    create_beatoraja_source_with_score(
        &source,
        &hash_to_hex(&sha),
        BeatorajaScoreFixture {
            date: 0,
            mode: 0,
            clear: 7,
            total_notes: 4,
            judged: 4,
            max_combo: 4,
        },
    );
    let report =
        import_beatoraja_scores(&source, ScoreImportKind::Lr2Oraja, &library, &mut scores, 1)
            .unwrap();
    assert_eq!((report.imported, report.failed), (0, 1));
    let issue = report
        .issues
        .iter()
        .find(|issue| issue.kind == ScoreImportIssueKind::NoteCountMismatch)
        .unwrap();
    assert_eq!(issue.count, 1);
    assert!(issue.example.contains("mode=0"));
    assert!(issue.example.contains("source=4"));
    assert!(issue.example.contains("ForceLn=2"));
    assert!(issue.example.contains("ForceCn=4"));
    // The importer must not rewrite the external score's LN mode to make it fit.
    let mode: i64 = source.query_row("SELECT mode FROM score", [], |r| r.get(0)).unwrap();
    assert_eq!(mode, 0);
}

#[test]
fn cn_and_hcn_import_use_stored_counts_when_every_chart_path_is_missing() {
    for mode in [1, 2] {
        let (library, mut scores, sha, _) =
            open_test_databases_with_chart(undefined_ln_chart(2, 2));
        library
            .conn()
            .execute("UPDATE chart_files SET path = 'Z:/bmz-import-nonexistent/song.bms'", [])
            .unwrap();
        let source = Connection::open_in_memory().unwrap();
        create_beatoraja_source_with_score(
            &source,
            &hash_to_hex(&sha),
            BeatorajaScoreFixture {
                date: 0,
                mode,
                clear: 7,
                total_notes: 4,
                judged: 4,
                max_combo: 4,
            },
        );
        let first =
            import_beatoraja_scores(&source, ScoreImportKind::Lr2Oraja, &library, &mut scores, 1)
                .unwrap();
        assert_eq!((first.imported, first.failed), (1, 0));
        let second =
            import_beatoraja_scores(&source, ScoreImportKind::Lr2Oraja, &library, &mut scores, 1)
                .unwrap();
        assert_eq!(second.skipped, 1);
        assert_eq!(second.issues[0].kind, ScoreImportIssueKind::Duplicate);
    }
}

#[test]
fn outdated_library_metadata_requests_rescan_without_reading_old_paths() {
    let (library, mut scores, sha, _) = open_test_databases_with_chart(undefined_ln_chart(2, 2));
    library.conn().execute("UPDATE charts SET import_version = 0", []).unwrap();
    let source = Connection::open_in_memory().unwrap();
    create_beatoraja_source(&source, &sha, 1, 1);
    let report =
        import_beatoraja_scores(&source, ScoreImportKind::Lr2Oraja, &library, &mut scores, 1)
            .unwrap();
    assert_eq!(report.failed, 1);
    assert_eq!(report.issues[0].kind, ScoreImportIssueKind::Metadata);
    assert!(report.issues[0].example.contains("rescan the library"));
}

#[test]
fn mode_zero_accepts_explicit_cn_and_ignores_obsolete_duplicate_metadata() {
    let mut chart = undefined_ln_chart(2, 2);
    for pair in &mut chart.long_notes {
        pair.mode = Some(bmz_chart::model::LongNoteMode::Cn);
    }
    let (mut library, mut scores, sha, _) = open_test_databases_with_chart(chart.clone());
    // Register a newer ID with old parser metadata at an alphabetically earlier path.
    for pair in &mut chart.long_notes {
        pair.mode = None;
    }
    let obsolete = library
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: Path::new("/aaa-old/score-import.bms"),
            file_size: 10,
            modified_at: 1,
            scanned_at: 1,
            chart: &chart,
        })
        .unwrap();
    library
        .conn()
        .execute("UPDATE charts SET import_version = 0 WHERE id = ?1", [obsolete])
        .unwrap();
    let source = Connection::open_in_memory().unwrap();
    create_beatoraja_source_with_score(
        &source,
        &hash_to_hex(&sha),
        BeatorajaScoreFixture {
            date: 0,
            mode: 0,
            clear: 7,
            total_notes: 4,
            judged: 4,
            max_combo: 4,
        },
    );
    let report =
        import_beatoraja_scores(&source, ScoreImportKind::Lr2Oraja, &library, &mut scores, 1)
            .unwrap();
    assert_eq!((report.imported, report.failed), (1, 0));
}

#[test]
fn stored_mixed_ln_counts_match_playable_chart_for_every_policy() {
    use bmz_chart::model::LongNoteMode;
    let mut chart = undefined_ln_chart(4, 4);
    chart.long_notes[1].mode = Some(LongNoteMode::Ln);
    chart.long_notes[2].mode = Some(LongNoteMode::Cn);
    chart.long_notes[3].mode = Some(LongNoteMode::Hcn);
    let (library, _, sha, _) = open_test_databases_with_chart(chart.clone());
    for policy in [
        LnScorePolicy::AutoLn,
        LnScorePolicy::AutoCn,
        LnScorePolicy::AutoHcn,
        LnScorePolicy::ForceLn,
        LnScorePolicy::ForceCn,
        LnScorePolicy::ForceHcn,
    ] {
        assert_eq!(
            expected_notes_for_policy(&library, sha, policy, &mut HashMap::new()).unwrap(),
            crate::ln_policy::expected_scored_note_count_for_policy(&chart, policy)
        );
    }
}

#[test]
fn missing_and_unreadable_rows_are_reported_separately() {
    let (library, mut scores, _, _) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    create_beatoraja_source(&source, &[9; 32], 1, 0);
    // SQLite is dynamically typed: reading notes as u32 must reject a text value.
    source.execute("INSERT INTO score SELECT * FROM score", []).unwrap();
    source
        .execute(
            "UPDATE score SET notes = 'broken' WHERE rowid = (SELECT MAX(rowid) FROM score)",
            [],
        )
        .unwrap();
    let report =
        import_beatoraja_scores(&source, ScoreImportKind::Lr2Oraja, &library, &mut scores, 1)
            .unwrap();
    assert_eq!((report.scanned, report.skipped, report.failed), (2, 1, 1));
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.kind == ScoreImportIssueKind::MissingChart && issue.count == 1)
    );
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.kind == ScoreImportIssueKind::ReadFailure && issue.count == 1)
    );
    assert!(report.summary().contains("missing chart 1"));
    assert!(report.summary().contains("read failure 1"));
}
