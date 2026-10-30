use super::*;

#[test]
fn select_enter_waits_for_new_set_and_fires_once_per_entry() {
    let mut pending = true;
    assert!(!take_select_enter(&mut pending, true, true, true, false));
    assert!(pending, "old set must not consume entry");
    assert!(!take_select_enter(&mut pending, true, false, false, false));
    assert!(take_select_enter(&mut pending, true, false, true, false));
    assert!(!take_select_enter(&mut pending, true, false, true, false));
    pending = true; // Result / Decideから再入場し、同じセットを再利用する場合も一度だけ。
    assert!(!take_select_enter(&mut pending, false, false, true, false));
    assert!(take_select_enter(&mut pending, true, false, true, false));
    pending = true;
    assert!(!take_select_enter(&mut pending, true, false, true, true));
    assert!(!pending, "Viewer suppressed entry must not play later");
}

#[test]
fn select_score_context_changes_only_for_rule_or_ln_mode() {
    let profile = ProfileConfig::new_default("default", "Default", 1);
    let before = SelectScoreContext::from_profile(&profile);

    let mut random_changed = profile.clone();
    random_changed.play.random = RandomOptionConfig::Mirror;
    assert_eq!(before, SelectScoreContext::from_profile(&random_changed));

    let mut rule_changed = profile.clone();
    rule_changed.play.rule_mode = RuleMode::Dx;
    assert_ne!(before, SelectScoreContext::from_profile(&rule_changed));

    let mut ln_changed = profile;
    ln_changed.play.ln_mode_policy = LnPolicySetting::ForceCn;
    assert_ne!(before, SelectScoreContext::from_profile(&ln_changed));
}

#[test]
fn select_bgm_waits_for_preview_and_pending_system_sound_set() {
    assert!(should_play_select_bgm_on_enter(false, false));
    assert!(!should_play_select_bgm_on_enter(true, false));
    assert!(!should_play_select_bgm_on_enter(false, true));
}

#[test]
fn select_preview_fade_factor_ramps_in_and_out() {
    let started_at = Instant::now();
    let half = started_at + SELECT_PREVIEW_FADE_DURATION / 2;
    let done = started_at + SELECT_PREVIEW_FADE_DURATION;

    assert_eq!(
        select_preview_fade_factor(SelectPreviewFade::FadingIn { started_at }, started_at),
        0.0
    );
    assert!(
        (select_preview_fade_factor(SelectPreviewFade::FadingIn { started_at }, half) - 0.5).abs()
            < 0.001
    );
    assert_eq!(select_preview_fade_factor(SelectPreviewFade::FadingIn { started_at }, done), 1.0);
    assert!(
        (select_preview_fade_factor(SelectPreviewFade::FadingOut { started_at }, half) - 0.5).abs()
            < 0.001
    );
    assert_eq!(select_preview_fade_factor(SelectPreviewFade::FadingOut { started_at }, done), 0.0);
}

#[test]
fn select_preview_normalization_gain_follows_chart_normalization_setting() {
    assert_eq!(select_preview_normalization_gain(true, 0.25), 0.25);
    assert_eq!(select_preview_normalization_gain(false, 0.25), 1.0);
    assert_eq!(select_preview_normalization_gain(true, f32::NAN), 1.0);
    assert_eq!(select_preview_normalization_gain(true, 1.5), 1.0);
}

#[test]
fn prepare_select_preview_keeps_sample_with_analyzed_gain() {
    let sample = DecodedSample { channels: 2, sample_rate: 48_000, frames: vec![1.0; 480] };

    let prepared = prepare_select_preview(sample.clone());

    assert_eq!(prepared.sample.frames, sample.frames);
    assert!(prepared.normalization_gain > 0.0);
    assert!(prepared.normalization_gain < 1.0);
}

#[test]
fn select_preview_key_waits_for_beatoraja_start_delay() {
    let key = Some("folder|preview.ogg".to_string());

    assert_eq!(
        select_preview_key_after_delay(
            key.clone(),
            None,
            SELECT_PREVIEW_START_DELAY - Duration::from_millis(1),
            SELECT_PREVIEW_START_DELAY,
        ),
        None
    );
    assert_eq!(
        select_preview_key_after_delay(
            key.clone(),
            None,
            SELECT_PREVIEW_START_DELAY,
            SELECT_PREVIEW_START_DELAY,
        ),
        key
    );
}

