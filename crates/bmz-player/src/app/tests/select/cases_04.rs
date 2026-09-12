use super::*;

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
fn select_snapshot_rows_centers_selection_and_copies_score_summary() {
    let rows: Vec<SelectItem> = (0..10)
        .map(|index| {
            let mut row = select_chart_row(index);
            if index == 5 {
                if let Some(analysis) = &mut row.chart_analysis {
                    analysis.speed_changes = vec![
                        crate::storage::library_db::ChartSpeedChange { speed: 100.0, time_ms: 0 },
                        crate::storage::library_db::ChartSpeedChange {
                            speed: 200.0,
                            time_ms: 45_000,
                        },
                    ];
                }
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
        vec![crate::storage::library_db::ChartDistributionSecond {
            key_taps: 2,
            key_long_heads: 1,
            ..Default::default()
        }],
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
    assert_eq!(snapshot_rows[3].chart_normal_notes, 45);
    assert_eq!(snapshot_rows[3].chart_long_notes, 6);
    assert_eq!(snapshot_rows[3].chart_peak_density, 12.5);
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
    assert_eq!(config.subtitle, "ゲーム全体の音量を調整します。");
    assert_eq!(config.play_level, "37");
    assert_eq!(binding.title, "E1 (KEYBOARD)");
    assert_eq!(binding.display_bar_text(), "E1 (KEYBOARD) [Q]");
    assert_eq!(binding.artist, "Q");
    assert_eq!(binding.subtitle, "操作またはレーンに使用する入力を割り当てます。");
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
    assert_eq!(backend.subtitle, "音声出力に使用するOS・ドライバのバックエンドを選びます。");
    assert_eq!(apply.title, "適用 (音声出力を開き直す)");
    assert_eq!(
        apply.subtitle,
        "「適用」で設定を保存し音声出力を再構築します。演奏中は選曲画面に戻ったときに適用します。"
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
    assert_eq!(table_level[0].table_level, "★12");
    assert_eq!(table_level[0].table_text_secondary, "★12");

    profile.select.difficulty_table_level_display =
        crate::config::profile_config::DifficultyTableLevelDisplay::Chart;
    let chart_level = select_snapshot_rows_in_difficulty_table_level(&rows, 0, 1, &profile);
    assert_eq!(chart_level[0].play_level, "9");
    assert!(chart_level[0].table_level.is_empty());
    assert_eq!(chart_level[0].table_text_secondary, "★12");

    let outside_table = select_snapshot_rows(&rows, 0, 1, &profile, None, &HashMap::new());
    assert_eq!(outside_table[0].table_level, "★12");

    let mut missing = rows[0].clone();
    if let SelectItem::Chart(row) = &mut missing {
        row.chart = None;
    }
    let missing = select_snapshot_rows_in_difficulty_table_level(&[missing], 0, 1, &profile);
    assert_eq!(missing[0].table_level, "★12");
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
    row.best_score = Some(crate::storage::score_db::CourseBestScore {
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
    assert_eq!(moved_select_index(0, 10, SelectMove::Last), 9);
    assert_eq!(moved_select_index(9, 10, SelectMove::First), 0);
}
