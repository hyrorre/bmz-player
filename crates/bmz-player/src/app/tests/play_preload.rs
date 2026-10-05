use super::*;
use crate::bootstrap::profile_tests::ProfileTestDir;
use crate::screens::play_session::PlaySessionOptions;

#[test]
fn imported_and_cached_workers_preserve_launch_identity_and_prepared_chart() {
    let data = ProfileTestDir::new();
    let (boot, path, copy) = super::boot_chart::registered_charts(&data);
    let chart_id = boot.library_db.chart_id_by_chart_file_path(&path).unwrap().unwrap();
    let options = PlaySessionOptions { sample_rate: 44_100, ..Default::default() };
    let pending = PendingPlayPreload::spawn(
        41,
        chart_id,
        options.clone(),
        PlayPreloadSource::Import {
            library_db_path: boot.app_paths.library_db.clone(),
            normalization_output_gain: 1.0,
        },
    );
    let result = pending.rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!((result.generation, result.chart_id), (41, chart_id));
    let imported = result.result.unwrap();
    let prepared = pending.prepared_chart.get().unwrap();
    assert!(Arc::ptr_eq(&prepared.chart, &imported.preloaded.chart));
    assert_eq!(imported.session_options.sample_rate, 44_100);

    // Same-arrangement retry must use the prepared chart even if the files have
    // disappeared. Its audio engine is still rebuilt at the requested rate.
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(copy).unwrap();
    let pending = PendingPlayPreload::spawn(
        42,
        chart_id,
        options,
        PlayPreloadSource::Cached { chart: Box::new(prepared.clone()), normalization_gain: 0.75 },
    );
    assert!(pending.prepared_chart.get().is_some());
    let result = pending.rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!((result.generation, result.chart_id), (42, chart_id));
    let cached = result.result.unwrap();
    assert!(Arc::ptr_eq(&cached.preloaded.chart, &imported.preloaded.chart));
    assert_eq!(cached.preloaded.score_key, imported.preloaded.score_key);
    assert_eq!(cached.preloaded.audio.output_sample_rate(), 44_100);
    assert_eq!(cached.preloaded.chart_normalization_gain, 0.75);
}

#[test]
fn failed_preload_keeps_the_request_generation_and_chart_identity() {
    let data = ProfileTestDir::new();
    let boot = data.boot();
    let pending = PendingPlayPreload::spawn(
        7,
        -1,
        PlaySessionOptions::default(),
        PlayPreloadSource::Import {
            library_db_path: boot.app_paths.library_db.clone(),
            normalization_output_gain: 1.0,
        },
    );
    let result = pending.rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!((result.generation, result.chart_id), (7, -1));
    assert!(result.result.is_err());
    assert!(pending.prepared_chart.get().is_none());
}
