use super::*;

/// Returns folder items for the virtual root, one entry per enabled root path.
pub fn root_folder_items(root_paths: &[String]) -> Vec<SelectItem> {
    root_paths
        .iter()
        .map(|path| {
            let name = std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path.as_str())
                .to_string();
            SelectItem::Folder {
                path: path.clone(),
                name,
                kind: SelectRowKind::Folder,
                summary: None,
            }
        })
        .collect()
}

// FAVORITE系はContainerとして作り、beatoraja系ではFEATURE、LR2では従来のカスタムフォルダにする。
pub fn favorite_root_item() -> SelectItem {
    SelectItem::Folder {
        path: FAVORITE_ROOT_PATH.to_string(),
        name: "FAVORITE".to_string(),
        kind: SelectRowKind::Container,
        summary: None,
    }
}

pub fn favorite_root_items(collection_db: &CollectionDatabase) -> Result<Vec<SelectItem>> {
    let mut items = Vec::new();
    if !collection_db.favorite_chart_records()?.is_empty() {
        items.push(SelectItem::Folder {
            path: FAVORITE_CHART_PATH.to_string(),
            name: "FAVORITE CHART".to_string(),
            kind: SelectRowKind::Container,
            summary: None,
        });
    }
    if !collection_db.favorite_song_records()?.is_empty() {
        items.push(SelectItem::Folder {
            path: FAVORITE_SONG_PATH.to_string(),
            name: "FAVORITE SONG".to_string(),
            kind: SelectRowKind::Container,
            summary: None,
        });
    }
    Ok(items)
}

#[derive(Clone, Copy)]
enum RandomSelectFilter {
    All,
    NoPlay,
    Failed,
    Below(bmz_core::clear::ClearType),
}

impl RandomSelectFilter {
    fn matches(self, score: Option<&BestScoreSummary>) -> bool {
        use bmz_core::clear::ClearType;
        match self {
            Self::All => true,
            Self::NoPlay => score.is_none_or(|score| score.play_count == 0),
            Self::Failed => score.is_some_and(|score| {
                ClearType::from_label(&score.clear_type) == Some(ClearType::Failed)
            }),
            Self::Below(threshold) => score.is_none_or(|score| {
                ClearType::from_label(&score.clear_type)
                    .is_some_and(|clear| (clear as u8) < threshold as u8)
            }),
        }
    }
}

/// Builds enabled random bars from the already filtered, score-enriched list.
pub fn random_select_items_from_items(
    items: &[SelectItem],
    config: &crate::config::profile_config::SelectStateConfig,
) -> Vec<SelectItem> {
    use RandomSelectFilter::*;
    use bmz_core::clear::ClearType;
    let definitions = [
        ("RANDOM SELECT", All),
        ("NO PLAY RANDOM SELECT", NoPlay),
        ("FAILED RANDOM SELECT", Failed),
        ("NOT EASY RANDOM SELECT", Below(ClearType::Easy)),
        ("NOT CLEAR RANDOM SELECT", Below(ClearType::Normal)),
        ("NOT HARD RANDOM SELECT", Below(ClearType::Hard)),
        ("NOT EX-HARD RANDOM SELECT", Below(ClearType::ExHard)),
        ("NOT FULL COMBO RANDOM SELECT", Below(ClearType::FullCombo)),
    ];
    definitions
        .into_iter()
        .zip(config.random_select_flags())
        .filter_map(|((title, filter), enabled)| {
            if !enabled {
                return None;
            }
            let chart_ids: Vec<_> = items
                .iter()
                .filter_map(|item| {
                    let SelectItem::Chart(row) = item else {
                        return None;
                    };
                    let chart = row.chart.as_ref()?;
                    filter.matches(row.best_score.as_ref()).then_some(chart.chart_id)
                })
                .collect();
            let minimum = if matches!(filter, All) { 2 } else { 1 };
            (chart_ids.len() >= minimum).then(|| {
                SelectItem::Executable(SelectExecutableRow {
                    title: title.to_string(),
                    kind: SelectExecutableKind::RandomSelect,
                    chart_ids,
                })
            })
        })
        .collect()
}

pub fn random_mix_item() -> SelectItem {
    SelectItem::Executable(SelectExecutableRow {
        title: "RANDOM MIX".to_string(),
        kind: SelectExecutableKind::RandomMix,
        chart_ids: Vec::new(),
    })
}
