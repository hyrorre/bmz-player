use std::collections::HashMap;

use anyhow::Result;
use bmz_render::scene::SelectRowKind;

use crate::storage::common::hash_to_hex;
use crate::storage::library_db::{ChartListItem, LibraryDatabase};
use crate::storage::score_db::{BestScoreSummary, ReplaySlotSummary, ScoreDatabase};

/// Virtual path prefix used for difficulty-table navigation.
/// `"bmz-table:"` is the root that lists all registered tables.
/// `"bmz-table:{source_url}"` lists the level folders of that table.
/// `"bmz-table:{source_url}\n{level}"` lists the charts of that table level.
pub const TABLE_ROOT_PATH: &str = "bmz-table:";

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
    pub chart: ChartListItem,
    pub best_score: Option<BestScoreSummary>,
    pub replay_slots: [bool; 4],
    pub table_level: String,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum SelectItem {
    Folder { path: String, name: String, kind: SelectRowKind },
    Chart(SelectChartRow),
}

impl SelectItem {
    pub fn display_name(&self) -> &str {
        match self {
            Self::Folder { name, .. } => name.as_str(),
            Self::Chart(row) => row.chart.title.as_str(),
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

/// Returns one folder item per level of the difficulty table, ordered by the
/// table's `level_order`.  Each folder leads to `load_select_items_in_table_level`.
pub fn table_level_folder_items(
    library_db: &LibraryDatabase,
    source_url: &str,
) -> Result<Vec<SelectItem>> {
    let Some(table) =
        library_db.list_difficulty_tables()?.into_iter().find(|t| t.source_url == source_url)
    else {
        return Ok(Vec::new());
    };

    Ok(table
        .level_order
        .iter()
        .map(|level| SelectItem::Folder {
            path: format!("{TABLE_ROOT_PATH}{source_url}{TABLE_LEVEL_SEPARATOR}{level}"),
            name: format!("{}{}", table.symbol, level),
            kind: SelectRowKind::TableFolder,
        })
        .collect())
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

    let mut chart_levels = library_db.list_charts_with_level_in_table(source_url)?;
    if let Some(level) = level_filter {
        chart_levels.retain(|(_, chart_level)| chart_level == level);
    }

    // Sort by the table's level_order, then alphabetically by title.
    let level_rank = |level: &str| -> usize {
        level_order.iter().position(|l| l == level).unwrap_or(usize::MAX)
    };
    chart_levels.sort_by(|(a, al), (b, bl)| {
        level_rank(al)
            .cmp(&level_rank(bl))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });

    // Batch score lookup.
    let hashes: Vec<[u8; 32]> = chart_levels.iter().map(|(c, _)| c.sha256).collect();
    let mut score_map: HashMap<[u8; 32], BestScoreSummary> = score_db
        .best_scores_for_charts(&hashes)?
        .into_iter()
        .map(|s| (s.chart_sha256, s))
        .collect();
    let mut replay_slot_map = replay_slot_map(score_db, &hashes)?;

    Ok(chart_levels
        .into_iter()
        .map(|(chart, level)| {
            let table_level = format!("{symbol}{level}");
            let best_score = score_map.remove(&chart.sha256);
            let replay_slots = replay_slot_map.remove(&chart.sha256).unwrap_or([false; 4]);
            SelectItem::Chart(SelectChartRow { chart, best_score, replay_slots, table_level })
        })
        .collect())
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
            chart,
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
        assert_eq!(charts[0].chart.title, "Alpha");
        assert!(charts[0].best_score.is_some());
        assert_eq!(charts[1].chart.title, "Beta");
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
        let titles: Vec<_> = items
            .iter()
            .filter_map(|i| {
                if let SelectItem::Chart(r) = i { Some(r.chart.title.as_str()) } else { None }
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
        assert!(matches!(&items[1], SelectItem::Chart(r) if r.chart.title == "B"));
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
            fetched_at: 0,
        };
        library_db.upsert_difficulty_table(&table).unwrap();

        let items =
            load_select_items_in_table(&library_db, &score_db, "https://example.com/table/")
                .unwrap();

        assert_eq!(items.len(), 2);
        let titles: Vec<_> = items
            .iter()
            .filter_map(|i| {
                if let SelectItem::Chart(r) = i { Some(r.chart.title.as_str()) } else { None }
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
        assert!(matches!(&items[0], SelectItem::Chart(r) if r.chart.title == "Easy Song"));
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
