use super::*;
use crate::bootstrap::profile_tests::ProfileTestDir;
use bmz_chart::import::{BmsRandomSource, import_bms_chart_with_random_source};

const SOURCE_URL: &str = "https://example.com/insane/";

fn register(boot: &mut bootstrap::BootstrappedApp, name: &str) -> (i64, String) {
    let bms = format!("#TITLE {name}\n#BPM 120\n#00111:01\n");
    let path = boot.app_paths.data_dir.join(format!("{name}.bms"));
    std::fs::write(&path, &bms).unwrap();
    let imported = import_bms_chart_with_random_source(
        &path,
        BmsRandomSource::Choices { random: vec![], switches: vec![] },
        true,
    )
    .unwrap();
    let md5 = crate::storage::common::hash_to_hex(&imported.chart.identity.file_md5);
    let chart_id = boot
        .library_db
        .upsert_chart_import(&crate::storage::library_db::ChartImportRecord {
            root_id: None,
            file_path: &path,
            file_size: bms.len() as u64,
            modified_at: 1,
            scanned_at: 1,
            chart: &imported.chart,
        })
        .unwrap();
    (chart_id, md5)
}

fn table_entry(level: &str, md5: &str) -> crate::difficulty_table::FetchedTableEntry {
    crate::difficulty_table::FetchedTableEntry {
        level: level.to_string(),
        md5: md5.to_string(),
        ..Default::default()
    }
}

fn table_boot() -> (ProfileTestDir, bootstrap::BootstrappedApp, [i64; 3]) {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let (first, first_md5) = register(&mut boot, "first");
    let (second, second_md5) = register(&mut boot, "second");
    let (third, third_md5) = register(&mut boot, "third");
    boot.library_db
        .upsert_difficulty_table(&crate::difficulty_table::FetchedDifficultyTable {
            source_url: SOURCE_URL.to_string(),
            head_url: format!("{SOURCE_URL}header.json"),
            name: "Insane".to_string(),
            symbol: "★".to_string(),
            level_order: vec!["1".to_string(), "2".to_string()],
            entries: vec![
                table_entry("1", &first_md5),
                table_entry("1", &second_md5),
                // 同じ譜面が複数の段に載っていても1回だけ流す。
                table_entry("2", &first_md5),
                table_entry("2", &third_md5),
                // ライブラリに無い譜面は流せない。
                table_entry("2", &"0".repeat(32)),
            ],
            courses: Vec::new(),
            fetched_at: 0,
        })
        .unwrap();
    boot.app_config.tables.sources = vec![crate::config::app_config::DifficultyTableSource {
        url: SOURCE_URL.to_string(),
        enabled: true,
    }];
    (data, boot, [first, second, third])
}

fn virtual_folders(
    boot: &bootstrap::BootstrappedApp,
) -> Result<crate::screens::select_model::VirtualFolderCatalog, String> {
    crate::screens::select_model::VirtualFolderCatalog::load(&boot.profile_paths.root_dir)
        .map_err(|error| error.to_string())
}

