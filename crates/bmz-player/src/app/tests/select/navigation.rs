use super::*;
use crate::bootstrap::profile_tests::ProfileTestDir;
use crate::screens::select_model::SelectCollectionCache;
use crate::screens::settings_model::CONFIG_ROOT_PATH;

fn folder(path: &str) -> SelectItem {
    SelectItem::Folder {
        path: path.to_string(),
        name: path.to_string(),
        kind: SelectRowKind::Folder,
        summary: None,
    }
}

fn load(
    boot: &BootstrappedApp,
    stack: &[String],
    mode: SelectModeFilter,
) -> Result<(Vec<SelectItem>, SelectModeFilter)> {
    load_items_for_stack(
        boot,
        &mut SelectCollectionCache::default(),
        stack,
        &[],
        mode,
        SelectDifficultyFilter::All,
        SelectSort::Title,
    )
}

#[test]
fn empty_folder_entry_keeps_parent_list_cursor_history_and_filter() {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let empty_root = data.paths.data_dir.join("empty-songs");
    std::fs::create_dir(&empty_root).unwrap();
    boot.app_config.songs.roots = vec![PathEntry {
        path: empty_root.to_string_lossy().into_owned(),
        enabled: true,
        recursive: true,
    }];
    let new_path = format!("{VIRTUAL_FOLDER_PATH_PREFIX}new");
    for (stack, path) in [
        (vec![new_path.clone()], format!("{new_path}/day-0")),
        (Vec::new(), empty_root.to_string_lossy().into_owned()),
    ] {
        let indices = vec![4; stack.len()];
        let (items, mode) = load(&boot, &stack, SelectModeFilter::K7).unwrap();
        let index = items
            .iter()
            .position(|item| select_item_key(item) == select_item_key(&folder(&path)))
            .unwrap();
        let before_rows =
            select_snapshot_rows(&items, index, 25, &boot.profile_config, None, &HashMap::new());
        let mut loads = 0;
        let prepared = prepare_select_list(
            &SelectListAction::Enter(path.clone()),
            &stack,
            &indices,
            index,
            Some(select_item_key(&items[index])),
            |candidate| {
                loads += 1;
                assert_eq!(candidate.last(), Some(&path));
                load(&boot, candidate, mode)
            },
        )
        .unwrap();
        assert!(prepared.is_none());
        assert_eq!(loads, 1);
        assert_eq!(mode, SelectModeFilter::K7);
        assert_eq!(indices, vec![4; stack.len()]);
        assert_eq!(select_item_key(&items[index]), select_item_key(&folder(&path)));
        assert_eq!(before_rows.len(), 25, "parent wheel must still have rows");
    }
}

