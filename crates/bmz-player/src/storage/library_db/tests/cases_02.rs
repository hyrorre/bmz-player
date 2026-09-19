use super::*;

#[test]
fn replace_import_warnings_replaces_previous_rows() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase { conn };
    let chart = chart("song");
    let record = ChartImportRecord {
        root_id: None,
        file_path: Path::new("/songs/song.bms"),
        file_size: 123,
        modified_at: 1,
        scanned_at: 2,
        chart: &chart,
    };
    db.upsert_chart_import(&record).unwrap();
    let chart_file_id = db.chart_file_id_by_path(record.file_path).unwrap().unwrap();

    db.replace_import_warnings(
        chart_file_id,
        &[ImportWarning::UnsupportedChannel { channel: 99 }],
        3,
    )
    .unwrap();
    db.replace_import_warnings(
        chart_file_id,
        &[ImportWarning::MissingWavDefinition { key: 10 }],
        4,
    )
    .unwrap();

    let (count, code): (u32, String) = db
        .conn()
        .query_row("SELECT COUNT(*), MAX(code) FROM chart_import_warnings", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();

    assert_eq!(count, 1);
    assert_eq!(code, "MissingWavDefinition");
}

#[test]
fn upsert_root_updates_existing_row() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);

    let first = db.upsert_root(Path::new("/songs"), true, true).unwrap();
    let second = db.upsert_root(Path::new("/songs"), false, false).unwrap();
    db.update_root_scanned_at(first, 42).unwrap();

    let (count, enabled, recursive, last_scan_at): (u32, bool, bool, i64) = db
        .conn()
        .query_row("SELECT COUNT(*), enabled, recursive, last_scan_at FROM roots", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap();

    assert_eq!(first, second);
    assert_eq!(count, 1);
    assert!(!enabled);
    assert!(!recursive);
    assert_eq!(last_scan_at, 42);
}

#[test]
fn list_charts_orders_by_title() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);
    let alpha = chart("Alpha");
    let beta = chart("beta");

    db.upsert_chart_import(&ChartImportRecord {
        root_id: None,
        file_path: Path::new("/songs/beta.bms"),
        file_size: 1,
        modified_at: 1,
        scanned_at: 1,
        chart: &beta,
    })
    .unwrap();
    db.upsert_chart_import(&ChartImportRecord {
        root_id: None,
        file_path: Path::new("/songs/alpha.bms"),
        file_size: 1,
        modified_at: 1,
        scanned_at: 1,
        chart: &alpha,
    })
    .unwrap();

    let charts = db.list_charts(10, 0).unwrap();

    assert_eq!(charts.len(), 2);
    assert_eq!(charts[0].title, "Alpha");
    assert_eq!(charts[1].title, "beta");
    assert_eq!(charts[0].mode, "7K");
    assert_eq!(charts[0].length_ms, 10_000);
}

#[test]
fn search_charts_matches_substring_across_metadata_fields_case_insensitively() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);

    let mut by_title = chart("Blue Sky");
    by_title.metadata.artist = "Composer A".to_string();
    by_title.metadata.genre = "Trance".to_string();
    let mut by_artist = chart("untitled");
    by_artist.metadata.artist = "DJ Blueprint".to_string();
    let mut by_genre = chart("other");
    by_genre.metadata.artist = "Nobody".to_string();
    by_genre.metadata.genre = "Drum & Bass (BLUE mix)".to_string();
    let mut unrelated = chart("Sunset");
    unrelated.metadata.artist = "Solo".to_string();
    unrelated.metadata.genre = "Ambient".to_string();

    for (path, c) in [
        ("/songs/a.bms", &by_title),
        ("/songs/b.bms", &by_artist),
        ("/songs/c.bms", &by_genre),
        ("/songs/d.bms", &unrelated),
    ] {
        db.upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: Path::new(path),
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: c,
        })
        .unwrap();
    }

    let hits = db.search_charts("blue").unwrap();
    let titles: Vec<&str> = hits.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles.len(), 3, "expected three matches, got {titles:?}");
    assert!(titles.contains(&"Blue Sky"));
    assert!(titles.contains(&"untitled"));
    assert!(titles.contains(&"other"));

    assert!(db.search_charts("nonexistent_query_xyz").unwrap().is_empty());
}

