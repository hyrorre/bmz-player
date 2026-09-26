use super::*;

pub(super) fn select_ir_cache_context(
    ln_policy_setting: crate::ln_policy::LnPolicySetting,
    ln_policy: crate::ln_policy::LnScorePolicy,
    double_option: crate::select_options::DoubleOptionScoreBucket,
    rule_mode: bmz_gameplay::rule::RuleMode,
) -> String {
    format!(
        "{}:{}:{}:{}",
        ln_policy_setting.as_ir_str(),
        ln_policy.as_str(),
        double_option.as_str(),
        rule_mode.as_str()
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CoursePlayMetrics {
    pub(super) total_notes: u32,
    pub(super) ln_mode: Option<bmz_chart::model::LongNoteMode>,
    pub(super) ln_policy: crate::ln_policy::LnScorePolicy,
}

pub(super) struct CourseLibrarySnapshot {
    pub(super) metrics: CoursePlayMetrics,
    pub(super) first_chart: ChartListItem,
    pub(super) titles: HashMap<i64, String>,
    pub(super) has_score_disabling_key_mode_conversion: bool,
}

fn finish_course_play_metrics(
    total_notes: u32,
    ln_mode: Option<bmz_chart::model::LongNoteMode>,
    source_ln_profile: crate::ln_policy::ChartLnProfile,
    ln_policy_setting: crate::ln_policy::LnPolicySetting,
    entry_start_options: &[PlayStartOptions],
) -> CoursePlayMetrics {
    let course_fallback = entry_start_options.first().and_then(|options| options.ln_mode_override);
    let ln_policy = crate::ln_policy::course_score_ln_policy_for_profiles(
        ln_policy_setting,
        course_fallback,
        [source_ln_profile],
    );
    CoursePlayMetrics { total_notes, ln_mode, ln_policy }
}

/// Resolve every course copy before any skin, replay or preload uses its folder.
/// Availability checks during library refresh do not verify the file contents.
pub(super) fn resolve_course_chart_sources(
    library_db: &LibraryDatabase,
    definition: &mut bmz_core::course::CourseDefinition,
) -> Result<()> {
    let chart_ids = definition
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let id = entry
                .chart_id
                .with_context(|| format!("course entry {} is not resolved", index + 1))?;
            library_db
                .verified_chart_source(id)
                .map(|source| source.chart.chart_id)
                .with_context(|| format!("course entry {} is unavailable", index + 1))
        })
        .collect::<Result<Vec<_>>>()?;
    for (entry, id) in definition.entries.iter_mut().zip(chart_ids) {
        entry.chart_id = Some(id);
    }
    Ok(())
}

/// DBへ保存済みの譜面メタデータだけを一括取得し、decide入場前に使う軽量な
/// コース集計値と先頭譜面メタデータを返す。
///
/// `#RANDOM` 分岐後の厳密値は background import で後から置き換える。
pub(super) fn course_play_metrics_from_library_metadata(
    library_db: &LibraryDatabase,
    definition: &bmz_core::course::CourseDefinition,
    ln_policy_setting: crate::ln_policy::LnPolicySetting,
    entry_start_options: &[PlayStartOptions],
) -> Result<CourseLibrarySnapshot> {
    let chart_ids = definition
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            entry.chart_id.with_context(|| format!("course entry {} is not resolved", index + 1))
        })
        .collect::<Result<Vec<_>>>()?;
    let charts = library_db.list_charts_by_ids(&chart_ids)?;
    course_play_metrics_from_chart_metadata(
        definition,
        ln_policy_setting,
        entry_start_options,
        charts,
    )
}

