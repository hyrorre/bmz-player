//! Discovery through links, deterministic link choice and pruning after discovery.

use std::io::Write;

use rusqlite::Connection;

use super::*;
use crate::storage::common::configure_connection;
use crate::storage::library_db::{LibraryDatabase, library_path_key};
use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};

fn config(follow_symlinks: bool) -> ScanConfig {
    ScanConfig {
        follow_symlinks,
        skip_hidden: true,
        use_everything: false,
        auto_rescan_on_startup: false,
        rescan_missing_files: true,
    }
}

fn temp() -> tempfile::TempDir {
    tempfile::Builder::new().prefix("bmz-scan-links-").tempdir().unwrap()
}

fn chart(path: &Path, title: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let mut file = std::fs::File::create(path).unwrap();
    write!(file, "#TITLE {title}\n#BPM 120\n#00011:01\n").unwrap();
}

fn db() -> LibraryDatabase {
    let mut conn = Connection::open_in_memory().unwrap();
    configure_connection(&conn).unwrap();
    run_migrations(&mut conn, LIBRARY_MIGRATIONS).unwrap();
    LibraryDatabase::from_connection(conn)
}

fn roots(root: &Path, recursive: bool) -> Vec<PathEntry> {
    vec![PathEntry { path: root.to_string_lossy().into_owned(), enabled: true, recursive }]
}

/// Discovered paths relative to `root`, sorted, with `/` separators.
fn discovered(root: &Path, recursive: bool, config: &ScanConfig) -> (Vec<String>, ChartDiscovery) {
    let discovery = discover_chart_files_with_progress(root, recursive, config, |_| {});
    let mut paths: Vec<String> = discovery
        .entries
        .iter()
        .map(|entry| {
            let key = library_path_key(&entry.path);
            key.strip_prefix(&format!("{}/", library_path_key(root))).unwrap().to_string()
        })
        .collect();
    paths.sort();
    (paths, discovery)
}

fn registered(db: &LibraryDatabase, root: &Path) -> Vec<String> {
    let mut stmt = db.conn().prepare("SELECT path FROM chart_files ORDER BY path").unwrap();
    let prefix = format!("{}/", library_path_key(root));
    stmt.query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(|path| path.unwrap().strip_prefix(&prefix).unwrap().to_string())
        .collect()
}

fn count(db: &LibraryDatabase, table: &str) -> i64 {
    db.conn().query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0)).unwrap()
}

#[cfg(unix)]
fn link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(unix)]
#[test]
fn link_to_an_ancestor_of_the_root_is_not_followed() {
    let base = temp();
    let root = base.path().join("music/songs/root");
    chart(&root.join("a/a.bms"), "A");
    chart(&base.path().join("music/other/x.bms"), "Outside");
    link(&base.path().join("music"), &root.join("up"));
    link(&root, &root.join("a/self"));

    let (paths, discovery) = discovered(&root, true, &config(true));

    assert_eq!(paths, ["a/a.bms"]);
    assert!(discovery.complete);
}

#[cfg(unix)]
#[test]
fn link_to_a_directory_inside_the_root_keeps_the_real_path() {
    let base = temp();
    let root = base.path().join("root");
    chart(&root.join("inner/a.bms"), "A");
    link(&root.join("inner"), &root.join("alias"));
    link(&root.join("inner"), &root.join("inner/again"));
    // A file link to a chart that is read through a real folder.
    link(&root.join("inner/a.bms"), &root.join("a-link.bms"));

    let (paths, discovery) = discovered(&root, true, &config(true));

    assert_eq!(paths, ["inner/a.bms"]);
    assert!(discovery.complete);
}

#[cfg(unix)]
#[test]
fn overlapping_external_links_read_every_folder_once() {
    let base = temp();
    let ext = base.path().join("ext");
    chart(&ext.join("c.bms"), "C");
    chart(&ext.join("sub/b.bms"), "B");

    // The descendant is chosen first; the ancestor is then read without it.
    let root = base.path().join("root1");
    std::fs::create_dir_all(&root).unwrap();
    link(&ext.join("sub"), &root.join("l1"));
    link(&ext, &root.join("l2"));
    assert_eq!(discovered(&root, true, &config(true)).0, ["l1/b.bms", "l2/c.bms"]);

    // The ancestor is chosen first; the descendant link is then already covered.
    let root = base.path().join("root2");
    std::fs::create_dir_all(&root).unwrap();
    link(&ext, &root.join("a"));
    link(&ext.join("sub"), &root.join("b"));
    assert_eq!(discovered(&root, true, &config(true)).0, ["a/c.bms", "a/sub/b.bms"]);

    // A link found inside an adopted tree that points at its ancestor.
    let root = base.path().join("root3");
    std::fs::create_dir_all(&root).unwrap();
    link(&ext.join("sub"), &root.join("a"));
    link(&ext, &ext.join("sub/parent"));
    assert_eq!(discovered(&root, true, &config(true)).0, ["a/b.bms", "a/parent/c.bms"]);
}