#[test]
fn search_charts_treats_like_wildcards_as_literal() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);

    let mut with_percent = chart("100% pure");
    with_percent.metadata.artist = "p".to_string();
    let mut without = chart("zero");
    without.metadata.artist = "z".to_string();

    for (path, c) in [("/songs/a.bms", &with_percent), ("/songs/b.bms", &without)] {
        db.upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: Path::new(path),
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: c,
        })
        .unwrap();
    }

    let hits = db.search_charts("%").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "100% pure");
}

#[test]
fn search_charts_limited_stops_at_the_requested_row_count() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);

    for (path, title) in [
        ("/songs/c.bms", "Match Charlie"),
        ("/songs/a.bms", "Match Alpha"),
        ("/songs/b.bms", "Match Bravo"),
    ] {
        let chart = chart(title);
        db.upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: Path::new(path),
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &chart,
        })
        .unwrap();
    }

    let hits = db.search_charts_limited("match", 2).unwrap();
    let titles = hits.iter().map(|chart| chart.title.as_str()).collect::<Vec<_>>();

    assert_eq!(titles, ["Match Alpha", "Match Bravo"]);
}

#[test]
fn primary_chart_file_path_returns_linked_file() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);
    let chart = chart("song");
    let chart_id = db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: Path::new("/songs/song.bms"),
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &chart,
        })
        .unwrap();

    assert_eq!(db.primary_chart_file_path(chart_id).unwrap(), Some("/songs/song.bms".to_string()));
    assert_eq!(db.primary_chart_file_path(chart_id + 1).unwrap(), None);
}

#[test]
fn chart_id_by_chart_file_path_resolves_linked_chart() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);
    let chart = chart("boot");
    let chart_id = db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: Path::new("/songs/boot.bms"),
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart: &chart,
        })
        .unwrap();

    assert_eq!(
        db.chart_id_by_chart_file_path(Path::new("/songs/boot.bms")).unwrap(),
        Some(chart_id)
    );
    assert_eq!(db.chart_id_by_chart_file_path(Path::new("/missing.bms")).unwrap(), None);
}

#[test]
fn upsert_failed_chart_file_records_failure_status_and_warning() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);

    let chart_file_id = db
        .upsert_failed_chart_file(None, Path::new("/songs/broken.bms"), 10, 1, 2, "broken")
        .unwrap();

    let (status, code): (String, String) = db
        .conn()
        .query_row(
            "SELECT chart_files.parse_status, chart_import_warnings.code
                FROM chart_files
                JOIN chart_import_warnings ON chart_import_warnings.chart_file_id = chart_files.id
                WHERE chart_files.id = ?1",
            params![chart_file_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(status, "Failed");
    assert_eq!(code, "ImportFailed");
}

#[test]
fn failed_reimport_removes_previous_chart_link_and_orphan() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);
    let path = Path::new("/songs/unsupported.bmson");
    db.upsert_chart_import(&record_for_chart(path.to_str().unwrap(), &chart("old"))).unwrap();

    db.upsert_failed_chart_file(None, path, 10, 2, 3, "unsupported chart mode").unwrap();

    assert_eq!(db.chart_id_by_chart_file_path(path).unwrap(), None);
    let chart_count: i64 =
        db.conn().query_row("SELECT COUNT(*) FROM charts", [], |row| row.get(0)).unwrap();
    assert_eq!(chart_count, 0);
}

#[test]
fn list_failed_chart_files_returns_failures() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);
    db.upsert_failed_chart_file(None, Path::new("/songs/broken.bms"), 10, 1, 2, "broken").unwrap();

    let failed = db.list_failed_chart_files(10, 0).unwrap();

    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].path, "/songs/broken.bms");
    assert_eq!(failed[0].message, "broken");
    assert_eq!(failed[0].scanned_at, 2);
}

