use std::collections::{HashMap, HashSet};

use anyhow::Result;
use bmz_core::course::CourseKind;
use bmz_render::scene::SelectRowKind;

use crate::storage::common::hash_to_hex;
use crate::storage::library_db::{ChartListItem, LibraryDatabase, TableEntryListItem};
use crate::storage::score_db::{BestScoreSummary, ReplaySlotSummary, ScoreDatabase};

/// Virtual path prefix used for difficulty-table navigation.
/// `"bmz-table:"` is the root that lists all registered tables.
/// `"bmz-table:{source_url}"` lists the level folders of that table.
/// `"bmz-table:{source_url}\n{level}"` lists the charts of that table level.
pub const TABLE_ROOT_PATH: &str = "bmz-table:";

/// Virtual path for the course list root.
pub const COURSE_ROOT_PATH: &str = "bmz-course:";

/// Separator between a table's `source_url` and a level inside a virtual
/// table path.  A newline never appears in a difficulty-table source URL,
/// so it is safe to use as a delimiter.
pub const TABLE_LEVEL_SEPARATOR: char = '\n';

/// Parsed form of a `"bmz-table:..."` virtual path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TablePath<'a> {
    /// `"bmz-table:"` — list of all registered tables.
    Root,
    /// `"bmz-table:{source_url}"` — list of level folders for the table.
    Table { source_url: &'a str },
    /// `"bmz-table:{source_url}\n{level}"` — charts of a specific level.
    Level { source_url: &'a str, level: &'a str },
}

/// Parses a virtual difficulty-table path. Returns `None` if `path` is not a
/// `"bmz-table:"` path.
pub fn parse_table_path(path: &str) -> Option<TablePath<'_>> {
    let rest = path.strip_prefix(TABLE_ROOT_PATH)?;
    if rest.is_empty() {
        return Some(TablePath::Root);
    }
    match rest.split_once(TABLE_LEVEL_SEPARATOR) {
        Some((source_url, level)) => Some(TablePath::Level { source_url, level }),
        None => Some(TablePath::Table { source_url: rest }),
    }
}

/// Returns the difficulty-table source URL implied by the current select navigation
/// context, if any.
pub fn table_source_url_from_context(
    folder_stack: &[String],
    selected: Option<&SelectItem>,
) -> Option<String> {
    if let Some(path) = folder_stack.last()
        && path.starts_with(TABLE_ROOT_PATH)
    {
        match parse_table_path(path) {
            Some(TablePath::Table { source_url }) | Some(TablePath::Level { source_url, .. }) => {
                return Some(source_url.to_string());
            }
            Some(TablePath::Root) | None => {}
        }
    }

    if let Some(SelectItem::Folder { path, .. }) = selected
        && path.starts_with(TABLE_ROOT_PATH)
        && path != TABLE_ROOT_PATH
    {
        return parse_table_path(path).and_then(|parsed| match parsed {
            TablePath::Table { source_url } => Some(source_url.to_string()),
            TablePath::Level { source_url, .. } => Some(source_url.to_string()),
            TablePath::Root => None,
        });
    }

    None
}

/// Returns the song folder path to scan implied by the current select navigation
/// context, if any.
pub fn song_scan_path_from_context(
    _folder_stack: &[String],
    selected: Option<&SelectItem>,
) -> Option<String> {
    match selected {
        Some(SelectItem::Folder { path, kind, .. }) if *kind == SelectRowKind::Folder => {
            Some(path.clone())
        }
        Some(SelectItem::Chart(row)) if row.in_library() => {
            row.chart.as_ref().map(|chart| chart.folder_path.clone())
        }
        _ => None,
    }
}

