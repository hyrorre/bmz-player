use super::*;
use crate::storage::collection_db::FavoriteHints;
use std::time::Instant;

/// Per-load collection flag cost when no cache survives between list loads.
/// Uses a copy of a real library.db and a synthetic collection.db.
#[test]
#[ignore = "release collection-flag probe; set BMZ_PROBE_LIBRARY_DB to a library.db copy"]
fn profile_collection_flags_per_list_load() -> Result<()> {
    use crate::storage::migration::{COLLECTION_MIGRATIONS, run_migrations};
    anyhow::ensure!(!cfg!(debug_assertions), "run with --release");
    let path = std::env::var_os("BMZ_PROBE_LIBRARY_DB")
        .ok_or_else(|| anyhow::anyhow!("set BMZ_PROBE_LIBRARY_DB"))?;
    let library = LibraryDatabase::open_read_only(std::path::Path::new(&path))?;
    let folders: Vec<String> = library
        .conn()
        .prepare("SELECT DISTINCT folder_path FROM charts ORDER BY folder_path LIMIT 1000")?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut charts = Vec::new();
    for folder in &folders {
        charts.extend(library.list_charts_in_folder(folder)?);
        if charts.len() >= 2000 {
            break;
        }
    }
    let row = |chart: &ChartListItem| {
        SelectItem::Chart(SelectChartRow {
            chart: Some(chart.clone()),
            chart_analysis: None,
            has_document: false,
            fallback_title: String::new(),
            fallback_artist: String::new(),
            entry_sha256: None,
            download_metadata: ChartDownloadMetadata::default(),
            best_score: None,
            last_played_at: None,
            lr2_added_at: None,
            replay_slots: [false; 4],
            favorite_chart: false,
            favorite_song: false,
            table_level: String::new(),
            table_text: DifficultyTableText::default(),
        })
    };
    let mut items: Vec<_> = charts.iter().take(1000).map(row).collect();
    eprintln!("library_charts_sampled={} items={}", charts.len(), items.len());
    for favorites in [0usize, 10, 100, 1000] {
        let dir = tempfile::tempdir()?;
        let mut conn = rusqlite::Connection::open(dir.path().join("collection.db"))?;
        run_migrations(&mut conn, COLLECTION_MIGRATIONS)?;
        let mut collection = CollectionDatabase::from_connection(conn);
        for chart in charts.iter().step_by((charts.len() / favorites.max(1)).max(1)).take(favorites)
        {
            let hints = FavoriteHints::new(&chart.title, &chart.artist, &chart.folder_path);
            collection.upsert_favorite_chart(chart.sha256, &hints, 1)?;
            collection.upsert_favorite_song(chart.sha256, &hints, 1)?;
        }
        let mut sample = |reuse: bool| -> Result<Vec<f64>> {
            let mut cache = SelectCollectionCache::default();
            let mut times = Vec::new();
            for iteration in 0..110 {
                if !reuse {
                    cache = SelectCollectionCache::default();
                }
                let start = Instant::now();
                cache.apply(&library, &collection, &mut items)?;
                if iteration >= 10 {
                    times.push(start.elapsed().as_secs_f64() * 1e3);
                }
            }
            times.sort_by(f64::total_cmp);
            Ok(times)
        };
        let fresh = sample(false)?;
        let cached = sample(true)?;
        eprintln!(
            "collection favorites={favorites} fresh_median_ms={:.3} fresh_p95_ms={:.3} fresh_max_ms={:.3} cached_median_ms={:.3}",
            fresh[50], fresh[95], fresh[99], cached[50]
        );
    }
    Ok(())
}
