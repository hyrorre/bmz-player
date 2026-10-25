use super::*;

#[test]
fn retry_audio_reload_preserves_bgm_and_keysound_asset_mapping() {
    let (path, bgm_path, key_path) = write_temp_bms_with_two_wavs(
        "\
#TITLE Retry audio
#BPM 120
#WAV01 bgm.wav
#WAV02 key.wav
#00001:01
#00011:02
",
    );
    let imported = import_bms_chart(&path, None, true).unwrap();
    let chart = Arc::new(imported.chart);
    let score_key =
        ScoreKey::new(chart.identity.file_sha256, crate::ln_policy::LnScorePolicy::AutoLn);
    let mut progress = Vec::new();

    let preloaded = preload_play_session_reloading_audio_with_progress(
        PreparedPlayChart {
            chart: Arc::clone(&chart),
            skin_attempt: Default::default(),
            source_ln_profile: crate::ln_policy::ChartLnProfile::default(),
            chart_length_ms: 123_456,
            render_snapshot_cache:
                crate::screens::play_snapshot::PlayRenderSnapshotCache::from_chart(&chart),
            applied_arrange: normal_applied_arrange(0, false, SRandomScheme::Lm120HzV1),
            score_key,
            assist_runtime: Default::default(),
            opponent_chart: None,
            score_save_disabled: false,
            archive_resources: None,
        },
        48_000,
        0.75,
        |loaded, total| progress.push((loaded, total)),
    );

    assert!(Arc::ptr_eq(&preloaded.chart, &chart));
    assert_eq!(preloaded.chart_length_ms, 123_456);
    assert_eq!(preloaded.chart_normalization_gain, 0.75);
    assert_eq!(preloaded.score_key, score_key);
    assert!(
        preloaded
            .sample_report
            .iter()
            .all(|report| matches!(report.status, LoadedSampleStatus::Loaded))
    );
    let bgm_id = preloaded
        .chart
        .sounds
        .iter()
        .find(|asset| asset.path == bgm_path)
        .map(|asset| asset.id)
        .expect("BGM asset");
    let key_id = preloaded
        .chart
        .sounds
        .iter()
        .find(|asset| asset.path == key_path)
        .map(|asset| asset.id)
        .expect("keysound asset");
    assert_eq!(preloaded.chart.bgm_events.first().map(|event| event.sound), Some(bgm_id));
    assert_eq!(
        preloaded.chart.lane_notes.iter().flatten().find_map(|note| note.sound),
        Some(key_id)
    );
    assert!(preloaded.audio.samples.get(bgm_id).unwrap().sample_stereo(0).0 > 0.4);
    assert!(preloaded.audio.samples.get(key_id).unwrap().sample_stereo(0).0 < -0.4);
    assert_eq!(progress.last(), Some(&(2, 2)));

    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(bgm_path).unwrap();
    std::fs::remove_file(key_path).unwrap();
}
