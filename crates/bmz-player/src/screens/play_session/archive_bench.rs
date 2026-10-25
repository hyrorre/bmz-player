//! Manual load benchmark for charts inside song archives.
//!
//! ```text
//! BMZ_ARCHIVE_BENCH_ROOT=<dir with archives> BMZ_ARCHIVE_BENCH_ITER=3 \
//!   cargo test -p bmz-player --release --lib archive_bench -- --ignored --nocapture
//! ```
//!
//! `BMZ_ARCHIVE_BENCH_FILTER` selects song folders by a substring of their path.
//! Each process starts cold, so the first iteration includes archive reading.
use super::*;
use crate::config::app_config::{AppConfig, PathEntry, ScanConfig};
use crate::storage::migration::{LIBRARY_MIGRATIONS, run_migrations};
use crate::storage::scan::scan_song_roots;
use bmz_audio::loader::LoadedSampleStatus;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

#[test]
#[ignore = "needs BMZ_ARCHIVE_BENCH_ROOT with song archives"]
fn archive_bench() {
    let Ok(root) = std::env::var("BMZ_ARCHIVE_BENCH_ROOT") else {
        return;
    };
    let filter = std::env::var("BMZ_ARCHIVE_BENCH_FILTER").unwrap_or_default();
    let iterations = std::env::var("BMZ_ARCHIVE_BENCH_ITER")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(3);
    let mut connection = rusqlite::Connection::open_in_memory().unwrap();
    run_migrations(&mut connection, LIBRARY_MIGRATIONS).unwrap();
    let mut db = LibraryDatabase::from_connection(connection);
    let roots = vec![PathEntry { path: root, enabled: true, recursive: false }];
    db.set_configured_song_roots(&roots).unwrap();
    let scan = ScanConfig { use_everything: false, skip_hidden: true, ..AppConfig::default().scan };
    let started = Instant::now();
    let report = scan_song_roots(&mut db, &roots, &scan, 10, false).unwrap();
    println!(
        "bench scan elapsed_ms={} imported={} failed={}",
        started.elapsed().as_millis(),
        report.summary.imported,
        report.summary.failed
    );

    // One chart per song folder keeps the folder's shared assets comparable.
    let mut folders = BTreeMap::<String, i64>::new();
    for chart in db.list_all_charts().unwrap() {
        if chart.folder_path.contains(&filter) {
            let id = folders.entry(chart.folder_path.clone()).or_insert(chart.chart_id);
            *id = (*id).min(chart.chart_id);
        }
    }
    crate::archive_resources::install_fallback();
    for (folder, chart_id) in folders {
        for iteration in 0..iterations {
            let options =
                PlaySessionOptions { read_archive_assets: true, ..PlaySessionOptions::default() };
            let started = Instant::now();
            let preloaded = preload_play_session_for_chart(&db, chart_id, options, 1.0).unwrap();
            let preload_ms = started.elapsed().as_millis();
            let failed_sounds = preloaded
                .sample_report
                .iter()
                .filter(|report| matches!(report.status, LoadedSampleStatus::Failed(_)))
                .count();

            let started = Instant::now();
            let mut budget = crate::animated_bga::AnimatedBgaBudget::default();
            let (mut images, mut failed_images) = (0, 0);
            for asset in &preloaded.chart.bga_assets {
                if asset.kind == bmz_chart::model::BgaAssetKind::Static {
                    match budget.load(&asset.path) {
                        Ok(_) => images += 1,
                        Err(_) => failed_images += 1,
                    }
                }
            }
            let bga_ms = started.elapsed().as_millis();

            let started = Instant::now();
            let video = preloaded
                .chart
                .bga_assets
                .iter()
                .find(|asset| asset.kind == bmz_chart::model::BgaAssetKind::Video)
                .map(|asset| bmz_video::decode_first_frame(&asset.path).is_ok());
            let video_ms = started.elapsed().as_millis();
            // Archive entries kept in memory for this chart instead of an extracted copy.
            let resident = match bmz_song_resource::SongPath::parse(Path::new(&folder)) {
                bmz_song_resource::SongPath::Archive { container, .. } => {
                    bmz_song_resource::find(&container).map_or(0, |set| set.loaded_bytes())
                }
                _ => 0,
            };
            println!(
                "bench folder={folder} iteration={iteration} preload_ms={preload_ms} \
                 sources={} failed_sounds={failed_sounds} bga_ms={bga_ms} images={images} \
                 failed_images={failed_images} video_ms={video_ms} video={video:?} \
                 resident_bytes={resident}",
                preloaded.audio.samples.source_count(),
            );
        }
    }
}