#[test]
fn archive_folder_autoplay_follows_groups_and_stops_below_song_folders() {
    use std::io::Write;

    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let archive = boot.app_paths.data_dir.join("pack.zip");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    for (entry, title) in [
        ("group_a/song/chart.bms", "Group"),
        ("group_a/song/sabun/chart.bms", "Hidden"),
        ("group_b/subgroup/song/chart.bms", "Deep"),
        ("group_c/song/chart.bms", "Shallow"),
    ] {
        writer.start_file(entry, zip::write::SimpleFileOptions::default()).unwrap();
        writer.write_all(format!("#TITLE {title}\n#BPM 120\n#00111:01\n").as_bytes()).unwrap();
    }
    writer.finish().unwrap();
    boot.app_config.songs.roots = vec![PathEntry {
        path: boot.app_paths.data_dir.to_string_lossy().into_owned(),
        enabled: true,
        recursive: true,
    }];
    let report = crate::storage::scan::scan_song_roots(
        &mut boot.library_db,
        &boot.app_config.songs.roots,
        &boot.app_config.scan,
        1,
        false,
    )
    .unwrap();
    assert_eq!(report.summary.imported, 4);
    let source = SelectListSource::from(&boot);
    let catalog = virtual_folders(&boot);
    let archive_path = format!("{}!", crate::storage::library_db::library_path_key(&archive));
    let item = SelectItem::Folder {
        path: archive_path.clone(),
        name: "pack".into(),
        kind: SelectRowKind::Folder,
        summary: None,
    };
    let ids = autoplay_folder_chart_ids(&source, &catalog, [&item]).unwrap();
    assert_eq!(ids.len(), 3);
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), 3);
    let visible_folders = ["group_a/song", "group_b/subgroup/song", "group_c/song"]
        .map(|entry| format!("{archive_path}/{entry}"));
    let expected_ids = boot
        .library_db
        .list_charts_in_folders(&visible_folders.iter().map(String::as_str).collect::<Vec<_>>())
        .unwrap()
        .into_iter()
        .map(|chart| chart.chart_id)
        .collect::<HashSet<_>>();
    assert_eq!(ids.iter().copied().collect::<HashSet<_>>(), expected_ids);
    let listed = build_select_items_for_stack(&source, &catalog, &[archive_path], &[]).unwrap();
    let mut from_items = autoplay_folder_chart_ids(&source, &catalog, &listed).unwrap();
    let mut expected = ids;
    from_items.sort_unstable();
    expected.sort_unstable();
    assert_eq!(from_items, expected);

    for (folder, title) in [("/group_a/song", "Group"), ("/group_c/song", "Shallow")] {
        let same_folder = same_folder_path(&format!(
            "{}!{folder}",
            crate::storage::library_db::library_path_key(&archive)
        ));
        let items = build_select_items_for_stack(&source, &catalog, &[same_folder], &[]).unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], SelectItem::Chart(row) if row.display_title() == title));
    }
}

#[test]
fn table_folder_autoplay_walks_every_level_once_per_chart() {
    let (_data, boot, [first, second, third]) = table_boot();
    let source = SelectListSource::from(&boot);
    let catalog = virtual_folders(&boot);
    let root = build_select_items_for_stack(&source, &catalog, &[], &[]).unwrap();
    let table = root
        .iter()
        .find(|item| {
            matches!(item, SelectItem::Folder { kind: SelectRowKind::TableFolder, path, .. }
                if path.contains(SOURCE_URL))
        })
        .expect("table folder row");
    assert!(autoplay_folder_row_supported(table));

    let chart_ids = autoplay_folder_chart_ids(&source, &catalog, [table]).unwrap();

    let mut sorted = chart_ids.clone();
    sorted.sort_unstable();
    let mut expected = vec![first, second, third];
    expected.sort_unstable();
    assert_eq!(sorted, expected);
    // 段の順にたどる。
    assert_eq!(chart_ids.last(), Some(&third));
}

