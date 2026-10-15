/// beatoraja `BarManager` 準拠の mode filter 自動送り。
///
/// 指定モードがこの一覧の全 bar を消す（= 残るのが mismatch のチャート行だけ）
/// 場合のみ、チャートが残るモードへ前方向に送る。フォルダ等チャート以外の行が
/// 1 つでも残る、または ALL のように絞り込まないモードの場合は据え置く。
pub(in crate::app) fn resolve_non_empty_mode_filter(
    items: &[SelectItem],
    start: SelectModeFilter,
) -> SelectModeFilter {
    let mut candidate = start;
    for _ in 0..SelectModeFilter::ORDER.len() {
        if !mode_filter_removes_everything(items, candidate) {
            return candidate;
        }
        candidate = candidate.next();
    }
    start
}

/// `apply_select_mode_filter` を適用すると一覧が空になるか。
pub(in crate::app) fn mode_filter_removes_everything(
    items: &[SelectItem],
    filter: SelectModeFilter,
) -> bool {
    if items.is_empty() {
        return false;
    }
    let Some(key_mode) = filter.key_mode() else {
        // ALL は絞り込まないので空にはならない。
        return false;
    };
    items.iter().all(|item| match item {
        SelectItem::Chart(row) => !row
            .chart
            .as_ref()
            .and_then(|chart| KeyMode::from_str_opt(&chart.mode))
            .is_some_and(|mode| mode == key_mode),
        // フォルダ・コース等は除去対象外なので、残れば「全除去」ではない。
        _ => false,
    })
}

pub(in crate::app) fn apply_select_mode_filter(
    items: &mut Vec<SelectItem>,
    filter: SelectModeFilter,
) {
    let Some(key_mode) = filter.key_mode() else {
        return;
    };
    items.retain(|item| match item {
        SelectItem::Chart(row) => row
            .chart
            .as_ref()
            .and_then(|chart| KeyMode::from_str_opt(&chart.mode))
            .is_some_and(|mode| mode == key_mode),
        _ => true,
    });
}

/// LR2風の譜面難易度フィルター。
///
/// 所持譜面を親フォルダ単位でまとめ、指定難易度があればその譜面を、なければ
/// グループ内の最高難易度を残す。未所持表エントリと非譜面行は対象外とする。
pub(in crate::app) fn apply_select_difficulty_filter(
    items: &mut Vec<SelectItem>,
    filter: SelectDifficultyFilter,
) {
    let target = filter.difficulty_code();
    if target == 0 {
        return;
    }

    let mut groups = std::collections::HashMap::<String, Vec<(usize, u8)>>::new();
    for (index, item) in items.iter().enumerate() {
        let SelectItem::Chart(row) = item else {
            continue;
        };
        let Some(chart) = &row.chart else {
            continue;
        };
        groups
            .entry(chart.folder_path.clone())
            .or_default()
            .push((index, difficulty_code(&chart.difficulty_name)));
    }

    let mut keep = vec![true; items.len()];
    for group in groups.values() {
        let has_target = group.iter().any(|(_, difficulty)| *difficulty == target);
        let fallback = group.iter().map(|(_, difficulty)| *difficulty).max().unwrap_or(0);
        let selected = if has_target { target } else { fallback };
        for &(index, difficulty) in group {
            keep[index] = difficulty == selected;
        }
    }

    let mut index = 0;
    items.retain(|_| {
        let retain = keep[index];
        index += 1;
        retain
    });
}

pub(in crate::app) fn difficulty_code(label: &str) -> u8 {
    match label.trim().to_ascii_uppercase().as_str() {
        "1" | "BEGINNER" => 1,
        "2" | "NORMAL" => 2,
        "3" | "HYPER" => 3,
        "4" | "ANOTHER" => 4,
        "5" | "INSANE" | "LEGGENDARIA" => 5,
        _ => 0,
    }
}

