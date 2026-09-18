use super::*;
use crate::app::play_flow_replay::{cycle_replay_slot, normalize_replay_slot};

#[test]
fn moved_select_index_handles_empty_rows() {
    assert_eq!(moved_select_index(9, 0, SelectMove::Last), 0);
}

#[test]
fn select_scroll_duration_config_uses_beatoraja_bounds() {
    let mut config = AppConfig::default();
    config.select.scroll_duration_low_ms = 0;
    config.select.scroll_duration_high_ms = 0;
    assert_eq!(select_scroll_duration_low_ms(&config), 2);
    assert_eq!(select_scroll_duration_high_ms(&config), 1);

    config.select.scroll_duration_low_ms = 5_000;
    config.select.scroll_duration_high_ms = 5_000;
    assert_eq!(select_scroll_duration_low_ms(&config), 1000);
    assert_eq!(select_scroll_duration_high_ms(&config), 1000);
}

#[test]
fn select_move_scroll_direction_matches_row_movement() {
    assert_eq!(select_move_scroll_direction(SelectMove::Previous), -1);
    assert_eq!(select_move_scroll_direction(SelectMove::Next), 1);
    assert_eq!(select_move_scroll_direction(SelectMove::PagePrevious), -1);
    assert_eq!(select_move_scroll_direction(SelectMove::PageNext), 1);
    assert_eq!(select_move_scroll_direction(SelectMove::First), 0);
    assert_eq!(select_move_scroll_direction(SelectMove::Last), 0);
}

#[test]
fn select_skin_event_state_cycles_supported_mode_filters() {
    assert_eq!(SelectModeFilter::All.next(), SelectModeFilter::K7);
    assert_eq!(SelectModeFilter::All.previous(), SelectModeFilter::K8);
    assert_eq!(SelectDifficultyFilter::All.next(), SelectDifficultyFilter::Beginner);
    assert_eq!(SelectDifficultyFilter::All.previous(), SelectDifficultyFilter::Insane);
    assert_eq!(SelectSort::Title.next(), SelectSort::Artist);
    assert_eq!(SelectSort::Title.previous(), SelectSort::Bp);
    assert_eq!(
        crate::ln_policy::LnPolicySetting::AutoLn.next(),
        crate::ln_policy::LnPolicySetting::AutoCn
    );
    assert_eq!(
        crate::ln_policy::LnPolicySetting::AutoLn.previous(),
        crate::ln_policy::LnPolicySetting::ForceHcn
    );
    assert_eq!(crate::ln_policy::LnPolicySetting::ForceHcn.display_label(), "FORCE(HCN)");
    assert_eq!(
        cycle_gauge_option_with_direction(GaugeTypeConfig::Normal, 1),
        GaugeTypeConfig::Hard
    );
    assert_eq!(
        cycle_gauge_option_with_direction(GaugeTypeConfig::Normal, -1),
        GaugeTypeConfig::Easy
    );
    assert_eq!(
        cycle_arrange_option_with_direction(ArrangeOption::Normal, -1),
        ArrangeOption::MFRandom
    );
    assert_eq!(
        cycle_double_option_with_direction(DoubleOption::Off, -1),
        DoubleOption::BattleAutoScratch
    );
    assert_eq!(cycle_hs_fix_option_with_direction(HsFixOption::Off, 1), HsFixOption::StartBpm);
    assert_eq!(cycle_hs_fix_option_with_direction(HsFixOption::StartBpm, 1), HsFixOption::MaxBpm);
    assert_eq!(cycle_hs_fix_option_with_direction(HsFixOption::MaxBpm, 1), HsFixOption::MainBpm);
    assert_eq!(cycle_hs_fix_option_with_direction(HsFixOption::MainBpm, 1), HsFixOption::MinBpm);
    assert_eq!(cycle_hs_fix_option_with_direction(HsFixOption::Off, -1), HsFixOption::MinBpm);
    assert_eq!(cycle_bga_option_with_direction(BgaModeConfig::On, -1), BgaModeConfig::Off);
    assert_eq!(
        cycle_bga_expand_with_direction(BgaExpandConfig::KeepAspect, 1),
        BgaExpandConfig::Full
    );
    assert_eq!(
        cycle_gauge_auto_shift_option_with_direction(GaugeAutoShiftConfig::Off, -1),
        GaugeAutoShiftConfig::SelectToUnder
    );
    assert_eq!(
        cycle_judge_algorithm_with_direction(JudgeAlgorithmConfig::Combo, 1),
        JudgeAlgorithmConfig::Duration
    );
    assert_eq!(
        cycle_judge_algorithm_with_direction(JudgeAlgorithmConfig::Combo, -1),
        JudgeAlgorithmConfig::Lowest
    );
}