#[test]
fn upsert_chart_import_updates_chart_in_place_when_content_changes() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);

    let v1 = chart("content-v1");
    let id1 = db.upsert_chart_import(&record_for_chart("/songs/track.bms", &v1)).unwrap();

    let v2 = chart("content-v2");
    let id2 = db.upsert_chart_import(&record_for_chart("/songs/track.bms", &v2)).unwrap();

    assert_eq!(id1, id2, "same path must return the same chart id");

    let count: i64 = db.conn().query_row("SELECT COUNT(*) FROM charts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1, "re-import of same path must not create an extra chart row");

    let title: String = db
        .conn()
        .query_row("SELECT title FROM charts WHERE id = ?1", params![id2], |r| r.get(0))
        .unwrap();
    assert_eq!(title, "content-v2");

    let link_count: i64 =
        db.conn().query_row("SELECT COUNT(*) FROM chart_file_links", [], |r| r.get(0)).unwrap();
    assert_eq!(link_count, 1);
}

#[test]
fn upsert_chart_import_treats_windows_extended_path_as_the_same_path() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);

    let old = chart("old bmson import");
    let old_id =
        db.upsert_chart_import(&record_for_chart(r"\\?\G:\BMS\song\Normal.bmson", &old)).unwrap();

    let refreshed = chart("refreshed bmson import");
    let refreshed_id =
        db.upsert_chart_import(&record_for_chart("G:/BMS/song/Normal.bmson", &refreshed)).unwrap();

    assert_eq!(old_id, refreshed_id, "path variants must update the same chart");
    let counts: (i64, i64, i64) = db
        .conn()
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM charts),
                (SELECT COUNT(*) FROM chart_files),
                (SELECT COUNT(*) FROM chart_file_links)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (1, 1, 1));

    let (title, path): (String, String) = db
        .conn()
        .query_row(
            "SELECT charts.title, chart_files.path
             FROM charts
             JOIN chart_file_links ON chart_file_links.chart_id = charts.id
             JOIN chart_files ON chart_files.id = chart_file_links.chart_file_id",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(title, "refreshed bmson import");
    assert_eq!(path, "G:/BMS/song/Normal.bmson");
}