pub(in crate::app) fn apply_select_sort(items: &mut [SelectItem], sort: SelectSort) {
    items.sort_by(|a, b| match (a, b) {
        (SelectItem::Chart(a), SelectItem::Chart(b)) => compare_select_chart_rows(a, b, sort),
        _ => std::cmp::Ordering::Equal,
    });
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) enum SelectItemKey {
    Folder(String),
    ChartId(i64),
    ChartFolder(String),
    ChartSha256([u8; 32]),
    ChartMd5([u8; 16]),
    ChartFallback { title: String, artist: String, table_level: String, table_full: String },
    Course(i64),
    Executable(String),
    Config(SettingsEntryId),
    AppConfig(AppSettingsEntryId),
    KeyBinding { key_mode: KeyMode, target: KeyBindingTarget },
    SettingsBack,
    SettingsClose,
    AdvancedSettings,
    ApplyAudioSettings,
}

pub(in crate::app) fn select_item_key(item: &SelectItem) -> SelectItemKey {
    match item {
        SelectItem::Folder { path, .. } => SelectItemKey::Folder(path.clone()),
        SelectItem::Chart(row) => row
            .chart
            .as_ref()
            .map(|chart| SelectItemKey::ChartId(chart.chart_id))
            .or_else(|| row.score_sha256().map(SelectItemKey::ChartSha256))
            .or_else(|| row.score_md5().map(SelectItemKey::ChartMd5))
            .unwrap_or_else(|| SelectItemKey::ChartFallback {
                title: row.display_title().to_string(),
                artist: row.display_artist().to_string(),
                table_level: row.table_level.clone(),
                table_full: row.table_text.table_full.clone(),
            }),
        SelectItem::Course(row) => SelectItemKey::Course(row.course_id),
        SelectItem::Executable(row) => SelectItemKey::Executable(row.title.clone()),
        SelectItem::Config(row) => SelectItemKey::Config(row.entry_id),
        SelectItem::AppConfig(row) => SelectItemKey::AppConfig(row.entry_id),
        SelectItem::KeyBinding(row) => {
            SelectItemKey::KeyBinding { key_mode: row.key_mode, target: row.target }
        }
        SelectItem::SettingsBack => SelectItemKey::SettingsBack,
        SelectItem::SettingsClose => SelectItemKey::SettingsClose,
        SelectItem::AdvancedSettings => SelectItemKey::AdvancedSettings,
        SelectItem::ApplyAudioSettings => SelectItemKey::ApplyAudioSettings,
    }
}

pub(in crate::app) fn favorite_hints_for_row(
    row: &crate::screens::select_model::SelectChartRow,
) -> FavoriteHints {
    let folder = row.chart.as_ref().map(|chart| chart.folder_path.clone()).unwrap_or_default();
    FavoriteHints {
        title: row.display_title().to_string(),
        artist: row.display_artist().to_string(),
        folder,
        chart_path: String::new(),
    }
}

pub(in crate::app) fn restored_select_index(
    items: &[SelectItem],
    previous_selected_key: Option<&SelectItemKey>,
    previous_index: usize,
) -> usize {
    previous_selected_key
        .and_then(|key| match key {
            // Missing table entries acquire a ChartId after a download. Keep
            // ChartId for local duplicates, but bridge this transition by hash.
            SelectItemKey::ChartSha256(hash) => items.iter().enumerate()
                .filter(|(_, item)| matches!(item, SelectItem::Chart(row) if row.score_sha256() == Some(*hash)))
                .min_by_key(|(index, _)| index.abs_diff(previous_index)).map(|(index, _)| index),
            SelectItemKey::ChartMd5(hash) => items.iter().enumerate()
                .filter(|(_, item)| matches!(item, SelectItem::Chart(row) if row.score_md5() == Some(*hash)))
                .min_by_key(|(index, _)| index.abs_diff(previous_index)).map(|(index, _)| index),
            SelectItemKey::ChartFolder(folder_path) => items.iter().position(|item| {
                matches!(item, SelectItem::Chart(row) if row.chart.as_ref().is_some_and(|chart| chart.folder_path == *folder_path))
            }),
            _ => items.iter().position(|item| select_item_key(item) == *key),
        })
        .unwrap_or_else(|| previous_index.min(items.len().saturating_sub(1)))
}

