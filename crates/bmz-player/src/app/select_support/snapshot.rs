use super::*;
use crate::screens::select_model::{SelectCourseRow, SelectFolderSummary};

#[derive(Default)]
pub(in crate::app) struct CachedSelectChartDistribution {
    pub notes: Vec<ChartDistributionSecond>,
    pub end_density: Option<f64>,
    pub bpm_graph_segments: Vec<bmz_render::chart_graph::BpmGraphSegment>,
}

impl CachedSelectChartDistribution {
    pub(in crate::app) fn new(
        graph: crate::storage::library_db::ChartGraphData,
        chart: &ChartListItem,
    ) -> Self {
        let notes = graph.distribution;
        let end_density = (!notes.is_empty()).then(|| {
            crate::storage::library_db::ChartAnalysis::ending_density(
                &notes,
                (chart.bms_total > 0.0).then_some(chart.bms_total),
                chart.ln_counts.canonical_total_notes(chart.total_notes),
            )
        });
        let bpm_graph_segments = select_bpm_graph_segments(&graph.speed_changes, chart.length_ms);
        Self { notes, end_density, bpm_graph_segments }
    }
}

pub(in crate::app) fn select_chart_distribution(
    distribution: &[ChartDistributionSecond],
) -> Vec<SelectChartDistributionSecond> {
    distribution
        .iter()
        .map(|second| SelectChartDistributionSecond {
            scratch_long_heads: second.scratch_long_heads,
            scratch_long_bodies: second.scratch_long_bodies,
            scratch_taps: second.scratch_taps,
            key_long_heads: second.key_long_heads,
            key_long_bodies: second.key_long_bodies,
            key_taps: second.key_taps,
            mines: second.mines,
        })
        .collect()
}

pub(in crate::app) fn select_bpm_graph_segments(
    speed_changes: &[crate::storage::library_db::ChartSpeedChange],
    length_ms: i64,
) -> Vec<bmz_render::chart_graph::BpmGraphSegment> {
    let duration_ms = length_ms.max(1) as f32;
    let mut segments = Vec::new();
    for (index, change) in speed_changes.iter().enumerate() {
        let start_ms = change.time_ms.max(0) as f32;
        let end_ms = speed_changes
            .get(index + 1)
            .map(|next| next.time_ms.max(change.time_ms) as f32)
            .unwrap_or(duration_ms)
            .min(duration_ms);
        if end_ms <= start_ms {
            continue;
        }
        segments.push(bmz_render::chart_graph::BpmGraphSegment {
            start_ratio: (start_ms / duration_ms).clamp(0.0, 1.0),
            end_ratio: (end_ms / duration_ms).clamp(0.0, 1.0),
            bpm: change.speed.max(0.0) as f32,
            is_stop: change.speed == 0.0,
        });
    }
    segments
}

pub(in crate::app) fn select_visible_item_indices(
    item_len: usize,
    selected_index: usize,
    visible_limit: usize,
) -> Vec<usize> {
    if item_len == 0 || visible_limit == 0 {
        return Vec::new();
    }

    let selected_index = selected_index.min(item_len - 1);
    let half_window = visible_limit / 2;
    let start = (selected_index + item_len - (half_window % item_len)) % item_len;
    (0..visible_limit).map(|offset| (start + offset) % item_len).collect()
}

struct SelectSnapshotRowContext<'a> {
    profile: &'a ProfileConfig,
    app_config: &'a AppConfig,
    in_difficulty_table_level: bool,
    key_config_edit: Option<&'a KeyConfigEditSession>,
    chart_distributions: &'a HashMap<i64, CachedSelectChartDistribution>,
    select_ir: Option<&'a crate::screens::select_ir::SelectIrRanking>,
}

#[cfg(test)]
pub(in crate::app) fn select_snapshot_rows(
    items: &[SelectItem],
    selected_index: usize,
    visible_limit: usize,
    profile: &ProfileConfig,
    key_config_edit: Option<&KeyConfigEditSession>,
    chart_distributions: &HashMap<i64, CachedSelectChartDistribution>,
) -> Vec<SelectRowSnapshot> {
    let app_config = AppConfig::default();
    select_snapshot_rows_with_rival(
        items,
        selected_index,
        visible_limit,
        profile,
        &app_config,
        false,
        key_config_edit,
        chart_distributions,
        None,
    )
}

#[cfg(test)]
pub(in crate::app) fn select_snapshot_rows_in_difficulty_table_level(
    items: &[SelectItem],
    selected_index: usize,
    visible_limit: usize,
    profile: &ProfileConfig,
) -> Vec<SelectRowSnapshot> {
    let app_config = AppConfig::default();
    select_snapshot_rows_with_rival(
        items,
        selected_index,
        visible_limit,
        profile,
        &app_config,
        true,
        None,
        &HashMap::new(),
        None,
    )
}