#[test]
fn folder_entry_accepts_subfolders_settings_and_course_actions_without_charts() {
    let data = ProfileTestDir::new();
    let boot = data.boot();
    for path in [
        format!("{VIRTUAL_FOLDER_PATH_PREFIX}new"),
        CONFIG_ROOT_PATH.to_string(),
        COURSE_ROOT_PATH.to_string(),
    ] {
        let mut loads = 0;
        let prepared = prepare_select_list(
            &SelectListAction::Enter(path.clone()),
            &[],
            &[],
            3,
            None,
            |stack| {
                loads += 1;
                load(&boot, stack, SelectModeFilter::K7)
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(loads, 1);
        assert!(!prepared.items.is_empty());
        assert!(prepared.items.iter().all(|item| !matches!(item, SelectItem::Chart(_))));
        assert_eq!(prepared.folder_stack, [path]);
        assert_eq!(prepared.selected_index_stack, [3]);
        assert_eq!(prepared.selected_index, 0);
        assert!(!prepared.left_empty_folder);
    }
}

#[test]
fn folder_entry_keeps_missing_table_chart_rows() {
    let mut row = select_chart_row(1);
    row.chart = None;
    row.fallback_title = "Missing chart".to_string();
    let prepared = prepare_select_list(
        &SelectListAction::Enter("table-level".to_string()),
        &[],
        &[],
        2,
        None,
        |_| Ok((vec![SelectItem::Chart(row.clone())], SelectModeFilter::All)),
    )
    .unwrap()
    .unwrap();
    assert_eq!(prepared.items.len(), 1);
    assert_eq!(prepared.items[0].display_name(), "Missing chart");
}

#[test]
fn folder_entry_checks_emptiness_after_mode_filter_fallback() {
    let data = ProfileTestDir::new();
    let (boot, path, _) = crate::app::tests::boot_chart::registered_charts(&data);
    let folder_path = path.parent().unwrap().to_string_lossy().into_owned();
    let (all_items, _) =
        load(&boot, std::slice::from_ref(&folder_path), SelectModeFilter::All).unwrap();
    assert!(!all_items.is_empty());
    let start_mode = SelectModeFilter::K14;
    let expected_mode = resolve_non_empty_mode_filter(&all_items, start_mode);
    assert_ne!(expected_mode, start_mode);
    let prepared =
        prepare_select_list(&SelectListAction::Enter(folder_path), &[], &[], 0, None, |stack| {
            load(&boot, stack, start_mode)
        })
        .unwrap()
        .unwrap();
    assert!(!prepared.items.is_empty());
    assert_eq!(prepared.mode_filter, expected_mode);
}

#[test]
fn folder_load_error_is_not_reported_as_empty_or_committed() {
    let data = ProfileTestDir::new();
    let boot = data.boot();
    std::fs::write(boot.profile_paths.root_dir.join("select-folders.toml"), "invalid [").unwrap();
    let stack = vec!["parent".to_string()];
    let indices = vec![3];
    let result = prepare_select_list(
        &SelectListAction::Enter(format!("{VIRTUAL_FOLDER_PATH_PREFIX}new/day-0")),
        &stack,
        &indices,
        7,
        None,
        |candidate| load(&boot, candidate, SelectModeFilter::K7),
    );
    assert!(result.is_err(), "a broken catalog is not a successfully loaded empty folder");
    assert_eq!(stack, ["parent"]);
    assert_eq!(indices, [3]);
}

#[test]
fn refresh_empty_folder_returns_to_nearest_parent_and_restores_folder_identity() {
    let stack = vec!["parent".to_string(), "child".to_string()];
    let indices = vec![4, 2];
    let mut visited = Vec::new();
    let prepared = prepare_select_list(
        &SelectListAction::Refresh,
        &stack,
        &indices,
        9,
        Some(SelectItemKey::ChartId(1)),
        |candidate| {
            visited.push(candidate.to_vec());
            let items = if candidate.len() == 2 {
                Vec::new()
            } else {
                vec![folder("child"), folder("other")]
            };
            Ok((items, SelectModeFilter::K7))
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(visited, [stack, vec!["parent".to_string()]]);
    assert_eq!(prepared.folder_stack, ["parent"]);
    assert_eq!(prepared.selected_index_stack, [4]);
    assert_eq!(prepared.selected_index, 0, "restore the folder even when its old index changed");
    assert!(prepared.left_empty_folder);
}

#[test]
fn refresh_last_favorite_removal_returns_to_root() {
    let data = ProfileTestDir::new();
    let (mut boot, path, _) = crate::app::tests::boot_chart::registered_charts(&data);
    let id = boot.library_db.chart_id_by_chart_file_path(&path).unwrap().unwrap();
    let chart = boot.library_db.list_charts_by_ids(&[id]).unwrap().remove(0);
    let hints = FavoriteHints::new(&chart.title, &chart.artist, &chart.folder_path);
    boot.collection_db.toggle_favorite_chart(chart.sha256, &hints, 1).unwrap();
    let stack = vec![FAVORITE_ROOT_PATH.to_string(), FAVORITE_CHART_PATH.to_string()];
    let (before, _) = load(&boot, &stack, SelectModeFilter::All).unwrap();
    assert!(!before.is_empty());
    boot.collection_db.toggle_favorite_chart(chart.sha256, &hints, 2).unwrap();
    let mut visited = Vec::new();
    let prepared = prepare_select_list(
        &SelectListAction::Refresh,
        &stack,
        &[0, 0],
        0,
        Some(select_item_key(&before[0])),
        |candidate| {
            visited.push(candidate.len());
            load(&boot, candidate, SelectModeFilter::All)
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(visited, [2, 1, 0]);
    assert!(prepared.folder_stack.is_empty());
    assert!(prepared.selected_index_stack.is_empty());
    assert!(!prepared.items.is_empty());
    assert!(prepared.left_empty_folder);
}

#[test]
fn exit_skips_empty_parent_and_clamps_saved_cursor_at_root() {
    let prepared = prepare_select_list(
        &SelectListAction::Exit,
        &["parent".to_string(), "child".to_string()],
        &[7, 3],
        0,
        None,
        |stack| {
            Ok((
                if stack.is_empty() { vec![folder("a"), folder("b")] } else { Vec::new() },
                SelectModeFilter::All,
            ))
        },
    )
    .unwrap()
    .unwrap();
    assert!(prepared.folder_stack.is_empty());
    assert!(prepared.selected_index_stack.is_empty());
    assert_eq!(prepared.selected_index, 1);
    assert!(prepared.left_empty_folder);
}

#[test]
fn refresh_error_in_parent_does_not_partially_pop_history() {
    let stack = vec!["parent".to_string(), "child".to_string()];
    let indices = vec![7, 3];
    let result =
        prepare_select_list(&SelectListAction::Refresh, &stack, &indices, 0, None, |candidate| {
            if candidate.len() == 2 {
                Ok((Vec::new(), SelectModeFilter::All))
            } else {
                anyhow::bail!("parent read failed")
            }
        });
    assert!(result.is_err());
    assert_eq!(stack, ["parent", "child"]);
    assert_eq!(indices, [7, 3]);
}

#[test]
fn refresh_nonempty_folder_preserves_selected_item_after_reordering() {
    let prepared = prepare_select_list(
        &SelectListAction::Refresh,
        &["parent".to_string()],
        &[5],
        1,
        Some(SelectItemKey::ChartId(2)),
        |_| {
            Ok((
                vec![
                    SelectItem::Chart(select_chart_row(2)),
                    SelectItem::Chart(select_chart_row(1)),
                ],
                SelectModeFilter::K7,
            ))
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(prepared.selected_index, 0);
    assert_eq!(prepared.selected_index_stack, [5]);
    assert!(!prepared.left_empty_folder);
}

#[test]
fn empty_root_refresh_is_bounded_and_exit_does_not_load() {
    let mut loads = 0;
    let prepared = prepare_select_list(&SelectListAction::Refresh, &[], &[], 12, None, |_| {
        loads += 1;
        Ok((Vec::new(), SelectModeFilter::All))
    })
    .unwrap()
    .unwrap();
    assert_eq!(loads, 1);
    assert_eq!(prepared.selected_index, 0);
    assert!(!prepared.left_empty_folder);
    assert!(
        prepare_select_list(&SelectListAction::Exit, &[], &[], 0, None, |_| panic!(
            "exiting root must not reload"
        ),)
        .unwrap()
        .is_none()
    );
}