#[test]
fn replay_slot_cycle_skips_empty_slots_and_wraps() {
    let slots = [true, false, true, false];

    assert_eq!(cycle_replay_slot(slots, None, 1), Some(0));
    assert_eq!(cycle_replay_slot(slots, Some(0), 1), Some(2));
    assert_eq!(cycle_replay_slot(slots, Some(2), 1), Some(0));
    assert_eq!(cycle_replay_slot(slots, Some(0), -1), Some(2));
    assert_eq!(cycle_replay_slot([false; 4], None, 1), None);
}

#[test]
fn replay_slot_normalization_keeps_available_selection_or_uses_first() {
    let slots = [false, true, false, true];

    assert_eq!(normalize_replay_slot(slots, Some(3)), Some(3));
    assert_eq!(normalize_replay_slot(slots, Some(0)), Some(1));
    assert_eq!(normalize_replay_slot(slots, None), Some(1));
}

#[test]
fn select_ir_context_separates_source_resolved_score_keys() {
    let auto_ln = select_ir_cache_context(
        crate::ln_policy::LnPolicySetting::AutoLn,
        crate::ln_policy::LnScorePolicy::AutoLn,
        crate::select_options::DoubleOptionScoreBucket::Off,
        bmz_gameplay::rule::RuleMode::Beatoraja,
    );
    let auto_cn = select_ir_cache_context(
        crate::ln_policy::LnPolicySetting::AutoLn,
        crate::ln_policy::LnScorePolicy::AutoCn,
        crate::select_options::DoubleOptionScoreBucket::Off,
        bmz_gameplay::rule::RuleMode::Beatoraja,
    );

    assert_ne!(auto_ln, auto_cn);
}

#[test]
fn select_mode_filter_keeps_matching_chart_rows() {
    let mut k7 = select_chart_row(1);
    k7.chart.as_mut().unwrap().mode = "7K".to_string();
    let mut k14 = select_chart_row(2);
    k14.chart.as_mut().unwrap().mode = "14K".to_string();
    let mut items = vec![
        SelectItem::Folder {
            path: "folder".to_string(),
            name: "folder".to_string(),
            kind: SelectRowKind::Folder,
            summary: None,
        },
        SelectItem::Chart(k7),
        SelectItem::Chart(k14),
    ];

    apply_select_mode_filter(&mut items, SelectModeFilter::K14);

    assert_eq!(items.len(), 2);
    assert!(matches!(items[0], SelectItem::Folder { .. }));
    assert_eq!(items[1].display_name(), "Title 2");
}

#[test]
fn resolve_mode_filter_keeps_mode_with_matching_charts() {
    let items = vec![chart_row_with_mode(1, "7K"), chart_row_with_mode(2, "5K")];
    // 7K のチャートがあるので据え置く。
    assert_eq!(resolve_non_empty_mode_filter(&items, SelectModeFilter::K7), SelectModeFilter::K7);
}