#[test]
fn library_migration_merges_windows_separator_variant_paths() {
    let migration_index =
        LIBRARY_MIGRATIONS.iter().position(|migration| migration.version == 30).unwrap();
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, &LIBRARY_MIGRATIONS[..migration_index]).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);
    // 現行 importer で fixture を作り、migration の直前に旧 schema へ戻す。
    db.conn()
        .execute_batch(
            "ALTER TABLE charts ADD COLUMN has_defined_hln INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE charts ADD COLUMN defined_hln_pairs INTEGER NOT NULL DEFAULT 0;",
        )
        .unwrap();

    db.conn()
        .execute_batch(
            "INSERT INTO roots (id, path, enabled, recursive, last_scan_at) VALUES
                (100, 'G:\\BMS', 1, 1, 10),
                (101, 'G:/BMS', 1, 1, 20);",
        )
        .unwrap();

    let old = chart("old bmson import");
    let old_id = db
        .upsert_chart_import(&record_for_chart("G:/BMS/song/old-placeholder.bmson", &old))
        .unwrap();
    let old_file_id =
        db.chart_file_id_by_path(Path::new("G:/BMS/song/old-placeholder.bmson")).unwrap().unwrap();

    let mut refreshed = chart("refreshed bmson import");
    refreshed.identity = old.identity.clone();
    let refreshed_id = db
        .upsert_chart_import(&record_for_chart("G:/BMS/song/new-placeholder.bmson", &refreshed))
        .unwrap();
    let refreshed_file_id =
        db.chart_file_id_by_path(Path::new("G:/BMS/song/new-placeholder.bmson")).unwrap().unwrap();

    let copy_id =
        db.upsert_chart_import(&record_for_chart("G:/BMS/copy/Normal.bmson", &refreshed)).unwrap();

    db.conn()
        .execute(
            "UPDATE chart_files
             SET root_id = 100, path = ?1, scanned_at = 10, first_seen_at = 5
             WHERE id = ?2",
            params![r"G:\BMS\song\Normal.bmson", old_file_id],
        )
        .unwrap();
    db.conn()
        .execute(
            "UPDATE chart_files
             SET root_id = 101, path = ?1, scanned_at = 20, first_seen_at = 15
             WHERE id = ?2",
            params!["G:/BMS/song/Normal.bmson", refreshed_file_id],
        )
        .unwrap();
    db.conn()
        .execute(
            "INSERT INTO chart_import_warnings (chart_file_id, code, message, created_at)
             VALUES (?1, 'OldWarning', 'stale', 10)",
            params![old_file_id],
        )
        .unwrap();

    let course = course_with_entries(vec![CourseEntry {
        title_hint: "BMSON course entry".to_string(),
        md5: Some(hash_to_hex(&old.identity.file_md5)),
        sha256: Some(hash_to_hex(&old.identity.file_sha256)),
        chart_id: Some(old_id),
    }]);
    let course_id = db.upsert_course("table:test", &course, 0, 1).unwrap();
    assert_eq!(db.list_course_entries(course_id).unwrap()[0].entry.chart_id, Some(old_id));

    db.conn()
        .execute_batch(
            "ALTER TABLE charts DROP COLUMN has_defined_hln;
        ALTER TABLE charts DROP COLUMN defined_hln_pairs;",
        )
        .unwrap();
    run_migrations(db.conn_mut(), &LIBRARY_MIGRATIONS[migration_index..]).unwrap();

    let version: i32 =
        db.conn().pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
    assert_eq!(version, 34);
    assert_eq!(db.list_course_entries(course_id).unwrap()[0].entry.chart_id, Some(refreshed_id));
    assert_ne!(copy_id, refreshed_id, "a real copy at another path must remain separate");

    let counts: (i64, i64, i64, i64, i64) = db
        .conn()
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM charts),
                (SELECT COUNT(*) FROM chart_files),
                (SELECT COUNT(*) FROM chart_file_links),
                (SELECT COUNT(*) FROM chart_import_warnings),
                (SELECT COUNT(*) FROM roots)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .unwrap();
    assert_eq!(counts, (2, 2, 2, 0, 1));

    let (title, path, first_seen_at, scanned_at, root_id): (String, String, i64, i64, i64) = db
        .conn()
        .query_row(
            "SELECT charts.title, chart_files.path, chart_files.first_seen_at,
                    chart_files.scanned_at, chart_files.root_id
             FROM charts
             JOIN chart_file_links ON chart_file_links.chart_id = charts.id
             JOIN chart_files ON chart_files.id = chart_file_links.chart_file_id
             WHERE charts.id = ?1",
            params![refreshed_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .unwrap();
    assert_eq!(title, "refreshed bmson import");
    assert_eq!(path, "G:/BMS/song/Normal.bmson");
    assert_eq!(first_seen_at, 5);
    assert_eq!(scanned_at, 20);
    assert_eq!(root_id, 101);

    let (root_path, last_scan_at): (String, i64) = db
        .conn()
        .query_row("SELECT path, last_scan_at FROM roots", [], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap();
    assert_eq!(root_path, "G:/BMS");
    assert_eq!(last_scan_at, 20);
}

#[test]
fn library_migration_removes_windows_extended_path_prefixes() {
    let migration_index =
        LIBRARY_MIGRATIONS.iter().position(|migration| migration.version == 31).unwrap();
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, &LIBRARY_MIGRATIONS[..migration_index]).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);
    db.conn()
        .execute_batch(
            "ALTER TABLE charts ADD COLUMN has_defined_hln INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE charts ADD COLUMN defined_hln_pairs INTEGER NOT NULL DEFAULT 0;",
        )
        .unwrap();

    db.conn()
        .execute_batch(
            "INSERT INTO roots (id, path, enabled, recursive, last_scan_at) VALUES
                (200, '//?/C:/songs', 1, 1, 10),
                (201, 'C:/songs', 1, 1, 20),
                (202, '//?/D:/sample', 1, 1, 30);",
        )
        .unwrap();

    let old_id = db
        .upsert_chart_import(&record_for_chart("C:/placeholder/old.bms", &chart("old extended")))
        .unwrap();
    let old_file_id =
        db.chart_file_id_by_path(Path::new("C:/placeholder/old.bms")).unwrap().unwrap();
    let keep_id = db
        .upsert_chart_import(&record_for_chart("C:/placeholder/keep.bms", &chart("ordinary")))
        .unwrap();
    let keep_file_id =
        db.chart_file_id_by_path(Path::new("C:/placeholder/keep.bms")).unwrap().unwrap();
    let only_id = db
        .upsert_chart_import(&record_for_chart("D:/placeholder/only.bms", &chart("prefix only")))
        .unwrap();
    let only_file_id =
        db.chart_file_id_by_path(Path::new("D:/placeholder/only.bms")).unwrap().unwrap();

    db.conn()
        .execute(
            "UPDATE chart_files
             SET root_id = 200, path = '//?/C:/songs/track.bms',
                 scanned_at = 10, first_seen_at = 5
             WHERE id = ?1",
            [old_file_id],
        )
        .unwrap();
    db.conn()
        .execute("UPDATE charts SET folder_path = '//?/C:/songs' WHERE id = ?1", [old_id])
        .unwrap();
    db.conn()
        .execute(
            "UPDATE chart_files
             SET root_id = 201, path = 'C:/songs/track.bms',
                 scanned_at = 20, first_seen_at = 15
             WHERE id = ?1",
            [keep_file_id],
        )
        .unwrap();
    db.conn()
        .execute("UPDATE charts SET folder_path = 'C:/songs' WHERE id = ?1", [keep_id])
        .unwrap();
    db.conn()
        .execute(
            "UPDATE chart_files
             SET root_id = 202, path = '//?/D:/sample/sample.bms',
                 scanned_at = 30, first_seen_at = 25
             WHERE id = ?1",
            [only_file_id],
        )
        .unwrap();
    db.conn()
        .execute("UPDATE charts SET folder_path = '//?/D:/sample' WHERE id = ?1", [only_id])
        .unwrap();

    db.conn()
        .execute_batch(
            "ALTER TABLE charts DROP COLUMN has_defined_hln;
        ALTER TABLE charts DROP COLUMN defined_hln_pairs;",
        )
        .unwrap();
    run_migrations(db.conn_mut(), &LIBRARY_MIGRATIONS[migration_index..]).unwrap();

    let version: i32 =
        db.conn().pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
    assert_eq!(version, 34);
    let counts: (i64, i64, i64) = db
        .conn()
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM roots),
                (SELECT COUNT(*) FROM chart_files),
                (SELECT COUNT(*) FROM charts)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (2, 2, 2));

    let (path, root_id, first_seen_at): (String, i64, i64) = db
        .conn()
        .query_row(
            "SELECT path, root_id, first_seen_at FROM chart_files WHERE id = ?1",
            [keep_file_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(path, "C:/songs/track.bms");
    assert_eq!(root_id, 201);
    assert_eq!(first_seen_at, 5);
    let old_chart_count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM charts WHERE id = ?1", [old_id], |row| row.get(0))
        .unwrap();
    assert_eq!(old_chart_count, 0);

    let (root_path, last_scan_at): (String, i64) = db
        .conn()
        .query_row("SELECT path, last_scan_at FROM roots WHERE id = 201", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(root_path, "C:/songs");
    assert_eq!(last_scan_at, 20);

    let (file_path, folder_path, root_path): (String, String, String) = db
        .conn()
        .query_row(
            "SELECT chart_files.path, charts.folder_path, roots.path
             FROM chart_files
             JOIN chart_file_links ON chart_file_links.chart_file_id = chart_files.id
             JOIN charts ON charts.id = chart_file_links.chart_id
             JOIN roots ON roots.id = chart_files.root_id
             WHERE chart_files.id = ?1",
            [only_file_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(file_path, "D:/sample/sample.bms");
    assert_eq!(folder_path, "D:/sample");
    assert_eq!(root_path, "D:/sample");
}

#[test]
fn upsert_chart_import_creates_separate_charts_for_different_paths_with_same_sha256() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);

    let same_chart = chart("duplicate");
    let id_a =
        db.upsert_chart_import(&record_for_chart("/songs/a/track.bms", &same_chart)).unwrap();
    let id_b =
        db.upsert_chart_import(&record_for_chart("/songs/b/track.bms", &same_chart)).unwrap();

    assert_ne!(id_a, id_b, "different paths must produce separate chart records");

    let count: i64 = db.conn().query_row("SELECT COUNT(*) FROM charts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 2);
}

#[test]
fn charts_by_md5s_prefers_newest_chart_id() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);

    let same_chart = chart("duplicate");
    let stale_id =
        db.upsert_chart_import(&record_for_chart("/songs/a/track.bms", &same_chart)).unwrap();
    let fresh_id =
        db.upsert_chart_import(&record_for_chart("/songs/b/track.bms", &same_chart)).unwrap();
    assert!(stale_id < fresh_id);

    let md5 = hash_to_hex(&same_chart.identity.file_md5);
    let resolved = db.charts_by_md5s(&[md5.as_str()]).unwrap();

    assert_eq!(resolved.get(&md5).map(|chart| chart.chart_id), Some(fresh_id));
}

#[test]
fn charts_by_md5s_batches_more_hashes_than_one_sqlite_variable_chunk() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);

    let first = chart("batch-first");
    let second = chart("batch-second");
    let first_id = db.upsert_chart_import(&record_for_chart("/songs/first.bms", &first)).unwrap();
    let second_id =
        db.upsert_chart_import(&record_for_chart("/songs/second.bms", &second)).unwrap();
    let first_md5 = hash_to_hex(&first.identity.file_md5);
    let second_md5 = hash_to_hex(&second.identity.file_md5);

    let mut hashes = (0..CHART_HASH_LOOKUP_BATCH_SIZE * 2 + 1)
        .map(|index| format!("{index:032x}"))
        .collect::<Vec<_>>();
    hashes.push(first_md5.clone());
    hashes.push(second_md5.clone());
    hashes.push(first_md5.clone());
    let hash_refs = hashes.iter().map(String::as_str).collect::<Vec<_>>();

    let resolved = db.charts_by_md5s(&hash_refs).unwrap();

    assert_eq!(resolved.len(), 2);
    assert_eq!(resolved.get(&first_md5).map(|chart| chart.chart_id), Some(first_id));
    assert_eq!(resolved.get(&second_md5).map(|chart| chart.chart_id), Some(second_id));
}

