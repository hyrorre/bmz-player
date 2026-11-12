//! Repeated discovery in one process, as the app runs its scans, on real song roots.
//!
//! ```text
//! BMZ_DISCOVERY_BENCH_ROOTS="G:/BMS/INSANE;G:/BMS/EVENTS" \
//!   cargo test -p bmz-player --release --lib -- --ignored --nocapture repeated_discovery_timing
//! ```
//!
//! The roots are only read. Roots are separated as in `PATH`. The first pass also
//! hashes every archive, since this process has no remembered generations; the app
//! seeds them from `library.db`, so later passes are the ones that match the app.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use super::*;

#[test]
#[ignore = "needs real song roots in BMZ_DISCOVERY_BENCH_ROOTS"]
fn repeated_discovery_timing() {
    let Some(roots) = std::env::var_os("BMZ_DISCOVERY_BENCH_ROOTS") else {
        eprintln!("skipping: BMZ_DISCOVERY_BENCH_ROOTS is not set");
        return;
    };
    let roots: Vec<PathBuf> = std::env::split_paths(&roots).collect();
    let passes = std::env::var("BMZ_DISCOVERY_BENCH_PASSES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3_usize);
    let scan = ScanConfig {
        follow_symlinks: true,
        use_everything: false,
        auto_rescan_on_startup: false,
        rescan_missing_files: true,
    };

    let mut archives = Vec::new();
    for pass in 1..=passes {
        archives.clear();
        let mut charts = 0;
        let started = Instant::now();
        for root in &roots {
            let discovery = discover_chart_files_with_progress(root, true, &scan, |_| {});
            charts += discovery.entries.len();
            archives.extend(discovery.archives.into_iter().map(|archive| archive.path));
        }
        eprintln!(
            "pass {pass}: discovery {:.3}s, {charts} charts, {} archives",
            started.elapsed().as_secs_f64(),
            archives.len()
        );
    }

    // The index reads alone, in parallel as discovery overlaps them and one by one.
    let started = Instant::now();
    archives.par_iter().for_each(|path| {
        let _ = crate::song_archive::inspect(path, &Default::default());
    });
    eprintln!("inspect, parallel: {:.3}s", started.elapsed().as_secs_f64());

    let mut formats = BTreeMap::<String, (usize, Duration)>::new();
    let (mut entries, mut name_bytes, mut chart_entries) = (0, 0, 0);
    for path in &archives {
        let started = Instant::now();
        let index = crate::song_archive::inspect(path, &Default::default());
        let elapsed = started.elapsed();
        let format = path
            .extension()
            .map_or_else(String::new, |ext| ext.to_string_lossy().to_ascii_lowercase());
        let total = formats.entry(format).or_default();
        total.0 += 1;
        total.1 += elapsed;
        if let Ok(index) = index {
            entries += index.entries.len();
            name_bytes += index.entries.iter().map(|entry| entry.name.len()).sum::<usize>();
            chart_entries += index.entries.iter().filter(|entry| entry.is_chart()).count();
        }
    }
    let serial: Duration = formats.values().map(|(_, elapsed)| *elapsed).sum();
    eprintln!("inspect, serial: {:.3}s", serial.as_secs_f64());
    for (format, (count, elapsed)) in &formats {
        eprintln!("  {format}: {count} archives, {:.3}s", elapsed.as_secs_f64());
    }
    eprintln!(
        "indexes: {entries} entries ({chart_entries} charts), {name_bytes} name bytes, \
         entry structs {} bytes",
        entries * std::mem::size_of::<crate::song_archive::ArchiveEntry>()
    );

    // Everything discovery does per archive, index lookup included.
    let started = Instant::now();
    for path in &archives {
        let _ = discover_archive_charts(path);
    }
    eprintln!("discover_archive_charts, serial: {:.3}s", started.elapsed().as_secs_f64());

    // What the remembered chart listings hold, without allocator and hash table slack:
    // the canonical path key, stamp, limits and listing, then each chart and its name.
    use crate::song_archive::{
        ArchiveLimits, ArchiveStamp, ChartListing, ListedChart, chart_listing,
    };
    let per_archive = std::mem::size_of::<PathBuf>()
        + std::mem::size_of::<ArchiveStamp>()
        + std::mem::size_of::<ArchiveLimits>()
        + std::mem::size_of::<std::sync::Arc<ChartListing>>()
        + std::mem::size_of::<ChartListing>()
        + 2 * std::mem::size_of::<usize>();
    let (mut listed, mut bytes) = (0, 0);
    for path in &archives {
        let Ok(listing) = chart_listing(path, &Default::default()) else {
            continue;
        };
        let key = std::fs::canonicalize(path).map_or(0, |path| path.as_os_str().len());
        listed += listing.charts.len();
        bytes += per_archive + key + listing.generation.fingerprint.len();
        bytes += listing
            .charts
            .iter()
            .map(|chart| std::mem::size_of::<ListedChart>() + chart.name.len())
            .sum::<usize>();
    }
    eprintln!(
        "listings: {} archives, {listed} charts, about {bytes} bytes ({per_archive} fixed per archive)",
        archives.len()
    );
}
