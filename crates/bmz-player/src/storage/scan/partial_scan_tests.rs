//! Partial scans (F5 on a folder) below a configured library root.
//!
//! A configured ancestor root must not keep registrations that the partial scan
//! proved missing, while every scope the partial scan did not verify stays intact.

use std::io::Write;

use rusqlite::Connection;

use super::*;
use crate::storage::common::configure_connection;
use crate::storage::library_db::{LibraryDatabase, PruneRequest, library_path_key};
use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};

fn config() -> ScanConfig {
    ScanConfig {
        follow_symlinks: true,
        use_everything: false,
        auto_rescan_on_startup: false,
        rescan_missing_files: true,
    }
}

fn temp() -> tempfile::TempDir {
    tempfile::Builder::new().prefix("bmz-partial-scan-").tempdir().unwrap()
}

fn chart_text(title: &str) -> String {
    format!("#TITLE {title}\n#BPM 120\n#00011:01\n")
}

fn chart(path: &Path, title: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(chart_text(title).as_bytes()).unwrap();
}

fn write_zip(path: &Path, entries: &[(&str, &str)]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, text) in entries {
        writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(text.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
}

fn write_7z(path: &Path, entries: &[(&str, &str)]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut writer = sevenz_rust2::ArchiveWriter::create(path).unwrap();
    let metadata =
        entries.iter().map(|(name, _)| sevenz_rust2::ArchiveEntry::new_file(name)).collect();
    let sources = entries
        .iter()
        .map(|(_, text)| sevenz_rust2::SourceReader::new(std::io::Cursor::new(text.as_bytes())))
        .collect();
    writer.push_archive_entries(metadata, sources).unwrap();
    writer.finish().unwrap();
}

#[test]
fn multi_folder_archives_register_and_expose_charts_at_every_depth() {
    use crate::screens::select_model::reachable_chart_folders;

    let titles = [chart_text("Root"), chart_text("Shallow"), chart_text("Deep")];
    let entries = [
        ("root.bms", titles[0].as_str()),
        ("song/chart.bms", titles[1].as_str()),
        ("group/subgroup/song/chart.bms", titles[2].as_str()),
    ];
    for name in ["pack.zip", "pack.7z"] {
        let root = temp();
        let archive = root.path().join(name);
        if name.ends_with(".zip") {
            write_zip(&archive, &entries);
        } else {
            write_7z(&archive, &entries);
        }
        let mut library = db();
        // Non-recursive applies to the filesystem, not directories inside an archive.
        configure_and_scan(&mut library, &[entry(root.path(), false)]);
        assert_eq!(registered(&library, root.path()).len(), 3);
        let folder = format!("{}!", library_path_key(&archive));
        let mut reachable = reachable_chart_folders(&library, &folder).unwrap();
        reachable.sort();
        assert_eq!(
            reachable,
            [folder.clone(), format!("{folder}/group/subgroup/song"), format!("{folder}/song")]
        );
        let chart = crate::song_archive::read_chart(
            &archive,
            "group/subgroup/song/chart.bms",
            &Default::default(),
        )
        .unwrap();
        assert_eq!(chart.bytes, titles[2].as_bytes());
    }
}

fn db() -> LibraryDatabase {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    LibraryDatabase::from_connection(conn)
}

fn entry(path: &Path, recursive: bool) -> PathEntry {
    PathEntry { path: path.to_string_lossy().into_owned(), enabled: true, recursive }
}

/// Configure `roots` as the library and run the full scan the settings screen runs.
fn configure_and_scan(db: &mut LibraryDatabase, roots: &[PathEntry]) {
    db.set_configured_song_roots(roots).unwrap();
    let report = scan_song_roots(db, roots, &config(), 1, false).unwrap();
    assert!(report.discovery_issues.is_empty(), "{:?}", report.discovery_issues);
}

/// The select screen's F5: scan one folder, then remember it as a partial scope.
fn partial_scan(db: &mut LibraryDatabase, target: PathEntry, scanned_at: i64) -> ScanReport {
    let report =
        scan_song_roots(db, std::slice::from_ref(&target), &config(), scanned_at, false).unwrap();
    db.register_partial_song_roots(&[target]).unwrap();
    report
}

/// Registered paths relative to `root`, sorted, with `/` separators.
fn registered(db: &LibraryDatabase, root: &Path) -> Vec<String> {
    let mut stmt = db.conn().prepare("SELECT path FROM chart_files ORDER BY path").unwrap();
    let prefix = format!("{}/", library_path_key(root));
    stmt.query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|path| path.unwrap().strip_prefix(&prefix).unwrap().to_string())
        .collect()
}

