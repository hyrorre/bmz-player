use super::*;
use std::path::Path;

/// Virtual path prefix used for difficulty-table navigation.
/// `"bmz-table:"` is the root that lists all registered tables.
/// `"bmz-table:{source_url}"` lists the level folders of that table.
/// `"bmz-table:{source_url}\n{level}"` lists the charts of that table level.
pub const TABLE_ROOT_PATH: &str = "bmz-table:";

/// Virtual path for the course list root.
pub const COURSE_ROOT_PATH: &str = "bmz-course:";

/// Virtual path prefix for the ordered chart list of a course.
pub const COURSE_CONTENTS_PATH_PREFIX: &str = "bmz-course-contents:";

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

pub fn course_contents_path(course_id: i64) -> String {
    format!("{COURSE_CONTENTS_PATH_PREFIX}{course_id}")
}

pub fn parse_course_contents_path(path: &str) -> Option<i64> {
    path.strip_prefix(COURSE_CONTENTS_PATH_PREFIX)?.parse().ok()
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
    search_history_folder_items_for_locale(history, AppLocale::DEFAULT)
}

pub fn search_history_folder_items_for_locale(
    history: &[String],
    locale: AppLocale,
) -> Vec<SelectItem> {
    let text = Localizer::new(locale);
    history
        .iter()
        .map(|query| {
            let mut args = FluentArgs::new();
            args.set("query", query.as_str());
            SelectItem::Folder {
                path: format!("{SEARCH_PATH_PREFIX}{query}"),
                name: text.format("select-search-history", &args),
                kind: SelectRowKind::SearchFolder,
                summary: None,
            }
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
    let path = match selected {
        Some(SelectItem::Folder { path, kind, .. }) if *kind == SelectRowKind::Folder => {
            Some(path.clone())
        }
        Some(SelectItem::Chart(row)) if row.in_library() => {
            row.chart.as_ref().map(|chart| chart.folder_path.clone())
        }
        _ => None,
    }?;
    match crate::chart_source::ChartLocator::parse(Path::new(&path)) {
        Ok(crate::chart_source::ChartLocator::Archive { container, .. }) => {
            Some(container.to_string_lossy().into_owned())
        }
        _ => Some(path),
    }
}

pub(super) fn insert_table_level(
    map: &mut HashMap<String, String>,
    key: String,
    symbol: &str,
    level: &str,
) {
    let entry = table_level_label(symbol, level);
    map.entry(key)
        .and_modify(|v| {
            v.push('/');
            v.push_str(&entry);
        })
        .or_insert(entry);
}

pub(super) fn table_level_label(symbol: &str, level: &str) -> String {
    format!("{symbol}{level}")
}

pub(super) fn insert_table_level_and_text(
    level_map: &mut HashMap<String, String>,
    text_map: &mut HashMap<String, DifficultyTableText>,
    key: String,
    entry: &DifficultyTableEntryRecord,
) {
    insert_table_level(level_map, key.clone(), &entry.table_symbol, &entry.level);
    text_map.entry(key).or_insert_with(|| DifficultyTableText::from_entry(entry));
}

pub(super) fn table_source_rank(source_url: &str, source_order: &[String]) -> usize {
    source_order.iter().position(|url| url == source_url).unwrap_or(usize::MAX)
}

pub(super) fn sort_difficulty_table_entries(
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

pub(super) fn choose_difficulty_table_text(
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

pub(super) fn retain_active_table_entries(
    entries: &mut Vec<DifficultyTableEntryRecord>,
    active_source_urls: Option<&[String]>,
) {
    let Some(active_source_urls) = active_source_urls else { return };
    let active: HashSet<&str> = active_source_urls.iter().map(String::as_str).collect();
    entries.retain(|entry| active.contains(entry.source_url.as_str()));
}

pub(super) fn path_is_under_or_equal(path: &str, root: &str) -> bool {
    let path = path.replace('\\', "/").trim_end_matches('/').to_string();
    let root = root.replace('\\', "/").trim_end_matches('/').to_string();
    path == root || path.starts_with(&format!("{root}/"))
}

pub(crate) fn chart_is_in_active_song_roots(
    chart: &ChartListItem,
    active_song_roots: Option<&[String]>,
) -> bool {
    let Some(active_song_roots) = active_song_roots else { return true };
    active_song_roots.iter().any(|root| path_is_under_or_equal(&chart.folder_path, root))
}

pub(super) fn folder_intersects_active_song_roots(
    path: &str,
    active_song_roots: Option<&[String]>,
) -> bool {
    let Some(active_song_roots) = active_song_roots else { return true };
    active_song_roots
        .iter()
        .any(|root| path_is_under_or_equal(path, root) || path_is_under_or_equal(root, path))
}

pub(super) fn retain_active_charts(
    charts: &mut Vec<ChartListItem>,
    active_song_roots: Option<&[String]>,
) {
    if active_song_roots.is_none() {
        return;
    }
    charts.retain(|chart| chart_is_in_active_song_roots(chart, active_song_roots));
}