pub(in crate::app) fn select_snapshot_rows_with_rival(
    items: &[SelectItem],
    selected_index: usize,
    visible_limit: usize,
    profile: &ProfileConfig,
    app_config: &AppConfig,
    in_difficulty_table_level: bool,
    key_config_edit: Option<&KeyConfigEditSession>,
    chart_distributions: &HashMap<i64, CachedSelectChartDistribution>,
    select_ir: Option<&crate::screens::select_ir::SelectIrRanking>,
) -> Vec<SelectRowSnapshot> {
    let context = SelectSnapshotRowContext {
        profile,
        app_config,
        in_difficulty_table_level,
        key_config_edit,
        chart_distributions,
        select_ir,
    };
    select_visible_item_indices(items.len(), selected_index, visible_limit)
        .into_iter()
        .map(|index| select_snapshot_row(index, &items[index], &context))
        .collect()
}

fn select_snapshot_row(
    index: usize,
    item: &SelectItem,
    context: &SelectSnapshotRowContext<'_>,
) -> SelectRowSnapshot {
    match item {
        SelectItem::Folder { name, kind, summary, .. } => {
            select_folder_snapshot(index, name, *kind, summary.as_ref())
        }
        SelectItem::Chart(row) => select_chart_snapshot(index, row, context),
        SelectItem::Course(row) => select_course_snapshot(index, row),
        SelectItem::Executable(row) => SelectRowSnapshot {
            index: index as u32,
            title: row.title.clone(),
            kind: if row.kind == SelectExecutableKind::RandomMix {
                bmz_render::scene::SelectRowKind::RandomCourse
            } else {
                bmz_render::scene::SelectRowKind::Executable
            },
            in_library: matches!(
                row.kind,
                SelectExecutableKind::NewCourse | SelectExecutableKind::RandomMix
            ) || !row.chart_ids.is_empty(),
            ..SelectRowSnapshot::default()
        },
        SelectItem::Config(row) => {
            let value = row.value_text(context.profile);
            let description = row.description_text(context.profile);
            select_config_snapshot(index, row.label().to_string(), value, description)
        }
        SelectItem::AppConfig(row) => {
            let value = row.value_text(context.app_config, context.profile.ui.locale());
            let description = row.description_text(context.profile);
            select_config_snapshot(index, row.label().to_string(), value, description)
        }
        SelectItem::KeyBinding(row) => {
            let value = context
                .key_config_edit
                .filter(|session| session.key_mode == row.key_mode && session.target == row.target)
                .map(|session| session.preview_value(context.profile))
                .unwrap_or_else(|| row.value_text(context.profile));
            let description = row.description_text(context.profile);
            select_config_snapshot(index, row.label(), value, description)
        }
        SelectItem::SettingsBack => select_navigation_snapshot(
            index,
            context.profile,
            "select-back",
            bmz_render::scene::SelectRowKind::SettingsBack,
        ),
        SelectItem::SettingsClose => select_navigation_snapshot(
            index,
            context.profile,
            "select-close",
            bmz_render::scene::SelectRowKind::SettingsClose,
        ),
        SelectItem::AdvancedSettings => select_navigation_snapshot(
            index,
            context.profile,
            "select-advanced-settings",
            bmz_render::scene::SelectRowKind::SettingsFolder,
        ),
        SelectItem::ApplyAudioSettings => select_settings_action_snapshot(
            index,
            context.profile,
            "settings-audio-apply",
            "settings-audio-apply-help",
        ),
    }
}

fn select_folder_snapshot(
    index: usize,
    name: &str,
    kind: bmz_render::scene::SelectRowKind,
    summary: Option<&SelectFolderSummary>,
) -> SelectRowSnapshot {
    SelectRowSnapshot {
        index: index as u32,
        title: name.to_string(),
        clear_type: summary.map(SelectFolderSummary::clear_type).unwrap_or_default(),
        folder_lamp_counts: summary.map(|summary| summary.lamp_counts).unwrap_or([0; 11]),
        is_folder: true,
        kind,
        ..SelectRowSnapshot::default()
    }
}