fn count(db: &LibraryDatabase, sql: &str) -> i64 {
    db.conn().query_row(sql, [], |row| row.get(0)).unwrap()
}

/// Links, charts and warnings never outlive the file registrations they belong to.
fn assert_consistent(db: &LibraryDatabase) {
    assert_eq!(
        count(
            db,
            "SELECT COUNT(*) FROM chart_file_links
             WHERE chart_file_id NOT IN (SELECT id FROM chart_files)
                OR chart_id NOT IN (SELECT id FROM charts)"
        ),
        0
    );
    assert_eq!(
        count(
            db,
            "SELECT COUNT(*) FROM charts WHERE id NOT IN (SELECT chart_id FROM chart_file_links)"
        ),
        0
    );
    assert_eq!(
        count(
            db,
            "SELECT COUNT(*) FROM chart_import_warnings
             WHERE chart_file_id NOT IN (SELECT id FROM chart_files)"
        ),
        0
    );
}

fn last_scan_at(db: &LibraryDatabase, root: &Path) -> Option<i64> {
    db.conn()
        .query_row(
            "SELECT last_scan_at FROM roots WHERE path = ?1",
            [library_path_key(root)],
            |row| row.get(0),
        )
        .unwrap()
}

/// Issue #69: F5 on `BMS/INSANE2` while `BMS` is the configured library root.
#[test]
fn partial_scan_below_configured_root_prunes_deleted_charts() {
    let base = temp();
    let bms = base.path().join("BMS");
    let insane2 = bms.join("INSANE2");
    chart(&bms.join("INSANE1/song_X/x.bms"), "X");
    chart(&bms.join("INSANE1/song_Y/y.bms"), "Y");
    chart(&insane2.join("song_A/a.bms"), "A");
    chart(&insane2.join("song_A/a.bme"), "A another");
    chart(&insane2.join("song_B/b.bms"), "B");
    chart(&insane2.join("song_B/b.bme"), "B another");
    chart(&insane2.join("song_B/b.bml"), "B long");
    chart(&insane2.join("song_B/b.pms"), "B pop");
    chart(&insane2.join("song_D.bms"), "D");
    write_zip(&insane2.join("song_C.zip"), &[("song_C/c.bms", &chart_text("C"))]);
    chart(&bms.join("EVENT/song_E/e.bms"), "E");
    let mut db = db();
    configure_and_scan(&mut db, &[entry(&bms, true)]);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM chart_files"), 11);

    std::fs::remove_dir_all(insane2.join("song_B")).unwrap();
    std::fs::remove_file(insane2.join("song_C.zip")).unwrap();
    std::fs::remove_file(insane2.join("song_D.bms")).unwrap();
    // Outside the partial scan: it must wait for a scan that covers it.
    std::fs::remove_file(bms.join("INSANE1/song_Y/y.bms")).unwrap();

    let report = partial_scan(&mut db, entry(&insane2, true), 2);

    assert_eq!(report.summary.removed_files, 6);
    assert_eq!(
        registered(&db, &bms),
        [
            "EVENT/song_E/e.bms",
            "INSANE1/song_X/x.bms",
            "INSANE1/song_Y/y.bms",
            "INSANE2/song_A/a.bme",
            "INSANE2/song_A/a.bms",
        ]
    );
    assert_eq!(count(&db, "SELECT COUNT(*) FROM charts"), 5);
    assert_consistent(&db);
    assert_eq!(last_scan_at(&db, &insane2), Some(2));

    // The full library scan still removes what the partial scan could not verify.
    let report = scan_song_roots(&mut db, &[entry(&bms, true)], &config(), 3, false).unwrap();
    assert_eq!(report.summary.removed_files, 1);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM chart_files"), 4);
    assert_consistent(&db);
}

