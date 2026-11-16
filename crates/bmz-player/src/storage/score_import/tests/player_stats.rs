use super::*;

fn add_player_table(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE player (
            date INTEGER PRIMARY KEY, playcount INTEGER, clear INTEGER,
            epg INTEGER, lpg INTEGER, egr INTEGER, lgr INTEGER, egd INTEGER, lgd INTEGER,
            ebd INTEGER, lbd INTEGER, epr INTEGER, lpr INTEGER, ems INTEGER, lms INTEGER,
            playtime INTEGER, maxcombo INTEGER
        );
        INSERT INTO player VALUES (100, 50, 25, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 500, 10),
                                  (200, 7948, 4458, 101, 202, 303, 404, 505, 606,
                                   707, 808, 909, 1010, 1111, 1212, 913342, 250);",
    )
    .unwrap();
}

#[test]
fn profile_totals_replace_old_import_counts_and_keep_bmz_plays() {
    let (mut library, mut db, sha256, _) = open_test_databases();
    let temp = crate::test_dir::TestDir::new("external-score");
    let source = Connection::open(&*temp).unwrap();
    create_beatoraja_source(&source, &sha256, 1_700_000_000, 0);
    let mut request = ScoreImportRequest {
        path: temp.to_path_buf(),
        kind: ScoreImportKind::Beatoraja,
        device_type: InputDeviceKind::Keyboard,
    };
    let mut local = imported_score_record(
        sha256,
        300,
        ClearType::Hard,
        128,
        ScoreState {
            judges: JudgeCounts {
                fast_pgreat: 8,
                slow_great: 6,
                slow_bad: 2,
                ..Default::default()
            },
            max_combo: 16,
            ..Default::default()
        },
        None,
        "Beatoraja",
        LnScorePolicy::ForceLn,
    );
    local.playtime_seconds = 90;
    let local_history_id = db.insert_score(&local).unwrap();
    local.playtime_seconds = 60;
    local.clear_type = ClearType::Failed;
    db.update_score_clear_only(&local).unwrap();

    // A previous BMZ version counted the best snapshot as one play. Legacy
    // score-only sources still use that behavior until totals are available.
    let initial_import = import_scores(&request, &mut library, &mut db, 400).unwrap();
    assert_eq!(initial_import.imported, 1);
    assert!(!initial_import.player_stats_imported);
    let previous_stats = db.player_stats().unwrap();
    assert_eq!((previous_stats.play_count, previous_stats.clear_count), (3, 2));
    db.conn_mut()
        .execute_batch("DROP TABLE imported_player_stats; PRAGMA user_version = 36;")
        .unwrap();
    run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
    assert_eq!(db.player_stats().unwrap(), previous_stats);

    add_player_table(&source);
    let report = import_scores(&request, &mut library, &mut db, 401).unwrap();
    assert!(report.player_stats_imported);
    assert!(!report.has_errors());
    assert_eq!(report.imported, 0);
    let expected = PlayerStats {
        play_count: 7950,
        clear_count: 4459,
        playtime_seconds: 913492,
        max_combo: 250,
        fast_pgreat: 117,
        slow_pgreat: 202,
        fast_great: 303,
        slow_great: 416,
        fast_good: 505,
        slow_good: 606,
        fast_bad: 707,
        slow_bad: 812,
        fast_poor: 909,
        slow_poor: 1010,
        fast_empty_poor: 1111,
        slow_empty_poor: 1212,
        updated_at: previous_stats.updated_at,
    };
    assert_eq!(db.player_stats().unwrap(), expected);

    // Switching the ruleset on the same source can add a per-chart score in
    // the other bucket, but it still represents the same cumulative baseline.
    request.kind = ScoreImportKind::Lr2Oraja;
    let report = import_scores(&request, &mut library, &mut db, 402).unwrap();
    assert_eq!(report.imported, 1);
    assert!(report.player_stats_imported);
    assert_eq!(db.player_stats().unwrap(), expected);

    // The same date is overwritten as the source records more plays that day.
    source.execute(
        "UPDATE player SET playcount = 7949, playtime = playtime + 30, epg = epg + 2 WHERE date = 200",
        [],
    ).unwrap();
    let report = import_scores(&request, &mut library, &mut db, 403).unwrap();
    assert!(report.player_stats_imported);
    let updated =
        PlayerStats { play_count: 7951, playtime_seconds: 913522, fast_pgreat: 119, ..expected };
    assert_eq!(db.player_stats().unwrap(), updated);
    import_scores(&request, &mut library, &mut db, 404).unwrap();
    assert_eq!(db.player_stats().unwrap(), updated);

    let imported_ids: Vec<_> = db
        .recent_history(10, 0)
        .unwrap()
        .into_iter()
        .filter(|row| row.source_kind != ScoreSourceKind::Local)
        .map(|row| row.id)
        .collect();
    assert_eq!(imported_ids.len(), 2);
    db.purge_score_history_ids_and_rebuild(&imported_ids).unwrap();
    let after_cleanup = db.player_stats().unwrap();
    assert_eq!(after_cleanup, updated);
    assert_eq!(db.recent_history(10, 0).unwrap()[0].id, local_history_id);

    local.played_at = 500;
    local.playtime_seconds = 45;
    local.clear_type = ClearType::Hard;
    db.insert_score(&local).unwrap();
    let after_play = db.player_stats().unwrap();
    assert_eq!((after_play.play_count, after_play.clear_count), (7952, 4460));
    assert_eq!(after_play.playtime_seconds, 913567);
    assert_eq!(after_play.fast_pgreat, 127);
}

#[test]
fn totals_import_even_without_a_matching_chart_and_invalid_source_keeps_previous_totals() {
    let (library, mut db, _, _) = open_test_databases();
    let source = Connection::open_in_memory().unwrap();
    create_beatoraja_source(&source, &[3; 32], 1_700_000_000, 0);
    add_player_table(&source);
    let report =
        import_beatoraja_scores(&source, ScoreImportKind::Lr2OrajaDx, &library, &mut db, 400)
            .unwrap();
    assert_eq!((report.imported, report.skipped), (0, 1));
    assert!(report.player_stats_imported);
    let totals = db.player_stats().unwrap();
    assert_eq!(
        (totals.play_count, totals.clear_count, totals.playtime_seconds),
        (7948, 4458, 913342)
    );

    source.execute("UPDATE player SET playcount = 1 WHERE date = 200", []).unwrap();
    let report =
        import_beatoraja_scores(&source, ScoreImportKind::Lr2OrajaDx, &library, &mut db, 401)
            .unwrap();
    assert!(!report.player_stats_imported);
    assert!(report.has_errors());
    assert_eq!(db.player_stats().unwrap(), totals);

    source.execute_batch("DROP TABLE player; ALTER TABLE score RENAME TO scoredatalog;").unwrap();
    let report =
        import_beatoraja_scores(&source, ScoreImportKind::Beatoraja, &library, &mut db, 402)
            .unwrap();
    assert!(!report.player_stats_imported);
    assert!(!report.has_errors());
    assert_eq!(db.player_stats().unwrap(), totals);
}