#[test]
fn resolve_mode_filter_advances_when_all_charts_mismatch() {
    // 5K しか無いフォルダで 7K フィルターを掛けると全消えになるため、
    // beatoraja 同様に前方向 (K7 -> K14 -> K9 -> K5) へ送って K5 で止まる。
    let items = vec![chart_row_with_mode(1, "5K"), chart_row_with_mode(2, "5K")];
    assert_eq!(resolve_non_empty_mode_filter(&items, SelectModeFilter::K7), SelectModeFilter::K5);
}

#[test]
fn resolve_mode_filter_does_not_advance_when_folder_remains() {
    // フォルダ行が残るなら全消えにはならないので据え置く（beatoraja 準拠）。
    let items = vec![
        SelectItem::Folder {
            path: "folder".to_string(),
            name: "folder".to_string(),
            kind: SelectRowKind::Folder,
            summary: None,
        },
        chart_row_with_mode(1, "5K"),
    ];
    assert_eq!(resolve_non_empty_mode_filter(&items, SelectModeFilter::K7), SelectModeFilter::K7);
}

#[test]
fn resolve_mode_filter_keeps_all_filter() {
    let items = vec![chart_row_with_mode(1, "5K")];
    assert_eq!(resolve_non_empty_mode_filter(&items, SelectModeFilter::All), SelectModeFilter::All);
}

#[test]
fn select_mode_filter_roundtrips_through_str() {
    for mode in SelectModeFilter::ORDER {
        assert_eq!(SelectModeFilter::from_str_or_default(mode.as_str()), mode);
    }
    assert_eq!(SelectModeFilter::from_str_or_default("24K"), SelectModeFilter::All);
    assert_eq!(SelectModeFilter::from_str_or_default("24K_DOUBLE"), SelectModeFilter::All);
    assert_eq!(SelectModeFilter::from_str_or_default("unknown"), SelectModeFilter::All);
}

#[test]
fn select_extended_mode_filters_cycle_filter_and_restore() {
    let order = ["ALL", "7K", "14K", "9K", "5K", "10K", "4K", "6K", "8K"];
    for (index, name) in order.iter().enumerate() {
        let filter = SelectModeFilter::from_str_or_default(name);
        assert_eq!(filter.next().as_str(), order[(index + 1) % order.len()]);
        assert_eq!(filter.previous().as_str(), order[(index + order.len() - 1) % order.len()]);
    }
    for (name, expected_mode) in [("4K", KeyMode::K4), ("6K", KeyMode::K6), ("8K", KeyMode::K8)] {
        let config = crate::config::profile_config::SelectStateConfig {
            mode_filter: name.to_string(),
            ..Default::default()
        };
        let restored: crate::config::profile_config::SelectStateConfig =
            toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        let filter = SelectModeFilter::from_str_or_default(&restored.mode_filter);
        assert_eq!(filter.key_mode(), Some(expected_mode));
        let mut items = vec![
            chart_row_with_mode(1, "4K"),
            chart_row_with_mode(2, "6K"),
            chart_row_with_mode(3, "8K"),
            chart_row_with_mode(4, "7K"),
        ];
        assert_eq!(resolve_non_empty_mode_filter(&items, filter), filter);
        apply_select_mode_filter(&mut items, filter);
        assert_eq!(items.len(), 1);
        assert!(
            matches!(&items[0], SelectItem::Chart(row) if row.chart.as_ref().unwrap().mode == name)
        );
        assert_eq!(resolve_non_empty_mode_filter(&items, SelectModeFilter::K10), filter);
    }
    assert_eq!(
        resolve_non_empty_mode_filter(&[chart_row_with_mode(1, "7K")], SelectModeFilter::K4),
        SelectModeFilter::All
    );
}

#[test]
fn select_difficulty_filter_roundtrips_through_str() {
    for filter in SelectDifficultyFilter::ORDER {
        assert_eq!(SelectDifficultyFilter::from_str_or_default(filter.as_str()), filter);
    }
    assert_eq!(SelectDifficultyFilter::from_str_or_default("unknown"), SelectDifficultyFilter::All);
}