/// Issue #69 as a full re-parse of one folder: with `force`, every remaining chart is
/// imported again alongside the pruning. (F5 now runs the same scan without `force`.)
#[test]
fn forced_partial_scan_below_configured_root_prunes_deleted_charts() {
    let base = temp();
    let bms = base.path().join("BMS");
    let insane2 = bms.join("INSANE2");
    chart(&bms.join("INSANE1/song_X/x.bms"), "X");
    chart(&insane2.join("song_A/a.bms"), "A");
    chart(&insane2.join("song_A/a.bme"), "A another");
    chart(&insane2.join("song_B/b.bms"), "B");
    write_zip(&insane2.join("song_C.zip"), &[("song_C/c.bms", &chart_text("C"))]);
    write_zip(&insane2.join("song_K.zip"), &[("song_K/k.bms", &chart_text("K"))]);
    let mut db = db();
    configure_and_scan(&mut db, &[entry(&bms, true)]);
    let chart_ids = |db: &LibraryDatabase| -> Vec<(String, i64)> {
        let mut stmt = db
            .conn()
            .prepare(
                "SELECT chart_files.path, chart_file_links.chart_id FROM chart_files
                 JOIN chart_file_links ON chart_file_links.chart_file_id = chart_files.id
                 ORDER BY chart_files.path",
            )
            .unwrap();
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let before = chart_ids(&db);

    std::fs::remove_dir_all(insane2.join("song_B")).unwrap();
    std::fs::remove_file(insane2.join("song_C.zip")).unwrap();
    let target = entry(&insane2, true);
    let report =
        scan_song_roots(&mut db, std::slice::from_ref(&target), &config(), 2, true).unwrap();
    db.register_partial_song_roots(&[target]).unwrap();

    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.summary.imported, 3);
    assert_eq!(report.summary.skipped, 0);
    assert_eq!(report.summary.removed_files, 2);
    assert_eq!(
        registered(&db, &bms),
        [
            "INSANE1/song_X/x.bms",
            "INSANE2/song_A/a.bme",
            "INSANE2/song_A/a.bms",
            "INSANE2/song_K.zip!/song_K/k.bms",
        ]
    );
    // Re-imported charts keep their rows, so scores and favorites keep their target.
    let expected: Vec<_> = before
        .into_iter()
        .filter(|(path, _)| !path.contains("/song_B/") && !path.contains("/song_C.zip!"))
        .collect();
    assert_eq!(chart_ids(&db), expected);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM charts"), 4);
    assert_consistent(&db);
    assert_eq!(last_scan_at(&db, &insane2), Some(2));
}

#[test]
fn partial_scan_prunes_deleted_song_folder_with_several_charts() {
    let base = temp();
    let bms = base.path().join("BMS");
    let insane2 = bms.join("INSANE2");
    for name in ["7key.bms", "14key.bme", "ln.bml", "pop.pms"] {
        chart(&insane2.join("pack/song_B").join(name), name);
    }
    chart(&insane2.join("pack/song_A/a.bms"), "A");
    let mut db = db();
    configure_and_scan(&mut db, &[entry(&bms, true)]);

    std::fs::remove_dir_all(insane2.join("pack/song_B")).unwrap();
    // A deeper subfolder than the deleted song, not the configured root, is scanned.
    let report = partial_scan(&mut db, entry(&insane2.join("pack"), true), 2);

    assert_eq!(report.summary.removed_files, 4);
    assert_eq!(registered(&db, &bms), ["INSANE2/pack/song_A/a.bms"]);
    assert_consistent(&db);
}