#[test]
fn autoplay_from_a_chart_row_cycles_the_open_list_from_the_cursor() {
    let (_data, boot, [first, second, _]) = table_boot();
    let source = SelectListSource::from(&boot);
    let catalog = virtual_folders(&boot);
    let folder_path = |items: &[SelectItem], name: &str| {
        items
            .iter()
            .find_map(|item| match item {
                SelectItem::Folder { path, name: row_name, .. }
                    if path.contains(SOURCE_URL) && row_name.contains(name) =>
                {
                    Some(path.clone())
                }
                _ => None,
            })
            .unwrap()
    };
    let root = build_select_items_for_stack(&source, &catalog, &[], &[]).unwrap();
    let table =
        build_select_items_for_stack(&source, &catalog, &[folder_path(&root, "")], &[]).unwrap();
    let level_one =
        build_select_items_for_stack(&source, &catalog, &[folder_path(&table, "★1")], &[]).unwrap();
    let chart_ids_of = |items: &[SelectItem]| {
        items
            .iter()
            .filter_map(|item| match item {
                SelectItem::Chart(row) => row.chart.as_ref().map(|chart| chart.chart_id),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let listed = chart_ids_of(&level_one);
    assert_eq!(listed.len(), 2);
    let cursor = level_one
        .iter()
        .position(|item| matches!(item, SelectItem::Chart(row) if row.chart.as_ref().is_some_and(|chart| chart.chart_id == listed[1])))
        .unwrap();

    let rotated = level_one[cursor..].iter().chain(&level_one[..cursor]);
    let chart_ids = autoplay_folder_chart_ids(&source, &catalog, rotated).unwrap();

    assert_eq!(chart_ids, vec![listed[1], listed[0]]);
    let mut sorted = chart_ids.clone();
    sorted.sort_unstable();
    let mut expected = vec![first, second];
    expected.sort_unstable();
    assert_eq!(sorted, expected);
}

#[test]
fn autoplay_folder_skips_rows_without_charts_to_walk() {
    for kind in [
        SelectRowKind::CourseFolder,
        SelectRowKind::Command,
        SelectRowKind::NewSongFolder,
        SelectRowKind::SettingsRoot,
        SelectRowKind::SettingsFolder,
    ] {
        let item = SelectItem::Folder {
            path: "bmz-command:test".into(),
            name: "test".into(),
            kind,
            summary: None,
        };
        assert!(!autoplay_folder_row_supported(&item), "{kind:?}");
    }
    for kind in [
        SelectRowKind::Folder,
        SelectRowKind::TableFolder,
        SelectRowKind::SearchFolder,
        SelectRowKind::Container,
    ] {
        let item =
            SelectItem::Folder { path: "/songs".into(), name: "test".into(), kind, summary: None };
        assert!(autoplay_folder_row_supported(&item), "{kind:?}");
    }
}

#[test]
fn builtin_virtual_folder_autoplay_walks_queries_new_songs_and_clear_types() {
    use crate::storage::score_db::{ScoreRecord, ScoreSourceKind};
    use bmz_core::clear::{ClearType, GaugeType};
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let (chart_id, _) = register(&mut boot, "virtual-song");
    boot.app_config.songs.roots = vec![PathEntry {
        path: boot.app_paths.data_dir.to_string_lossy().into_owned(),
        enabled: true,
        recursive: true,
    }];
    let sha = boot.library_db.chart_sha256_by_chart_id(chart_id).unwrap().unwrap();
    boot.score_db
        .insert_score(&ScoreRecord {
            chart_sha256: sha,
            ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
            double_option: Default::default(),
            applied_double_option: Default::default(),
            played_at: 1,
            clear_type: ClearType::Normal,
            gauge_type: Some(GaugeType::Normal),
            gauge_value: Some(80.0),
            total_notes: 1,
            playtime_seconds: 0,
            score: Default::default(),
            count_unprocessed_notes: false,
            random_seed: None,
            seed_scheme: String::new(),
            arrange: "Normal".into(),
            arrange_2p: "Normal".into(),
            gauge_option: String::new(),
            rule_mode: String::new(),
            assist_mask: 0,
            autoplay: false,
            device_type: Default::default(),
            replay_path: String::new(),
            source_kind: ScoreSourceKind::Local,
            source_min_bp: None,
        })
        .unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
        as i64;
    boot.library_db.conn().execute("UPDATE chart_files SET first_seen_at = ?1", [now]).unwrap();
    let source = SelectListSource::from(&boot);
    let catalog = virtual_folders(&boot);
    let root = build_select_items_for_stack(&source, &catalog, &[], &[]).unwrap();
    for (path, kind) in [
        ("bmz-filter:my-best", SelectRowKind::Command),
        ("bmz-filter:new", SelectRowKind::NewSongFolder),
        ("bmz-filter:clear-type", SelectRowKind::Container),
    ] {
        let item = root
            .iter()
            .find(|item| {
                matches!(item,
            SelectItem::Folder { path: actual, kind: actual_kind, .. }
                if actual == path && *actual_kind == kind)
            })
            .expect(path);
        assert!(autoplay_folder_row_supported(item));
        assert_eq!(
            autoplay_folder_chart_ids(&source, &catalog, [item]).unwrap(),
            vec![chart_id],
            "{path}"
        );
    }
    let new_items =
        build_select_items_for_stack(&source, &catalog, &["bmz-filter:new".into()], &[]).unwrap();
    let today = new_items
        .iter()
        .find(|item| {
            matches!(item,
        SelectItem::Folder { path, .. } if path == "bmz-filter:new/day-0")
        })
        .unwrap();
    assert_eq!(autoplay_folder_chart_ids(&source, &catalog, [today]).unwrap(), vec![chart_id]);
}
