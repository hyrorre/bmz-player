use super::*;
use crate::app::play_preload::play_preload_session_options;
use crate::bootstrap::profile_tests::ProfileTestDir;
use crate::screens::play_session::{
    PlaySessionOptions, build_prepared_play_session_from_preloaded,
};

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
fn g_battle_retry_keeps_source_mode_lane_settings() {
    let data = ProfileTestDir::new();
    let mut boot = data.boot();
    let path = boot.app_paths.data_dir.join("7keys.bms");
    std::fs::write(&path, "#TITLE G-BATTLE retry\n#BPM 120\n#00111:01\n#00219:01\n").unwrap();
    let chart_id =
        crate::storage::import::import_chart_file(&mut boot.library_db, &path, None, None, 0)
            .unwrap()
            .chart_id;

    let profile = &mut boot.profile_config;
    profile.normalize_play_mode_configs();
    profile.play.lane_effect = LaneEffectConfig::Sudden;
    profile.play.hs_fix = HsFixConfig::StartBpm;
    profile.lane.sudden = 80;
    profile.lane.lift_enabled = true;
    profile.lane.lift = 500;
    profile.lane.target_green_number = 273;
    profile.activate_play_mode(KeyMode::K14);
    profile.play.lane_effect = LaneEffectConfig::Off;
    profile.play.hs_fix = HsFixConfig::Off;
    profile.lane.sudden = 0;
    profile.lane.lift_enabled = false;
    profile.lane.lift = 0;
    profile.lane.target_green_number = 300;

    let start = PlayStartOptions { session_mode: SessionMode::GBattle, ..Default::default() };
    let import = || PlayPreloadSource::Import {
        library_db_path: boot.app_paths.library_db.clone(),
        normalization_output_gain: 1.0,
    };
    let load = |generation, source| {
        let options =
            play_preload_session_options(&boot.app_config, profile, KeyMode::K7, start.clone());
        let pending = PendingPlayPreload::spawn(generation, chart_id, options, source);
        pending.rx.recv_timeout(Duration::from_secs(10)).unwrap().result.unwrap()
    };
    let initial = load(1, import());
    assert_eq!(initial.preloaded.skin_attempt.source_key_mode, Some(KeyMode::K7));
    let cached = PlayPreloadSource::Cached {
        chart: Box::new(initial.preloaded.prepared_chart()),
        normalization_gain: initial.preloaded.chart_normalization_gain,
    };

    // Same arrangement reloads the cached chart's audio. Different arrangement reimports it.
    for (index, source) in [cached, import()].into_iter().enumerate() {
        let retry = load(index as u64 + 2, source);
        assert_eq!(Arc::ptr_eq(&retry.preloaded.chart, &initial.preloaded.chart), index == 0);
        let prepared = build_prepared_play_session_from_preloaded(
            retry.preloaded,
            profile,
            retry.session_options,
            Box::new(bmz_gameplay::input::backend::NullInputBackend),
        );
        let session = prepared.session;
        assert_eq!(session.chart.metadata.key_mode, KeyMode::K14);
        assert_eq!(session.play_config_key_mode, KeyMode::K7);
        assert_eq!(session.target_green_number, 273);
        assert!(session.lanecover_enabled);
        assert!((session.lane_cover - 0.08).abs() < f32::EPSILON);
        assert!(session.lift_enabled);
        assert_eq!(session.lift, 0.5);
        assert_eq!(session.hsfix_index, 1);
    }
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