pub(in crate::app) fn download_selection_index(
    selection: &DownloadSelection,
    folder_stack: &[String],
    generation: u64,
    items: &[SelectItem],
) -> Option<usize> {
    if selection.folder_stack != folder_stack || selection.generation != generation {
        return None;
    }
    // Keep a matching table reference where possible. Never fall back to a
    // title or an index when the requested file is absent/filtered out.
    let matches = |item: &SelectItem| {
        matches!(item,
        SelectItem::Chart(row) if row.chart.as_ref().is_some_and(|chart| selection.identity.matches(chart)))
    };
    items
        .iter()
        .position(|item| {
            matches(item)
                && selection.selected_key.as_ref().is_some_and(|key| select_item_key(item) == *key)
        })
        .or_else(|| items.iter().position(matches))
}

pub(in crate::app) fn compare_select_chart_rows(
    a: &crate::screens::select_model::SelectChartRow,
    b: &crate::screens::select_model::SelectChartRow,
    sort: SelectSort,
) -> std::cmp::Ordering {
    let ordering = match sort {
        SelectSort::Title => compare_case_insensitive(a.display_title(), b.display_title()),
        SelectSort::Artist => compare_case_insensitive(a.display_artist(), b.display_artist()),
        SelectSort::Bpm => chart_initial_bpm(a).total_cmp(&chart_initial_bpm(b)),
        SelectSort::Length => chart_length_ms(a).cmp(&chart_length_ms(b)),
        SelectSort::Level => compare_play_level(a, b),
        SelectSort::Clear => clear_rank(a).cmp(&clear_rank(b)),
        SelectSort::Score => ex_score(a).cmp(&ex_score(b)),
        SelectSort::Bp => bp(a).cmp(&bp(b)),
    };
    ordering.then_with(|| compare_case_insensitive(a.display_title(), b.display_title()))
}

pub(in crate::app) fn compare_case_insensitive(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

pub(in crate::app) fn chart_initial_bpm(row: &crate::screens::select_model::SelectChartRow) -> f64 {
    row.chart.as_ref().map(|chart| chart.initial_bpm).unwrap_or(0.0)
}

pub(in crate::app) fn chart_length_ms(row: &crate::screens::select_model::SelectChartRow) -> i64 {
    row.chart.as_ref().map(|chart| chart.length_ms).unwrap_or(0)
}

pub(in crate::app) fn compare_play_level(
    a: &crate::screens::select_model::SelectChartRow,
    b: &crate::screens::select_model::SelectChartRow,
) -> std::cmp::Ordering {
    play_level_number(a)
        .total_cmp(&play_level_number(b))
        .then_with(|| compare_case_insensitive(a.display_title(), b.display_title()))
}

pub(in crate::app) fn play_level_number(row: &crate::screens::select_model::SelectChartRow) -> f64 {
    row.chart.as_ref().and_then(|chart| chart.play_level.parse::<f64>().ok()).unwrap_or(0.0)
}

pub(in crate::app) fn clear_rank(row: &crate::screens::select_model::SelectChartRow) -> i8 {
    if !row.in_library() {
        // 難易度表にあるがローカル未所持。NoPlay よりさらに下位へ並べる。
        return -1;
    }
    // 所持済み: NoPlay / 未記録 = 0、Failed=1 .. Max=10。
    ClearType::rank_from_label(
        row.best_score.as_ref().map(|score| score.clear_type.as_str()).unwrap_or_default(),
    ) as i8
}

pub(in crate::app) fn ex_score(row: &crate::screens::select_model::SelectChartRow) -> u32 {
    row.best_score.as_ref().map(|score| score.ex_score).unwrap_or(0)
}

pub(in crate::app) fn bp(row: &crate::screens::select_model::SelectChartRow) -> u32 {
    row.best_score.as_ref().map(|score| score.bp).unwrap_or(u32::MAX)
}
use super::*;