#[test]
fn select_difficulty_filter_keeps_matching_charts_per_song_folder() {
    let mut beginner = select_chart_row(1);
    beginner.chart.as_mut().unwrap().folder_path = "song-a".to_string();
    beginner.chart.as_mut().unwrap().difficulty_name = "BEGINNER".to_string();
    let mut normal = select_chart_row(2);
    normal.chart.as_mut().unwrap().folder_path = "song-a".to_string();
    normal.chart.as_mut().unwrap().difficulty_name = "NORMAL".to_string();
    let mut hyper = select_chart_row(3);
    hyper.chart.as_mut().unwrap().folder_path = "song-b".to_string();
    hyper.chart.as_mut().unwrap().difficulty_name = "HYPER".to_string();
    let mut another = select_chart_row(4);
    another.chart.as_mut().unwrap().folder_path = "song-b".to_string();
    another.chart.as_mut().unwrap().difficulty_name = "ANOTHER".to_string();
    let mut items = vec![
        SelectItem::Chart(beginner),
        SelectItem::Chart(normal),
        SelectItem::Chart(hyper),
        SelectItem::Chart(another),
    ];

    apply_select_difficulty_filter(&mut items, SelectDifficultyFilter::Normal);

    assert_eq!(
        items.iter().map(SelectItem::display_name).collect::<Vec<_>>(),
        ["Title 2", "Title 4"]
    );
}

#[test]
fn select_difficulty_filter_keeps_unknown_when_it_is_only_chart() {
    let mut unknown = select_chart_row(1);
    unknown.chart.as_mut().unwrap().folder_path = "song-a".to_string();
    unknown.chart.as_mut().unwrap().difficulty_name.clear();
    let mut items = vec![SelectItem::Chart(unknown)];

    apply_select_difficulty_filter(&mut items, SelectDifficultyFilter::Insane);

    assert_eq!(items.len(), 1);
}

#[test]
fn select_sort_roundtrips_through_str() {
    for sort in SelectSort::ORDER {
        assert_eq!(SelectSort::from_str_or_default(sort.as_str()), sort);
    }
    assert_eq!(SelectSort::from_str_or_default("unknown"), SelectSort::Title);
}

#[test]
fn select_sort_orders_chart_rows_without_moving_folders() {
    let mut slow = select_chart_row(1);
    slow.chart.as_mut().unwrap().title = "Slow".to_string();
    slow.chart.as_mut().unwrap().initial_bpm = 100.0;
    let mut fast = select_chart_row(2);
    fast.chart.as_mut().unwrap().title = "Fast".to_string();
    fast.chart.as_mut().unwrap().initial_bpm = 200.0;
    let mut items = vec![
        SelectItem::Folder {
            path: "folder".to_string(),
            name: "folder".to_string(),
            kind: SelectRowKind::Folder,
            summary: None,
        },
        SelectItem::Chart(fast),
        SelectItem::Chart(slow),
    ];

    apply_select_sort(&mut items, SelectSort::Bpm);

    assert!(matches!(items[0], SelectItem::Folder { .. }));
    assert_eq!(items[1].display_name(), "Slow");
    assert_eq!(items[2].display_name(), "Fast");
}

#[test]
fn restored_select_index_keeps_chart_when_clear_sort_moves_after_score_update() {
    let mut played = select_chart_row(1);
    played.chart.as_mut().unwrap().title = "Played".to_string();
    let mut other = select_chart_row(2);
    other.chart.as_mut().unwrap().title = "Other".to_string();
    let old_items = [SelectItem::Chart(played.clone()), SelectItem::Chart(other.clone())];
    let selected_key = select_item_key(&old_items[0]);

    played.best_score = Some(BestScoreSummary {
        clear_type: "Hard".to_string(),
        ..best_score_with_replay(100, "played.json")
    });
    let mut new_items = vec![SelectItem::Chart(played), SelectItem::Chart(other)];
    apply_select_sort(&mut new_items, SelectSort::Clear);

    assert_eq!(restored_select_index(&new_items, Some(&selected_key), 0), 1);
    assert_eq!(new_items[1].display_name(), "Played");
}