#[cfg(unix)]
#[test]
fn links_to_the_same_folder_choose_the_same_path_every_scan() {
    let base = temp();
    let ext = base.path().join("ext");
    chart(&ext.join("song.bms"), "Shared");
    let root = base.path().join("root");
    std::fs::create_dir_all(root.join("deep")).unwrap();
    // Created in an order unrelated to the expected choice.
    link(&ext, &root.join("linkB"));
    link(&ext, &root.join("deep/linkA"));
    link(&ext, &root.join("linkA"));

    for _ in 0..20 {
        assert_eq!(discovered(&root, true, &config(true)).0, ["linkA/song.bms"]);
    }
    // A different thread count must not change the choice either.
    let pool = rayon::ThreadPoolBuilder::new().num_threads(1).build().unwrap();
    pool.install(|| assert_eq!(discovered(&root, true, &config(true)).0, ["linkA/song.bms"]));
}

#[cfg(unix)]
#[test]
fn cyclic_links_terminate_without_duplicates_or_incomplete_scans() {
    let base = temp();
    let root = base.path().join("root");
    let ext = base.path().join("ext");
    chart(&root.join("r.bms"), "Root");
    chart(&ext.join("e.bms"), "Ext");
    chart(&ext.join("nested/n.bms"), "Nested");
    link(&ext, &root.join("ext"));
    link(&root, &ext.join("back"));
    link(&ext, &ext.join("nested/self"));
    // Links that only resolve through each other.
    link(&root.join("loop-b"), &root.join("loop-a"));
    link(&root.join("loop-a"), &root.join("loop-b"));

    let (paths, discovery) = discovered(&root, true, &config(true));

    assert_eq!(paths, ["ext/e.bms", "ext/nested/n.bms", "r.bms"]);
    assert!(discovery.complete);
    assert!(discovery.issues.is_empty());
}

#[cfg(unix)]
#[test]
fn real_folder_wins_over_a_link_reaching_the_same_charts() {
    let base = temp();
    let root = base.path().join("root");
    let ext = base.path().join("ext");
    chart(&root.join("real/a.bms"), "A");
    chart(&ext.join("e.bms"), "E");
    link(&ext, &root.join("ext"));
    link(&root.join("real"), &ext.join("to-real"));
    link(&root.join("real/a.bms"), &ext.join("a.bms"));

    assert_eq!(discovered(&root, true, &config(true)).0, ["ext/e.bms", "real/a.bms"]);
}