pub(super) fn course_play_metrics_from_chart_metadata(
    definition: &bmz_core::course::CourseDefinition,
    ln_policy_setting: crate::ln_policy::LnPolicySetting,
    entry_start_options: &[PlayStartOptions],
    charts: Vec<ChartListItem>,
) -> Result<CourseLibrarySnapshot> {
    anyhow::ensure!(
        definition.entries.len() == entry_start_options.len(),
        "course entry option count mismatch: entries={}, options={}",
        definition.entries.len(),
        entry_start_options.len()
    );
    let chart_ids = definition
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            entry.chart_id.with_context(|| format!("course entry {} is not resolved", index + 1))
        })
        .collect::<Result<Vec<_>>>()?;
    let charts_by_id =
        charts.into_iter().map(|chart| (chart.chart_id, chart)).collect::<HashMap<_, _>>();
    let first_chart_id = *chart_ids.first().context("course has no entries")?;
    let first_chart = charts_by_id
        .get(&first_chart_id)
        .cloned()
        .with_context(|| format!("course first chart {first_chart_id} is not in the library"))?;

    let mut total_notes = 0u32;
    let mut ln_mode = None;
    let mut source_ln_profile = crate::ln_policy::ChartLnProfile::default();
    let mut has_score_disabling_key_mode_conversion = false;
    for (index, (chart_id, start_options)) in chart_ids.iter().zip(entry_start_options).enumerate()
    {
        let chart = charts_by_id.get(chart_id).with_context(|| {
            format!("course entry {} chart {chart_id} is not in the library", index + 1)
        })?;
        let score_policy = crate::ln_policy::course_score_ln_policy(
            ln_policy_setting,
            start_options.ln_mode_override,
            chart.ln_profile,
        );
        let key_mode = KeyMode::from_str_opt(&chart.mode).unwrap_or_default();
        let conversion_applies = !start_options.session_mode.is_battle()
            && start_options.battle_target.is_none()
            && !matches!(
                start_options.double_option,
                DoubleOption::Battle | DoubleOption::BattleAutoScratch
            )
            && start_options.key_mode_conversion.applies_to(key_mode);
        has_score_disabling_key_mode_conversion |= conversion_applies
            && start_options
                .key_mode_conversion
                .score_persistence_disabled(start_options.seven_to_nine_rule_mode);
        let double_option = if conversion_applies {
            DoubleOption::Off
        } else {
            start_options.double_option.normalize_for_key_mode(key_mode)
        };
        let multiplier =
            if matches!(double_option, DoubleOption::Battle | DoubleOption::BattleAutoScratch) {
                2
            } else {
                1
            };
        total_notes = total_notes
            .saturating_add(chart.scored_total_notes(score_policy).saturating_mul(multiplier));
        ln_mode = crate::ln_policy::max_long_note_mode(
            ln_mode,
            crate::ln_policy::played_ln_mode(chart.ln_profile, score_policy),
        );
        source_ln_profile = source_ln_profile.merge(chart.ln_profile);
    }

    let metrics = finish_course_play_metrics(
        total_notes,
        ln_mode,
        source_ln_profile,
        ln_policy_setting,
        entry_start_options,
    );
    let titles =
        charts_by_id.into_iter().map(|(chart_id, chart)| (chart_id, chart.title)).collect();
    Ok(CourseLibrarySnapshot {
        metrics,
        first_chart,
        titles,
        has_score_disabling_key_mode_conversion,
    })
}