#[test]
fn select_item_key_uses_typed_settings_identity() {
    let config = SelectItem::Config(crate::screens::settings_model::ConfigSelectRow {
        entry_id: SettingsEntryId::MasterVolume,
    });
    assert_eq!(select_item_key(&config), SelectItemKey::Config(SettingsEntryId::MasterVolume));

    let app_config = SelectItem::AppConfig(crate::screens::settings_model::AppConfigSelectRow {
        entry_id: AppSettingsEntryId::VideoVsyncMode,
    });
    assert_eq!(
        select_item_key(&app_config),
        SelectItemKey::AppConfig(AppSettingsEntryId::VideoVsyncMode)
    );

    let binding = SelectItem::KeyBinding(crate::screens::settings_model::KeyBindingSelectRow {
        key_mode: KeyMode::K7,
        target: KeyBindingTarget::Action {
            action: InputActionConfig::E1,
            slot: KeyBindingSlot::KeyboardPrimary,
        },
    });
    assert_eq!(
        select_item_key(&binding),
        SelectItemKey::KeyBinding {
            key_mode: KeyMode::K7,
            target: KeyBindingTarget::Action {
                action: InputActionConfig::E1,
                slot: KeyBindingSlot::KeyboardPrimary,
            },
        }
    );
}

#[test]
fn select_skin_key_config_preserves_folder_history_for_return() {
    let original_folders = vec!["songs".to_string(), "songs/genre".to_string()];
    let original_indices = vec![3, 5];
    let mut folders = original_folders.clone();
    let mut indices = original_indices.clone();

    assert!(crate::app::select_flow_navigation::push_key_config_folder_history(
        &mut folders,
        &mut indices,
        7,
    ));
    assert_eq!(folders.last().map(String::as_str), Some(CONFIG_KEYS_PATH));
    assert_eq!(indices.last(), Some(&7));

    folders.pop();
    let restored = indices.pop();
    assert_eq!(folders, original_folders);
    assert_eq!(indices, original_indices);
    assert_eq!(restored, Some(7));
}

#[test]
fn select_skin_key_config_does_not_duplicate_current_key_config_folder() {
    let mut folders = vec!["songs".to_string(), CONFIG_KEYS_PATH.to_string()];
    let mut indices = vec![3, 5];

    assert!(!crate::app::select_flow_navigation::push_key_config_folder_history(
        &mut folders,
        &mut indices,
        7,
    ));
    assert_eq!(folders, ["songs", CONFIG_KEYS_PATH]);
    assert_eq!(indices, [3, 5]);
}

#[test]
fn select_skin_explorer_target_accepts_chart_and_real_folder_only() {
    let mut row = select_chart_row(1);
    row.chart.as_mut().unwrap().folder_path = "songs/genre/song".to_string();
    let chart = SelectItem::Chart(row);
    let folder = SelectItem::Folder {
        path: "songs/genre".to_string(),
        name: "genre".to_string(),
        kind: SelectRowKind::Folder,
        summary: None,
    };
    let virtual_folder = SelectItem::Folder {
        path: "table://example".to_string(),
        name: "table".to_string(),
        kind: SelectRowKind::TableFolder,
        summary: None,
    };

    assert_eq!(
        crate::app::select_flow_navigation::select_explorer_path(&chart),
        Some(std::path::PathBuf::from("songs/genre/song"))
    );
    assert_eq!(
        crate::app::select_flow_navigation::select_explorer_path(&folder),
        Some(std::path::PathBuf::from("songs/genre"))
    );
    assert_eq!(crate::app::select_flow_navigation::select_explorer_path(&virtual_folder), None);
}
