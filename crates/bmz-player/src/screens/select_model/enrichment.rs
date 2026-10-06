use super::*;

/// Wraps a `ChartListItem` set into `SelectItem::Chart` entries with best-score,
/// replay-slot, and difficulty-table-level metadata resolved.
pub(super) fn chart_items_with_enrichment(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    all_charts: Vec<ChartListItem>,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
    table_source_order: &[String],
    active_table_sources: Option<&[String]>,
) -> Result<Vec<SelectItem>> {
    chart_items_with_optional_table_enrichment(
        library_db,
        score_db,
        all_charts,
        ln_policy_setting,
        rule_mode,
        Some((table_source_order, active_table_sources)),
    )
}

/// Wraps charts with runtime score metadata without querying difficulty tables.
pub(super) fn chart_items_without_table_enrichment(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    all_charts: Vec<ChartListItem>,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> Result<Vec<SelectItem>> {
    chart_items_with_optional_table_enrichment(
        library_db,
        score_db,
        all_charts,
        ln_policy_setting,
        rule_mode,
        None,
    )
}

fn chart_items_with_optional_table_enrichment(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    all_charts: Vec<ChartListItem>,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
    table_enrichment: Option<(&[String], Option<&[String]>)>,
) -> Result<Vec<SelectItem>> {
    let keys: Vec<ScoreKey> =
        all_charts.iter().map(|c| score_key_for_chart(c, ln_policy_setting, rule_mode)).collect();
    let score_map: HashMap<ScoreKey, BestScoreSummary> = score_db
        .best_scores_for_charts(&keys)?
        .into_iter()
        .map(|s| {
            (ScoreKey::with_options(s.chart_sha256, s.ln_policy, s.double_option, s.rule_mode), s)
        })
        .collect();
    let replay_slot_map = replay_slot_map(score_db, &keys)?;
    let last_played = score_db.last_played_times_for_charts(&keys)?;
    let chart_ids: Vec<i64> = all_charts.iter().map(|c| c.chart_id).collect();
    let mut analysis_map = library_db.chart_analysis_summaries_by_chart_ids(&chart_ids)?;

    let mut md5_level_map: HashMap<String, String> = HashMap::new();
    let mut md5_text_map: HashMap<String, DifficultyTableText> = HashMap::new();
    let mut sha256_level_map: HashMap<String, String> = HashMap::new();
    let mut sha256_text_map: HashMap<String, DifficultyTableText> = HashMap::new();

    if let Some((table_source_order, active_table_sources)) = table_enrichment {
        // MD5 lookup (multiple tables per MD5 joined with '/')
        let mut seen_md5 = HashSet::new();
        let md5_hexes: Vec<String> = all_charts
            .iter()
            .map(|c| hash_to_hex(&c.md5))
            .filter(|md5| seen_md5.insert(md5.clone()))
            .collect();
        let md5_refs: Vec<&str> = md5_hexes.iter().map(|s| s.as_str()).collect();
        let mut md5_entries = library_db.list_difficulty_table_entries_by_md5s(&md5_refs)?;
        retain_active_table_entries(&mut md5_entries, active_table_sources);
        sort_difficulty_table_entries(&mut md5_entries, table_source_order);
        for e in md5_entries {
            insert_table_level_and_text(&mut md5_level_map, &mut md5_text_map, e.md5.clone(), &e);
        }

        // SHA256 fallback for charts not matched by MD5
        let mut seen_sha256 = HashSet::new();
        let missing_sha256_hexes: Vec<String> = all_charts
            .iter()
            .filter(|c| !md5_level_map.contains_key(&hash_to_hex(&c.md5)))
            .map(|c| hash_to_hex(&c.sha256))
            .filter(|sha256| seen_sha256.insert(sha256.clone()))
            .collect();
        if !missing_sha256_hexes.is_empty() {
            let sha256_refs: Vec<&str> = missing_sha256_hexes.iter().map(|s| s.as_str()).collect();
            let mut sha256_entries =
                library_db.list_difficulty_table_entries_by_sha256s(&sha256_refs)?;
            retain_active_table_entries(&mut sha256_entries, active_table_sources);
            sort_difficulty_table_entries(&mut sha256_entries, table_source_order);
            for e in sha256_entries {
                insert_table_level_and_text(
                    &mut sha256_level_map,
                    &mut sha256_text_map,
                    e.sha256.clone(),
                    &e,
                );
            }
        }
    }

    let mut items = Vec::with_capacity(all_charts.len());
    for chart in all_charts {
        let score_key = score_key_for_chart(&chart, ln_policy_setting, rule_mode);
        let best_score = score_map.get(&score_key).cloned();
        let replay_slots = replay_slot_map.get(&score_key).copied().unwrap_or([false; 4]);
        let md5_hex = hash_to_hex(&chart.md5);
        let sha256_hex = hash_to_hex(&chart.sha256);
        let table_level = md5_level_map
            .get(&md5_hex)
            .cloned()
            .or_else(|| sha256_level_map.get(&sha256_hex).cloned())
            .unwrap_or_default();
        let table_text = md5_text_map
            .get(&md5_hex)
            .cloned()
            .or_else(|| sha256_text_map.get(&sha256_hex).cloned());
        let has_document = chart.has_document;
        items.push(SelectItem::Chart(SelectChartRow {
            chart_analysis: analysis_map.remove(&chart.chart_id),
            chart: Some(chart),
            has_document,
            fallback_title: String::new(),
            fallback_artist: String::new(),
            entry_sha256: None,
            download_metadata: ChartDownloadMetadata::default(),
            best_score,
            last_played_at: last_played.get(&score_key).copied(),
            replay_slots,
            favorite_chart: false,
            favorite_song: false,
            table_level,
            table_text: table_text.unwrap_or_default(),
        }));
    }

    Ok(items)
}

pub fn select_folder_summary(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    path: &str,
    kind: SelectRowKind,
    ln_policy_setting: LnPolicySetting,
) -> Result<Option<SelectFolderSummary>> {
    select_folder_summary_for_rule_mode(
        library_db,
        score_db,
        path,
        kind,
        ln_policy_setting,
        RuleMode::Beatoraja,
    )
}

pub fn select_folder_summary_for_rule_mode(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    path: &str,
    kind: SelectRowKind,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> Result<Option<SelectFolderSummary>> {
    match kind {
        SelectRowKind::Folder => {
            folder_summary_for_song_folder(library_db, score_db, path, ln_policy_setting, rule_mode)
                .map(Some)
        }
        SelectRowKind::SearchFolder => {
            if let Some(query) = parse_search_query(path) {
                return folder_summary_for_charts(
                    score_db,
                    library_db.search_charts(query)?,
                    ln_policy_setting,
                    rule_mode,
                )
                .map(Some);
            }
            Ok(None)
        }
        SelectRowKind::TableFolder => match parse_table_path(path) {
            Some(TablePath::Table { source_url }) => folder_summary_for_table(
                library_db,
                score_db,
                source_url,
                None,
                ln_policy_setting,
                rule_mode,
            )
            .map(Some),
            Some(TablePath::Level { source_url, level }) => folder_summary_for_table(
                library_db,
                score_db,
                source_url,
                Some(level),
                ln_policy_setting,
                rule_mode,
            )
            .map(Some),
            Some(TablePath::Root) | None => Ok(None),
        },
        SelectRowKind::Song
        | SelectRowKind::Course
        | SelectRowKind::Executable
        | SelectRowKind::RandomCourse
        | SelectRowKind::Command
        | SelectRowKind::Container
        | SelectRowKind::NoSong
        | SelectRowKind::SettingsRoot
        | SelectRowKind::SettingsFolder
        | SelectRowKind::SettingsBack
        | SelectRowKind::SettingsClose
        | SelectRowKind::Config => Ok(None),
    }
}

pub(super) fn folder_summary_for_song_folder(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    folder_path: &str,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> Result<SelectFolderSummary> {
    let folder_key = folder_path.replace('\\', "/");
    let mut paths = Vec::new();
    paths.push(folder_key.clone());
    paths.extend(library_db.list_descendant_folder_paths(&folder_key)?);
    let path_refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    folder_summary_for_charts(
        score_db,
        library_db.list_charts_in_folders(&path_refs)?,
        ln_policy_setting,
        rule_mode,
    )
}

pub(super) fn folder_summary_for_table(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    source_url: &str,
    level_filter: Option<&str>,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> Result<SelectFolderSummary> {
    let mut entries =
        library_db.list_table_entries_with_chart_at_level(source_url, level_filter)?;
    entries = dedupe_table_entries(entries);
    let charts = entries.into_iter().filter_map(|entry| entry.chart).collect();
    folder_summary_for_charts(score_db, charts, ln_policy_setting, rule_mode)
}

pub(super) fn folder_summary_for_charts(
    score_db: &ScoreDatabase,
    charts: Vec<ChartListItem>,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> Result<SelectFolderSummary> {
    let mut seen = HashSet::new();
    let keys: Vec<ScoreKey> = charts
        .iter()
        .filter_map(|chart| {
            let key = score_key_for_chart(chart, ln_policy_setting, rule_mode);
            seen.insert(key).then_some(key)
        })
        .collect();
    let score_map: HashMap<ScoreKey, BestScoreSummary> = score_db
        .best_scores_for_charts(&keys)?
        .into_iter()
        .map(|score| {
            (
                ScoreKey::with_double_option(
                    score.chart_sha256,
                    score.ln_policy,
                    score.double_option,
                )
                .with_rule_mode(score.rule_mode),
                score,
            )
        })
        .collect();

    let mut lamp_counts = [0; 11];
    for key in keys {
        let index = score_map
            .get(&key)
            .map(|score| folder_lamp_index_from_clear_type(&score.clear_type))
            .unwrap_or(0);
        lamp_counts[index] += 1;
    }
    Ok(SelectFolderSummary { lamp_counts })
}

pub(super) fn replay_slot_map(
    score_db: &ScoreDatabase,
    keys: &[ScoreKey],
) -> Result<HashMap<ScoreKey, [bool; 4]>> {
    Ok(score_db
        .replay_slots_for_charts(keys)?
        .into_iter()
        .map(
            |ReplaySlotSummary {
                 chart_sha256,
                 ln_policy,
                 double_option,
                 rule_mode,
                 replay_slots,
             }| {
                (
                    ScoreKey::with_options(chart_sha256, ln_policy, double_option, rule_mode),
                    replay_slots,
                )
            },
        )
        .collect())
}

pub(super) fn score_key_for_chart(
    chart: &ChartListItem,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> ScoreKey {
    ScoreKey::new(chart.sha256, score_ln_policy(ln_policy_setting, chart.ln_profile))
        .with_rule_mode(rule_mode)
}
