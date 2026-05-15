use std::collections::HashMap;

use anyhow::Result;

use crate::storage::library_db::{ChartListItem, LibraryDatabase};
use crate::storage::score_db::{BestScoreSummary, ScoreDatabase};

#[derive(Debug, Clone, PartialEq)]
pub struct SelectChartRow {
    pub chart: ChartListItem,
    pub best_score: Option<BestScoreSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectItem {
    Folder { path: String, name: String },
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
            SelectItem::Folder { path: path.clone(), name }
        })
        .collect()
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
    let folder_names = library_db.list_child_folder_names(folder_path)?;
    let direct_charts = library_db.list_charts_in_folder(folder_path)?;

    let mut non_leaf_folders: Vec<(String, String)> = Vec::new();
    let mut all_charts = direct_charts;

    for name in folder_names {
        let child_path = format!("{}/{}", folder_path, name);
        let child_sub_folders = library_db.list_child_folder_names(&child_path)?;
        if child_sub_folders.is_empty() {
            let leaf_charts = library_db.list_charts_in_folder(&child_path)?;
            all_charts.extend(leaf_charts);
        } else {
            non_leaf_folders.push((child_path, name));
        }
    }

    let hashes: Vec<[u8; 32]> = all_charts.iter().map(|c| c.sha256).collect();
    let mut score_map: HashMap<[u8; 32], BestScoreSummary> = score_db
        .best_scores_for_charts(&hashes)?
        .into_iter()
        .map(|s| (s.chart_sha256, s))
        .collect();

    let mut items = Vec::with_capacity(non_leaf_folders.len() + all_charts.len());

    for (path, name) in non_leaf_folders {
        items.push(SelectItem::Folder { path, name });
    }
    for chart in all_charts {
        let best_score = score_map.remove(&chart.sha256);
        items.push(SelectItem::Chart(SelectChartRow { chart, best_score }));
    }

    Ok(items)
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
            timing_events: Vec::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
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