#[test]
fn partial_scan_prunes_deleted_and_changed_archives() {
    let base = temp();
    let bms = base.path().join("BMS");
    let insane2 = bms.join("INSANE2");
    write_zip(
        &insane2.join("kept.zip"),
        &[("song/a.bms", &chart_text("A")), ("song/b.bms", &chart_text("B"))],
    );
    write_zip(&insane2.join("gone.zip"), &[("song/c.bms", &chart_text("C"))]);
    write_7z(&insane2.join("gone.7z"), &[("song/d.bms", &chart_text("D"))]);
    write_7z(&insane2.join("kept.7z"), &[("song/e.bms", &chart_text("E"))]);
    let mut db = db();
    configure_and_scan(&mut db, &[entry(&bms, true)]);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM chart_files"), 5);

    std::fs::remove_file(insane2.join("gone.zip")).unwrap();
    std::fs::remove_file(insane2.join("gone.7z")).unwrap();
    // One chart deleted from an archive that still exists, another updated.
    write_zip(&insane2.join("kept.zip"), &[("song/a.bms", &chart_text("A updated"))]);

    let report = partial_scan(&mut db, entry(&insane2, true), 2);

    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.summary.removed_files, 3);
    assert_eq!(
        registered(&db, &bms),
        ["INSANE2/kept.7z!/song/e.bms", "INSANE2/kept.zip!/song/a.bms"]
    );
    let titles: Vec<String> = {
        let mut stmt = db.conn().prepare("SELECT title FROM charts ORDER BY title").unwrap();
        stmt.query_map([], |row| row.get(0)).unwrap().map(Result::unwrap).collect()
    };
    assert_eq!(titles, ["A updated", "E"]);
    assert_consistent(&db);
}