#[test]
fn chart_analysis_summaries_batch_more_ids_than_one_sqlite_variable_chunk() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);

    let first = chart("analysis-batch-first");
    let second = chart("analysis-batch-second");
    let first_id = db.upsert_chart_import(&record_for_chart("/songs/first.bms", &first)).unwrap();
    let second_id =
        db.upsert_chart_import(&record_for_chart("/songs/second.bms", &second)).unwrap();
    let mut ids =
        (10_000..10_000 + CHART_ANALYSIS_LOOKUP_BATCH_SIZE as i64 * 2 + 1).collect::<Vec<_>>();
    ids.push(first_id);
    ids.push(second_id);
    ids.push(first_id);

    let summaries = db.chart_analysis_summaries_by_chart_ids(&ids).unwrap();

    assert_eq!(summaries.len(), 2);
    assert!(summaries.contains_key(&first_id));
    assert!(summaries.contains_key(&second_id));
}

#[test]
fn chart_file_fingerprint_reads_imported_file_metadata() {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(conn);
    let chart = chart("song");
    db.upsert_chart_import(&ChartImportRecord {
        root_id: None,
        file_path: Path::new("/songs/song.bms"),
        file_size: 123,
        modified_at: 456,
        scanned_at: 789,
        chart: &chart,
    })
    .unwrap();

    assert_eq!(
        db.chart_file_fingerprint(Path::new("/songs/song.bms")).unwrap(),
        Some(ChartFileFingerprint {
            file_size: 123,
            modified_at: 456,
            import_version: CHART_IMPORT_VERSION,
        })
    );
}