fn insert_table_level(map: &mut HashMap<String, String>, key: String, symbol: &str, level: &str) {
    let entry = format!("{symbol}{level}");
    map.entry(key)
        .and_modify(|v| {
            v.push('/');
            v.push_str(&entry);
        })
        .or_insert(entry);
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectChartRow {
    pub chart: Option<ChartListItem>,
    pub fallback_title: String,
    pub fallback_artist: String,
    pub entry_sha256: Option<[u8; 32]>,
    pub best_score: Option<BestScoreSummary>,
    pub replay_slots: [bool; 4],
    pub table_level: String,
}

impl SelectChartRow {
    pub fn display_title(&self) -> &str {
        self.chart
            .as_ref()
            .map(|chart| chart.title.as_str())
            .filter(|title| !title.is_empty())
            .unwrap_or(self.fallback_title.as_str())
    }

    pub fn display_artist(&self) -> &str {
        self.chart
            .as_ref()
            .map(|chart| chart.artist.as_str())
            .filter(|artist| !artist.is_empty())
            .unwrap_or(self.fallback_artist.as_str())
    }

    pub fn in_library(&self) -> bool {
        self.chart.is_some()
    }

    pub fn score_sha256(&self) -> Option<[u8; 32]> {
        self.chart.as_ref().map(|chart| chart.sha256).or(self.entry_sha256)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectCourseRow {
    pub course_id: i64,
    pub title: String,
    pub kind: CourseKind,
    /// Total number of entries in the course.
    pub entry_count: usize,
    /// Number of entries whose `chart_id` is resolved in the local library.
    pub resolved_count: usize,
    /// Total notes across all resolved entries.
    pub total_notes: u32,
    /// Sum of length in milliseconds across resolved entries.
    pub total_length_ms: i64,
    /// Minimum / maximum BPM among resolved entries.
    pub min_bpm: f32,
    pub max_bpm: f32,
    /// Difficulty band derived from constraints (e.g. "DAN" / "COURSE").
    pub category_label: String,
    /// Trophy names defined for this course (e.g. ["silvermedal", "goldmedal"]).
    pub trophy_names: Vec<String>,
    /// Entries inside the course, used by the preview panel.
    pub entry_previews: Vec<CourseEntryPreview>,
    /// Best persisted course score, if any.  Populated from the
    /// `course_scores` table; `None` when the course has never been played
    /// successfully or when the lookup failed.
    pub best_score: Option<crate::storage::library_db::CourseBestScore>,
    /// Which of the four course replay slots have a saved attempt.  Used by
    /// the select skin to render slot indicators on course rows.
    pub replay_slots: [bool; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct CourseEntryPreview {
    /// Title taken from the resolved library chart when available, otherwise
    /// the title_hint declared in the course JSON.
    pub title: String,
    pub artist: String,
    pub play_level: String,
    pub difficulty_name: String,
    pub total_notes: u32,
    /// True when this entry is resolved to a chart in the local library.
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum SelectItem {
    Folder { path: String, name: String, kind: SelectRowKind },
    Chart(SelectChartRow),
    Course(SelectCourseRow),
}

impl SelectItem {
    pub fn display_name(&self) -> &str {
        match self {
            Self::Folder { name, .. } => name.as_str(),
            Self::Chart(row) => row.display_title(),
            Self::Course(row) => row.title.as_str(),
        }
    }
}

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
            SelectItem::Folder { path: path.clone(), name, kind: SelectRowKind::Folder }
        })
        .collect()
}

/// Returns one folder item per registered difficulty table.
pub fn table_folder_items(library_db: &LibraryDatabase) -> Result<Vec<SelectItem>> {
    let tables = library_db.list_difficulty_tables()?;
    Ok(tables
        .into_iter()
        .map(|t| SelectItem::Folder {
            path: format!("{TABLE_ROOT_PATH}{}", t.source_url),
            name: format!("[{}] {}", t.symbol, t.name),
            kind: SelectRowKind::TableFolder,
        })
        .collect())
}

/// Returns a folder item for the course list root.
pub fn course_root_item() -> SelectItem {
    SelectItem::Folder {
        path: COURSE_ROOT_PATH.to_string(),
        name: "COURSE".to_string(),
        kind: SelectRowKind::TableFolder,
    }
}

/// Loads manually-imported courses (not from a difficulty table) as `SelectItem::Course` entries.
/// Table-sourced courses appear inside each table's folder via `table_level_folder_items`.
pub fn load_select_items_for_courses(library_db: &LibraryDatabase) -> Result<Vec<SelectItem>> {
    let courses = library_db.list_courses()?;
    Ok(courses
        .into_iter()
        .filter(|stored| !stored.source.starts_with("table:"))
        .map(|stored| build_select_course_row(library_db, stored))
        .collect())
}

/// Aggregates per-entry chart stats into a `SelectCourseRow`.
fn build_select_course_row(
    library_db: &LibraryDatabase,
    stored: crate::storage::library_db::StoredCourse,
) -> SelectItem {
    let entry_count = stored.definition.entries.len();
    let resolved_count = stored.definition.entries.iter().filter(|e| e.chart_id.is_some()).count();

    let chart_ids: Vec<i64> = stored.definition.entries.iter().filter_map(|e| e.chart_id).collect();
    let charts = library_db.list_charts_by_ids(&chart_ids).unwrap_or_default();
    let chart_by_id: std::collections::HashMap<i64, &ChartListItem> =
        charts.iter().map(|c| (c.chart_id, c)).collect();

    let entry_previews: Vec<CourseEntryPreview> = stored
        .definition
        .entries
        .iter()
        .map(|entry| match entry.chart_id.and_then(|id| chart_by_id.get(&id).copied()) {
            Some(chart) => CourseEntryPreview {
                title: chart.title.clone(),
                artist: chart.artist.clone(),
                play_level: chart.play_level.clone(),
                difficulty_name: chart.difficulty_name.clone(),
                total_notes: chart.total_notes,
                resolved: true,
            },
            None => CourseEntryPreview {
                title: entry.title_hint.clone(),
                artist: String::new(),
                play_level: String::new(),
                difficulty_name: String::new(),
                total_notes: 0,
                resolved: false,
            },
        })
        .collect();

    let total_notes: u32 = charts.iter().map(|c| c.total_notes).sum();
    let total_length_ms: i64 = charts.iter().map(|c| c.length_ms).sum();
    let min_bpm = charts.iter().map(|c| c.min_bpm as f32).fold(f32::INFINITY, f32::min);
    let max_bpm = charts.iter().map(|c| c.max_bpm as f32).fold(f32::NEG_INFINITY, f32::max);
    let (min_bpm, max_bpm) =
        if min_bpm.is_finite() && max_bpm.is_finite() { (min_bpm, max_bpm) } else { (0.0, 0.0) };

    let category_label = match stored.definition.kind {
        bmz_core::course::CourseKind::Dan => "DAN".to_string(),
        bmz_core::course::CourseKind::Course => "COURSE".to_string(),
    };
    let trophy_names: Vec<String> =
        stored.definition.trophies.iter().map(|t| t.name.clone()).collect();

    let best_score = library_db.best_course_score(stored.id).unwrap_or_else(|error| {
        tracing::warn!(%error, course_id = stored.id, "failed to load best course score");
        None
    });
    let replay_slots = library_db.course_replay_slot_presence(stored.id).unwrap_or_else(|error| {
        tracing::warn!(
            %error,
            course_id = stored.id,
            "failed to load course_replay_slot_presence"
        );
        [false; 4]
    });

    SelectItem::Course(SelectCourseRow {
        course_id: stored.id,
        title: stored.definition.title,
        kind: stored.definition.kind,
        entry_count,
        resolved_count,
        total_notes,
        total_length_ms,
        min_bpm,
        max_bpm,
        category_label,
        trophy_names,
        entry_previews,
        best_score,
        replay_slots,
    })
}

/// Returns one folder item per level of the difficulty table, ordered by the
/// table's `level_order`, followed by any courses imported from that table.
pub fn table_level_folder_items(
    library_db: &LibraryDatabase,
    source_url: &str,
) -> Result<Vec<SelectItem>> {
    let Some(table) =
        library_db.list_difficulty_tables()?.into_iter().find(|t| t.source_url == source_url)
    else {
        return Ok(Vec::new());
    };

    let mut items: Vec<SelectItem> = table
        .level_order
        .iter()
        .map(|level| SelectItem::Folder {
            path: format!("{TABLE_ROOT_PATH}{source_url}{TABLE_LEVEL_SEPARATOR}{level}"),
            name: format!("{}{}", table.symbol, level),
            kind: SelectRowKind::TableFolder,
        })
        .collect();

    // Append courses that were imported from this table.
    let table_source = format!("table:{source_url}");
    if let Ok(courses) = library_db.list_courses_by_source(&table_source) {
        tracing::info!(source = %table_source, count = courses.len(), "courses found for table");
        for stored in courses {
            items.push(build_select_course_row(library_db, stored));
        }
    }

    Ok(items)
}

/// Loads charts that are stored in the local library and belong to the given
/// difficulty table (identified by `source_url`).  Charts are sorted by the
/// table's `level_order`, then by title within each level.
pub fn load_select_items_in_table(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    source_url: &str,
) -> Result<Vec<SelectItem>> {
    load_select_items_in_table_filtered(library_db, score_db, source_url, None)
}

/// Loads the charts of a single level of the difficulty table.
pub fn load_select_items_in_table_level(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    source_url: &str,
    level: &str,
) -> Result<Vec<SelectItem>> {
    load_select_items_in_table_filtered(library_db, score_db, source_url, Some(level))
}

fn load_select_items_in_table_filtered(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    source_url: &str,
    level_filter: Option<&str>,
) -> Result<Vec<SelectItem>> {
    // Fetch table metadata for symbol and level ordering.
    let (symbol, level_order) = library_db
        .list_difficulty_tables()?
        .into_iter()
        .find(|t| t.source_url == source_url)
        .map(|t| (t.symbol, t.level_order))
        .unwrap_or_default();

    let mut entries = library_db.list_table_entries_with_chart(source_url)?;
    if let Some(level) = level_filter {
        entries.retain(|entry| entry.level == level);
    }
    entries = dedupe_table_entries(entries);

    // Sort by the table's level_order, then alphabetically by display title.
    let level_rank = |level: &str| -> usize {
        level_order.iter().position(|l| l == level).unwrap_or(usize::MAX)
    };
    entries.sort_by(|a, b| {
        level_rank(&a.level).cmp(&level_rank(&b.level)).then_with(|| {
            entry_display_title(a).to_lowercase().cmp(&entry_display_title(b).to_lowercase())
        })
    });

    // Batch score lookup.
    let hashes: Vec<[u8; 32]> = entries.iter().filter_map(entry_score_sha256).collect();
    let mut score_map: HashMap<[u8; 32], BestScoreSummary> = score_db
        .best_scores_for_charts(&hashes)?
        .into_iter()
        .map(|s| (s.chart_sha256, s))
        .collect();
    let mut replay_slot_map = replay_slot_map(score_db, &hashes)?;

    Ok(entries
        .into_iter()
        .map(|entry| {
            let table_level = format!("{symbol}{}", entry.level);
            let score_sha256 = entry_score_sha256(&entry);
            let best_score = score_sha256.and_then(|hash| score_map.remove(&hash));
            let replay_slots =
                score_sha256.and_then(|hash| replay_slot_map.remove(&hash)).unwrap_or([false; 4]);
            SelectItem::Chart(select_chart_row_from_table_entry(
                entry,
                best_score,
                replay_slots,
                table_level,
            ))
        })
        .collect())
}

fn entry_display_title(entry: &TableEntryListItem) -> &str {
    entry
        .chart
        .as_ref()
        .map(|chart| chart.title.as_str())
        .filter(|title| !title.is_empty())
        .unwrap_or(entry.title.as_str())
}

/// Collapses duplicate difficulty-table rows that refer to the same local chart.
///
/// Tables often contain redundant hash rows for the same song.  When also showing
/// unmatched entries we drop duplicate matched charts and stale rows that no longer
/// resolve to a unique missing song.
fn dedupe_table_entries(entries: Vec<TableEntryListItem>) -> Vec<TableEntryListItem> {
    let mut claimed_md5_by_level: HashMap<String, HashSet<String>> = HashMap::new();
    let mut claimed_sha256_by_level: HashMap<String, HashSet<String>> = HashMap::new();
    let mut claimed_titles_by_level: HashMap<String, HashSet<String>> = HashMap::new();

    for entry in &entries {
        let Some(chart) = &entry.chart else {
            continue;
        };
        let md5s = claimed_md5_by_level.entry(entry.level.clone()).or_default();
        let sha256s = claimed_sha256_by_level.entry(entry.level.clone()).or_default();
        let titles = claimed_titles_by_level.entry(entry.level.clone()).or_default();

        if entry.md5.len() >= 24 {
            md5s.insert(entry.md5.clone());
        }
        if entry.sha256.len() >= 24 {
            sha256s.insert(entry.sha256.clone());
        }
        md5s.insert(hash_to_hex(&chart.md5));
        sha256s.insert(hash_to_hex(&chart.sha256));
        if !entry.title.is_empty() {
            titles.insert(entry.title.to_lowercase());
        }
        if !chart.title.is_empty() {
            titles.insert(chart.title.to_lowercase());
        }
    }

    let mut seen_chart_sha256_by_level: HashSet<(String, [u8; 32])> = HashSet::new();
    let mut seen_unmatched_keys: HashSet<(String, String, String)> = HashSet::new();
    let mut result = Vec::with_capacity(entries.len());

    for entry in entries {
        if let Some(chart) = &entry.chart {
            let identity = (entry.level.clone(), chart.sha256);
            if !seen_chart_sha256_by_level.insert(identity) {
                continue;
            }
            result.push(entry);
            continue;
        }

        if entry_claimed_by_matched_entry(&entry, &claimed_md5_by_level, &claimed_sha256_by_level) {
            continue;
        }
        if !entry.title.is_empty()
            && claimed_titles_by_level
                .get(&entry.level)
                .is_some_and(|titles| titles.contains(&entry.title.to_lowercase()))
        {
            continue;
        }

        let unmatched_key = (entry.level.clone(), entry.md5.clone(), entry.sha256.clone());
        if !seen_unmatched_keys.insert(unmatched_key) {
            continue;
        }

        result.push(entry);
    }

    result
}

fn entry_claimed_by_matched_entry(
    entry: &TableEntryListItem,
    claimed_md5_by_level: &HashMap<String, HashSet<String>>,
    claimed_sha256_by_level: &HashMap<String, HashSet<String>>,
) -> bool {
    if entry.md5.len() >= 24
        && claimed_md5_by_level.get(&entry.level).is_some_and(|hashes| hashes.contains(&entry.md5))
    {
        return true;
    }
    entry.sha256.len() >= 24
        && claimed_sha256_by_level
            .get(&entry.level)
            .is_some_and(|hashes| hashes.contains(&entry.sha256))
}

fn entry_score_sha256(entry: &TableEntryListItem) -> Option<[u8; 32]> {
    if let Some(chart) = &entry.chart {
        return Some(chart.sha256);
    }
    if entry.sha256.len() >= 48 {
        return hex_to_hash::<32>(&entry.sha256).ok();
    }
    None
}

fn select_chart_row_from_table_entry(
    entry: TableEntryListItem,
    best_score: Option<BestScoreSummary>,
    replay_slots: [bool; 4],
    table_level: String,
) -> SelectChartRow {
    let entry_sha256 = entry_score_sha256(&entry);
    SelectChartRow {
        chart: entry.chart,
        fallback_title: entry.title,
        fallback_artist: entry.artist,
        entry_sha256,
        best_score,
        replay_slots,
        table_level,
    }
}

fn hex_to_hash<const N: usize>(hex: &str) -> Result<[u8; N]> {
    crate::storage::common::hex_to_hash(hex).map_err(Into::into)
}

/// Loads folders and charts immediately under `folder_path`.
/// Non-leaf folders are listed first, followed by charts.
/// Leaf folders (subfolders that contain charts but no further subfolders) are
/// flattened: their charts appear directly at this level instead of as a folder entry.
pub fn load_select_items_in_folder(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    folder_path: &str,
) -> Result<Vec<SelectItem>> {
    // 子孫 folder_path を 1 回だけ引き、直下の子と各子が leaf かどうかを
    // Rust 側で集計する。`/` 区切り後の最初のセグメントが「直下の子の名前」、
    // それより深いセグメントが残っていれば leaf でない。
    let folder_key = folder_path.replace('\\', "/");
    let prefix_len = folder_key.len() + 1; // including the trailing '/'
    let descendants = library_db.list_descendant_folder_paths(&folder_key)?;

    // child_name -> has_grandchild (= 非 leaf)
    let mut child_state: std::collections::BTreeMap<String, bool> =
        std::collections::BTreeMap::new();
    for descendant in &descendants {
        let Some(rest) = descendant.get(prefix_len..) else { continue };
        let (child, deeper) = match rest.split_once('/') {
            Some((head, tail)) => (head, !tail.is_empty()),
            None => (rest, false),
        };
        if child.is_empty() {
            continue;
        }
        let entry = child_state.entry(child.to_string()).or_insert(false);
        if deeper {
            *entry = true;
        }
    }

    let mut non_leaf_folders: Vec<(String, String)> = Vec::new();
    let mut leaf_folder_paths: Vec<String> = Vec::new();
    for (name, has_grandchild) in child_state {
        let child_path = format!("{folder_key}/{name}");
        if has_grandchild {
            non_leaf_folders.push((child_path, name));
        } else {
            leaf_folder_paths.push(child_path);
        }
    }
    // 表示順は元実装に合わせ COLLATE NOCASE 相当。BTreeMap は code-point 順
    // なので、ここで lowercase 比較に揃え直す。
    non_leaf_folders.sort_by_key(|(_, name)| name.to_lowercase());

    // 親フォルダ自身 + leaf 子フォルダ群の charts を 1 つのプリペアド
    // ステートメントで取得する。
    let mut fetch_paths: Vec<&str> = Vec::with_capacity(1 + leaf_folder_paths.len());
    fetch_paths.push(folder_key.as_str());
    fetch_paths.extend(leaf_folder_paths.iter().map(String::as_str));
    let all_charts = library_db.list_charts_in_folders(&fetch_paths)?;

    let hashes: Vec<[u8; 32]> = all_charts.iter().map(|c| c.sha256).collect();
    let mut score_map: HashMap<[u8; 32], BestScoreSummary> = score_db
        .best_scores_for_charts(&hashes)?
        .into_iter()
        .map(|s| (s.chart_sha256, s))
        .collect();
    let mut replay_slot_map = replay_slot_map(score_db, &hashes)?;

    // MD5 lookup (multiple tables per MD5 joined with '/')
    let md5_hexes: Vec<String> = all_charts.iter().map(|c| hash_to_hex(&c.md5)).collect();
    let md5_refs: Vec<&str> = md5_hexes.iter().map(|s| s.as_str()).collect();
    let mut md5_level_map: HashMap<String, String> = HashMap::new();
    for e in library_db.list_difficulty_table_entries_by_md5s(&md5_refs)? {
        insert_table_level(&mut md5_level_map, e.md5, &e.table_symbol, &e.level);
    }

    // SHA256 fallback for charts not matched by MD5
    let missing_sha256_hexes: Vec<String> = all_charts
        .iter()
        .filter(|c| !md5_level_map.contains_key(&hash_to_hex(&c.md5)))
        .map(|c| hash_to_hex(&c.sha256))
        .collect();
    let mut sha256_level_map: HashMap<String, String> = HashMap::new();
    if !missing_sha256_hexes.is_empty() {
        let sha256_refs: Vec<&str> = missing_sha256_hexes.iter().map(|s| s.as_str()).collect();
        for e in library_db.list_difficulty_table_entries_by_sha256s(&sha256_refs)? {
            insert_table_level(&mut sha256_level_map, e.sha256, &e.table_symbol, &e.level);
        }
    }

    let mut items = Vec::with_capacity(non_leaf_folders.len() + all_charts.len());

    for (path, name) in non_leaf_folders {
        items.push(SelectItem::Folder { path, name, kind: SelectRowKind::Folder });
    }
    for chart in all_charts {
        let best_score = score_map.remove(&chart.sha256);
        let replay_slots = replay_slot_map.remove(&chart.sha256).unwrap_or([false; 4]);
        let table_level = md5_level_map
            .remove(&hash_to_hex(&chart.md5))
            .or_else(|| sha256_level_map.remove(&hash_to_hex(&chart.sha256)))
            .unwrap_or_default();
        items.push(SelectItem::Chart(SelectChartRow {
            chart: Some(chart),
            fallback_title: String::new(),
            fallback_artist: String::new(),
            entry_sha256: None,
            best_score,
            replay_slots,
            table_level,
        }));
    }

    Ok(items)
}

fn replay_slot_map(
    score_db: &ScoreDatabase,
    hashes: &[[u8; 32]],
) -> Result<HashMap<[u8; 32], [bool; 4]>> {
    Ok(score_db
        .replay_slots_for_charts(hashes)?
        .into_iter()
        .map(|ReplaySlotSummary { chart_sha256, replay_slots }| (chart_sha256, replay_slots))
        .collect())
}

#[cfg(test)]
mod tests {
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, PlayableChart};
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::ids::NoteId;
    use bmz_core::judge::{Judge, TimingSide};
    use bmz_core::time::TimeUs;
    use bmz_gameplay::judge::model::JudgementEvent;
    use bmz_gameplay::score::ScoreState;
    use rusqlite::Connection;

    use super::*;
    use crate::storage::common::configure_connection;
    use crate::storage::library_db::{ChartImportRecord, LibraryDatabase};
    use crate::storage::migration::{LIBRARY_MIGRATIONS, SCORE_MIGRATIONS, run_migrations};
    use crate::storage::score_db::{ScoreDatabase, ScoreRecord};

    #[test]
    fn load_select_items_in_folder_attaches_best_scores_by_hash() {
        let (mut library_db, mut score_db) = open_in_memory_dbs();
        let alpha = chart("Alpha");
        let beta = chart("Beta");

        library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();
        library_db.upsert_chart_import(&record_for_chart("/songs/beta.bms", &beta)).unwrap();
        score_db.insert_score(&score_for_chart(alpha.identity.file_sha256)).unwrap();

        let items = load_select_items_in_folder(&library_db, &score_db, "/songs").unwrap();

        let charts: Vec<_> = items
            .iter()
            .filter_map(|i| if let SelectItem::Chart(r) = i { Some(r) } else { None })
            .collect();
        assert_eq!(charts.len(), 2);
        assert_eq!(charts[0].display_title(), "Alpha");
        assert!(charts[0].best_score.is_some());
        assert_eq!(charts[1].display_title(), "Beta");
        assert!(charts[1].best_score.is_none());
    }

    #[test]
    fn load_select_items_in_folder_attaches_replay_slots_from_replay_slots_table() {
        let (mut library_db, mut score_db) = open_in_memory_dbs();
        let alpha = chart("Alpha");

        library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();
        for slot in 0..4_u8 {
            score_db
                .upsert_replay_slot(&crate::storage::score_db::ReplaySlotRecord {
                    chart_sha256: alpha.identity.file_sha256,
                    slot,
                    rule: crate::config::profile_config::ReplaySlotRule::Always,
                    replay_path: format!("replay/{slot}.toml"),
                    played_at: 1_700_000_030 + slot as i64,
                    ex_score: 10 * slot as u32,
                    miss_count: 0,
                    max_combo: 10,
                    clear_rank: ClearType::Normal as u8,
                })
                .unwrap();
        }

        let items = load_select_items_in_folder(&library_db, &score_db, "/songs").unwrap();

        let row = items
            .iter()
            .find_map(|i| if let SelectItem::Chart(r) = i { Some(r) } else { None })
            .unwrap();
        assert_eq!(row.replay_slots, [true, true, true, true]);
    }

    #[test]
    fn load_select_items_in_folder_flattens_leaf_subfolders() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let chart_a = chart("A");
        let chart_b = chart("B");

        // chart_b directly in /bms; chart_a is in a leaf sub-folder (no deeper nesting)
        library_db
            .upsert_chart_import(&record_for_chart("/bms/genre/song_a.bms", &chart_a))
            .unwrap();
        library_db.upsert_chart_import(&record_for_chart("/bms/song_b.bms", &chart_b)).unwrap();

        let items = load_select_items_in_folder(&library_db, &score_db, "/bms").unwrap();

        // genre is a leaf folder so its chart appears directly, not as a Folder entry
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| matches!(i, SelectItem::Chart(_))));
        let titles: Vec<_> =
            items
                .iter()
                .filter_map(|i| {
                    if let SelectItem::Chart(r) = i { Some(r.display_title()) } else { None }
                })
                .collect();
        assert!(titles.contains(&"A"));
        assert!(titles.contains(&"B"));
    }

    #[test]
    fn load_select_items_in_folder_shows_non_leaf_subfolder_as_folder() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let chart_a = chart("A");
        let chart_b = chart("B");

        // genre/subgenre/song_a — genre has a subfolder so it is non-leaf
        library_db
            .upsert_chart_import(&record_for_chart("/bms/genre/subgenre/song_a.bms", &chart_a))
            .unwrap();
        library_db.upsert_chart_import(&record_for_chart("/bms/song_b.bms", &chart_b)).unwrap();

        let items = load_select_items_in_folder(&library_db, &score_db, "/bms").unwrap();

        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], SelectItem::Folder { name, .. } if name == "genre"));
        assert!(matches!(&items[1], SelectItem::Chart(r) if r.display_title() == "B"));
    }

    #[test]
    fn root_folder_items_returns_folder_per_root() {
        let roots = vec!["/bms/a".to_string(), "/bms/b".to_string()];
        let items = root_folder_items(&roots);

        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], SelectItem::Folder { name, .. } if name == "a"));
        assert!(matches!(&items[1], SelectItem::Folder { name, .. } if name == "b"));
    }

    fn open_in_memory_dbs() -> (LibraryDatabase, ScoreDatabase) {
        let mut library_conn = Connection::open_in_memory().unwrap();
        configure_connection(&library_conn).unwrap();
        run_migrations(&mut library_conn, LIBRARY_MIGRATIONS).unwrap();
        let mut score_conn = Connection::open_in_memory().unwrap();
        configure_connection(&score_conn).unwrap();
        run_migrations(&mut score_conn, SCORE_MIGRATIONS).unwrap();
        (LibraryDatabase::from_connection(library_conn), ScoreDatabase::from_connection(score_conn))
    }

    fn chart(title: &str) -> PlayableChart {
        PlayableChart {
            identity: compute_chart_identity(title.as_bytes()),
            metadata: ChartMetadata {
                title: title.to_string(),
                artist: "artist".to_string(),
                initial_bpm: 128.0,
                ..Default::default()
            },
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            bga_events: Vec::new(),
            timing_events: Vec::new(),

            scroll_events: Vec::new(),

            speed_events: Vec::new(),
            judge_rank_events: Vec::new(),
            bgm_volume_events: Vec::new(),
            key_volume_events: Vec::new(),
            text_events: Vec::new(),
            bga_opacity_events: Vec::new(),
            bga_argb_events: Vec::new(),
            swbga_definitions: Vec::new(),
            bga_keybound_events: Vec::new(),
            bga_asset_by_bmp_key: std::collections::HashMap::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            bga_assets: Vec::new(),
            total_notes: 0,
            end_time: TimeUs(10_000_000),
        }
    }

    fn record_for_chart<'a>(path: &'a str, chart: &'a PlayableChart) -> ChartImportRecord<'a> {
        ChartImportRecord {
            root_id: None,
            file_path: std::path::Path::new(path),
            file_size: 1,
            modified_at: 1,
            scanned_at: 1,
            chart,
        }
    }

    #[test]
    fn load_select_items_attaches_table_level_via_md5() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let alpha = chart("Alpha");
        library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();

        let table = difficulty_table_for_md5(&alpha.identity.file_md5, "★", "3");
        library_db.upsert_difficulty_table(&table).unwrap();

        let items = load_select_items_in_folder(&library_db, &score_db, "/songs").unwrap();

        let row = items
            .iter()
            .find_map(|i| if let SelectItem::Chart(r) = i { Some(r) } else { None })
            .unwrap();
        assert_eq!(row.table_level, "★3");
    }

    #[test]
    fn load_select_items_joins_multiple_table_levels_with_slash() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let alpha = chart("Alpha");
        library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();

        library_db
            .upsert_difficulty_table(&difficulty_table_for_md5(&alpha.identity.file_md5, "★", "3"))
            .unwrap();
        library_db
            .upsert_difficulty_table(&difficulty_table_for_md5(&alpha.identity.file_md5, "☆", "5"))
            .unwrap();

        let items = load_select_items_in_folder(&library_db, &score_db, "/songs").unwrap();

        let row = items
            .iter()
            .find_map(|i| if let SelectItem::Chart(r) = i { Some(r) } else { None })
            .unwrap();
        assert!(row.table_level.contains("★3"), "got: {}", row.table_level);
        assert!(row.table_level.contains("☆5"), "got: {}", row.table_level);
        assert!(row.table_level.contains('/'), "got: {}", row.table_level);
    }

    #[test]
    fn load_select_items_falls_back_to_sha256_when_no_md5_match() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let alpha = chart("Alpha");
        library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();

        let table = difficulty_table_for_sha256(&alpha.identity.file_sha256, "◆", "7");
        library_db.upsert_difficulty_table(&table).unwrap();

        let items = load_select_items_in_folder(&library_db, &score_db, "/songs").unwrap();

        let row = items
            .iter()
            .find_map(|i| if let SelectItem::Chart(r) = i { Some(r) } else { None })
            .unwrap();
        assert_eq!(row.table_level, "◆7");
    }

    fn difficulty_table_for_md5(
        md5: &[u8; 16],
        symbol: &str,
        level: &str,
    ) -> crate::difficulty_table::FetchedDifficultyTable {
        use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
        FetchedDifficultyTable {
            source_url: format!("https://example.com/{symbol}/"),
            head_url: format!("https://example.com/{symbol}/header.json"),
            name: format!("Table {symbol}"),
            symbol: symbol.to_string(),
            level_order: vec![level.to_string()],
            entries: vec![FetchedTableEntry {
                level: level.to_string(),
                md5: hash_to_hex(md5),
                sha256: String::new(),
                title: String::new(),
                artist: String::new(),
                comment: String::new(),
            }],
            courses: Vec::new(),
            fetched_at: 0,
        }
    }

    fn difficulty_table_for_sha256(
        sha256: &[u8; 32],
        symbol: &str,
        level: &str,
    ) -> crate::difficulty_table::FetchedDifficultyTable {
        use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
        FetchedDifficultyTable {
            source_url: format!("https://example.com/{symbol}-sha/"),
            head_url: format!("https://example.com/{symbol}-sha/header.json"),
            name: format!("Table {symbol} SHA"),
            symbol: symbol.to_string(),
            level_order: vec![level.to_string()],
            entries: vec![FetchedTableEntry {
                level: level.to_string(),
                md5: String::new(),
                sha256: hash_to_hex(sha256),
                title: String::new(),
                artist: String::new(),
                comment: String::new(),
            }],
            courses: Vec::new(),
            fetched_at: 0,
        }
    }

    #[test]
    fn table_folder_items_returns_one_folder_per_table() {
        let (mut library_db, _) = open_in_memory_dbs();
        let alpha = chart("Alpha");
        // Register table using md5 so there's at least one entry (content does not matter here)
        let table = difficulty_table_for_md5(&alpha.identity.file_md5, "★", "1");
        library_db.upsert_difficulty_table(&table).unwrap();

        let items = table_folder_items(&library_db).unwrap();

        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            SelectItem::Folder { path, name, kind }
            if path.starts_with(TABLE_ROOT_PATH) && name.contains("★") && *kind == SelectRowKind::TableFolder
        ));
    }

    #[test]
    fn load_select_items_in_table_returns_charts_sorted_by_level_order() {
        let (mut library_db, score_db) = open_in_memory_dbs();

        let hard = chart("Hard Song");
        let easy = chart("Easy Song");
        library_db.upsert_chart_import(&record_for_chart("/songs/hard.bms", &hard)).unwrap();
        library_db.upsert_chart_import(&record_for_chart("/songs/easy.bms", &easy)).unwrap();

        // Table has level_order ["5", "10"] — easy(5) before hard(10)
        use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
        let table = FetchedDifficultyTable {
            source_url: "https://example.com/table/".to_string(),
            head_url: "https://example.com/table/header.json".to_string(),
            name: "Test Table".to_string(),
            symbol: "★".to_string(),
            level_order: vec!["5".to_string(), "10".to_string()],
            entries: vec![
                FetchedTableEntry {
                    level: "10".to_string(),
                    md5: hash_to_hex(&hard.identity.file_md5),
                    sha256: String::new(),
                    title: String::new(),
                    artist: String::new(),
                    comment: String::new(),
                },
                FetchedTableEntry {
                    level: "5".to_string(),
                    md5: hash_to_hex(&easy.identity.file_md5),
                    sha256: String::new(),
                    title: String::new(),
                    artist: String::new(),
                    comment: String::new(),
                },
            ],
            courses: Vec::new(),
            fetched_at: 0,
        };
        library_db.upsert_difficulty_table(&table).unwrap();

        let items =
            load_select_items_in_table(&library_db, &score_db, "https://example.com/table/")
                .unwrap();

        assert_eq!(items.len(), 2);
        let titles: Vec<_> =
            items
                .iter()
                .filter_map(|i| {
                    if let SelectItem::Chart(r) = i { Some(r.display_title()) } else { None }
                })
                .collect();
        assert_eq!(titles[0], "Easy Song");
        assert_eq!(titles[1], "Hard Song");

        // table_level should be formatted as symbol+level
        let levels: Vec<_> = items
            .iter()
            .filter_map(|i| {
                if let SelectItem::Chart(r) = i { Some(r.table_level.as_str()) } else { None }
            })
            .collect();
        assert_eq!(levels[0], "★5");
        assert_eq!(levels[1], "★10");
    }

    #[test]
    fn table_source_url_from_context_reads_stack_and_selection() {
        let stack = vec!["bmz-table:https://example.com/t/\n12".to_string()];
        assert_eq!(
            table_source_url_from_context(&stack, None),
            Some("https://example.com/t/".to_string())
        );

        let selected = SelectItem::Folder {
            path: "bmz-table:https://example.com/other/".to_string(),
            name: "[★] Other".to_string(),
            kind: SelectRowKind::TableFolder,
        };
        assert_eq!(
            table_source_url_from_context(&[], Some(&selected)),
            Some("https://example.com/other/".to_string())
        );

        assert_eq!(table_source_url_from_context(&[], None), None);
    }

    #[test]
    fn song_scan_path_from_context_reads_folder_and_chart() {
        let folder = SelectItem::Folder {
            path: "/music/bms".to_string(),
            name: "bms".to_string(),
            kind: SelectRowKind::Folder,
        };
        assert_eq!(song_scan_path_from_context(&[], Some(&folder)), Some("/music/bms".to_string()));

        let chart = SelectItem::Chart(SelectChartRow {
            chart: Some(ChartListItem {
                chart_id: 1,
                md5: [0; 16],
                sha256: [0; 32],
                title: "Song".to_string(),
                subtitle: String::new(),
                artist: String::new(),
                difficulty_name: String::new(),
                play_level: String::new(),
                mode: String::new(),
                total_notes: 10,
                initial_bpm: 120.0,
                min_bpm: 120.0,
                max_bpm: 120.0,
                length_ms: 0,
                folder_path: "/music/bms/album".to_string(),
                stage_file: String::new(),
                banner_file: String::new(),
                backbmp_file: String::new(),
                preview_file: String::new(),
            }),
            fallback_title: String::new(),
            fallback_artist: String::new(),
            entry_sha256: None,
            best_score: None,
            replay_slots: [false; 4],
            table_level: String::new(),
        });
        assert_eq!(
            song_scan_path_from_context(&[], Some(&chart)),
            Some("/music/bms/album".to_string())
        );
    }

    #[test]
    fn parse_table_path_distinguishes_root_table_and_level() {
        assert_eq!(parse_table_path("bmz-table:"), Some(TablePath::Root));
        assert_eq!(
            parse_table_path("bmz-table:https://example.com/t/"),
            Some(TablePath::Table { source_url: "https://example.com/t/" })
        );
        assert_eq!(
            parse_table_path("bmz-table:https://example.com/t/\n12"),
            Some(TablePath::Level { source_url: "https://example.com/t/", level: "12" })
        );
        assert_eq!(parse_table_path("/songs/folder"), None);
    }

    #[test]
    fn table_level_folder_items_returns_folder_per_level() {
        let (mut library_db, _) = open_in_memory_dbs();
        let chart_a = chart("A");
        use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
        let table = FetchedDifficultyTable {
            source_url: "https://example.com/insane/".to_string(),
            head_url: "https://example.com/insane/header.json".to_string(),
            name: "Insane".to_string(),
            symbol: "★".to_string(),
            level_order: vec!["1".to_string(), "2".to_string(), "25".to_string()],
            entries: vec![FetchedTableEntry {
                level: "2".to_string(),
                md5: hash_to_hex(&chart_a.identity.file_md5),
                sha256: String::new(),
                title: String::new(),
                artist: String::new(),
                comment: String::new(),
            }],
            courses: Vec::new(),
            fetched_at: 0,
        };
        library_db.upsert_difficulty_table(&table).unwrap();

        let items = table_level_folder_items(&library_db, "https://example.com/insane/").unwrap();

        assert_eq!(items.len(), 3);
        assert!(matches!(
            &items[0],
            SelectItem::Folder { path, name, kind }
            if name == "★1" && path == "bmz-table:https://example.com/insane/\n1" && *kind == SelectRowKind::TableFolder
        ));
        assert!(matches!(&items[2], SelectItem::Folder { name, .. } if name == "★25"));
    }

    #[test]
    fn load_select_items_in_table_level_filters_by_level() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let easy = chart("Easy Song");
        let hard = chart("Hard Song");
        library_db.upsert_chart_import(&record_for_chart("/songs/easy.bms", &easy)).unwrap();
        library_db.upsert_chart_import(&record_for_chart("/songs/hard.bms", &hard)).unwrap();

        use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
        let table = FetchedDifficultyTable {
            source_url: "https://example.com/insane/".to_string(),
            head_url: "https://example.com/insane/header.json".to_string(),
            name: "Insane".to_string(),
            symbol: "★".to_string(),
            level_order: vec!["5".to_string(), "10".to_string()],
            entries: vec![
                FetchedTableEntry {
                    level: "5".to_string(),
                    md5: hash_to_hex(&easy.identity.file_md5),
                    sha256: String::new(),
                    title: String::new(),
                    artist: String::new(),
                    comment: String::new(),
                },
                FetchedTableEntry {
                    level: "10".to_string(),
                    md5: hash_to_hex(&hard.identity.file_md5),
                    sha256: String::new(),
                    title: String::new(),
                    artist: String::new(),
                    comment: String::new(),
                },
            ],
            courses: Vec::new(),
            fetched_at: 0,
        };
        library_db.upsert_difficulty_table(&table).unwrap();

        let items = load_select_items_in_table_level(
            &library_db,
            &score_db,
            "https://example.com/insane/",
            "5",
        )
        .unwrap();

        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], SelectItem::Chart(r) if r.display_title() == "Easy Song"));
    }

    #[test]
    fn load_select_items_in_table_level_shows_missing_library_entry() {
        let (mut library_db, score_db) = open_in_memory_dbs();

        use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
        let table = FetchedDifficultyTable {
            source_url: "https://example.com/missing/".to_string(),
            head_url: "https://example.com/missing/header.json".to_string(),
            name: "Missing".to_string(),
            symbol: "★".to_string(),
            level_order: vec!["12".to_string()],
            entries: vec![FetchedTableEntry {
                level: "12".to_string(),
                md5: "aabbcc".repeat(5) + "aabb",
                sha256: String::new(),
                title: "Missing Song".to_string(),
                artist: "Missing Artist".to_string(),
                comment: String::new(),
            }],
            courses: Vec::new(),
            fetched_at: 0,
        };
        library_db.upsert_difficulty_table(&table).unwrap();

        let items = load_select_items_in_table_level(
            &library_db,
            &score_db,
            "https://example.com/missing/",
            "12",
        )
        .unwrap();

        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            SelectItem::Chart(row)
            if row.display_title() == "Missing Song"
                && row.display_artist() == "Missing Artist"
                && !row.in_library()
        ));
    }

    #[test]
    fn load_select_items_in_table_level_prefers_library_title_when_registered() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let chart = chart("Library Title");
        library_db.upsert_chart_import(&record_for_chart("/songs/registered.bms", &chart)).unwrap();

        use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
        let table = FetchedDifficultyTable {
            source_url: "https://example.com/registered/".to_string(),
            head_url: "https://example.com/registered/header.json".to_string(),
            name: "Registered".to_string(),
            symbol: "★".to_string(),
            level_order: vec!["12".to_string()],
            entries: vec![FetchedTableEntry {
                level: "12".to_string(),
                md5: hash_to_hex(&chart.identity.file_md5),
                sha256: String::new(),
                title: "Table Title".to_string(),
                artist: "Table Artist".to_string(),
                comment: String::new(),
            }],
            courses: Vec::new(),
            fetched_at: 0,
        };
        library_db.upsert_difficulty_table(&table).unwrap();

        let items = load_select_items_in_table_level(
            &library_db,
            &score_db,
            "https://example.com/registered/",
            "12",
        )
        .unwrap();

        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            SelectItem::Chart(row)
            if row.display_title() == "Library Title" && row.in_library()
        ));
    }

    #[test]
    fn load_select_items_in_table_level_dedupes_matched_chart_and_stale_hash_row() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let chart = chart("Registered Song");
        library_db.upsert_chart_import(&record_for_chart("/songs/registered.bms", &chart)).unwrap();

        use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
        let table = FetchedDifficultyTable {
            source_url: "https://example.com/dedupe/".to_string(),
            head_url: "https://example.com/dedupe/header.json".to_string(),
            name: "Dedupe".to_string(),
            symbol: "★".to_string(),
            level_order: vec!["12".to_string()],
            entries: vec![
                FetchedTableEntry {
                    level: "12".to_string(),
                    md5: hash_to_hex(&chart.identity.file_md5),
                    sha256: String::new(),
                    title: "Registered Song".to_string(),
                    artist: String::new(),
                    comment: String::new(),
                },
                FetchedTableEntry {
                    level: "12".to_string(),
                    md5: "deadbeef".repeat(4),
                    sha256: String::new(),
                    title: "Registered Song".to_string(),
                    artist: String::new(),
                    comment: String::new(),
                },
            ],
            courses: Vec::new(),
            fetched_at: 0,
        };
        library_db.upsert_difficulty_table(&table).unwrap();

        let items = load_select_items_in_table_level(
            &library_db,
            &score_db,
            "https://example.com/dedupe/",
            "12",
        )
        .unwrap();

        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            SelectItem::Chart(row)
            if row.display_title() == "Registered Song" && row.in_library()
        ));
    }

    #[test]
    fn load_select_items_in_table_level_dedupes_md5_and_sha256_rows_for_same_chart() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let chart = chart("Dual Hash Song");
        library_db.upsert_chart_import(&record_for_chart("/songs/dual.bms", &chart)).unwrap();

        use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
        let table = FetchedDifficultyTable {
            source_url: "https://example.com/dual/".to_string(),
            head_url: "https://example.com/dual/header.json".to_string(),
            name: "Dual".to_string(),
            symbol: "★".to_string(),
            level_order: vec!["12".to_string()],
            entries: vec![
                FetchedTableEntry {
                    level: "12".to_string(),
                    md5: hash_to_hex(&chart.identity.file_md5),
                    sha256: String::new(),
                    title: String::new(),
                    artist: String::new(),
                    comment: String::new(),
                },
                FetchedTableEntry {
                    level: "12".to_string(),
                    md5: String::new(),
                    sha256: hash_to_hex(&chart.identity.file_sha256),
                    title: String::new(),
                    artist: String::new(),
                    comment: String::new(),
                },
            ],
            courses: Vec::new(),
            fetched_at: 0,
        };
        library_db.upsert_difficulty_table(&table).unwrap();

        let items = load_select_items_in_table_level(
            &library_db,
            &score_db,
            "https://example.com/dual/",
            "12",
        )
        .unwrap();

        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], SelectItem::Chart(row) if row.in_library()));
    }

    #[test]
    fn load_select_items_in_table_level_dedupes_duplicate_library_chart_ids() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let chart = chart("Duplicate Import Song");
        let chart_id_a = library_db
            .upsert_chart_import(&record_for_chart("/songs/a/track.bms", &chart))
            .unwrap();
        let chart_id_b = library_db
            .upsert_chart_import(&record_for_chart("/songs/b/track.bms", &chart))
            .unwrap();
        assert_ne!(chart_id_a, chart_id_b);

        use crate::difficulty_table::{FetchedDifficultyTable, FetchedTableEntry};
        let table = FetchedDifficultyTable {
            source_url: "https://example.com/dup-import/".to_string(),
            head_url: "https://example.com/dup-import/header.json".to_string(),
            name: "Dup Import".to_string(),
            symbol: "★".to_string(),
            level_order: vec!["12".to_string()],
            entries: vec![
                FetchedTableEntry {
                    level: "12".to_string(),
                    md5: hash_to_hex(&chart.identity.file_md5),
                    sha256: String::new(),
                    title: String::new(),
                    artist: String::new(),
                    comment: String::new(),
                },
                FetchedTableEntry {
                    level: "12".to_string(),
                    md5: String::new(),
                    sha256: hash_to_hex(&chart.identity.file_sha256),
                    title: String::new(),
                    artist: String::new(),
                    comment: String::new(),
                },
            ],
            courses: Vec::new(),
            fetched_at: 0,
        };
        library_db.upsert_difficulty_table(&table).unwrap();

        let items = load_select_items_in_table_level(
            &library_db,
            &score_db,
            "https://example.com/dup-import/",
            "12",
        )
        .unwrap();

        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], SelectItem::Chart(row) if row.in_library()));
    }

    fn score_for_chart(chart_sha256: [u8; 32]) -> ScoreRecord {
        let mut score = ScoreState::default();
        score.apply(&JudgementEvent {
            note_id: Some(NoteId(1)),
            lane: bmz_core::lane::Lane::Key1,
            judge: Judge::PGreat,
            side: TimingSide::Slow,
            delta: TimeUs(0),
            time: TimeUs(0),
        });

        ScoreRecord {
            chart_sha256,
            played_at: 1_700_000_030,
            clear_type: ClearType::Normal,
            gauge_type: Some(GaugeType::Normal),
            gauge_value: 80.0,
            total_notes: 1,
            score,
            random_seed: None,
            gauge_option: String::new(),
            assist_mask: 0,
            autoplay: false,
            replay_path: String::new(),
        }
    }
}