#[test]
fn partial_scan_keeps_chart_shared_with_a_file_outside_the_scan() {
    let base = temp();
    let bms = base.path().join("BMS");
    chart(&bms.join("INSANE1/dup/song.bms"), "Same");
    chart(&bms.join("INSANE2/dup/song.bms"), "Same");
    write_zip(&bms.join("INSANE2/dup.zip"), &[("song.bms", &chart_text("Same"))]);
    let mut db = db();
    configure_and_scan(&mut db, &[entry(&bms, true)]);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM chart_files"), 3);
    assert_eq!(count(&db, "SELECT COUNT(DISTINCT sha256) FROM chart_files"), 1);
    // Every file keeps its own chart row; remember the one the survivor links to.
    let survivor = library_path_key(&bms.join("INSANE1/dup/song.bms"));
    let linked = |db: &LibraryDatabase| -> (i64, String, String) {
        db.conn()
            .query_row(
                "SELECT charts.id, charts.sha256, chart_files.sha256 FROM charts
                 JOIN chart_file_links ON chart_file_links.chart_id = charts.id
                 JOIN chart_files ON chart_files.id = chart_file_links.chart_file_id
                 WHERE chart_files.path = ?1",
                [&survivor],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap()
    };
    let before = linked(&db);
    assert_eq!(before.1, before.2);
    // The survivor inherits the earliest "new song" date of the removed copies.
    db.conn()
        .execute("UPDATE chart_files SET first_seen_at = 5 WHERE path = ?1", [&survivor])
        .unwrap();

    std::fs::remove_dir_all(bms.join("INSANE2/dup")).unwrap();
    std::fs::remove_file(bms.join("INSANE2/dup.zip")).unwrap();
    let report = partial_scan(&mut db, entry(&bms.join("INSANE2"), true), 2);

    assert_eq!(report.summary.removed_files, 2);
    assert_eq!(registered(&db, &bms), ["INSANE1/dup/song.bms"]);
    // The chart row, its id and its hash survive through the remaining file.
    assert_eq!(linked(&db), before);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM charts"), 1);
    assert_eq!(count(&db, "SELECT first_seen_at FROM chart_files"), 1);
    assert_consistent(&db);
}

#[test]
fn shallow_partial_scan_keeps_descendants_it_did_not_read() {
    let base = temp();
    let bms = base.path().join("BMS");
    let insane2 = bms.join("INSANE2");
    chart(&insane2.join("direct.bms"), "Direct");
    chart(&insane2.join("kept.bms"), "Kept");
    chart(&insane2.join("song_B/nested.bms"), "Nested");
    write_zip(&insane2.join("song_B/nested.zip"), &[("n.bms", &chart_text("Nested zip"))]);
    let mut db = db();
    configure_and_scan(&mut db, &[entry(&bms, true)]);

    std::fs::remove_file(insane2.join("direct.bms")).unwrap();
    std::fs::remove_dir_all(insane2.join("song_B")).unwrap();
    let report = partial_scan(&mut db, entry(&insane2, false), 2);

    assert_eq!(report.summary.removed_files, 1);
    assert_eq!(
        registered(&db, &bms),
        ["INSANE2/kept.bms", "INSANE2/song_B/nested.bms", "INSANE2/song_B/nested.zip!/n.bms"]
    );
    assert_consistent(&db);
}

#[test]
fn partial_scan_of_one_root_keeps_independent_and_nested_roots() {
    let base = temp();
    let first = base.path().join("first");
    let second = base.path().join("second");
    let nested = first.join("nested");
    chart(&first.join("a.bms"), "A");
    chart(&first.join("gone.bms"), "Gone");
    chart(&nested.join("n.bms"), "Nested");
    chart(&second.join("b.bms"), "B");
    chart(&second.join("gone.bms"), "Gone too");
    let mut db = db();
    // `nested` is configured on its own (non-recursive), inside `first`.
    configure_and_scan(
        &mut db,
        &[entry(&first, true), entry(&second, true), entry(&nested, false)],
    );

    std::fs::remove_file(first.join("gone.bms")).unwrap();
    std::fs::remove_file(nested.join("n.bms")).unwrap();
    std::fs::remove_file(second.join("gone.bms")).unwrap();
    let report = partial_scan(&mut db, entry(&first, true), 2);

    // A scan of `first` cannot speak for `second`, nor for the unscanned root
    // configured below it.
    assert_eq!(report.summary.removed_files, 1);
    assert_eq!(
        registered(&db, base.path()),
        ["first/a.bms", "first/nested/n.bms", "second/b.bms", "second/gone.bms"]
    );
    assert_consistent(&db);
}

#[test]
fn incomplete_partial_scan_keeps_unverified_registrations() {
    let base = temp();
    let bms = base.path().join("BMS");
    let insane2 = bms.join("INSANE2");
    chart(&insane2.join("song_A/a.bms"), "A");
    chart(&insane2.join("song_B/b.bms"), "B");
    let mut db = db();
    configure_and_scan(&mut db, &[entry(&bms, true)]);

    std::fs::remove_dir_all(insane2.join("song_B")).unwrap();
    // An archive that cannot be read makes discovery of INSANE2 incomplete.
    std::fs::write(insane2.join("broken.zip"), b"not an archive").unwrap();
    let report = partial_scan(&mut db, entry(&insane2, true), 2);

    assert!(!report.discovery_issues.is_empty());
    assert_eq!(report.summary.removed_files, 0);
    assert_eq!(registered(&db, &bms), ["INSANE2/song_A/a.bms", "INSANE2/song_B/b.bms"]);
    assert_eq!(last_scan_at(&db, &insane2), None);

    std::fs::remove_file(insane2.join("broken.zip")).unwrap();
    let report = partial_scan(&mut db, entry(&insane2, true), 3);
    assert_eq!(report.summary.removed_files, 1);
    assert_eq!(registered(&db, &bms), ["INSANE2/song_A/a.bms"]);
}

#[cfg(unix)]
#[test]
fn inaccessible_folder_in_partial_scan_keeps_unverified_registrations() {
    use std::os::unix::fs::PermissionsExt;
    // Root ignores permission bits, so the folder would stay readable.
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("skipping: permission bits are not enforced for root");
        return;
    }
    let base = temp();
    let bms = base.path().join("BMS");
    let insane2 = bms.join("INSANE2");
    chart(&insane2.join("song_A/a.bms"), "A");
    chart(&insane2.join("song_B/b.bms"), "B");
    chart(&insane2.join("locked/c.bms"), "C");
    let mut db = db();
    configure_and_scan(&mut db, &[entry(&bms, true)]);

    std::fs::remove_dir_all(insane2.join("song_B")).unwrap();
    let locked = insane2.join("locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let result = scan_song_roots(&mut db, &[entry(&insane2, true)], &config(), 2, false);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    let report = result.unwrap();

    assert_eq!(report.summary.discovery_skipped, 1);
    assert_eq!(report.summary.removed_files, 0);
    assert_eq!(
        registered(&db, &bms),
        ["INSANE2/locked/c.bms", "INSANE2/song_A/a.bms", "INSANE2/song_B/b.bms"]
    );
}

#[test]
fn unreadable_partial_scan_target_keeps_registrations() {
    let base = temp();
    let bms = base.path().join("BMS");
    let insane2 = bms.join("INSANE2");
    chart(&insane2.join("song_A/a.bms"), "A");
    let mut db = db();
    configure_and_scan(&mut db, &[entry(&bms, true)]);

    std::fs::rename(&insane2, base.path().join("offline")).unwrap();
    let report = partial_scan(&mut db, entry(&insane2, true), 2);

    assert_eq!(report.summary.roots_unreadable, 1);
    assert_eq!(report.summary.removed_files, 0);
    assert_eq!(registered(&db, &bms), ["INSANE2/song_A/a.bms"]);
}

/// Dot-named charts are ordinary charts: discovered, kept while present, and pruned.
#[test]
fn partial_scan_treats_dot_named_charts_like_others() {
    let base = temp();
    let bms = base.path().join("BMS");
    let insane2 = bms.join("INSANE2");
    chart(&insane2.join(".hidden/song.bms"), "Hidden");
    chart(&insane2.join(".gone.bms"), "Hidden gone");
    write_zip(&insane2.join("pack.zip"), &[(".hidden/z.bms", &chart_text("Hidden zip"))]);
    let mut db = db();
    configure_and_scan(&mut db, &[entry(&bms, true)]);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM chart_files"), 3);

    std::fs::remove_file(insane2.join(".gone.bms")).unwrap();
    let report = partial_scan(&mut db, entry(&insane2, true), 2);

    assert_eq!(report.summary.files_seen, 2);
    assert_eq!(report.summary.removed_files, 1);
    assert_eq!(
        registered(&db, &bms),
        ["INSANE2/.hidden/song.bms", "INSANE2/pack.zip!/.hidden/z.bms"]
    );

    write_zip(&insane2.join("pack.zip"), &[]);
    let report = partial_scan(&mut db, entry(&insane2, true), 3);
    assert_eq!(report.summary.removed_files, 1);
    assert_eq!(registered(&db, &bms), ["INSANE2/.hidden/song.bms"]);
}

#[test]
fn archive_rewritten_during_partial_scan_keeps_registrations() {
    let base = temp();
    let bms = base.path().join("BMS");
    let insane2 = bms.join("INSANE2");
    let pack = insane2.join("pack.zip");
    let text = chart_text("Song");
    write_zip(&pack, &[("a.bms", &text), ("b.bms", &text)]);
    chart(&insane2.join("song_B/b.bms"), "B");
    let mut db = db();
    configure_and_scan(&mut db, &[entry(&bms, true)]);

    std::fs::remove_dir_all(insane2.join("song_B")).unwrap();
    write_zip(&pack, &[("a.bms", &text)]);
    let mut replaced = false;
    let target = entry(&insane2, true);
    let report = scan_song_roots_with_progress(
        &mut db,
        std::slice::from_ref(&target),
        &config(),
        2,
        false,
        |progress| {
            // After the archive was imported, before its generation is checked.
            if !replaced && progress.done == 1 && progress.total == 1 {
                write_zip(&pack, &[("a.bms", &text), ("b.bms", &text)]);
                replaced = true;
            }
        },
    )
    .unwrap();
    db.register_partial_song_roots(&[target]).unwrap();

    assert!(replaced);
    assert!(!report.failures.is_empty());
    assert_eq!(report.summary.removed_files, 0);
    assert_eq!(
        registered(&db, &bms),
        ["INSANE2/pack.zip!/a.bms", "INSANE2/pack.zip!/b.bms", "INSANE2/song_B/b.bms"]
    );
    assert_eq!(last_scan_at(&db, &insane2), None);
}

/// Windows spellings of the configured root and the scanned folder, checked
/// without touching the disk: the rows point at paths that do not exist.
#[test]
fn enclosing_root_spellings_do_not_block_pruning() {
    let base = temp();
    let source = base.path().join("source");
    for name in ["a", "b", "c", "d"] {
        chart(&source.join(format!("{name}.bms")), name);
    }
    let mut db = db();
    scan_song_roots(&mut db, &[entry(&source, true)], &config(), 1, false).unwrap();
    let drive = "C:/bmz-issue69-missing/BMS";
    for (name, path) in [
        ("a", format!("{drive}/INSANE2/song_B/a.bms")),
        ("b", format!("{drive}/INSANE2/b.bms")),
        ("c", format!("{drive}/INSANE2/protected/c.bms")),
        ("d", format!("{drive}/INSANE2-old/d.bms")),
    ] {
        db.conn()
            .execute(
                "UPDATE chart_files SET path = ?1 WHERE path = ?2",
                [path, library_path_key(&source.join(format!("{name}.bms")))],
            )
            .unwrap();
    }
    let protect = |path: &str, recursive| PathEntry { path: path.into(), enabled: true, recursive };
    let mut protected_roots = vec![
        protect(r"C:\bmz-issue69-missing\BMS\", true),
        protect(r"\\?\C:\bmz-issue69-missing\BMS", true),
        // Non-recursive ancestors cover none of the scanned folder's charts.
        protect(r"C:\bmz-issue69-missing\BMS", false),
        protect(r"C:\bmz-issue69-missing", false),
    ];
    #[cfg(windows)]
    protected_roots.push(protect(r"c:\BMZ-ISSUE69-MISSING\bms", true));
    // A configured root below the scanned folder still owns its registrations.
    protected_roots.push(protect(r"C:\bmz-issue69-missing\BMS\INSANE2\protected", true));
    let discovered = HashSet::new();

    let mut index = db.chart_path_index().unwrap();
    let removed = db
        .prune_missing_chart_files(
            &mut index,
            &PruneRequest {
                root: Path::new(r"\\?\C:\bmz-issue69-missing\BMS\INSANE2"),
                recursive: true,
                omission_proves_absence: true,
                discovered_any: &discovered,
                protected_roots: &protected_roots,
            },
        )
        .unwrap();

    assert_eq!(removed, 2);
    let mut stmt = db.conn().prepare("SELECT path FROM chart_files ORDER BY path").unwrap();
    let paths: Vec<String> =
        stmt.query_map([], |row| row.get(0)).unwrap().map(Result::unwrap).collect();
    assert_eq!(
        paths,
        [format!("{drive}/INSANE2-old/d.bms"), format!("{drive}/INSANE2/protected/c.bms")]
    );
}

/// UNC shares resolve to `//server/share`, whichever way they are spelled.
#[cfg(not(windows))]
#[test]
fn enclosing_unc_root_does_not_block_pruning() {
    let base = temp();
    let source = base.path().join("source");
    chart(&source.join("a.bms"), "A");
    chart(&source.join("b.bms"), "B");
    let mut db = db();
    scan_song_roots(&mut db, &[entry(&source, true)], &config(), 1, false).unwrap();
    for (name, path) in [
        ("a", "//bmz-issue69/share/BMS/INSANE2/a.bms"),
        ("b", "//bmz-issue69/share/BMS/EVENT/b.bms"),
    ] {
        db.conn()
            .execute(
                "UPDATE chart_files SET path = ?1 WHERE path = ?2",
                [path.to_string(), library_path_key(&source.join(format!("{name}.bms")))],
            )
            .unwrap();
    }
    let protected_roots =
        [PathEntry { path: r"\\bmz-issue69\share\BMS".into(), enabled: true, recursive: true }];
    let discovered = HashSet::new();

    let mut index = db.chart_path_index().unwrap();
    let removed = db
        .prune_missing_chart_files(
            &mut index,
            &PruneRequest {
                root: Path::new(r"\\?\UNC\bmz-issue69\share\BMS\INSANE2"),
                recursive: true,
                omission_proves_absence: true,
                discovered_any: &discovered,
                protected_roots: &protected_roots,
            },
        )
        .unwrap();

    assert_eq!(removed, 1);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM chart_files"), 1);
}