fn select_chart_snapshot(
    index: usize,
    row: &SelectChartRow,
    context: &SelectSnapshotRowContext<'_>,
) -> SelectRowSnapshot {
    let chart = row.chart.as_ref();
    let score = row.best_score.as_ref();
    let analysis = row.chart_analysis.as_ref();
    let scored_total_notes = chart
        .map(|chart| chart.scored_total_notes_for_setting(context.profile.play.ln_mode_policy))
        .unwrap_or(0);
    let distribution = chart.and_then(|chart| context.chart_distributions.get(&chart.chart_id));
    let rival_clear_index = context
        .select_ir
        .filter(|select_ir| select_ir.active_rival_display_name().is_some())
        .and_then(|select_ir| {
            let chart_sha256 = row.score_sha256()?;
            let ln_profile = chart.map(|chart| chart.ln_profile).unwrap_or_default();
            let policy =
                crate::ln_policy::score_ln_policy(context.profile.play.ln_mode_policy, ln_profile);
            let ln_mode = crate::screens::select_ir::rian_ln_mode_for_chart(ln_profile, policy);
            select_ir.active_rival_score(chart_sha256, ln_mode)
        })
        .map(|score| i64::from(score.clear_type).clamp(0, ClearType::Max as i64) as usize)
        .unwrap_or(0);
    let level_display_override = context.in_difficulty_table_level.then(|| {
        if context.profile.select.difficulty_table_level_display
            == crate::config::profile_config::DifficultyTableLevelDisplay::Chart
            && let Some(chart) = chart.filter(|chart| !chart.play_level.trim().is_empty())
        {
            chart.play_level.clone()
        } else {
            row.table_text.level.clone()
        }
    });
    let table_level = if context.in_difficulty_table_level
        && context.profile.select.difficulty_table_level_display
            == crate::config::profile_config::DifficultyTableLevelDisplay::Chart
        && chart.is_some_and(|chart| !chart.play_level.trim().is_empty())
    {
        String::new()
    } else {
        row.table_level.clone()
    };

    SelectRowSnapshot {
        index: index as u32,
        title: row.display_title().to_string(),
        subtitle: chart.map(|chart| chart.subtitle.clone()).unwrap_or_default(),
        artist: row.display_artist().to_string(),
        genre: chart.map(|chart| chart.genre.clone()).unwrap_or_default(),
        difficulty_name: chart.map(|chart| chart.difficulty_name.clone()).unwrap_or_default(),
        play_level: chart.map(|chart| chart.play_level.clone()).unwrap_or_default(),
        level_display_override,
        table_level,
        table_text_primary: row.table_text.table_name.clone(),
        table_text_secondary: row.table_text.table_level.clone(),
        table_text_fallback: row.table_text.table_full.clone(),
        judge_rank: chart.and_then(|chart| chart.judge_rank),
        total_notes: scored_total_notes,
        initial_bpm: chart.map(|chart| chart.initial_bpm as f32).unwrap_or(0.0),
        min_bpm: chart.map(|chart| chart.min_bpm as f32).unwrap_or(0.0),
        max_bpm: chart.map(|chart| chart.max_bpm as f32).unwrap_or(0.0),
        length_ms: chart.map(|chart| chart.length_ms).unwrap_or(0),
        clear_type: score.map(|score| score.clear_type.clone()).unwrap_or_default(),
        rival_clear_index,
        ex_score: score.map(|score| score.ex_score),
        max_combo: score.map(|score| score.max_combo),
        gauge_value: score.and_then(|score| score.gauge_value),
        bp: score.map(|score| score.bp),
        cb: score.map(|score| score.cb),
        judge_counts: score.map(|score| score.judge_counts).unwrap_or_default(),
        fast_slow_counts: score.map(|score| score.fast_slow_counts),
        play_count: score.map(|score| score.play_count).unwrap_or(0),
        clear_count: score.map(|score| score.clear_count).unwrap_or(0),
        replay_slots: row.replay_slots,
        favorite_chart: row.favorite_chart,
        favorite_song: row.favorite_song,
        has_document: row.has_document,
        has_bga: chart.is_some_and(|chart| chart.has_bga),
        has_long_notes: chart.is_some_and(|chart| chart.has_long_notes),
        has_mines: chart.is_some_and(|chart| chart.has_mines),
        has_random: chart.is_some_and(|chart| chart.has_bms_random),
        source_ln_profile_bits: chart
            .map(|chart| crate::skin_extension::source_ln_profile_bits(chart.ln_profile)),
        chart_normal_notes: analysis
            .map(|analysis| analysis.normal_notes.saturating_sub(analysis.scratch_notes))
            .unwrap_or_else(|| chart.map(|chart| chart.total_notes).unwrap_or(0)),
        chart_long_notes: analysis
            .map(|analysis| analysis.long_notes.saturating_sub(analysis.long_scratch_notes))
            .unwrap_or(0),
        chart_scratch_notes: analysis.map(|analysis| analysis.scratch_notes).unwrap_or(0),
        chart_long_scratch_notes: analysis.map(|analysis| analysis.long_scratch_notes).unwrap_or(0),
        chart_mine_notes: distribution
            .map(|distribution| {
                distribution.notes.iter().map(|second| u32::from(second.mines)).sum()
            })
            .unwrap_or(0),
        chart_density: analysis.map(|analysis| analysis.density as f32).unwrap_or(0.0),
        chart_peak_density: analysis.map(|analysis| analysis.peak_density as f32).unwrap_or(0.0),
        chart_end_density: distribution
            .and_then(|cached| cached.end_density)
            .or_else(|| analysis.map(|analysis| analysis.end_density))
            .unwrap_or(0.0) as f32,
        chart_total_gauge: chart
            .map(|chart| {
                bmz_gameplay::gauge::gauge_total_for_chart(
                    (chart.bms_total > 0.0).then_some(chart.bms_total),
                    scored_total_notes,
                ) as f32
            })
            .unwrap_or(0.0),
        chart_main_bpm: analysis
            .map(|analysis| analysis.main_bpm as f32)
            .unwrap_or_else(|| chart.map(|chart| chart.initial_bpm as f32).unwrap_or(0.0)),
        chart_distribution: distribution
            .map(|distribution| select_chart_distribution(&distribution.notes))
            .unwrap_or_default(),
        chart_bpm_graph_segments: distribution
            .map(|cached| cached.bpm_graph_segments.clone())
            .unwrap_or_default(),
        in_library: row.in_library(),
        chart_key_mode: chart.and_then(|chart| KeyMode::from_str_opt(&chart.mode)),
        ..SelectRowSnapshot::default()
    }
}