#[cfg(unix)]
#[test]
fn repeated_scans_do_not_accumulate_aliases_and_keep_chart_identity() {
    let base = temp();
    let ext = base.path().join("ext");
    chart(&ext.join("song.bms"), "Shared");
    let root = base.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    let mut db = db();

    // An older scan registered the chart through linkB.
    link(&ext, &root.join("linkB"));
    scan_song_roots(&mut db, &roots(&root, true), &config(true), 100, false).unwrap();
    assert_eq!(registered(&db, &root), ["linkB/song.bms"]);
    let sha: String =
        db.conn().query_row("SELECT sha256 FROM charts", [], |row| row.get(0)).unwrap();

    // linkA now sorts first; the alias is replaced, not kept beside it.
    link(&ext, &root.join("linkA"));
    for scanned_at in 200..205 {
        scan_song_roots(&mut db, &roots(&root, true), &config(true), scanned_at, false).unwrap();
        assert_eq!(registered(&db, &root), ["linkA/song.bms"]);
        assert_eq!(count(&db, "chart_files"), 1);
        assert_eq!(count(&db, "charts"), 1);
        assert_eq!(count(&db, "chart_file_links"), 1);
    }
    let (kept_sha, first_seen_at): (String, i64) = db
        .conn()
        .query_row(
            "SELECT charts.sha256, chart_files.first_seen_at FROM charts
             JOIN chart_file_links ON chart_file_links.chart_id = charts.id
             JOIN chart_files ON chart_files.id = chart_file_links.chart_file_id",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    // Scores and favorites are keyed by this hash, and "new songs" by first_seen_at.
    assert_eq!(kept_sha, sha);
    assert_eq!(first_seen_at, 100);
}

#[test]
fn same_hash_at_two_real_paths_stays_registered_twice() {
    let base = temp();
    let root = base.path().join("root");
    chart(&root.join("a/song.bms"), "Same");
    chart(&root.join("b/song.bms"), "Same");
    let mut db = db();

    for scanned_at in 1..4 {
        scan_song_roots(&mut db, &roots(&root, true), &config(true), scanned_at, false).unwrap();
    }

    assert_eq!(registered(&db, &root), ["a/song.bms", "b/song.bms"]);
}

#[cfg(unix)]
#[test]
fn dangling_links_do_not_stop_pruning_deleted_charts() {
    let base = temp();
    let root = base.path().join("root");
    let ext = base.path().join("ext");
    chart(&root.join("a.bms"), "A");
    chart(&root.join("b.bms"), "B");
    chart(&ext.join("target.bms"), "Target");
    link(&ext.join("target.bms"), &root.join("linked.bms"));
    link(&base.path().join("missing-folder"), &root.join("dead"));
    let mut db = db();
    scan_song_roots(&mut db, &roots(&root, true), &config(true), 1, false).unwrap();
    assert_eq!(registered(&db, &root), ["a.bms", "b.bms", "linked.bms"]);

    std::fs::remove_file(root.join("b.bms")).unwrap();
    std::fs::remove_file(ext.join("target.bms")).unwrap();
    let report = scan_song_roots(&mut db, &roots(&root, true), &config(true), 2, false).unwrap();

    assert!(report.discovery_issues.is_empty());
    assert_eq!(report.summary.removed_files, 2);
    assert_eq!(registered(&db, &root), ["a.bms"]);
    let last_scan_at: Option<i64> =
        db.conn().query_row("SELECT last_scan_at FROM roots", [], |row| row.get(0)).unwrap();
    assert_eq!(last_scan_at, Some(2));
}

#[cfg(unix)]
#[test]
fn inaccessible_link_target_keeps_existing_registrations() {
    use std::os::unix::fs::PermissionsExt;
    // Root ignores permission bits, so the folder would stay readable.
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("skipping: permission bits are not enforced for root");
        return;
    }
    let base = temp();
    let root = base.path().join("root");
    let ext = base.path().join("ext");
    chart(&root.join("a.bms"), "A");
    chart(&root.join("b.bms"), "B");
    chart(&ext.join("e.bms"), "E");
    link(&ext, &root.join("ext"));
    let mut db = db();
    scan_song_roots(&mut db, &roots(&root, true), &config(true), 1, false).unwrap();
    assert_eq!(registered(&db, &root), ["a.bms", "b.bms", "ext/e.bms"]);

    std::fs::remove_file(root.join("b.bms")).unwrap();
    std::fs::set_permissions(&ext, std::fs::Permissions::from_mode(0o000)).unwrap();
    let result = scan_song_roots(&mut db, &roots(&root, true), &config(true), 2, false);
    std::fs::set_permissions(&ext, std::fs::Permissions::from_mode(0o755)).unwrap();
    let report = result.unwrap();

    assert_eq!(report.summary.discovery_skipped, 1);
    assert_eq!(report.summary.removed_files, 0);
    assert_eq!(registered(&db, &root), ["a.bms", "b.bms", "ext/e.bms"]);
}

#[test]
fn chart_deleted_after_discovery_is_pruned() {
    let base = temp();
    let root = base.path().join("root");
    chart(&root.join("a.bms"), "A");
    chart(&root.join("b.bms"), "B");
    let mut db = db();
    scan_song_roots(&mut db, &roots(&root, true), &config(true), 1, false).unwrap();
    chart(&root.join("new.bms"), "New");

    // The progress report that closes discovery is the hook: all three charts were
    // discovered, then the unchanged b.bms and the new new.bms disappear.
    let mut deleted = false;
    let report = scan_song_roots_with_progress(
        &mut db,
        &roots(&root, true),
        &config(true),
        2,
        false,
        |progress| {
            if !deleted && progress.done == 0 && progress.total == 3 {
                std::fs::remove_file(root.join("b.bms")).unwrap();
                std::fs::remove_file(root.join("new.bms")).unwrap();
                deleted = true;
            }
        },
    )
    .unwrap();

    assert!(deleted);
    assert_eq!(report.summary.files_seen, 3);
    assert_eq!(registered(&db, &root), ["a.bms"]);
}

#[cfg(unix)]
#[test]
fn links_are_ignored_without_follow_symlinks() {
    let base = temp();
    let root = base.path().join("root");
    let ext = base.path().join("ext");
    chart(&root.join("a.bms"), "A");
    chart(&ext.join("e.bms"), "E");
    chart(&base.path().join("other/o.bms"), "O");
    link(&ext, &root.join("ext"));
    link(&base.path().join("other/o.bms"), &root.join("o-link.bms"));
    link(&base.path().join("missing"), &root.join("dead"));

    let (paths, discovery) = discovered(&root, true, &config(false));

    assert_eq!(paths, ["a.bms"]);
    assert!(discovery.complete);
    // A registration through a link is out of scope once links are not followed.
    let mut db = db();
    scan_song_roots(&mut db, &roots(&root, true), &config(true), 1, false).unwrap();
    assert_eq!(registered(&db, &root), ["a.bms", "ext/e.bms", "o-link.bms"]);
    scan_song_roots(&mut db, &roots(&root, true), &config(false), 2, false).unwrap();
    assert_eq!(registered(&db, &root), ["a.bms"]);
}