/// 先頭譜面はPlay preload workerが既に変換した値を再利用し、残りの譜面だけを
/// sourceからimportして厳密なコース集計値を返す。
pub(super) fn course_play_metrics_for_definition_reusing_first(
    library_db: &LibraryDatabase,
    definition: &bmz_core::course::CourseDefinition,
    app_config: &AppConfig,
    ln_policy_setting: crate::ln_policy::LnPolicySetting,
    rule_mode: bmz_gameplay::rule::RuleMode,
    entry_start_options: &[PlayStartOptions],
    first_metrics: crate::screens::play_session::ScoredChartMetrics,
) -> Result<CoursePlayMetrics> {
    anyhow::ensure!(
        definition.entries.len() == entry_start_options.len(),
        "course entry option count mismatch: entries={}, options={}",
        definition.entries.len(),
        entry_start_options.len()
    );
    anyhow::ensure!(!definition.entries.is_empty(), "course has no entries");

    let mut total_notes = 0u32;
    let mut ln_mode = None;
    let mut source_ln_profile = crate::ln_policy::ChartLnProfile::default();
    for (index, (entry, start_options)) in
        definition.entries.iter().zip(entry_start_options).enumerate()
    {
        let entry_started_at = Instant::now();
        let metrics = if index == 0 {
            first_metrics
        } else {
            let chart_id = entry
                .chart_id
                .with_context(|| format!("course entry {} is not resolved", index + 1))?;
            let mut session_options =
                play_session_options_from_start(app_config, start_options.clone());
            session_options.ln_policy_setting = ln_policy_setting;
            session_options.rule_mode = rule_mode;
            crate::screens::play_session::scored_chart_metrics_for_chart(
                library_db,
                chart_id,
                &session_options,
            )
            .with_context(|| format!("failed to count course entry {} from source", index + 1))?
        };
        tracing::info!(
            entry_index = index + 1,
            reused_play_preload = index == 0,
            elapsed_ms = entry_started_at.elapsed().as_millis(),
            total_notes = metrics.total_notes,
            "course background metrics counted entry"
        );
        total_notes = total_notes.saturating_add(metrics.total_notes);
        ln_mode = crate::ln_policy::max_long_note_mode(ln_mode, metrics.ln_mode);
        source_ln_profile = source_ln_profile.merge(metrics.source_ln_profile);
    }
    Ok(finish_course_play_metrics(
        total_notes,
        ln_mode,
        source_ln_profile,
        ln_policy_setting,
        entry_start_options,
    ))
}

pub(super) fn apply_course_entry_title_hints(
    definition: &mut bmz_core::course::CourseDefinition,
    titles: &HashMap<i64, String>,
) {
    for entry in &mut definition.entries {
        let Some(chart_id) = entry.chart_id else {
            continue;
        };
        let Some(title) = titles.get(&chart_id).filter(|title| !title.trim().is_empty()) else {
            continue;
        };
        entry.title_hint.clone_from(title);
    }
}

pub(super) fn player_stats_snapshot(
    score_db: &ScoreDatabase,
    library_db: &LibraryDatabase,
    day_start_hour: u8,
) -> PlayerStatsSnapshot {
    let mut snapshot = match score_db.player_stats() {
        Ok(stats) => player_stats_snapshot_from_stats(&stats),
        Err(error) => {
            tracing::warn!(%error, "failed to load player statistics");
            PlayerStatsSnapshot::default()
        }
    };
    match score_db.current_daily_statistics_range(day_start_hour) {
        Ok((start_at, end_at)) => {
            match score_db.daily_player_stats_between(start_at, end_at) {
                Ok(stats) => snapshot.daily = daily_player_stats_snapshot_from_stats(&stats),
                Err(error) => tracing::warn!(%error, "failed to load daily player statistics"),
            }
            match score_db.daily_recent_chart_sha256s_between(start_at, end_at, 10) {
                Ok(hashes) => {
                    for (index, hash) in hashes.into_iter().enumerate() {
                        snapshot.daily.recent_titles[index] = library_db
                            .list_charts_by_sha256(hash)
                            .ok()
                            .and_then(|charts| charts.into_iter().next())
                            .map(|chart| chart.title)
                            .unwrap_or_default();
                    }
                }
                Err(error) => tracing::warn!(%error, "failed to load recent daily chart titles"),
            }
        }
        Err(error) => tracing::warn!(%error, "failed to resolve daily statistics range"),
    }
    snapshot
}