#[test]
fn select_preview_key_keeps_current_source_during_bar_animation() {
    let key = Some("folder|preview.ogg".to_string());
    let elapsed = Duration::ZERO;

    assert_eq!(
        select_preview_key_after_delay(
            key.clone(),
            Some("folder|preview.ogg"),
            elapsed,
            SELECT_PREVIEW_START_DELAY,
        ),
        key
    );
    assert_eq!(
        select_preview_key_after_delay(
            Some("folder|next.ogg".to_string()),
            Some("folder|preview.ogg"),
            elapsed,
            SELECT_PREVIEW_START_DELAY,
        ),
        None
    );
}

#[test]
fn select_preview_load_queue_keeps_only_latest_pending_request() {
    let mut queue = SelectPreviewLoadQueue::default();

    assert_eq!(queue.request("first".to_string()), Some("first".to_string()));
    assert_eq!(queue.request("second".to_string()), None);
    assert_eq!(queue.request("latest".to_string()), None);
    assert_eq!(queue.finish(), Some("latest".to_string()));
    assert_eq!(queue.finish(), None);
    assert_eq!(queue.request("after-idle".to_string()), Some("after-idle".to_string()));
}

#[test]
fn select_preview_uses_generated_fallback_after_explicit_preview_fails() {
    assert!(should_use_generated_preview("", false));
    assert!(should_use_generated_preview("missing-preview.ogg", true));
    assert!(!should_use_generated_preview("preview.ogg", false));
}

#[test]
fn audio_diagnostic_marks_generated_preview_callback_pressure() {
    assert_eq!(
        classify_audio_output_issue(AudioOutputIssueMetrics {
            callback_over_budget: true,
            generated_preview_loading: true,
            ..Default::default()
        }),
        AudioOutputIssueCause::GeneratedPreviewCpuPressure
    );
    assert_eq!(
        classify_audio_output_issue(AudioOutputIssueMetrics {
            engine_lock_misses: 1,
            callback_over_budget: true,
            generated_preview_loading: true,
            ..Default::default()
        }),
        AudioOutputIssueCause::CallbackLockContention
    );
    assert_eq!(
        classify_audio_output_issue(AudioOutputIssueMetrics {
            clipped_samples: 1,
            generated_preview_loading: true,
            ..Default::default()
        }),
        AudioOutputIssueCause::MixClipping
    );
    assert_eq!(
        classify_audio_output_issue(AudioOutputIssueMetrics::default()),
        AudioOutputIssueCause::Unknown
    );
}

#[test]
fn late_schedule_classification_uses_per_interval_maximum() {
    // ~2 ms threshold at 48 kHz is 96 frames.
    assert!(late_schedule_from_interval(96, 48_000));
    assert!(!late_schedule_from_interval(95, 48_000));
    assert!(!late_schedule_from_interval(0, 48_000));
    // A tiny or zero sample rate never makes every interval late.
    assert!(!late_schedule_from_interval(0, 0));
    assert!(late_schedule_from_interval(1, 0));

    // A prior interval's large spike must not keep later tiny delays as late_schedule.
    let first_interval = late_schedule_from_interval(480, 48_000);
    let second_interval = late_schedule_from_interval(10, 48_000);
    assert!(first_interval);
    assert!(!second_interval);
    assert_eq!(
        classify_audio_output_issue(AudioOutputIssueMetrics {
            late_schedule: second_interval,
            ..Default::default()
        }),
        AudioOutputIssueCause::Unknown
    );
    assert_eq!(
        classify_audio_output_issue(AudioOutputIssueMetrics {
            late_schedule: first_interval,
            ..Default::default()
        }),
        AudioOutputIssueCause::LateSchedule
    );
}