#[cfg(unix)]
#[test]
fn shallow_scan_follows_file_links_but_not_folders() {
    let base = temp();
    let root = base.path().join("root");
    let ext = base.path().join("ext");
    chart(&root.join("a.bms"), "A");
    chart(&root.join("sub/s.bms"), "Sub");
    chart(&ext.join("e.bms"), "E");
    chart(&base.path().join("other/f.bms"), "F");
    link(&ext, &root.join("ext"));
    link(&base.path().join("other/f.bms"), &root.join("f.bms"));

    assert_eq!(discovered(&root, false, &config(true)).0, ["a.bms", "f.bms"]);

    // A shallow scan neither registers nor prunes deeper charts.
    let mut db = db();
    scan_song_roots(&mut db, &roots(&root, true), &config(true), 1, false).unwrap();
    assert_eq!(registered(&db, &root), ["a.bms", "ext/e.bms", "f.bms", "sub/s.bms"]);
    std::fs::remove_file(root.join("sub/s.bms")).unwrap();
    scan_song_roots(&mut db, &roots(&root, false), &config(true), 2, false).unwrap();
    assert_eq!(registered(&db, &root), ["a.bms", "ext/e.bms", "f.bms", "sub/s.bms"]);
}

fn write_zip(path: &Path, entries: &[(&str, &str)]) {
    let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, text) in entries {
        writer.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(text.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
}

#[test]
fn archive_entries_and_archives_are_pruned_only_when_gone() {
    let base = temp();
    let root = base.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    let pack = root.join("pack.zip");
    let chart_text = |title: &str| format!("#TITLE {title}\n#BPM 120\n#00011:01\n");
    write_zip(&pack, &[("song/a.bms", &chart_text("A")), ("song/b.bms", &chart_text("B"))]);
    chart(&root.join("plain.bms"), "Plain");
    let mut db = db();
    scan_song_roots(&mut db, &roots(&root, true), &config(true), 1, false).unwrap();
    assert_eq!(
        registered(&db, &root),
        ["pack.zip!/song/a.bms", "pack.zip!/song/b.bms", "plain.bms"]
    );

    // Unchanged archives are confirmed without being re-read.
    scan_song_roots(&mut db, &roots(&root, true), &config(true), 2, false).unwrap();
    assert_eq!(count(&db, "chart_files"), 3);

    write_zip(&pack, &[("song/a.bms", &chart_text("A"))]);
    scan_song_roots(&mut db, &roots(&root, true), &config(true), 3, false).unwrap();
    assert_eq!(registered(&db, &root), ["pack.zip!/song/a.bms", "plain.bms"]);

    std::fs::remove_file(&pack).unwrap();
    scan_song_roots(&mut db, &roots(&root, true), &config(true), 4, false).unwrap();
    assert_eq!(registered(&db, &root), ["plain.bms"]);
}

#[cfg(unix)]
#[test]
fn linked_archive_is_read_once() {
    let base = temp();
    let root = base.path().join("root");
    std::fs::create_dir_all(&root).unwrap();
    write_zip(&root.join("pack.zip"), &[("a.bms", "#TITLE A\n#BPM 120\n#00011:01\n")]);
    link(&root.join("pack.zip"), &root.join("alias.zip"));

    assert_eq!(discovered(&root, true, &config(true)).0, ["pack.zip!/a.bms"]);
}

#[test]
fn write_error_stops_the_parse_pipeline_and_keeps_earlier_batches() {
    let base = temp();
    let root = base.path().join("root");
    let total = super::import::IMPORT_BATCH_SIZE * 3;
    for index in 0..total {
        chart(&root.join(format!("song{index:04}.bms")), &format!("Song {index}"));
    }
    let mut db = db();
    // Fails the second batch, whichever charts it holds.
    db.conn()
        .execute_batch(
            "CREATE TEMP TRIGGER fail_after_300 BEFORE INSERT ON main.chart_files
             WHEN (SELECT COUNT(*) FROM main.chart_files) >= 300
             BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;",
        )
        .unwrap();

    let error = scan_song_roots(&mut db, &roots(&root, true), &config(true), 1, false)
        .expect_err("the injected failure must surface");

    assert!(format!("{error:#}").contains("injected write failure"));
    // Whole batches only: the first commits, the failing one rolls back.
    assert_eq!(count(&db, "chart_files"), super::import::IMPORT_BATCH_SIZE as i64);
    let last_scan_at: Option<i64> =
        db.conn().query_row("SELECT last_scan_at FROM roots", [], |row| row.get(0)).unwrap();
    assert_eq!(last_scan_at, None);
}
