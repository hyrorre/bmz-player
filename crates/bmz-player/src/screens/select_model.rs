use std::collections::{HashMap, HashSet};

use anyhow::Result;
use bmz_core::course::CourseKind;
use bmz_gameplay::rule::RuleMode;
use bmz_render::scene::SelectRowKind;

use crate::ln_policy::{LnPolicySetting, LnScorePolicy, score_ln_policy};
use crate::screens::settings_model::{ConfigSelectRow, KeyBindingSelectRow};
use crate::storage::collection_db::{CollectionDatabase, FavoriteChartRecord, FavoriteSongRecord};
use crate::storage::common::hash_to_hex;
use crate::storage::library_db::{
    ChartAnalysisSummary, ChartListItem, DifficultyTableEntryRecord, LibraryDatabase,
    TableEntryListItem,
};
use crate::storage::score_db::ScoreKey;
use crate::storage::score_db::{BestScoreSummary, ReplaySlotSummary, ScoreDatabase};

/// Virtual path prefix used for difficulty-table navigation.
/// `"bmz-table:"` is the root that lists all registered tables.
/// `"bmz-table:{source_url}"` lists the level folders of that table.
/// `"bmz-table:{source_url}\n{level}"` lists the charts of that table level.
pub const TABLE_ROOT_PATH: &str = "bmz-table:";

/// Virtual path for the course list root.
pub const COURSE_ROOT_PATH: &str = "bmz-course:";

/// Virtual path prefix for song search results.
/// `"bmz-search:<query>"` resolves to the list of charts matching `<query>`.
pub const SEARCH_PATH_PREFIX: &str = "bmz-search:";

/// Virtual path for user collection/favorite navigation.
pub const FAVORITE_ROOT_PATH: &str = "bmz-favorite:";
pub const FAVORITE_CHART_PATH: &str = "bmz-favorite:chart";
pub const FAVORITE_SONG_PATH: &str = "bmz-favorite:song";
pub const FAVORITE_SONG_DETAIL_PREFIX: &str = "bmz-favorite:song:";

/// Virtual path prefix for the same-folder view.
pub const SAME_FOLDER_PATH_PREFIX: &str = "bmz-same-folder:";

/// Maximum entries kept in the in-memory search history (FIFO eviction).
pub const MAX_SEARCH_HISTORY: usize = 8;

/// Returns the embedded query for a `"bmz-search:<query>"` virtual path.
/// `None` when the path is not a search path or the query is empty.
pub fn parse_search_query(path: &str) -> Option<&str> {
    let rest = path.strip_prefix(SEARCH_PATH_PREFIX)?;
    if rest.is_empty() { None } else { Some(rest) }
}

pub fn same_folder_path(folder_path: &str) -> String {
    format!("{SAME_FOLDER_PATH_PREFIX}{folder_path}")
}

pub fn parse_same_folder_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix(SAME_FOLDER_PATH_PREFIX)?;
    if rest.is_empty() { None } else { Some(rest) }
}

pub fn favorite_song_detail_path(representative_sha256: [u8; 32]) -> String {
    format!("{FAVORITE_SONG_DETAIL_PREFIX}{}", hash_to_hex(&representative_sha256))
}

pub fn parse_favorite_song_detail_path(path: &str) -> Option<[u8; 32]> {
    let rest = path.strip_prefix(FAVORITE_SONG_DETAIL_PREFIX)?;
    if rest.is_empty() || rest == "chart" {
        return None;
    }
    hex_to_hash::<32>(rest).ok()
}

/// Returns one folder item per entry in the search history, newest last
/// (matching the order in which `history` is maintained by the caller).
pub fn search_history_folder_items(history: &[String]) -> Vec<SelectItem> {
    history
        .iter()
        .map(|query| SelectItem::Folder {
            path: format!("{SEARCH_PATH_PREFIX}{query}"),
            name: format!("Search : '{query}'"),
            kind: SelectRowKind::SearchFolder,
            summary: None,
        })
        .collect()
}

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
    let entry = table_level_label(symbol, level);
    map.entry(key)
        .and_modify(|v| {
            v.push('/');
            v.push_str(&entry);
        })
        .or_insert(entry);
}

fn table_level_label(symbol: &str, level: &str) -> String {
    format!("{symbol}{level}")
}

fn insert_table_level_and_text(
    level_map: &mut HashMap<String, String>,
    text_map: &mut HashMap<String, DifficultyTableText>,
    key: String,
    entry: &DifficultyTableEntryRecord,
) {
    insert_table_level(level_map, key.clone(), &entry.table_symbol, &entry.level);
    text_map.entry(key).or_insert_with(|| DifficultyTableText::from_entry(entry));
}

fn table_source_rank(source_url: &str, source_order: &[String]) -> usize {
    source_order.iter().position(|url| url == source_url).unwrap_or(usize::MAX)
}

fn sort_difficulty_table_entries(
    entries: &mut [DifficultyTableEntryRecord],
    source_order: &[String],
) {
    entries.sort_by(|a, b| {
        table_source_rank(&a.source_url, source_order)
            .cmp(&table_source_rank(&b.source_url, source_order))
            .then_with(|| a.source_url.cmp(&b.source_url))
            .then_with(|| a.table_name.cmp(&b.table_name))
            .then_with(|| a.table_symbol.cmp(&b.table_symbol))
            .then_with(|| a.level.cmp(&b.level))
    });
}

fn choose_difficulty_table_text(
    mut entries: Vec<DifficultyTableEntryRecord>,
    source_order: &[String],
    source_hint: Option<&str>,
) -> DifficultyTableText {
    if entries.is_empty() {
        return DifficultyTableText::default();
    }
    sort_difficulty_table_entries(&mut entries, source_order);
    if let Some(source_hint) = source_hint
        && let Some(entry) = entries.iter().find(|entry| entry.source_url == source_hint)
    {
        return DifficultyTableText::from_entry(entry);
    }
    entries.first().map(DifficultyTableText::from_entry).unwrap_or_default()
}

fn retain_active_table_entries(
    entries: &mut Vec<DifficultyTableEntryRecord>,
    active_source_urls: Option<&[String]>,
) {
    let Some(active_source_urls) = active_source_urls else { return };
    let active: HashSet<&str> = active_source_urls.iter().map(String::as_str).collect();
    entries.retain(|entry| active.contains(entry.source_url.as_str()));
}

fn path_is_under_or_equal(path: &str, root: &str) -> bool {
    let path = path.replace('\\', "/").trim_end_matches('/').to_string();
    let root = root.replace('\\', "/").trim_end_matches('/').to_string();
    path == root || path.starts_with(&format!("{root}/"))
}

fn chart_is_in_active_song_roots(
    chart: &ChartListItem,
    active_song_roots: Option<&[String]>,
) -> bool {
    let Some(active_song_roots) = active_song_roots else { return true };
    active_song_roots.iter().any(|root| path_is_under_or_equal(&chart.folder_path, root))
}