#[test]
fn late_schedule_collection_survives_source_replacement_and_aggregates_sources() {
    use crate::audio::{RetiredLateSchedule, take_interval_command_diagnostics};
    use bmz_audio::command::AudioEngineHandle;
    use bmz_audio::engine::AudioEngine;
    use bmz_audio::queue::ScheduledSound;
    use bmz_core::ids::SoundId;

    fn late_by(engine: &AudioEngineHandle, frame: u64, late: u64) {
        let sound = ScheduledSound::one_shot(frame - late, SoundId(1), 1.0, 0.0);
        assert!(engine.schedule_sound(sound));
        assert!(engine.processor().render_stereo(frame, &mut [0.0; 2]));
    }
    // Same composition as `collect_audio_diagnostics`: the runtime snapshot
    // carries retired sources' peaks, then each live source adds its interval.
    fn collect(
        retired: &RetiredLateSchedule,
        sources: &[&AudioEngineHandle],
    ) -> crate::audio::AudioOutputDiagnostics {
        let mut snapshot = crate::audio::AudioOutputDiagnostics {
            scheduling_max_late_frames: retired.take(),
            ..Default::default()
        };
        for source in sources {
            snapshot.add_command_queue(take_interval_command_diagnostics(source));
        }
        snapshot
    }

    let retired = RetiredLateSchedule::default();
    let system = AudioEngineHandle::new(AudioEngine::new(48_000));
    let play = AudioEngineHandle::new(AudioEngine::new(48_000));

    late_by(&system, 1_000, 0);
    late_by(&play, 1_000, 480);
    let first = collect(&retired, &[&system, &play]);
    assert_eq!(first.scheduling_max_late_frames, 480);
    assert!(late_schedule_from_interval(first.scheduling_max_late_frames, 48_000));

    // The play source is late, then replaced (retry / drain end) before the
    // next collect. The cumulative sum shrinks, so its delta saturates to 0.
    late_by(&play, 2_000, 200);
    retired.retire(&play);
    drop(play);
    let next_play = AudioEngineHandle::new(AudioEngine::new(48_000));
    late_by(&next_play, 2_000, 5);
    let second = collect(&retired, &[&system, &next_play]);
    assert_eq!(second.scheduling_late_frames.saturating_sub(first.scheduling_late_frames), 0);
    assert_eq!(second.scheduling_max_late_frames, 200);
    assert!(late_schedule_from_interval(second.scheduling_max_late_frames, 48_000));

    // A normal interval after large spikes is not late_schedule.
    late_by(&next_play, 3_000, 10);
    let third = collect(&retired, &[&system, &next_play]);
    assert_eq!(third.scheduling_max_late_frames, 10);
    assert!(!late_schedule_from_interval(third.scheduling_max_late_frames, 48_000));

    // The interval maximum is the peak across all live sources.
    late_by(&system, 4_000, 150);
    late_by(&next_play, 4_000, 20);
    let fourth = collect(&retired, &[&system, &next_play]);
    assert_eq!(fourth.scheduling_max_late_frames, 150);
    assert!(late_schedule_from_interval(fourth.scheduling_max_late_frames, 48_000));

    // Lifetime diagnostics used by the play-loop debug log are unaffected.
    assert_eq!(system.diagnostics().scheduling_max_late_frames, 150);
    assert_eq!(next_play.diagnostics().scheduling_late_frames, 35);
}

