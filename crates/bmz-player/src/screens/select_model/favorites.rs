use super::*;

pub fn load_select_items_for_favorite_charts(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    collection_db: &CollectionDatabase,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
    table_source_order: &[String],
    active_song_roots: Option<&[String]>,
    active_table_sources: Option<&[String]>,
) -> Result<Vec<SelectItem>> {
    let records = collection_db.favorite_chart_records()?;
    let mut found_charts = Vec::new();
    let mut missing_records = Vec::new();
    let mut seen_chart_ids = HashSet::new();
    for record in records {
        let mut charts = library_db.list_charts_by_sha256(record.chart_sha256)?;
        retain_active_charts(&mut charts, active_song_roots);
        if charts.is_empty() {
            missing_records.push(record);
            continue;
        }
        for chart in charts {
            if seen_chart_ids.insert(chart.chart_id) {
                found_charts.push(chart);
            }
        }
    }

    let mut items = chart_items_with_enrichment(
        library_db,
        score_db,
        found_charts,
        ln_policy_setting,
        rule_mode,
        table_source_order,
        active_table_sources,
    )?;
    for record in missing_records {
        items.push(missing_favorite_chart_item(score_db, record, rule_mode)?);
    }
    apply_collection_flags(library_db, collection_db, &mut items)?;
    Ok(items)
}

pub fn load_select_items_for_favorite_songs(
    collection_db: &CollectionDatabase,
) -> Result<Vec<SelectItem>> {
    Ok(collection_db
        .favorite_song_records()?
        .into_iter()
        .map(|record| SelectItem::Folder {
            path: favorite_song_detail_path(record.representative_sha256),
            name: if record.title_hint.is_empty() {
                short_sha_title(record.representative_sha256)
            } else {
                record.title_hint
            },
            kind: SelectRowKind::TableFolder,
            summary: None,
        })
        .collect())
}

pub fn load_select_items_for_favorite_song(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    collection_db: &CollectionDatabase,
    representative_sha256: [u8; 32],
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
    table_source_order: &[String],
    active_song_roots: Option<&[String]>,
    active_table_sources: Option<&[String]>,
) -> Result<Vec<SelectItem>> {
    let Some(record) = collection_db
        .favorite_song_records()?
        .into_iter()
        .find(|record| record.representative_sha256 == representative_sha256)
    else {
        return Ok(Vec::new());
    };
    let folder_paths = resolved_favorite_song_folders(library_db, &record)?;
    let folder_refs: Vec<&str> = folder_paths.iter().map(String::as_str).collect();
    let mut charts = library_db.list_charts_in_folders(&folder_refs)?;
    retain_active_charts(&mut charts, active_song_roots);
    let mut items = chart_items_with_enrichment(
        library_db,
        score_db,
        charts,
        ln_policy_setting,
        rule_mode,
        table_source_order,
        active_table_sources,
    )?;
    if items.is_empty() {
        items.push(missing_favorite_song_item(score_db, record, rule_mode)?);
    }
    apply_collection_flags(library_db, collection_db, &mut items)?;
    Ok(items)
}

pub fn favorite_song_representatives_for_folder(
    library_db: &LibraryDatabase,
    collection_db: &CollectionDatabase,
    folder_path: &str,
) -> Result<Vec<[u8; 32]>> {
    let folder_key = folder_path.replace('\\', "/");
    let mut representatives = Vec::new();
    for record in collection_db.favorite_song_records()? {
        let folders = resolved_favorite_song_folders(library_db, &record)?;
        if folders.iter().any(|folder| folder == &folder_key) {
            representatives.push(record.representative_sha256);
        }
    }
    Ok(representatives)
}

pub fn apply_collection_flags(
    library_db: &LibraryDatabase,
    collection_db: &CollectionDatabase,
    items: &mut [SelectItem],
) -> Result<()> {
    SelectCollectionCache::default().apply(library_db, collection_db, items)
}

#[derive(Default)]
pub struct SelectCollectionCache {
    cached: Option<CachedCollectionFlags>,
}

struct CachedCollectionFlags {
    revisions: [(i64, i64); 2],
    charts: HashSet<[u8; 32]>,
    folders: HashSet<String>,
}

impl SelectCollectionCache {
    /// Connections must remain the same; profile installation explicitly invalidates this cache.
    pub fn invalidate(&mut self) {
        self.cached = None;
    }

