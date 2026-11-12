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
    let SelectItem::Folder { kind, .. } = table else { unreachable!() };
    assert!(autoplay_folder_row_supported(*kind));

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
        assert!(!autoplay_folder_row_supported(kind), "{kind:?}");
    }
    for kind in [
        SelectRowKind::Folder,
        SelectRowKind::TableFolder,
        SelectRowKind::SearchFolder,
        SelectRowKind::Container,
    ] {
        assert!(autoplay_folder_row_supported(kind), "{kind:?}");
    }
}