pub(super) fn player_stats_snapshot_from_stats(stats: &PlayerStats) -> PlayerStatsSnapshot {
    PlayerStatsSnapshot {
        session_play_count: 0,
        session_play_notes: 0,
        play_count: stats.play_count,
        clear_count: stats.clear_count,
        playtime_seconds: stats.playtime_seconds,
        max_combo: stats.max_combo,
        fast_pgreat: stats.fast_pgreat,
        slow_pgreat: stats.slow_pgreat,
        fast_great: stats.fast_great,
        slow_great: stats.slow_great,
        fast_good: stats.fast_good,
        slow_good: stats.slow_good,
        fast_bad: stats.fast_bad,
        slow_bad: stats.slow_bad,
        fast_poor: stats.fast_poor,
        slow_poor: stats.slow_poor,
        fast_empty_poor: stats.fast_empty_poor,
        slow_empty_poor: stats.slow_empty_poor,
        daily: DailyPlayerStatsSnapshot::default(),
    }
}

pub(super) fn daily_player_stats_snapshot_from_stats(
    stats: &DailyPlayerStats,
) -> DailyPlayerStatsSnapshot {
    DailyPlayerStatsSnapshot {
        play_count: stats.play_count,
        clear_count: stats.clear_count,
        pgreat: stats.pgreat,
        great: stats.great,
        good: stats.good,
        bad: stats.bad,
        poor: stats.poor,
        empty_poor: stats.empty_poor,
        score_update_count: stats.score_update_count,
        clear_update_count: stats.clear_update_count,
        miss_count_update_count: stats.miss_count_update_count,
        recent_titles: Default::default(),
    }
}

pub(super) fn record_skin_session_play(
    stats: &mut PlayerStatsSnapshot,
    result: &bmz_gameplay::result::PlayResult,
    saved: bool,
    replay: bool,
) {
    if saved && !replay && !result.autoplay {
        stats.session_play_count = stats.session_play_count.saturating_add(1);
        stats.session_play_notes =
            stats.session_play_notes.saturating_add(u64::from(result.score.past_notes));
    }
}

pub(super) fn initialize_gamepad_backend(
    kind: GamepadBackendKind,
    configs: [crate::input::gamepad::GamepadScratchConfig; 2],
    bridge: Option<crate::input::rawinput::RawInputBridge>,
) -> Option<crate::input::capture::InputCapture> {
    match crate::input::capture::InputCapture::new(Some(kind), configs, bridge) {
        Ok(capture) => Some(capture),
        Err(error) => {
            tracing::error!(%error, "failed to start input capture");
            None
        }
    }
}

pub(super) fn initialize_gilrs_backend(
    configs: [crate::input::gamepad::GamepadScratchConfig; 2],
) -> Option<crate::input::capture::InputCapture> {
    initialize_gamepad_backend(GamepadBackendKind::Gilrs, configs, None)
}

pub(super) fn gamepad_scratch_configs(
    input: &ProfileInputConfig,
) -> [crate::input::gamepad::GamepadScratchConfig; 2] {
    [input.gamepad1, input.gamepad2].map(|config| crate::input::gamepad::GamepadScratchConfig {
        analog_scratch: config.analog_scratch,
        sensitivity: config.analog_scratch_sensitivity,
        threshold: config.analog_scratch_threshold,
    })
}

pub(super) fn resolve_gamepad_runtime_slots(
    config: &GlobalInputConfig,
    backend: Option<&crate::input::capture::InputCapture>,
) -> [Option<DeviceId>; 2] {
    let connected = backend
        .into_iter()
        .flat_map(crate::input::capture::InputCapture::connected_gamepads)
        .collect::<Vec<_>>();
    let using_gilrs = backend.is_some_and(crate::input::capture::InputCapture::is_gilrs);
    crate::input::gamepad::resolve_gamepad_slot_assignments(
        config.gamepad_slot_device_ids.each_ref().map(Option::as_deref),
        config.gamepad_slot_gilrs_ids,
        using_gilrs,
        !using_gilrs,
        &connected,
    )
}