#[test]
fn select_distributions_load_in_background_and_reuse_cached_results() {
    let path = std::env::temp_dir().join(format!(
        "bmz-distribution-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE chart_analysis(chart_id INTEGER PRIMARY KEY, distribution_json TEXT NOT NULL, speed_changes_json TEXT)").unwrap();
    let notes = vec![crate::storage::library_db::ChartDistributionSecond {
        key_taps: 12,
        ..Default::default()
    }];
    conn.execute(
        "INSERT INTO chart_analysis VALUES (1, ?1, '[{\"speed\":120.0,\"time_ms\":0}]')",
        [serde_json::to_string(&notes).unwrap()],
    )
    .unwrap();
    drop(conn);
    let chart = select_chart_row(1).chart.unwrap();
    let missing = select_chart_row(2).chart.unwrap();
    let mut runtime = crate::app::select_distribution::SelectDistributionRuntime::default();
    let mut cache = HashMap::new();
    runtime.refresh(&path, &[&chart, &missing], &mut cache);
    assert!(cache.is_empty(), "first request must not wait for the worker");
    let deadline = Instant::now() + Duration::from_secs(10);
    while cache.len() < 2 {
        assert!(Instant::now() < deadline, "distribution worker did not complete");
        std::thread::sleep(Duration::from_millis(1));
        runtime.refresh(&path, &[&chart, &missing], &mut cache);
    }
    assert_eq!(cache[&1].notes[0].key_taps, 12);
    assert!(cache[&1].end_density.is_some());
    assert_eq!(cache[&1].bpm_graph_segments.len(), 1);
    assert!(cache[&2].bpm_graph_segments.is_empty());
    assert!(cache[&2].notes.is_empty());
    std::fs::remove_file(&path).unwrap();
    runtime.refresh(&path, &[], &mut cache);
    runtime.refresh(&path, &[&chart, &missing], &mut cache);
    assert_eq!(cache[&1].notes[0].key_taps, 12);
}

#[test]
fn select_snapshot_rows_centers_selection_and_copies_score_summary() {
    let rows: Vec<SelectItem> = (0..10)
        .map(|index| {
            let mut row = select_chart_row(index);
            if index == 5 {
                let mut best_score = best_score_with_replay(1234, "replay/test.toml");
                best_score.bp = 12;
                best_score.cb = 8;
                best_score.max_combo = 345;
                row.best_score = Some(best_score);
                row.replay_slots = [true, false, false, false];
                row.table_text =
                    DifficultyTableText::from_parts("Test Table".to_string(), "T", "5");
                row.table_level = row.table_text.table_level.clone();
            }
            SelectItem::Chart(row)
        })
        .collect();

    let profile = ProfileConfig::new_default("default", "Default", 0);
    let mut chart_distributions = HashMap::new();
    chart_distributions.insert(
        5,
        CachedSelectChartDistribution {
            notes: vec![crate::storage::library_db::ChartDistributionSecond {
                key_taps: 2,
                key_long_heads: 1,
                ..Default::default()
            }],
            end_density: Some(21.6),
            bpm_graph_segments: select_bpm_graph_segments(
                &[
                    crate::storage::library_db::ChartSpeedChange { speed: 100.0, time_ms: 0 },
                    crate::storage::library_db::ChartSpeedChange { speed: 200.0, time_ms: 45_000 },
                ],
                90_000,
            ),
        },
    );
    let snapshot_rows = select_snapshot_rows(&rows, 5, 7, &profile, None, &chart_distributions);

    assert_eq!(snapshot_rows.len(), 7);
    assert_eq!(snapshot_rows[0].index, 2);
    assert_eq!(snapshot_rows[3].index, 5);
    assert_eq!(snapshot_rows[3].title, "Title 5");
    assert_eq!(snapshot_rows[3].clear_type, "Normal");
    assert_eq!(snapshot_rows[3].ex_score, Some(1234));
    assert_eq!(snapshot_rows[3].bp, Some(12));
    assert_eq!(snapshot_rows[3].cb, Some(8));
    assert_eq!(snapshot_rows[3].max_combo, Some(345));
    assert_eq!(snapshot_rows[3].judge_rank, Some(1));
    assert_eq!(snapshot_rows[3].play_count, 42);
    assert_eq!(snapshot_rows[3].clear_count, 31);
    assert_eq!(snapshot_rows[3].replay_slots, [true, false, false, false]);
    assert_eq!(snapshot_rows[3].chart_normal_notes, 42);
    assert_eq!(snapshot_rows[3].chart_scratch_notes, 3);
    assert_eq!(snapshot_rows[3].chart_long_notes, 5);
    assert_eq!(snapshot_rows[3].chart_long_scratch_notes, 1);
    assert_eq!(snapshot_rows[3].chart_peak_density, 12.5);
    assert_eq!(snapshot_rows[3].chart_end_density, 21.6);
    assert_eq!(snapshot_rows[3].chart_distribution.len(), 1);
    assert_eq!(snapshot_rows[3].chart_distribution[0].key_taps, 2);
    assert_eq!(snapshot_rows[3].chart_bpm_graph_segments.len(), 2);
    assert_eq!(snapshot_rows[3].chart_bpm_graph_segments[0].start_ratio, 0.0);
    assert_eq!(snapshot_rows[3].chart_bpm_graph_segments[0].end_ratio, 0.5);
    assert_eq!(snapshot_rows[3].chart_bpm_graph_segments[1].start_ratio, 0.5);
    assert_eq!(snapshot_rows[3].chart_bpm_graph_segments[1].end_ratio, 1.0);
    assert_eq!(snapshot_rows[3].table_text_primary, "Test Table");
    assert_eq!(snapshot_rows[3].table_text_secondary, "T5");
    assert_eq!(snapshot_rows[3].table_text_fallback, "T5Test Table");
}

#[test]
fn select_snapshot_rows_preserves_settings_action_kinds() {
    let rows = vec![SelectItem::SettingsBack, SelectItem::SettingsClose];
    let profile = ProfileConfig::new_default("default", "Default", 0);

    let snapshot_rows = select_snapshot_rows(&rows, 0, 2, &profile, None, &HashMap::new());

    let back = snapshot_rows
        .iter()
        .find(|row| row.kind == bmz_render::scene::SelectRowKind::SettingsBack)
        .unwrap();
    let close = snapshot_rows
        .iter()
        .find(|row| row.kind == bmz_render::scene::SelectRowKind::SettingsClose)
        .unwrap();
    assert_eq!(back.title, "戻る");
    assert_eq!(close.title, "閉じる");
    assert_eq!(back.display_bar_text(), "戻る");
    assert_eq!(close.display_bar_text(), "閉じる");
    assert!(back.is_folder);
    assert!(close.is_folder);
}

#[test]
fn select_snapshot_rows_append_values_only_to_config_bar_text() {
    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    profile.audio_mix.master_volume = 37;
    let rows = vec![
        SelectItem::Config(crate::screens::settings_model::ConfigSelectRow {
            entry_id: SettingsEntryId::MasterVolume,
        }),
        SelectItem::KeyBinding(crate::screens::settings_model::KeyBindingSelectRow {
            key_mode: KeyMode::K7,
            target: KeyBindingTarget::Action {
                action: InputActionConfig::E1,
                slot: KeyBindingSlot::KeyboardPrimary,
            },
        }),
    ];

    let snapshot_rows = select_snapshot_rows(&rows, 0, 2, &profile, None, &HashMap::new());

    let config = snapshot_rows.iter().find(|row| row.index == 0).unwrap();
    let binding = snapshot_rows.iter().find(|row| row.index == 1).unwrap();
    assert_eq!(config.title, "MASTER");
    assert_eq!(config.display_bar_text(), "MASTER [37]");
    assert_eq!(config.artist, "37");
    assert_eq!(
        config.subtitle,
        crate::i18n::text(crate::i18n::AppLocale::Ja, "settings-entry-description-master-volume")
    );
    assert_eq!(config.play_level, "37");
    assert_eq!(binding.title, "E1 (KEYBOARD)");
    assert_eq!(binding.display_bar_text(), "E1 (KEYBOARD) [Q]");
    assert_eq!(binding.artist, "Q");
    assert_eq!(
        binding.subtitle,
        crate::i18n::text(crate::i18n::AppLocale::Ja, "settings-key-binding-description")
    );
    assert_eq!(binding.play_level, "Q");

    profile.audio_mix.master_volume = 38;
    let updated = select_snapshot_rows(&rows, 0, 2, &profile, None, &HashMap::new());
    let updated_config = updated.iter().find(|row| row.index == 0).unwrap();
    assert_eq!(updated_config.display_bar_text(), "MASTER [38]");
    assert_eq!(updated_config.artist, "38");
    assert_eq!(updated_config.subtitle, config.subtitle);
}

#[test]
fn select_snapshot_rows_formats_app_config_values_and_audio_apply_action() {
    let profile = ProfileConfig::new_default("default", "Default", 0);
    let mut app_config = AppConfig::default();
    app_config.audio.backend = crate::config::app_config::AudioBackend::Wasapi;
    let rows = vec![
        SelectItem::AppConfig(crate::screens::settings_model::AppConfigSelectRow {
            entry_id: AppSettingsEntryId::AudioBackend,
        }),
        SelectItem::ApplyAudioSettings,
    ];

    let snapshot = select_snapshot_rows_with_rival(
        &rows,
        0,
        2,
        &profile,
        &app_config,
        false,
        None,
        &HashMap::new(),
        None,
    );

    let backend = snapshot.iter().find(|row| row.index == 0).unwrap();
    let apply = snapshot.iter().find(|row| row.index == 1).unwrap();
    assert_eq!(backend.title, "AUDIO BACKEND");
    assert_eq!(backend.display_bar_text(), "AUDIO BACKEND [WASAPI]");
    assert_eq!(backend.artist, "WASAPI");
    assert_eq!(
        backend.subtitle,
        crate::i18n::text(crate::i18n::AppLocale::Ja, "settings-entry-description-audio-backend")
    );
    assert_eq!(apply.title, "適用");
    assert_eq!(
        apply.subtitle,
        crate::i18n::text(crate::i18n::AppLocale::Ja, "settings-audio-apply-help")
    );
}

#[test]
fn difficulty_table_level_display_can_use_chart_original_level() {
    let mut row = select_chart_row(12);
    row.chart.as_mut().unwrap().play_level = "9".to_string();
    row.table_text = DifficultyTableText::from_parts("Test Table".to_string(), "★", "12");
    row.table_level = row.table_text.table_level.clone();
    let rows = vec![SelectItem::Chart(row)];

    let mut profile = ProfileConfig::new_default("default", "Default", 0);
    let table_level = select_snapshot_rows_in_difficulty_table_level(&rows, 0, 1, &profile);
    assert_eq!(table_level[0].play_level, "9");
    assert_eq!(table_level[0].level_display_override.as_deref(), Some("12"));
    assert_eq!(table_level[0].table_level, "★12");
    assert_eq!(table_level[0].table_text_secondary, "★12");

    profile.select.difficulty_table_level_display =
        crate::config::profile_config::DifficultyTableLevelDisplay::Chart;
    let chart_level = select_snapshot_rows_in_difficulty_table_level(&rows, 0, 1, &profile);
    assert_eq!(chart_level[0].play_level, "9");
    assert_eq!(chart_level[0].level_display_override.as_deref(), Some("9"));
    assert!(chart_level[0].table_level.is_empty());
    assert_eq!(chart_level[0].table_text_secondary, "★12");

    let outside_table = select_snapshot_rows(&rows, 0, 1, &profile, None, &HashMap::new());
    assert_eq!(outside_table[0].table_level, "★12");
    assert_eq!(outside_table[0].level_display_override, None);

    let mut missing = rows[0].clone();
    if let SelectItem::Chart(row) = &mut missing {
        row.chart = None;
    }
    let missing = select_snapshot_rows_in_difficulty_table_level(&[missing], 0, 1, &profile);
    assert_eq!(missing[0].table_level, "★12");
    assert_eq!(missing[0].level_display_override.as_deref(), Some("12"));
}

#[test]
fn table_level_override_preserves_raw_labels_without_parsing_symbols_or_memberships() {
    let profile = ProfileConfig::new_default("default", "Default", 0);
    for level in ["0", "-1", "12", "???", "地力A", "12.5"] {
        let mut row = select_chart_row(12);
        row.table_level = "★2/★4".into();
        row.table_text = DifficultyTableText::from_parts("Table".into(), "BMS2", level);
        let snapshot = select_snapshot_rows_in_difficulty_table_level(
            &[SelectItem::Chart(row)],
            0,
            1,
            &profile,
        );
        assert_eq!(snapshot[0].level_display_override.as_deref(), Some(level));
        assert_eq!(snapshot[0].table_text_secondary, format!("BMS2{level}"));
    }
}

#[test]
fn select_snapshot_rows_uses_policy_scored_note_count() {
    let mut row = select_chart_row(0);
    let chart = row.chart.as_mut().unwrap();
    chart.total_notes = 100;
    chart.bms_total = 0.0;
    chart.ln_profile =
        crate::ln_policy::ChartLnProfile { has_defined_cn: true, ..Default::default() };
    chart.ln_counts = crate::ln_policy::ChartLnCounts { defined_cn_pairs: 2, ..Default::default() };
    let rows = vec![SelectItem::Chart(row)];
    let profile = ProfileConfig::new_default("default", "Default", 0);

    let snapshot = select_snapshot_rows(&rows, 0, 1, &profile, None, &HashMap::new());

    assert_eq!(snapshot[0].total_notes, 102);
    assert_eq!(snapshot[0].chart_total_gauge, bmz_gameplay::gauge::default_gauge_total(102) as f32);
}

#[test]
fn select_snapshot_rows_exposes_effective_bms_scale_total() {
    let mut row = select_chart_row(0);
    let chart = row.chart.as_mut().unwrap();
    chart.total_notes = 100;
    chart.bms_total = 520.0;
    let rows = vec![SelectItem::Chart(row)];
    let profile = ProfileConfig::new_default("default", "Default", 0);

    let snapshot = select_snapshot_rows(&rows, 0, 1, &profile, None, &HashMap::new());

    assert_eq!(snapshot[0].chart_total_gauge, 520.0);
}

#[test]
fn select_snapshot_rows_copies_course_best_score_summary() {
    let mut row = select_course_row(2, 2);
    row.trophy_names = vec!["goldmedal".into(), "bronzemedal".into()];
    row.best_score = Some(crate::storage::score_db::CourseBestScore {
        last_played_at: None,
        course_score_id: 99,
        course_hash: "course-hash".to_string(),
        ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
        rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
        ex_score: 1234,
        max_ex_score: 2000,
        clear_type: "Hard".to_string(),
        gauge_type: "Class".to_string(),
        gauge_value: 80.0,
        max_combo: 345,
        bp: 12,
        cb: 8,
        judge_counts: DisplayJudgeCounts {
            pgreat: 500,
            great: 100,
            good: 20,
            bad: 10,
            poor: 5,
            empty_poor: 3,
        },
        fast_slow_counts: bmz_render::snapshot::FastSlowJudgeCounts {
            fast_pgreat: 300,
            slow_pgreat: 200,
            ..Default::default()
        },
        course_failed: false,
        course_clear: true,
        play_count: 42,
        clear_count: 31,
        played_at: 1,
    });
    row.replay_slots = [true, false, true, false];
    let rows = vec![SelectItem::Course(row)];

    let profile = ProfileConfig::new_default("default", "Default", 0);
    let snapshot_rows = select_snapshot_rows(&rows, 0, 1, &profile, None, &HashMap::new());

    assert_eq!(snapshot_rows.len(), 1);
    assert_eq!(snapshot_rows[0].kind, bmz_render::scene::SelectRowKind::Course);
    assert!(snapshot_rows[0].play_level.is_empty());
    assert_eq!(snapshot_rows[0].clear_type, "Hard");
    assert_eq!(snapshot_rows[0].ex_score, Some(1234));
    assert_eq!(snapshot_rows[0].bp, Some(12));
    assert_eq!(snapshot_rows[0].cb, Some(8));
    assert_eq!(snapshot_rows[0].max_combo, Some(345));
    assert_eq!(snapshot_rows[0].judge_counts.pgreat, 500);
    assert_eq!(snapshot_rows[0].judge_counts.empty_poor, 3);
    assert_eq!(snapshot_rows[0].fast_slow_counts.unwrap().fast_pgreat, 300);
    assert_eq!(snapshot_rows[0].play_count, 42);
    assert_eq!(snapshot_rows[0].clear_count, 31);
    assert_eq!(snapshot_rows[0].replay_slots, [true, false, true, false]);
    assert_eq!(snapshot_rows[0].course_trophy_names, ["goldmedal", "bronzemedal"]);
}

#[test]
fn select_snapshot_course_labels_combine_resolved_stage_features() {
    let mut row = select_course_row(2, 2);
    row.entry_previews[0].has_long_notes = true;
    row.entry_previews[0].ln_profile.has_undefined_ln = true;
    row.entry_previews[0].has_mines = true;
    row.entry_previews[1].has_long_notes = true;
    row.entry_previews[1].ln_profile.has_defined_hcn = true;
    row.entry_previews[1].has_random = true;
    let profile = ProfileConfig::new_default("test", "Test", 0);
    let snapshot = select_snapshot_rows(
        &[SelectItem::Course(row.clone())],
        0,
        1,
        &profile,
        None,
        &HashMap::new(),
    );
    assert!(snapshot[0].has_long_notes && snapshot[0].has_mines && snapshot[0].has_random);
    assert_eq!(
        snapshot[0].source_ln_profile_bits,
        Some(
            bmz_render::snapshot::SKIN_SOURCE_LN_UNDEFINED_BIT
                | bmz_render::snapshot::SKIN_SOURCE_LN_DEFINED_HCN_BIT
        )
    );
    row.entry_previews[1].resolved = false;
    row.resolved_count = 1;
    let snapshot =
        select_snapshot_rows(&[SelectItem::Course(row)], 0, 1, &profile, None, &HashMap::new());
    assert!(!snapshot[0].in_library);
    assert!(!snapshot[0].has_random);
    assert!(snapshot[0].source_ln_profile_bits.is_none());
}

#[test]
fn select_snapshot_rows_wraps_near_edges() {
    let rows: Vec<SelectItem> = (0..4).map(|i| SelectItem::Chart(select_chart_row(i))).collect();

    let profile = ProfileConfig::new_default("default", "Default", 0);
    let snapshot_rows = select_snapshot_rows(&rows, 0, 7, &profile, None, &HashMap::new());

    assert_eq!(snapshot_rows.len(), 7);
    assert_eq!(
        snapshot_rows.iter().map(|row| row.index).collect::<Vec<_>>(),
        vec![1, 2, 3, 0, 1, 2, 3]
    );
}

#[test]
fn select_snapshot_rows_keeps_twelve_rows_around_selection() {
    let rows: Vec<SelectItem> = (0..30).map(|i| SelectItem::Chart(select_chart_row(i))).collect();

    let profile = ProfileConfig::new_default("default", "Default", 0);
    let snapshot_rows = select_snapshot_rows(&rows, 2, 25, &profile, None, &HashMap::new());

    assert_eq!(snapshot_rows.len(), 25);
    assert_eq!(snapshot_rows[0].index, 20);
    assert_eq!(snapshot_rows[12].index, 2);
    assert_eq!(snapshot_rows[24].index, 14);
}

#[test]
fn moved_select_index_moves_by_single_page_and_wraps_edges() {
    assert_eq!(moved_select_index(4, 10, SelectMove::Previous), 3);
    assert_eq!(moved_select_index(4, 10, SelectMove::Next), 5);
    assert_eq!(moved_select_index(9, 10, SelectMove::Next), 0);
    assert_eq!(moved_select_index(0, 10, SelectMove::Previous), 9);
    assert_eq!(moved_select_index(8, 10, SelectMove::PagePrevious), 1);
    assert_eq!(moved_select_index(4, 10, SelectMove::PagePrevious), 7);
    assert_eq!(moved_select_index(7, 10, SelectMove::PageNext), 4);
    assert_eq!(moved_select_index(2, 10, SelectMove::JumpPrevious), 7);
    assert_eq!(moved_select_index(7, 10, SelectMove::JumpNext), 2);
    assert_eq!(moved_select_index(1, 3, SelectMove::JumpNext), 0);
    assert_eq!(moved_select_index(1, 3, SelectMove::JumpPrevious), 2);
    assert_eq!(moved_select_index(0, 10, SelectMove::Last), 9);
    assert_eq!(moved_select_index(9, 10, SelectMove::First), 0);
}

#[test]
fn select_snapshot_rows_expose_course_stage_levels() {
    let mut row = select_course_row(2, 3);
    for (entry, level) in row.entry_previews.iter_mut().zip(["10", "12", "14"]) {
        entry.play_level = level.to_string();
    }
    let rows = vec![SelectItem::Course(row)];
    let profile = ProfileConfig::new_default("default", "Default", 0);
    let snapshot = select_snapshot_rows(&rows, 0, 1, &profile, None, &HashMap::new());
    // Unresolved stages have no level.
    assert_eq!(&snapshot[0].course_levels[..4], &[Some(10), Some(12), None, None]);
}
