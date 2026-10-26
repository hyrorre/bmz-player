//! Manual load benchmark for charts inside song archives.
//!
//! ```text
//! BMZ_ARCHIVE_BENCH_ROOT=<dir with archives> BMZ_ARCHIVE_BENCH_ITER=3 \
//!   cargo test -p bmz-player --release --lib --locked \
//!   screens::play_session::archive_bench::archive_bench -- --exact --ignored --nocapture
//! ```
//!
//! `BMZ_ARCHIVE_BENCH_FILTER` selects song folders by a substring of their path.
//! Each process starts without the resource cache, so the first iteration includes
//! archive reading. This does not flush the operating system's file cache.
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
    let _ = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_test_writer()
        .with_max_level(tracing::Level::INFO)
        .try_init();
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
        if std::env::var_os("BMZ_ARCHIVE_BENCH_PREFETCH").is_some() {
            let source = db.available_chart_source(chart_id).unwrap().unwrap();
            if let crate::chart_source::ChartLocator::Archive { container, entry } =
                source.locator().unwrap()
            {
                let started = Instant::now();
                crate::archive_resources::prefetch_chart(&container, &entry, &Default::default())
                    .unwrap();
                println!(
                    "bench prefetch folder={folder} elapsed_ms={}",
                    started.elapsed().as_millis()
                );
            }
        }
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

#[test]
#[ignore = "needs BMZ_LOAD_BENCH_DB pointing to an isolated library snapshot"]
fn native_load_latency_bench() {
    let Ok(path) = std::env::var("BMZ_LOAD_BENCH_DB") else {
        return;
    };
    let _ = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_test_writer()
        .with_max_level(tracing::Level::INFO)
        .try_init();
    let db = LibraryDatabase::open(Path::new(&path)).unwrap();
    let ids = std::env::var("BMZ_LOAD_BENCH_CHART_IDS").unwrap_or_else(|_| "29721,36102".into());
    for id in ids.split(',').map(|id| id.parse::<i64>().unwrap()) {
        let mut chart_for_audio = None;
        for iteration in 0..4 {
            let started = Instant::now();
            let preloaded = preload_play_session_for_chart(
                &db,
                id,
                PlaySessionOptions {
                    sample_rate: 44_100,
                    read_archive_assets: true,
                    ..Default::default()
                },
                1.0,
            )
            .unwrap();
            println!(
                "native bench chart_id={id} iteration={iteration} preload_ms={} sources={} failed_sounds={}",
                started.elapsed().as_millis(),
                preloaded.audio.samples.source_count(),
                preloaded
                    .sample_report
                    .iter()
                    .filter(|report| matches!(report.status, LoadedSampleStatus::Failed(_)))
                    .count()
            );
            chart_for_audio = Some(preloaded.chart.clone());
        }
        let chart = chart_for_audio.unwrap();
        for iteration in 0..4 {
            for cached in if iteration % 2 == 0 { [false, true] } else { [true, false] } {
                let started = Instant::now();
                let (engine, report) = if cached {
                    build_audio_engine_for_chart_with_progress(&chart, 44_100, |_, _| {})
                } else {
                    let mut engine = bmz_audio::engine::AudioEngine::new(44_100);
                    let make = || {
                        Box::new(bmz_audio::ffmpeg_loader::FfmpegSampleLoader::default())
                            as Box<dyn bmz_audio::loader::SampleLoader + Send>
                    };
                    let report = bmz_audio::loader::load_chart_samples_parallel_with_progress(
                        &mut engine,
                        &chart,
                        bmz_audio::loader::ParallelSampleDecode {
                            threads: play_sample_decode_threads(),
                            make_loader: &make,
                        },
                        |_, _| {},
                    );
                    (engine, report)
                };
                println!(
                    "audio bench chart_id={id} cached={cached} iteration={iteration} elapsed_ms={} sources={} failed_sounds={}",
                    started.elapsed().as_millis(),
                    engine.samples.source_count(),
                    report
                        .iter()
                        .filter(|report| matches!(report.status, LoadedSampleStatus::Failed(_)))
                        .count()
                );
            }
        }
    }
}
