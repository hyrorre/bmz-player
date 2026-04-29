use std::collections::HashMap;

use anyhow::Result;

use crate::storage::library_db::{ChartListItem, LibraryDatabase};
use crate::storage::score_db::{BestScoreSummary, ScoreDatabase};

#[derive(Debug, Clone, PartialEq)]
pub struct SelectChartRow {
    pub chart: ChartListItem,
    pub best_score: Option<BestScoreSummary>,
}

pub fn load_select_chart_rows(
    library_db: &LibraryDatabase,
    score_db: &ScoreDatabase,
    limit: u32,
    offset: u32,
) -> Result<Vec<SelectChartRow>> {
    let charts = library_db.list_charts(limit, offset)?;
    let hashes: Vec<[u8; 32]> = charts.iter().map(|chart| chart.sha256).collect();
    let scores = score_db.best_scores_for_charts(&hashes)?;
    let mut scores_by_hash: HashMap<[u8; 32], BestScoreSummary> =
        scores.into_iter().map(|score| (score.chart_sha256, score)).collect();

    Ok(charts
        .into_iter()
        .map(|chart| {
            let best_score = scores_by_hash.remove(&chart.sha256);
            SelectChartRow { chart, best_score }
        })
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
    fn load_select_chart_rows_attaches_best_scores_by_hash() {
        let mut library_conn = Connection::open_in_memory().unwrap();
        configure_connection(&library_conn).unwrap();
        run_migrations(&mut library_conn, LIBRARY_MIGRATIONS).unwrap();
        let mut library_db = LibraryDatabase::from_connection(library_conn);
        let mut score_conn = Connection::open_in_memory().unwrap();
        configure_connection(&score_conn).unwrap();
        run_migrations(&mut score_conn, SCORE_MIGRATIONS).unwrap();
        let mut score_db = ScoreDatabase::from_connection(score_conn);
        let alpha = chart("Alpha");
        let beta = chart("Beta");

        library_db.upsert_chart_import(&record_for_chart("/songs/alpha.bms", &alpha)).unwrap();
        library_db.upsert_chart_import(&record_for_chart("/songs/beta.bms", &beta)).unwrap();
        score_db.insert_score(&score_for_chart(alpha.identity.file_sha256)).unwrap();

        let rows = load_select_chart_rows(&library_db, &score_db, 10, 0).unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].chart.title, "Alpha");
        assert!(rows[0].best_score.is_some());
        assert_eq!(rows[1].chart.title, "Beta");
        assert!(rows[1].best_score.is_none());
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
