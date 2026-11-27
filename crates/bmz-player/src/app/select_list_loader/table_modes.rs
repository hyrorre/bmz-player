use super::*;
use crate::bootstrap::profile_tests::ProfileTestDir;
use crate::config::app_config::DifficultyTableSource;
use crate::difficulty_table::FetchedTableEntry;
use crate::song_download::test_support::HttpFixture;
use crate::table_cmd::{TableFetchDownloadOutcome, download_table_urls_with_progress};
use serde_json::json;
use sha2::{Digest, Sha256};

fn enable_table(boot: &mut BootstrappedApp, source_url: &str) {
    boot.app_config.tables.auto_fetch_on_startup = false;
    boot.app_config.tables.sources =
        vec![DifficultyTableSource { url: source_url.into(), enabled: true }];
}

fn level_request(boot: &BootstrappedApp, source_url: &str, level: &str) -> SelectListRequest {
    let mut request = tests::request(boot, SelectListAction::Refresh);
    request.folder_stack = vec![format!("{TABLE_ROOT_PATH}{source_url}\n{level}")];
    request.mode = SelectModeFilter::K7;
    request
}

fn names(list: &PreparedSelectList) -> Vec<String> {
    list.items.iter().map(SelectItem::display_name).collect()
}

#[test]
fn stored_table_modes_filter_select_and_follow_library_changes() {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let root = boot.app_paths.data_dir.join("mode-fixtures");
    std::fs::create_dir(&root).unwrap();
    boot.app_config.songs.roots = vec![PathEntry {
        path: root.to_string_lossy().into_owned(),
        enabled: true,
        recursive: true,
    }];
    boot.app_config.scan.use_everything = false;

    let chart_file = |title: &str, mode_hint: &str| {
        json!({
            "version": "1.0.0",
            "info": {"title": title, "artist": "", "genre": "", "mode_hint": mode_hint,
                     "level": 1, "init_bpm": 120, "resolution": 240},
            "sound_channels": [{"name": "key.wav", "notes": [{"x": 1, "y": 0, "l": 0, "c": false}]}]
        })
        .to_string()
    };
    let seven = chart_file("Z Owned 7K", "beat-7k");
    let fourteen = chart_file("Y Owned 14K", "beat-14k");
    let later = chart_file("X Downloaded 7K", "beat-7k");
    std::fs::write(root.join("seven.bmson"), &seven).unwrap();
    std::fs::write(root.join("fourteen.bmson"), &fourteen).unwrap();
    let scan = |boot: &mut BootstrappedApp| {
        crate::storage::scan::scan_song_roots(
            &mut boot.library_db,
            &boot.app_config.songs.roots,
            &boot.app_config.scan,
            1,
            false,
        )
        .unwrap();
    };
    scan(&mut boot);
    boot.library_db.set_configured_song_roots(&boot.app_config.songs.roots).unwrap();

    let mut table = crate::table_cmd::profile::synthetic_table(0);
    let missing7 = "11".repeat(32);
    let owned7 = format!("{:x}", Sha256::digest(&seven));
    let owned14 = format!("{:x}", Sha256::digest(&fourteen));
    table.entries = [
        ("Owned table entry", owned7.clone(), Some("14K")),
        ("Other owned table entry", owned14.clone(), Some("7K")),
        ("A Missing 7K", missing7.clone(), Some("7K")),
        ("B Missing 14K", format!("{:x}", Sha256::digest(&later)), Some("14K")),
        ("C Unknown", "22".repeat(32), None),
        ("D Missing 24K", "33".repeat(32), Some("24K")),
    ]
    .into_iter()
    .map(|(title, sha256, mode)| FetchedTableEntry {
        level: "1".into(),
        title: title.into(),
        sha256,
        mode: mode.map(str::to_owned),
        url: "https://example.com/song".into(),
        append_url: "https://example.com/append".into(),
        ..Default::default()
    })
    .collect();
    table.courses = crate::difficulty_table::parse_courses_from_header_for_test(
        &table.source_url,
        &Some(json!([{"name": "Ordered", "charts": [
            {"sha256": owned7}, {"sha256": missing7}, {"sha256": owned14}
        ]}])),
    );
    boot.library_db.upsert_difficulty_table(&table).unwrap();
    enable_table(&mut boot, &table.source_url);
    boot.profile_config.select.random_select_no_play = true;

    let mut request = level_request(&boot, &table.source_url, "1");
    let list = load(&request).unwrap().prepared.unwrap();
    assert_eq!(list.mode_filter, SelectModeFilter::K7);
    assert_eq!(names(&list), ["NO PLAY RANDOM SELECT", "Z Owned 7K", "A Missing 7K", "C Unknown"]);
    let [SelectItem::Executable(random), SelectItem::Chart(owned), SelectItem::Chart(missing), _] =
        &list.items[..]
    else {
        panic!("expected a random row and three charts");
    };
    assert_eq!(owned.table_mode.as_deref(), Some("14K"));
    assert_eq!(random.chart_ids, [owned.chart.as_ref().unwrap().chart_id]);
    assert!(!missing.in_library());
    assert_eq!(missing.score_sha256(), Some([0x11; 32]));
    assert!(matches!(
        crate::song_download::choose_missing_chart_action(&Default::default(), &missing.download_metadata),
        crate::song_download::MissingChartAction::Browser(urls)
            if urls == ["https://example.com/song", "https://example.com/append"]
    ));

    request.mode = SelectModeFilter::All;
    let list = load(&request).unwrap().prepared.unwrap();
    assert_eq!(list.mode_filter, SelectModeFilter::All);
    assert_eq!(
        names(&list),
        [
            "NO PLAY RANDOM SELECT",
            "Y Owned 14K",
            "Z Owned 7K",
            "A Missing 7K",
            "B Missing 14K",
            "C Unknown",
            "D Missing 24K"
        ]
    );

    request.action = SelectListAction::RefreshDownloads;
    request.mode = SelectModeFilter::K7;
    let course_id = boot.library_db.list_courses().unwrap()[0].id;
    let mut course_request = tests::request(&boot, SelectListAction::RefreshDownloads);
    course_request.folder_stack = vec![course_contents_path(course_id)];
    course_request.mode = SelectModeFilter::K7;
    let course = load(&course_request).unwrap().prepared.unwrap();
    assert_eq!(course.mode_filter, SelectModeFilter::K7);
    assert_eq!(names(&course), ["Z Owned 7K", "A Missing 7K", "Y Owned 14K"]);

    std::fs::write(root.join("later.bmson"), &later).unwrap();
    scan(&mut boot);
    let refreshed = load(&request).unwrap().prepared.unwrap();
    assert_eq!(
        names(&refreshed),
        ["NO PLAY RANDOM SELECT", "X Downloaded 7K", "Z Owned 7K", "A Missing 7K", "C Unknown"]
    );
    boot.app_config.songs.roots[0].enabled = false;
    boot.library_db.set_configured_song_roots(&boot.app_config.songs.roots).unwrap();
    request.action = SelectListAction::Refresh;
    request.app_config = boot.app_config.clone();
    let disabled = load(&request).unwrap().prepared.unwrap();
    assert_eq!(disabled.mode_filter, SelectModeFilter::K7);
    assert_eq!(names(&disabled), ["A Missing 7K", "C Unknown", "Other owned table entry"]);
    assert!(
        disabled
            .items
            .iter()
            .all(|item| matches!(item, SelectItem::Chart(row) if !row.in_library()))
    );
    boot.app_config.songs.roots[0].enabled = true;
    boot.library_db.set_configured_song_roots(&boot.app_config.songs.roots).unwrap();
    request.app_config = boot.app_config.clone();
    let reenabled = load(&request).unwrap().prepared.unwrap();
    assert_eq!(reenabled.mode_filter, SelectModeFilter::K7);
    assert_eq!(names(&reenabled), names(&refreshed));
}