    pub fn apply(
        &mut self,
        library_db: &LibraryDatabase,
        collection_db: &CollectionDatabase,
        items: &mut [SelectItem],
    ) -> Result<()> {
        if !items.iter().any(|item| matches!(item, SelectItem::Chart(_))) {
            return Ok(());
        }
        // total_changes sees our writes; data_version sees commits on scan/other connections.
        let revision = |conn: &rusqlite::Connection| -> rusqlite::Result<(i64, i64)> {
            conn.query_row(
                "SELECT total_changes(), data_version FROM pragma_data_version",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        };
        let revisions = [revision(library_db.conn())?, revision(collection_db.conn())?];
        if self.cached.as_ref().is_none_or(|cached| cached.revisions != revisions) {
            self.cached = Some(CachedCollectionFlags {
                revisions,
                charts: collection_db.favorite_chart_set()?,
                folders: favorite_song_folder_set(library_db, collection_db)?,
            });
        }
        let cached = self.cached.as_ref().expect("collection flags loaded");
        for item in items {
            let SelectItem::Chart(row) = item else { continue };
            row.favorite_chart =
                row.score_sha256().is_some_and(|sha256| cached.charts.contains(&sha256));
            row.favorite_song =
                row.chart.as_ref().is_some_and(|chart| cached.folders.contains(&chart.folder_path));
        }
        Ok(())
    }
}

pub(super) fn missing_favorite_chart_item(
    score_db: &ScoreDatabase,
    record: FavoriteChartRecord,
    rule_mode: RuleMode,
) -> Result<SelectItem> {
    let (best_score, replay_slots) =
        score_and_replays_for_missing_favorite(score_db, record.chart_sha256, rule_mode)?;
    Ok(SelectItem::Chart(SelectChartRow {
        chart: None,
        chart_analysis: None,
        has_document: false,
        fallback_title: fallback_favorite_title(&record.title_hint, record.chart_sha256),
        fallback_artist: record.artist_hint,
        entry_sha256: Some(record.chart_sha256),
        download_metadata: ChartDownloadMetadata::default(),
        best_score,
        replay_slots,
        favorite_chart: true,
        favorite_song: false,
        table_level: String::new(),
        table_text: DifficultyTableText::default(),
    }))
}

pub(super) fn missing_favorite_song_item(
    score_db: &ScoreDatabase,
    record: FavoriteSongRecord,
    rule_mode: RuleMode,
) -> Result<SelectItem> {
    let (best_score, replay_slots) =
        score_and_replays_for_missing_favorite(score_db, record.representative_sha256, rule_mode)?;
    Ok(SelectItem::Chart(SelectChartRow {
        chart: None,
        chart_analysis: None,
        has_document: false,
        fallback_title: fallback_favorite_title(&record.title_hint, record.representative_sha256),
        fallback_artist: record.artist_hint,
        entry_sha256: Some(record.representative_sha256),
        download_metadata: ChartDownloadMetadata::default(),
        best_score,
        replay_slots,
        favorite_chart: false,
        favorite_song: true,
        table_level: String::new(),
        table_text: DifficultyTableText::default(),
    }))
}

pub(super) fn score_and_replays_for_missing_favorite(
    score_db: &ScoreDatabase,
    sha256: [u8; 32],
    rule_mode: RuleMode,
) -> Result<(Option<BestScoreSummary>, [bool; 4])> {
    let key = ScoreKey::new(sha256, LnScorePolicy::ForceLn).with_rule_mode(rule_mode);
    let best_score = score_db.best_scores_for_charts(&[key])?.into_iter().next();
    let mut replay_slots_map = replay_slot_map(score_db, &[key])?;
    let replay_slots = replay_slots_map.remove(&key).unwrap_or([false; 4]);
    Ok((best_score, replay_slots))
}

pub(super) fn fallback_favorite_title(title_hint: &str, sha256: [u8; 32]) -> String {
    if title_hint.is_empty() { short_sha_title(sha256) } else { title_hint.to_string() }
}

pub(super) fn short_sha_title(sha256: [u8; 32]) -> String {
    let hex = hash_to_hex(&sha256);
    format!("sha256:{}", &hex[..12])
}

pub(super) fn resolved_favorite_song_folders(
    library_db: &LibraryDatabase,
    record: &FavoriteSongRecord,
) -> Result<Vec<String>> {
    let mut folders = Vec::new();
    let mut seen = HashSet::new();
    for chart in library_db.list_charts_by_sha256(record.representative_sha256)? {
        let folder = chart.folder_path;
        if seen.insert(folder.clone()) {
            folders.push(folder);
        }
    }
    if folders.is_empty() && !record.origin_folder_hint.is_empty() {
        let folder = record.origin_folder_hint.replace('\\', "/");
        folders.push(folder);
    }
    Ok(folders)
}

pub(super) fn favorite_song_folder_set(
    library_db: &LibraryDatabase,
    collection_db: &CollectionDatabase,
) -> Result<HashSet<String>> {
    let mut folders = HashSet::new();
    for record in collection_db.favorite_song_records()? {
        folders.extend(resolved_favorite_song_folders(library_db, &record)?);
    }
    Ok(folders)
}