fn select_course_snapshot(index: usize, row: &SelectCourseRow) -> SelectRowSnapshot {
    let score = row.best_score.as_ref();
    SelectRowSnapshot {
        index: index as u32,
        title: row.title.clone(),
        artist: row.trophy_names.join(" / "),
        difficulty_name: row.category_label.clone(),
        total_notes: row.total_notes,
        initial_bpm: row.min_bpm,
        min_bpm: row.min_bpm,
        max_bpm: row.max_bpm,
        length_ms: row.total_length_ms,
        clear_type: score.map(|score| score.clear_type.clone()).unwrap_or_default(),
        ex_score: score.map(|score| score.ex_score),
        max_combo: score.map(|score| score.max_combo),
        gauge_value: score.map(|score| score.gauge_value),
        bp: score.map(|score| score.bp),
        cb: score.map(|score| score.cb),
        judge_counts: score.map(|score| score.judge_counts).unwrap_or_default(),
        fast_slow_counts: score.map(|score| score.fast_slow_counts),
        play_count: score.map(|score| score.play_count).unwrap_or(0),
        clear_count: score.map(|score| score.clear_count).unwrap_or(0),
        replay_slots: row.replay_slots,
        kind: bmz_render::scene::SelectRowKind::Course,
        in_library: row.exists_all_songs(),
        achieved_trophy_names: row.achieved_trophy_names.clone(),
        course_titles: course_titles_from_entries(
            row.entry_previews.iter().map(|entry| (entry.title.as_str(), entry.resolved)),
        ),
        course_constraints: course_constraint_flags(&row.constraints),
        ..SelectRowSnapshot::default()
    }
}

fn select_config_snapshot(
    index: usize,
    title: String,
    value: String,
    description: String,
) -> SelectRowSnapshot {
    let bar_text = format!("{title} [{value}]");
    SelectRowSnapshot {
        index: index as u32,
        title,
        bar_text,
        subtitle: description,
        artist: value.clone(),
        play_level: value,
        kind: bmz_render::scene::SelectRowKind::Config,
        ..SelectRowSnapshot::default()
    }
}

fn select_navigation_snapshot(
    index: usize,
    profile: &ProfileConfig,
    title_id: &str,
    kind: bmz_render::scene::SelectRowKind,
) -> SelectRowSnapshot {
    SelectRowSnapshot {
        index: index as u32,
        title: Localizer::new(profile.ui.locale()).text(title_id),
        is_folder: true,
        kind,
        ..SelectRowSnapshot::default()
    }
}

fn select_settings_action_snapshot(
    index: usize,
    profile: &ProfileConfig,
    title_id: &str,
    description_id: &str,
) -> SelectRowSnapshot {
    let text = Localizer::new(profile.ui.locale());
    SelectRowSnapshot {
        index: index as u32,
        title: text.text(title_id),
        subtitle: text.text(description_id),
        kind: bmz_render::scene::SelectRowKind::SettingsFolder,
        ..SelectRowSnapshot::default()
    }
}