fn folder_intersects_active_song_roots(path: &str, active_song_roots: Option<&[String]>) -> bool {
    let Some(active_song_roots) = active_song_roots else { return true };
    active_song_roots
        .iter()
        .any(|root| path_is_under_or_equal(path, root) || path_is_under_or_equal(root, path))
}

fn retain_active_charts(charts: &mut Vec<ChartListItem>, active_song_roots: Option<&[String]>) {
    if active_song_roots.is_none() {
        return;
    }
    charts.retain(|chart| chart_is_in_active_song_roots(chart, active_song_roots));
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DifficultyTableText {
    pub table_name: String,
    pub table_level: String,
    pub table_full: String,
}

impl DifficultyTableText {
    pub fn from_parts(table_name: String, table_symbol: &str, level: &str) -> Self {
        let table_level = table_level_label(table_symbol, level);
        let table_full = format!("{table_level}{table_name}");
        Self { table_name, table_level, table_full }
    }

    pub fn from_entry(entry: &DifficultyTableEntryRecord) -> Self {
        Self::from_parts(entry.table_name.clone(), &entry.table_symbol, &entry.level)
    }

    pub fn is_table_song(&self) -> bool {
        !self.table_name.is_empty()
    }

    pub fn as_tuple(&self) -> (String, String, String) {
        (self.table_name.clone(), self.table_level.clone(), self.table_full.clone())
    }
}

/// Resolves beatoraja TEXT_TABLE1/2/3 information for a chart.
///
/// TEXT_TABLE1 is the table name, TEXT_TABLE2 is symbol+level, and TEXT_TABLE3
/// is TEXT_TABLE2 + TEXT_TABLE1, matching PlayerResource#getTableFullname().
/// MD5 has priority; SHA-256 is used only when no MD5 table row is found.
pub fn difficulty_table_text_for_chart(
    library_db: &LibraryDatabase,
    chart: &ChartListItem,
    source_order: &[String],
    source_hint: Option<&str>,
) -> Result<DifficultyTableText> {
    difficulty_table_text_for_chart_with_active_sources(
        library_db,
        chart,
        source_order,
        source_hint,
        None,
    )
}

pub fn difficulty_table_text_for_chart_with_active_sources(
    library_db: &LibraryDatabase,
    chart: &ChartListItem,
    source_order: &[String],
    source_hint: Option<&str>,
    active_source_urls: Option<&[String]>,
) -> Result<DifficultyTableText> {
    let md5_hex = hash_to_hex(&chart.md5);
    let mut md5_entries = library_db.list_difficulty_table_entries_by_md5s(&[md5_hex.as_str()])?;
    retain_active_table_entries(&mut md5_entries, active_source_urls);
    if !md5_entries.is_empty() {
        return Ok(choose_difficulty_table_text(md5_entries, source_order, source_hint));
    }

    let sha256_hex = hash_to_hex(&chart.sha256);
    let mut sha256_entries =
        library_db.list_difficulty_table_entries_by_sha256s(&[sha256_hex.as_str()])?;
    retain_active_table_entries(&mut sha256_entries, active_source_urls);
    Ok(choose_difficulty_table_text(sha256_entries, source_order, source_hint))
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectChartRow {
    pub chart: Option<ChartListItem>,
    pub chart_analysis: Option<ChartAnalysisSummary>,
    pub fallback_title: String,
    pub fallback_artist: String,
    pub entry_sha256: Option<[u8; 32]>,
    pub best_score: Option<BestScoreSummary>,
    pub replay_slots: [bool; 4],
    pub favorite_chart: bool,
    pub favorite_song: bool,
    pub table_level: String,
    pub table_text: DifficultyTableText,
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
    pub constraints: bmz_core::course::CourseConstraints,
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
    pub best_score: Option<crate::storage::score_db::CourseBestScore>,
    /// Which of the four course replay slots have a saved attempt.  Used by
    /// the select skin to render slot indicators on course rows.
    pub replay_slots: [bool; 4],
    /// Names of trophies that have been earned at least once across all
    /// stored attempts of this course (`course_trophy_achievements`).  A
    /// strict subset of `trophy_names`.
    pub achieved_trophy_names: Vec<String>,
}

impl SelectCourseRow {
    /// beatoraja `GradeBar.existsAllSongs()`: a course is playable only when
    /// every declared entry resolves to a local song.
    pub fn exists_all_songs(&self) -> bool {
        self.entry_count > 0 && self.resolved_count == self.entry_count
    }
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
pub struct SelectFolderSummary {
    pub lamp_counts: [u32; 11],
}

impl SelectFolderSummary {
    pub fn clear_type(&self) -> String {
        let index = self.lamp_counts.iter().position(|count| *count > 0).unwrap_or(0);
        clear_type_name_for_folder_lamp(index).to_string()
    }
}

impl From<&[SelectChartRow]> for SelectFolderSummary {
    fn from(rows: &[SelectChartRow]) -> Self {
        let mut lamp_counts = [0; 11];
        for row in rows {
            let index = row
                .best_score
                .as_ref()
                .map(|score| folder_lamp_index_from_clear_type(&score.clear_type))
                .unwrap_or(0);
            lamp_counts[index] += 1;
        }
        Self { lamp_counts }
    }
}

fn folder_lamp_index_from_clear_type(clear_type: &str) -> usize {
    usize::from(bmz_core::clear::ClearType::rank_from_label(clear_type))
}

fn clear_type_name_for_folder_lamp(index: usize) -> &'static str {
    match index {
        1 => "Failed",
        2 => "AssistEasy",
        3 => "LightAssistEasy",
        4 => "Easy",
        5 => "Normal",
        6 => "Hard",
        7 => "ExHard",
        8 => "FullCombo",
        9 => "Perfect",
        10 => "Max",
        _ => "",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectExecutableKind {
    RandomSelect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectExecutableRow {
    pub title: String,
    pub kind: SelectExecutableKind,
    pub chart_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum SelectItem {
    Folder {
        path: String,
        name: String,
        kind: SelectRowKind,
        summary: Option<SelectFolderSummary>,
    },
    Chart(SelectChartRow),
    Course(SelectCourseRow),
    Executable(SelectExecutableRow),
    Config(ConfigSelectRow),
    KeyBinding(KeyBindingSelectRow),
    /// 現在のフォルダから 1 階層戻るアクション行。
    Back,
    /// ゲーム内設定から egui の詳細設定ウィンドウを開くアクション行。
    AdvancedSettings,
}

impl SelectItem {
    pub fn display_name(&self) -> String {
        match self {
            Self::Folder { name, .. } => name.clone(),
            Self::Chart(row) => row.display_title().to_string(),
            Self::Course(row) => row.title.clone(),
            Self::Executable(row) => row.title.clone(),
            Self::Config(row) => row.label().to_string(),
            Self::KeyBinding(row) => row.label(),
            Self::Back => "戻る".to_string(),
            Self::AdvancedSettings => "詳細設定".to_string(),
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
            SelectItem::Folder {
                path: path.clone(),
                name,
                kind: SelectRowKind::Folder,
                summary: None,
            }
        })
        .collect()
}

pub fn favorite_root_item() -> SelectItem {
    SelectItem::Folder {
        path: FAVORITE_ROOT_PATH.to_string(),
        name: "FAVORITE".to_string(),
        kind: SelectRowKind::TableFolder,
        summary: None,
    }
}

pub fn favorite_root_items(collection_db: &CollectionDatabase) -> Result<Vec<SelectItem>> {
    let mut items = Vec::new();
    if !collection_db.favorite_chart_records()?.is_empty() {
        items.push(SelectItem::Folder {
            path: FAVORITE_CHART_PATH.to_string(),
            name: "FAVORITE CHART".to_string(),
            kind: SelectRowKind::TableFolder,
            summary: None,
        });
    }
    if !collection_db.favorite_song_records()?.is_empty() {
        items.push(SelectItem::Folder {
            path: FAVORITE_SONG_PATH.to_string(),
            name: "FAVORITE SONG".to_string(),
            kind: SelectRowKind::TableFolder,
            summary: None,
        });
    }
    Ok(items)
}

pub fn random_select_item_from_items(items: &[SelectItem]) -> Option<SelectItem> {
    let mut chart_ids = Vec::new();
    for item in items {
        if let SelectItem::Chart(row) = item
            && let Some(chart) = &row.chart
        {
            chart_ids.push(chart.chart_id);
        }
    }
    (!chart_ids.is_empty()).then(|| {
        SelectItem::Executable(SelectExecutableRow {
            title: "RANDOM SELECT".to_string(),
            kind: SelectExecutableKind::RandomSelect,
            chart_ids,
        })
    })
}

/// Returns one folder item per registered difficulty table.
pub fn table_folder_items(
    library_db: &LibraryDatabase,
    source_order: &[String],
) -> Result<Vec<SelectItem>> {
    table_folder_items_for_active_sources(library_db, source_order, None)
}

pub fn table_folder_items_for_active_sources(
    library_db: &LibraryDatabase,
    source_order: &[String],
    active_source_urls: Option<&[String]>,
) -> Result<Vec<SelectItem>> {
    let mut tables = library_db.list_difficulty_tables()?;
    if let Some(active_source_urls) = active_source_urls {
        let active: HashSet<&str> = active_source_urls.iter().map(String::as_str).collect();
        tables.retain(|table| active.contains(table.source_url.as_str()));
    }
    if !source_order.is_empty() {
        let order: HashMap<&str, usize> = source_order
            .iter()
            .enumerate()
            .map(|(index, source_url)| (source_url.as_str(), index))
            .collect();
        tables.sort_by_key(|table| {
            order.get(table.source_url.as_str()).copied().unwrap_or(usize::MAX)
        });
    }
    Ok(tables
        .into_iter()
        .map(|t| SelectItem::Folder {
            path: format!("{TABLE_ROOT_PATH}{}", t.source_url),
            name: t.name,
            kind: SelectRowKind::TableFolder,
            summary: None,
        })
        .collect())
}

/// Returns a folder item for the course list root.
pub fn course_root_item() -> SelectItem {
    SelectItem::Folder {
        path: COURSE_ROOT_PATH.to_string(),
        name: "COURSE".to_string(),
        kind: SelectRowKind::TableFolder,
        summary: None,
    }
}

/// Loads manually-imported courses (not from a difficulty table) as `SelectItem::Course` entries.
/// Table-sourced courses appear inside each table's folder via `table_level_folder_items`.
pub fn load_select_items_for_courses(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
) -> Result<Vec<SelectItem>> {
    let courses = library_db.list_courses()?;
    Ok(courses
        .into_iter()
        .filter(|stored| !stored.source.starts_with("table:"))
        .map(|stored| build_select_course_row(library_db, score_db, stored))
        .collect())
}

/// Aggregates per-entry chart stats into a `SelectCourseRow`.
fn build_select_course_row(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
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

    let identity = crate::ir::course_payload::course_identity_from_stored(library_db, &stored);
    let best_score = identity.as_ref().and_then(|identity| {
        score_db.best_course_score(&identity.course_hash).unwrap_or_else(|error| {
            tracing::warn!(
                %error,
                course_id = stored.id,
                course_hash = %identity.course_hash,
                "failed to load best course score"
            );
            None
        })
    });
    let replay_slots = identity
        .as_ref()
        .map(|identity| {
            score_db.course_replay_slot_presence(&identity.course_hash).unwrap_or_else(|error| {
                tracing::warn!(
                    %error,
                    course_id = stored.id,
                    course_hash = %identity.course_hash,
                    "failed to load course_replay_slot_presence"
                );
                [false; 4]
            })
        })
        .unwrap_or([false; 4]);
    let achieved_trophy_names = identity
        .as_ref()
        .map(|identity| {
            score_db.achieved_trophy_names_for_course(&identity.course_hash).unwrap_or_else(
                |error| {
                    tracing::warn!(
                        %error,
                        course_id = stored.id,
                        course_hash = %identity.course_hash,
                        "failed to load achieved_trophy_names_for_course"
                    );
                    Vec::new()
                },
            )
        })
        .unwrap_or_default();

    SelectItem::Course(SelectCourseRow {
        course_id: stored.id,
        title: stored.definition.title,
        kind: stored.definition.kind,
        constraints: stored.definition.constraints,
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
        achieved_trophy_names,
    })
}

/// Returns one folder item per level of the difficulty table, ordered by the
/// table's `level_order`, followed by any courses imported from that table.
pub fn table_level_folder_items(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
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
            summary: None,
        })
        .collect();

    // Append courses that were imported from this table.
    let table_source = format!("table:{source_url}");
    if let Ok(courses) = library_db.list_courses_by_source(&table_source) {
        tracing::info!(source = %table_source, count = courses.len(), "courses found for table");
        for stored in courses {
            items.push(build_select_course_row(library_db, score_db, stored));
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
    ln_policy_setting: LnPolicySetting,
) -> Result<Vec<SelectItem>> {
    load_select_items_in_table_for_rule_mode(
        library_db,
        score_db,
        source_url,
        ln_policy_setting,
        RuleMode::Beatoraja,
    )
}

pub fn load_select_items_in_table_for_rule_mode(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    source_url: &str,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> Result<Vec<SelectItem>> {
    load_select_items_in_table_filtered(
        library_db,
        score_db,
        source_url,
        None,
        ln_policy_setting,
        rule_mode,
    )
}

/// Loads the charts of a single level of the difficulty table.
pub fn load_select_items_in_table_level(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    source_url: &str,
    level: &str,
    ln_policy_setting: LnPolicySetting,
) -> Result<Vec<SelectItem>> {
    load_select_items_in_table_level_for_rule_mode(
        library_db,
        score_db,
        source_url,
        level,
        ln_policy_setting,
        RuleMode::Beatoraja,
    )
}

pub fn load_select_items_in_table_level_for_rule_mode(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    source_url: &str,
    level: &str,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> Result<Vec<SelectItem>> {
    load_select_items_in_table_filtered(
        library_db,
        score_db,
        source_url,
        Some(level),
        ln_policy_setting,
        rule_mode,
    )
}

fn load_select_items_in_table_filtered(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    source_url: &str,
    level_filter: Option<&str>,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> Result<Vec<SelectItem>> {
    // Fetch table metadata for symbol and level ordering.
    let (table_name, symbol, level_order) = library_db
        .list_difficulty_tables()?
        .into_iter()
        .find(|t| t.source_url == source_url)
        .map(|t| (t.name, t.symbol, t.level_order))
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
    let keys: Vec<ScoreKey> = entries
        .iter()
        .filter_map(|entry| entry_score_key(entry, ln_policy_setting, rule_mode))
        .collect();
    let mut score_map: HashMap<ScoreKey, BestScoreSummary> = score_db
        .best_scores_for_charts(&keys)?
        .into_iter()
        .map(|s| {
            (ScoreKey::with_options(s.chart_sha256, s.ln_policy, s.double_option, s.rule_mode), s)
        })
        .collect();
    let mut replay_slot_map = replay_slot_map(score_db, &keys)?;
    let chart_ids: Vec<i64> = entries
        .iter()
        .filter_map(|entry| entry.chart.as_ref().map(|chart| chart.chart_id))
        .collect();
    let mut analysis_map = library_db.chart_analysis_summaries_by_chart_ids(&chart_ids)?;

    Ok(entries
        .into_iter()
        .map(|entry| {
            let table_text =
                DifficultyTableText::from_parts(table_name.clone(), &symbol, &entry.level);
            let score_key = entry_score_key(&entry, ln_policy_setting, rule_mode);
            let best_score = score_key.and_then(|key| score_map.remove(&key));
            let replay_slots =
                score_key.and_then(|key| replay_slot_map.remove(&key)).unwrap_or([false; 4]);
            let chart_analysis =
                entry.chart.as_ref().and_then(|chart| analysis_map.remove(&chart.chart_id));
            SelectItem::Chart(select_chart_row_from_table_entry(
                entry,
                chart_analysis,
                best_score,
                replay_slots,
                table_text,
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

fn entry_score_key(
    entry: &TableEntryListItem,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> Option<ScoreKey> {
    if let Some(chart) = &entry.chart {
        return Some(score_key_for_chart(chart, ln_policy_setting, rule_mode));
    }
    entry_score_sha256(entry)
        .map(|sha256| ScoreKey::new(sha256, LnScorePolicy::ForceLn).with_rule_mode(rule_mode))
}

fn select_chart_row_from_table_entry(
    entry: TableEntryListItem,
    chart_analysis: Option<ChartAnalysisSummary>,
    best_score: Option<BestScoreSummary>,
    replay_slots: [bool; 4],
    table_text: DifficultyTableText,
) -> SelectChartRow {
    let entry_sha256 = entry_score_sha256(&entry);
    let table_level = table_text.table_level.clone();
    SelectChartRow {
        chart: entry.chart,
        chart_analysis,
        fallback_title: entry.title,
        fallback_artist: entry.artist,
        entry_sha256,
        best_score,
        replay_slots,
        favorite_chart: false,
        favorite_song: false,
        table_level,
        table_text,
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
    ln_policy_setting: LnPolicySetting,
) -> Result<Vec<SelectItem>> {
    load_select_items_in_folder_for_rule_mode(
        library_db,
        score_db,
        folder_path,
        ln_policy_setting,
        RuleMode::Beatoraja,
    )
}

pub fn load_select_items_in_folder_for_rule_mode(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    folder_path: &str,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> Result<Vec<SelectItem>> {
    load_select_items_in_folder_for_rule_mode_with_table_order(
        library_db,
        score_db,
        folder_path,
        ln_policy_setting,
        rule_mode,
        &[],
    )
}

pub fn load_select_items_in_folder_for_rule_mode_with_table_order(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    folder_path: &str,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
    table_source_order: &[String],
) -> Result<Vec<SelectItem>> {
    load_select_items_in_folder_for_rule_mode_with_filters(
        library_db,
        score_db,
        folder_path,
        ln_policy_setting,
        rule_mode,
        table_source_order,
        None,
        None,
    )
}

pub fn load_select_items_in_folder_for_rule_mode_with_filters(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    folder_path: &str,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
    table_source_order: &[String],
    active_song_roots: Option<&[String]>,
    active_table_sources: Option<&[String]>,
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
    let mut all_charts = library_db.list_charts_in_folders(&fetch_paths)?;
    retain_active_charts(&mut all_charts, active_song_roots);

    let chart_items = chart_items_with_enrichment(
        library_db,
        score_db,
        all_charts,
        ln_policy_setting,
        rule_mode,
        table_source_order,
        active_table_sources,
    )?;

    let mut items = Vec::with_capacity(non_leaf_folders.len() + chart_items.len());
    for (path, name) in non_leaf_folders {
        if !folder_intersects_active_song_roots(&path, active_song_roots) {
            continue;
        }
        items.push(SelectItem::Folder { path, name, kind: SelectRowKind::Folder, summary: None });
    }
    items.extend(chart_items);

    Ok(items)
}

/// Loads chart `SelectItem`s for a search query against title / subtitle / artist
/// / subartist / genre. Enrichment (best score, replay slots, difficulty table
/// level) mirrors `load_select_items_in_folder`.
pub fn load_select_items_for_search(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    query: &str,
    ln_policy_setting: LnPolicySetting,
) -> Result<Vec<SelectItem>> {
    load_select_items_for_search_for_rule_mode(
        library_db,
        score_db,
        query,
        ln_policy_setting,
        RuleMode::Beatoraja,
    )
}

pub fn load_select_items_for_search_for_rule_mode(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    query: &str,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> Result<Vec<SelectItem>> {
    load_select_items_for_search_for_rule_mode_with_table_order(
        library_db,
        score_db,
        query,
        ln_policy_setting,
        rule_mode,
        &[],
    )
}

pub fn load_select_items_for_search_for_rule_mode_with_table_order(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    query: &str,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
    table_source_order: &[String],
) -> Result<Vec<SelectItem>> {
    load_select_items_for_search_for_rule_mode_with_filters(
        library_db,
        score_db,
        query,
        ln_policy_setting,
        rule_mode,
        table_source_order,
        None,
        None,
    )
}

pub fn load_select_items_for_search_for_rule_mode_with_filters(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    query: &str,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
    table_source_order: &[String],
    active_song_roots: Option<&[String]>,
    active_table_sources: Option<&[String]>,
) -> Result<Vec<SelectItem>> {
    let mut charts = library_db.search_charts(query)?;
    retain_active_charts(&mut charts, active_song_roots);
    chart_items_with_enrichment(
        library_db,
        score_db,
        charts,
        ln_policy_setting,
        rule_mode,
        table_source_order,
        active_table_sources,
    )
}

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
    let favorite_charts = collection_db.favorite_chart_set()?;
    let favorite_song_folders = favorite_song_folder_set(library_db, collection_db)?;
    for item in items {
        let SelectItem::Chart(row) = item else { continue };
        if let Some(sha256) = row.score_sha256() {
            row.favorite_chart = favorite_charts.contains(&sha256);
        }
        row.favorite_song = row
            .chart
            .as_ref()
            .is_some_and(|chart| favorite_song_folders.contains(&chart.folder_path));
    }
    Ok(())
}

fn missing_favorite_chart_item(
    score_db: &ScoreDatabase,
    record: FavoriteChartRecord,
    rule_mode: RuleMode,
) -> Result<SelectItem> {
    let (best_score, replay_slots) =
        score_and_replays_for_missing_favorite(score_db, record.chart_sha256, rule_mode)?;
    Ok(SelectItem::Chart(SelectChartRow {
        chart: None,
        chart_analysis: None,
        fallback_title: fallback_favorite_title(&record.title_hint, record.chart_sha256),
        fallback_artist: record.artist_hint,
        entry_sha256: Some(record.chart_sha256),
        best_score,
        replay_slots,
        favorite_chart: true,
        favorite_song: false,
        table_level: String::new(),
        table_text: DifficultyTableText::default(),
    }))
}

fn missing_favorite_song_item(
    score_db: &ScoreDatabase,
    record: FavoriteSongRecord,
    rule_mode: RuleMode,
) -> Result<SelectItem> {
    let (best_score, replay_slots) =
        score_and_replays_for_missing_favorite(score_db, record.representative_sha256, rule_mode)?;
    Ok(SelectItem::Chart(SelectChartRow {
        chart: None,
        chart_analysis: None,
        fallback_title: fallback_favorite_title(&record.title_hint, record.representative_sha256),
        fallback_artist: record.artist_hint,
        entry_sha256: Some(record.representative_sha256),
        best_score,
        replay_slots,
        favorite_chart: false,
        favorite_song: true,
        table_level: String::new(),
        table_text: DifficultyTableText::default(),
    }))
}

fn score_and_replays_for_missing_favorite(
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

fn fallback_favorite_title(title_hint: &str, sha256: [u8; 32]) -> String {
    if title_hint.is_empty() { short_sha_title(sha256) } else { title_hint.to_string() }
}

fn short_sha_title(sha256: [u8; 32]) -> String {
    let hex = hash_to_hex(&sha256);
    format!("sha256:{}", &hex[..12])
}

fn resolved_favorite_song_folders(
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

fn favorite_song_folder_set(
    library_db: &LibraryDatabase,
    collection_db: &CollectionDatabase,
) -> Result<HashSet<String>> {
    let mut folders = HashSet::new();
    for record in collection_db.favorite_song_records()? {
        folders.extend(resolved_favorite_song_folders(library_db, &record)?);
    }
    Ok(folders)
}

/// Wraps a `ChartListItem` set into `SelectItem::Chart` entries with best-score,
/// replay-slot, and difficulty-table-level metadata resolved.
fn chart_items_with_enrichment(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    all_charts: Vec<ChartListItem>,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
    table_source_order: &[String],
    active_table_sources: Option<&[String]>,
) -> Result<Vec<SelectItem>> {
    let keys: Vec<ScoreKey> =
        all_charts.iter().map(|c| score_key_for_chart(c, ln_policy_setting, rule_mode)).collect();
    let mut score_map: HashMap<ScoreKey, BestScoreSummary> = score_db
        .best_scores_for_charts(&keys)?
        .into_iter()
        .map(|s| {
            (ScoreKey::with_options(s.chart_sha256, s.ln_policy, s.double_option, s.rule_mode), s)
        })
        .collect();
    let mut replay_slot_map = replay_slot_map(score_db, &keys)?;
    let chart_ids: Vec<i64> = all_charts.iter().map(|c| c.chart_id).collect();
    let mut analysis_map = library_db.chart_analysis_summaries_by_chart_ids(&chart_ids)?;

    // MD5 lookup (multiple tables per MD5 joined with '/')
    let md5_hexes: Vec<String> = all_charts.iter().map(|c| hash_to_hex(&c.md5)).collect();
    let md5_refs: Vec<&str> = md5_hexes.iter().map(|s| s.as_str()).collect();
    let mut md5_level_map: HashMap<String, String> = HashMap::new();
    let mut md5_text_map: HashMap<String, DifficultyTableText> = HashMap::new();
    let mut md5_entries = library_db.list_difficulty_table_entries_by_md5s(&md5_refs)?;
    retain_active_table_entries(&mut md5_entries, active_table_sources);
    sort_difficulty_table_entries(&mut md5_entries, table_source_order);
    for e in md5_entries {
        insert_table_level_and_text(&mut md5_level_map, &mut md5_text_map, e.md5.clone(), &e);
    }

    // SHA256 fallback for charts not matched by MD5
    let missing_sha256_hexes: Vec<String> = all_charts
        .iter()
        .filter(|c| !md5_level_map.contains_key(&hash_to_hex(&c.md5)))
        .map(|c| hash_to_hex(&c.sha256))
        .collect();
    let mut sha256_level_map: HashMap<String, String> = HashMap::new();
    let mut sha256_text_map: HashMap<String, DifficultyTableText> = HashMap::new();
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

    let mut items = Vec::with_capacity(all_charts.len());
    for chart in all_charts {
        let score_key = score_key_for_chart(&chart, ln_policy_setting, rule_mode);
        let best_score = score_map.remove(&score_key);
        let replay_slots = replay_slot_map.remove(&score_key).unwrap_or([false; 4]);
        let md5_hex = hash_to_hex(&chart.md5);
        let sha256_hex = hash_to_hex(&chart.sha256);
        let table_level = md5_level_map
            .remove(&md5_hex)
            .or_else(|| sha256_level_map.remove(&sha256_hex))
            .unwrap_or_default();
        let table_text =
            md5_text_map.remove(&md5_hex).or_else(|| sha256_text_map.remove(&sha256_hex));
        items.push(SelectItem::Chart(SelectChartRow {
            chart_analysis: analysis_map.remove(&chart.chart_id),
            chart: Some(chart),
            fallback_title: String::new(),
            fallback_artist: String::new(),
            entry_sha256: None,
            best_score,
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
        | SelectRowKind::SettingsFolder
        | SelectRowKind::Config => Ok(None),
    }
}

fn folder_summary_for_song_folder(
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

fn folder_summary_for_table(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    source_url: &str,
    level_filter: Option<&str>,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> Result<SelectFolderSummary> {
    let mut entries = library_db.list_table_entries_with_chart(source_url)?;
    if let Some(level) = level_filter {
        entries.retain(|entry| entry.level == level);
    }
    entries = dedupe_table_entries(entries);
    let charts = entries.into_iter().filter_map(|entry| entry.chart).collect();
    folder_summary_for_charts(score_db, charts, ln_policy_setting, rule_mode)
}

fn folder_summary_for_charts(
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

fn replay_slot_map(
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

fn score_key_for_chart(
    chart: &ChartListItem,
    ln_policy_setting: LnPolicySetting,
    rule_mode: RuleMode,
) -> ScoreKey {
    ScoreKey::new(chart.sha256, score_ln_policy(ln_policy_setting, chart.ln_profile))
        .with_rule_mode(rule_mode)
}

#[cfg(test)]
mod tests {
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, LongNotePair, LongNoteStyle, PlayableChart};
    use bmz_core::clear::{ClearType, GaugeType};
    use bmz_core::ids::NoteId;
    use bmz_core::judge::{Judge, TimingSide};
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};
    use bmz_gameplay::judge::model::JudgementEvent;
    use bmz_gameplay::score::ScoreState;
    use rusqlite::Connection;

    use super::*;
    use crate::storage::common::configure_connection;
    use crate::storage::library_db::{ChartImportRecord, LibraryDatabase};
    use crate::storage::migration::{
        COLLECTION_MIGRATIONS, LIBRARY_MIGRATIONS, SCORE_MIGRATIONS, run_migrations,
    };
    use crate::storage::score_db::{ScoreDatabase, ScoreRecord};

    #[test]
    fn load_select_items_in_folder_attaches_best_scores_by_hash() {
        let (mut library_db, mut score_db) = open_in_memory_dbs();
        let alpha = chart("Alpha");
        let beta = chart("Beta");

        library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();
        library_db.upsert_chart_import(&record_for_chart("/songs/beta.bms", &beta)).unwrap();
        score_db.insert_score(&score_for_chart(alpha.identity.file_sha256)).unwrap();

        let items =
            load_select_items_in_folder(&library_db, &score_db, "/songs", LnPolicySetting::AutoLn)
                .unwrap();

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
                    ln_policy: LnScorePolicy::ForceLn,
                    double_option: crate::select_options::DoubleOptionScoreBucket::Off,
                    rule_mode: RuleMode::Beatoraja,
                    slot,
                    rule: crate::config::profile_config::ReplaySlotRule::Always,
                    replay_path: format!("replay/{slot}.toml"),
                    played_at: 1_700_000_030 + slot as i64,
                    ex_score: 10 * slot as u32,
                    bp: 0,
                    cb: 0,
                    max_combo: 10,
                    clear_rank: ClearType::Normal as u8,
                })
                .unwrap();
        }

        let items =
            load_select_items_in_folder(&library_db, &score_db, "/songs", LnPolicySetting::AutoLn)
                .unwrap();

        let row = items
            .iter()
            .find_map(|i| if let SelectItem::Chart(r) = i { Some(r) } else { None })
            .unwrap();
        assert_eq!(row.replay_slots, [true, true, true, true]);
    }

    #[test]
    fn load_select_items_uses_profile_ln_policy_for_score_lookup() {
        let (mut library_db, mut score_db) = open_in_memory_dbs();
        let mut alpha = chart("Alpha");
        alpha.long_notes.push(undefined_ln_pair());
        library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();
        let mut force_ln_score = score_for_chart(alpha.identity.file_sha256);
        force_ln_score.ln_policy = LnScorePolicy::ForceLn;
        force_ln_score.score.judges.slow_pgreat = 50;
        let mut force_cn_score = score_for_chart(alpha.identity.file_sha256);
        force_cn_score.ln_policy = LnScorePolicy::ForceCn;
        force_cn_score.score.judges.slow_pgreat = 100;
        score_db.insert_score(&force_ln_score).unwrap();
        score_db.insert_score(&force_cn_score).unwrap();

        let items =
            load_select_items_in_folder(&library_db, &score_db, "/songs", LnPolicySetting::AutoCn)
                .unwrap();

        let row = items
            .iter()
            .find_map(|i| if let SelectItem::Chart(r) = i { Some(r) } else { None })
            .unwrap();
        assert_eq!(row.best_score.as_ref().map(|s| s.ln_policy), Some(LnScorePolicy::ForceCn));
        assert_eq!(row.best_score.as_ref().map(|s| s.ex_score), Some(200));
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

        let items =
            load_select_items_in_folder(&library_db, &score_db, "/bms", LnPolicySetting::AutoLn)
                .unwrap();

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

        let items =
            load_select_items_in_folder(&library_db, &score_db, "/bms", LnPolicySetting::AutoLn)
                .unwrap();

        assert_eq!(items.len(), 2);
        assert!(matches!(&items[0], SelectItem::Folder { name, .. } if name == "genre"));
        assert!(matches!(&items[1], SelectItem::Chart(r) if r.display_title() == "B"));
    }

    #[test]
    fn load_select_items_in_folder_with_filters_hides_charts_outside_active_roots() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let active = chart("Active Song");
        let stale = chart("Stale Song");
        library_db
            .upsert_chart_import(&record_for_chart("/songs/enabled/active.bms", &active))
            .unwrap();
        library_db
            .upsert_chart_import(&record_for_chart("/songs/removed/stale.bms", &stale))
            .unwrap();

        let active_roots = vec!["/songs/enabled".to_string()];
        let items = load_select_items_in_folder_for_rule_mode_with_filters(
            &library_db,
            &score_db,
            "/songs",
            LnPolicySetting::AutoLn,
            RuleMode::Beatoraja,
            &[],
            Some(&active_roots),
            None,
        )
        .unwrap();

        let titles: Vec<_> = items
            .iter()
            .filter_map(|item| {
                if let SelectItem::Chart(row) = item { Some(row.display_title()) } else { None }
            })
            .collect();
        assert_eq!(titles, vec!["Active Song"]);
    }

    #[test]
    fn select_folder_summary_counts_recursive_folder_lamps() {
        let (mut library_db, mut score_db) = open_in_memory_dbs();
        let normal = chart("Normal");
        let hard = chart("Hard");
        let unplayed = chart("Unplayed");
        let outside = chart("Outside");
        library_db
            .upsert_chart_import(&record_for_chart("/songs/folder/normal.bms", &normal))
            .unwrap();
        library_db
            .upsert_chart_import(&record_for_chart("/songs/folder/sub/hard.bms", &hard))
            .unwrap();
        library_db
            .upsert_chart_import(&record_for_chart("/songs/folder/sub/unplayed.bms", &unplayed))
            .unwrap();
        library_db.upsert_chart_import(&record_for_chart("/songs/outside.bms", &outside)).unwrap();
        score_db.insert_score(&score_for_chart(normal.identity.file_sha256)).unwrap();
        let mut hard_score = score_for_chart(hard.identity.file_sha256);
        hard_score.clear_type = ClearType::Hard;
        score_db.insert_score(&hard_score).unwrap();
        score_db.insert_score(&score_for_chart(outside.identity.file_sha256)).unwrap();

        let summary = select_folder_summary(
            &library_db,
            &score_db,
            "/songs/folder",
            SelectRowKind::Folder,
            LnPolicySetting::AutoLn,
        )
        .unwrap()
        .unwrap();

        assert_eq!(summary.lamp_counts[0], 1);
        assert_eq!(summary.lamp_counts[5], 1);
        assert_eq!(summary.lamp_counts[6], 1);
        assert_eq!(summary.lamp_counts.iter().sum::<u32>(), 3);
        assert_eq!(summary.clear_type(), "");
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

    fn open_in_memory_collection_db() -> CollectionDatabase {
        let mut collection_conn = Connection::open_in_memory().unwrap();
        configure_connection(&collection_conn).unwrap();
        run_migrations(&mut collection_conn, COLLECTION_MIGRATIONS).unwrap();
        CollectionDatabase::from_connection(collection_conn)
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
    fn favorite_song_resolves_all_duplicate_sha256_folders() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let mut collection_db = open_in_memory_collection_db();
        let shared = chart("Shared");
        library_db
            .upsert_chart_import(&record_for_chart("/pack-a/song/shared.bms", &shared))
            .unwrap();
        library_db
            .upsert_chart_import(&record_for_chart("/pack-b/song/shared.bms", &shared))
            .unwrap();
        collection_db
            .upsert_favorite_song(
                shared.identity.file_sha256,
                &crate::storage::collection_db::FavoriteHints::new(
                    "Shared",
                    "artist",
                    "/pack-a/song",
                ),
                10,
            )
            .unwrap();

        assert_eq!(
            favorite_song_representatives_for_folder(&library_db, &collection_db, "/pack-a/song")
                .unwrap(),
            vec![shared.identity.file_sha256]
        );
        assert_eq!(
            favorite_song_representatives_for_folder(&library_db, &collection_db, "/pack-b/song")
                .unwrap(),
            vec![shared.identity.file_sha256]
        );

        let items = load_select_items_for_favorite_song(
            &library_db,
            &score_db,
            &collection_db,
            shared.identity.file_sha256,
            LnPolicySetting::AutoLn,
            RuleMode::Beatoraja,
            &[],
            None,
            None,
        )
        .unwrap();
        let folders: HashSet<String> = items
            .iter()
            .filter_map(|item| match item {
                SelectItem::Chart(row) => row.chart.as_ref().map(|chart| chart.folder_path.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(folders.len(), 2);
        assert!(folders.contains("/pack-a/song"));
        assert!(folders.contains("/pack-b/song"));
        assert!(items.iter().all(|item| match item {
            SelectItem::Chart(row) => row.favorite_song,
            _ => true,
        }));
    }

    fn undefined_ln_pair() -> LongNotePair {
        LongNotePair {
            lane: Lane::Key1,
            style: LongNoteStyle::ChannelPair,
            mode: None,
            start_note_id: NoteId(10),
            end_note_id: NoteId(11),
            start_tick: ChartTick(0),
            end_tick: ChartTick(192),
            start_time: TimeUs(0),
            end_time: TimeUs(1_000_000),
            sound: None,
        }
    }

    #[test]
    fn load_select_items_attaches_table_level_via_md5() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let alpha = chart("Alpha");
        library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();

        let table = difficulty_table_for_md5(&alpha.identity.file_md5, "★", "3");
        library_db.upsert_difficulty_table(&table).unwrap();

        let items =
            load_select_items_in_folder(&library_db, &score_db, "/songs", LnPolicySetting::AutoLn)
                .unwrap();

        let row = items
            .iter()
            .find_map(|i| if let SelectItem::Chart(r) = i { Some(r) } else { None })
            .unwrap();
        assert_eq!(row.table_level, "★3");
        assert_eq!(row.table_text.table_name, "Table");
        assert_eq!(row.table_text.table_level, "★3");
        assert_eq!(row.table_text.table_full, "★3Table");
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

        let items =
            load_select_items_in_folder(&library_db, &score_db, "/songs", LnPolicySetting::AutoLn)
                .unwrap();

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

        let items =
            load_select_items_in_folder(&library_db, &score_db, "/songs", LnPolicySetting::AutoLn)
                .unwrap();

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
            name: "Table".to_string(),
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
            name: "Table SHA".to_string(),
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

        let items = table_folder_items(&library_db, &[]).unwrap();

        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            SelectItem::Folder { path, name, kind, .. }
            if path.starts_with(TABLE_ROOT_PATH) && name == "Table" && *kind == SelectRowKind::TableFolder
        ));
    }

    #[test]
    fn table_folder_items_follow_config_source_order() {
        let (mut library_db, _) = open_in_memory_dbs();
        let chart = chart("Table Song");
        let table_a = difficulty_table_for_md5(&chart.identity.file_md5, "A", "1");
        let table_b = difficulty_table_for_md5(&chart.identity.file_md5, "B", "1");
        library_db.upsert_difficulty_table(&table_a).unwrap();
        library_db.upsert_difficulty_table(&table_b).unwrap();

        let items = table_folder_items(
            &library_db,
            &["https://example.com/B/".to_string(), "https://example.com/A/".to_string()],
        )
        .unwrap();

        let folders: Vec<_> = items
            .iter()
            .filter_map(|item| {
                if let SelectItem::Folder { path, name, .. } = item {
                    Some((path.as_str(), name.as_str()))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            folders,
            vec![
                ("bmz-table:https://example.com/B/", "Table"),
                ("bmz-table:https://example.com/A/", "Table"),
            ]
        );
    }

    #[test]
    fn table_folder_items_with_active_sources_hides_removed_tables() {
        let (mut library_db, _) = open_in_memory_dbs();
        let chart = chart("Table Song");
        let table_a = difficulty_table_for_md5(&chart.identity.file_md5, "A", "1");
        let table_b = difficulty_table_for_md5(&chart.identity.file_md5, "B", "1");
        library_db.upsert_difficulty_table(&table_a).unwrap();
        library_db.upsert_difficulty_table(&table_b).unwrap();

        let active_sources = vec!["https://example.com/B/".to_string()];
        let items = table_folder_items_for_active_sources(
            &library_db,
            &active_sources,
            Some(&active_sources),
        )
        .unwrap();

        assert_eq!(items.len(), 1);
        assert!(matches!(
            &items[0],
            SelectItem::Folder { path, .. } if path == "bmz-table:https://example.com/B/"
        ));
    }

    #[test]
    fn chart_enrichment_with_filters_hides_removed_table_levels() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let chart = chart("Table Song");
        library_db.upsert_chart_import(&record_for_chart("/songs/table.bms", &chart)).unwrap();
        library_db
            .upsert_difficulty_table(&difficulty_table_for_md5(&chart.identity.file_md5, "A", "1"))
            .unwrap();
        library_db
            .upsert_difficulty_table(&difficulty_table_for_md5(&chart.identity.file_md5, "B", "2"))
            .unwrap();

        let active_roots = vec!["/songs".to_string()];
        let active_sources = vec!["https://example.com/B/".to_string()];
        let items = load_select_items_in_folder_for_rule_mode_with_filters(
            &library_db,
            &score_db,
            "/songs",
            LnPolicySetting::AutoLn,
            RuleMode::Beatoraja,
            &active_sources,
            Some(&active_roots),
            Some(&active_sources),
        )
        .unwrap();

        let row = items
            .iter()
            .find_map(|item| if let SelectItem::Chart(row) = item { Some(row) } else { None })
            .unwrap();
        assert_eq!(row.table_level, "B2");
        assert_eq!(row.table_text.table_level, "B2");
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

        let items = load_select_items_in_table(
            &library_db,
            &score_db,
            "https://example.com/table/",
            LnPolicySetting::AutoLn,
        )
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
            summary: None,
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
            summary: None,
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
                subartist: String::new(),
                genre: String::new(),
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
                has_long_notes: false,
                has_mines: false,
                judge_rank: None,
                bms_total: 0.0,
                ln_profile: Default::default(),
            }),
            chart_analysis: None,
            fallback_title: String::new(),
            fallback_artist: String::new(),
            entry_sha256: None,
            best_score: None,
            replay_slots: [false; 4],
            favorite_chart: false,
            favorite_song: false,
            table_level: String::new(),
            table_text: DifficultyTableText::default(),
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
        let (mut library_db, score_db) = open_in_memory_dbs();
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

        let items = table_level_folder_items(&library_db, &score_db, "https://example.com/insane/")
            .unwrap();

        assert_eq!(items.len(), 3);
        assert!(matches!(
            &items[0],
            SelectItem::Folder { path, name, kind, .. }
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
            LnPolicySetting::AutoLn,
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
            LnPolicySetting::AutoLn,
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
            LnPolicySetting::AutoLn,
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
            LnPolicySetting::AutoLn,
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
            LnPolicySetting::AutoLn,
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
            LnPolicySetting::AutoLn,
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
            affects_score: true,
        });

        ScoreRecord {
            chart_sha256,
            ln_policy: LnScorePolicy::ForceLn,
            double_option: crate::select_options::DoubleOptionScoreBucket::Off,
            played_at: 1_700_000_030,
            clear_type: ClearType::Normal,
            gauge_type: Some(GaugeType::Normal),
            gauge_value: 80.0,
            total_notes: 1,
            playtime_seconds: 0,
            score,
            count_unprocessed_notes: false,
            random_seed: None,
            arrange: "Normal".to_string(),
            gauge_option: String::new(),
            rule_mode: String::new(),
            assist_mask: 0,
            autoplay: false,
            device_type: bmz_core::input::InputDeviceKind::Keyboard,
            replay_path: String::new(),
        }
    }

    #[test]
    fn parse_search_query_round_trips() {
        assert_eq!(parse_search_query("bmz-search:blue"), Some("blue"));
        assert_eq!(parse_search_query("bmz-search:"), None);
        assert_eq!(parse_search_query("/songs/blue"), None);
        assert_eq!(parse_search_query("bmz-table:foo"), None);
    }

    #[test]
    fn search_history_folder_items_formats_each_entry() {
        let history = vec!["alpha".to_string(), "beta".to_string()];
        let items = search_history_folder_items(&history);
        assert_eq!(items.len(), 2);
        match &items[0] {
            SelectItem::Folder { path, name, kind, summary } => {
                assert_eq!(path, "bmz-search:alpha");
                assert_eq!(name, "Search : 'alpha'");
                assert_eq!(*kind, SelectRowKind::SearchFolder);
                assert_eq!(*summary, None);
            }
            other => panic!("expected folder, got {other:?}"),
        }
        match &items[1] {
            SelectItem::Folder { name, .. } => assert_eq!(name, "Search : 'beta'"),
            other => panic!("expected folder, got {other:?}"),
        }
    }

    #[test]
    fn load_select_items_for_search_returns_chart_rows_with_best_score() {
        let (mut library_db, mut score_db) = open_in_memory_dbs();
        let mut sky = chart("Blue Sky");
        sky.metadata.artist = "Composer A".to_string();
        let mut unrelated = chart("Sunset");
        unrelated.metadata.artist = "Solo".to_string();

        library_db.upsert_chart_import(&record_for_chart("/songs/a.bms", &sky)).unwrap();
        library_db.upsert_chart_import(&record_for_chart("/songs/b.bms", &unrelated)).unwrap();
        score_db.insert_score(&score_for_chart(sky.identity.file_sha256)).unwrap();

        let items =
            load_select_items_for_search(&library_db, &score_db, "blue", LnPolicySetting::AutoLn)
                .unwrap();
        assert_eq!(items.len(), 1);
        let row = match &items[0] {
            SelectItem::Chart(r) => r,
            other => panic!("expected chart row, got {other:?}"),
        };
        assert_eq!(row.display_title(), "Blue Sky");
        assert!(row.best_score.is_some());
    }

    #[test]
    fn load_select_items_for_search_with_filters_hides_removed_song_roots() {
        let (mut library_db, score_db) = open_in_memory_dbs();
        let active = chart("Blue Active");
        let stale = chart("Blue Stale");
        library_db
            .upsert_chart_import(&record_for_chart("/songs/enabled/active.bms", &active))
            .unwrap();
        library_db
            .upsert_chart_import(&record_for_chart("/songs/removed/stale.bms", &stale))
            .unwrap();

        let active_roots = vec!["/songs/enabled".to_string()];
        let items = load_select_items_for_search_for_rule_mode_with_filters(
            &library_db,
            &score_db,
            "blue",
            LnPolicySetting::AutoLn,
            RuleMode::Beatoraja,
            &[],
            Some(&active_roots),
            None,
        )
        .unwrap();

        assert_eq!(items.len(), 1);
        assert!(
            matches!(&items[0], SelectItem::Chart(row) if row.display_title() == "Blue Active")
        );
    }
}