#[tokio::test]
async fn table_refetch_updates_mode_and_retains_cached_rows_on_failure() {
    use crate::app::table_fetch_runtime::startup_difficulty_table_fetch_urls_for_boot;
    use std::sync::atomic::AtomicBool;

    let unavailable = Arc::new(AtomicBool::new(true));
    let fixture = HttpFixture::with_handler(
        {
            let unavailable = unavailable.clone();
            move |path| {
                let (status, body) = match path {
                    "/header.json" => (
                        200,
                        json!({
                            "name": "Mode table", "symbol": "T", "mode": "beat-14k",
                            "data_url": ["first.json", "second.json"],
                            "data_rule": [{"1": "10"}, {"2": "20"}]
                        }),
                    ),
                    "/first.json" => (
                        200,
                        json!([
                            {"level": 1, "md5": "11".repeat(16), "title": "Entry", "mode": "beat-7k"},
                            {"level": 1, "md5": "22".repeat(16), "title": "Unsupported", "mode": "keyboard-24k-double"}
                        ]),
                    ),
                    "/unknown.json" => (
                        200,
                        json!({
                            "name": "14K in the table name", "symbol": "T", "mode": "14K",
                            "data_url": "second.json"
                        }),
                    ),
                    "/second.json" if unavailable.load(Ordering::Relaxed) => (503, json!({})),
                    "/second.json" => (
                        200,
                        json!([
                            {"level": 2, "md5": "33".repeat(16), "title": "Missing"},
                            {"level": 2, "md5": "44".repeat(16), "title": "Invalid", "mode": "BEAT-7K"},
                            {"level": 2, "md5": "55".repeat(16), "title": "Empty", "mode": ""},
                            {"level": 2, "md5": "66".repeat(16), "title": "Null", "mode": null},
                            {"level": 2, "md5": "77".repeat(16), "title": "Number", "mode": 7}
                        ]),
                    ),
                    _ => (404, json!({})),
                };
                (status, "application/json".into(), body.to_string().into_bytes())
            }
        },
        Duration::ZERO,
    );
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let source_url = format!("{}/header.json", fixture.url);
    enable_table(&mut boot, &source_url);
    let mut cached = crate::table_cmd::profile::synthetic_table(1);
    cached.source_url = source_url.clone();
    cached.entries[0].level = "20".into();
    cached.entries[0].title = "Cached".into();
    boot.library_db.upsert_difficulty_table(&cached).unwrap();
    boot.library_db
        .conn()
        .execute("UPDATE difficulty_tables SET download_metadata_version = 1", [])
        .unwrap();

    let request = level_request(&boot, &source_url, "20");
    let f5_target = table_source_url_from_context(&request.folder_stack, None).unwrap();
    assert_eq!(f5_target, source_url);
    let fetch = || async {
        let (_tx, rx) = tokio::sync::watch::channel(true);
        let mut outcome = None;
        download_table_urls_with_progress(vec![f5_target.clone()], rx, |result| {
            outcome = Some(result);
        })
        .await
        .unwrap();
        outcome.unwrap()
    };
    assert!(matches!(fetch().await, TableFetchDownloadOutcome::Failed(_)));
    assert_eq!(
        startup_difficulty_table_fetch_urls_for_boot(&boot, Instant::now()),
        std::slice::from_ref(&source_url)
    );
    let list = load(&request).unwrap().prepared.unwrap();
    assert_eq!(list.mode_filter, SelectModeFilter::K7);
    assert_eq!(names(&list), ["Cached"]);

    unavailable.store(false, Ordering::Relaxed);
    let TableFetchDownloadOutcome::Succeeded(table) = fetch().await else {
        panic!("expected the table download to succeed");
    };
    let pending = PendingTableStore::ordinary(boot.app_paths.library_db.clone(), table);
    let outcome = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(outcome) = pending.poll() {
                break outcome;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    assert!(matches!(outcome, TableStoreCompletion::Ordinary(TableFetchOutcome::Succeeded(_))));
    assert!(startup_difficulty_table_fetch_urls_for_boot(&boot, Instant::now()).is_empty());

    let list = load(&request).unwrap().prepared.unwrap();
    assert_eq!(list.mode_filter, SelectModeFilter::K14);
    assert_eq!(names(&list), ["Empty", "Invalid", "Missing", "Null", "Number"]);
    assert!(list.items.iter().all(|item| matches!(
        item, SelectItem::Chart(row) if !row.in_library() && row.table_mode.as_deref() == Some("14K")
    )));
    let mut request = level_request(&boot, &source_url, "10");
    let list = load(&request).unwrap().prepared.unwrap();
    assert_eq!(list.mode_filter, SelectModeFilter::K7);
    assert_eq!(names(&list), ["Entry"]);
    request.action = SelectListAction::RefreshDownloads;
    request.mode = SelectModeFilter::K14;
    let list = load(&request).unwrap().prepared.unwrap();
    assert_eq!(list.mode_filter, SelectModeFilter::All);
    assert_eq!(names(&list), ["Entry", "Unsupported"]);
    assert!(
        matches!(&list.items[1], SelectItem::Chart(row) if row.table_mode.as_deref() == Some("48K"))
    );

    let unknown_url = format!("{}/unknown.json", fixture.url);
    let report =
        crate::table_cmd::fetch_table_urls(vec![unknown_url.clone()], &mut boot.library_db)
            .await
            .unwrap();
    assert_eq!(report.succeeded_count(), 1);
    enable_table(&mut boot, &unknown_url);
    assert!(startup_difficulty_table_fetch_urls_for_boot(&boot, Instant::now()).is_empty());
    let list = load(&level_request(&boot, &unknown_url, "2")).unwrap().prepared.unwrap();
    assert_eq!(list.mode_filter, SelectModeFilter::K7);
    assert_eq!(list.items.len(), 5);
    assert!(list.items.iter().all(|item| matches!(
        item, SelectItem::Chart(row) if !row.in_library() && row.table_mode.is_none()
    )));
}
