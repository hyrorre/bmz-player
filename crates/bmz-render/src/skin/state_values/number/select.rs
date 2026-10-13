use super::*;

pub(in crate::skin) fn select_settings_screen_number_hidden(ref_id: i32) -> bool {
    matches!(
        ref_id,
        45..=49 | 71 | 72 | 74 | 77 | 78 | 90 | 91 | 92 | 1163 | 1164 | 121 | 150 | 170 | 350 | 370
    )
}

pub(in crate::skin) fn select_volume_number(volume: f32) -> i64 {
    (volume.clamp(0.0, 1.0) * 100.0 + 0.0001) as i64
}

pub(in crate::skin) fn select_settings_screen_number(
    ref_id: i32,
    state: &SkinDrawState,
) -> Option<i64> {
    match ref_id {
        96 if state.select_row_kind == SelectRowKind::Config => {
            Some(if state.play_level != 0 { state.play_level } else { state.select_play_level })
        }
        57 => Some(select_volume_number(state.select_master_volume)),
        58 => Some(select_volume_number(state.select_key_volume)),
        59 => Some(select_volume_number(state.select_bgm_volume)),
        12 => Some(state.judge_timing_offset_ms as i64),
        _ => None,
    }
}

pub(in crate::skin) fn select_chart_metadata_available(state: &SkinDrawState) -> bool {
    !state.select_screen
        || (state.select_row_kind == SelectRowKind::Song
            && !state.select_is_folder
            && state.select_in_library)
}

pub(in crate::skin) fn select_score_metadata_available(state: &SkinDrawState) -> bool {
    !state.select_screen
        || (matches!(state.select_row_kind, SelectRowKind::Song | SelectRowKind::Course)
            && !state.select_is_folder
            && state.select_in_library)
}

pub(in crate::skin) fn select_chart_score_number_requires_song(ref_id: i32) -> bool {
    matches!(
        ref_id,
        45..=49
            | 71
            | 72
            | 74..=89
            | 100..=116
            | 150..=158
            | 170..=178
            | 183
            | 184
            | 400
            | 410..=427
    )
}

pub(in crate::skin) fn select_chart_detail_number_requires_song(ref_id: i32) -> bool {
    matches!(ref_id, 350..=368 | 1163 | 1164)
}

pub(in crate::skin) fn player_stat_u64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

pub(in crate::skin) fn player_total_pgreat(stats: &PlayerStatsSnapshot) -> u64 {
    stats.fast_pgreat.saturating_add(stats.slow_pgreat)
}

pub(in crate::skin) fn player_total_great(stats: &PlayerStatsSnapshot) -> u64 {
    stats.fast_great.saturating_add(stats.slow_great)
}

pub(in crate::skin) fn player_total_good(stats: &PlayerStatsSnapshot) -> u64 {
    stats.fast_good.saturating_add(stats.slow_good)
}

pub(in crate::skin) fn player_total_bad(stats: &PlayerStatsSnapshot) -> u64 {
    stats.fast_bad.saturating_add(stats.slow_bad)
}

pub(in crate::skin) fn player_total_poor(stats: &PlayerStatsSnapshot) -> u64 {
    stats.fast_poor.saturating_add(stats.slow_poor)
}

pub(in crate::skin) fn player_total_play_notes(stats: &PlayerStatsSnapshot) -> u64 {
    player_total_pgreat(stats)
        .saturating_add(player_total_great(stats))
        .saturating_add(player_total_good(stats))
        .saturating_add(player_total_bad(stats))
}

pub(in crate::skin) fn daily_play_notes(stats: &DailyPlayerStatsSnapshot) -> u64 {
    stats.pgreat.saturating_add(stats.great).saturating_add(stats.good).saturating_add(stats.bad)
}

pub(in crate::skin) fn daily_completed_notes(stats: &DailyPlayerStatsSnapshot) -> u64 {
    daily_play_notes(stats).saturating_add(stats.poor).saturating_add(stats.empty_poor)
}

pub(in crate::skin) fn daily_ex_score(stats: &DailyPlayerStatsSnapshot) -> u64 {
    stats.pgreat.saturating_mul(2).saturating_add(stats.great)
}

pub(in crate::skin) fn daily_max_ex_score(stats: &DailyPlayerStatsSnapshot) -> u64 {
    daily_play_notes(stats).saturating_mul(2)
}

pub(in crate::skin) fn daily_rate_basis_points(stats: &DailyPlayerStatsSnapshot) -> u64 {
    let max = daily_max_ex_score(stats);
    daily_ex_score(stats).saturating_mul(10_000).checked_div(max).unwrap_or(0)
}

pub(in crate::skin) fn daily_rank_index(stats: &DailyPlayerStatsSnapshot) -> i64 {
    let max = daily_max_ex_score(stats);
    if max == 0 {
        return 7;
    }
    let scaled = daily_ex_score(stats).saturating_mul(9);
    (2..=8)
        .rev()
        .find(|threshold| scaled >= max.saturating_mul(*threshold))
        .map_or(7, |threshold| 8 - threshold as i64)
}

pub(in crate::skin) fn daily_rank_label(stats: &DailyPlayerStatsSnapshot) -> &'static str {
    ["AAA", "AA", "A", "B", "C", "D", "E", "F"][daily_rank_index(stats) as usize]
}

pub(in crate::skin) fn rounded_percent(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    ((numerator as f64 / denominator as f64) * 10_000.0).round() / 100.0
}

pub(in crate::skin) fn m_select_daily_rank(stats: &DailyPlayerStatsSnapshot) -> &'static str {
    daily_rank_label(stats)
}

pub(in crate::skin) fn m_select_daily_stats_text(
    id: &str,
    stats: &DailyPlayerStatsSnapshot,
) -> Option<String> {
    let notes = daily_play_notes(stats);
    let judge = |count: u64| format!("{}  ({}%)", count, rounded_percent(count, notes));
    Some(match id {
        "defaultNotesProcessingCounter_exscore" => {
            stats.pgreat.saturating_mul(2).saturating_add(stats.great).to_string()
        }
        "defaultNotesProcessingCounter_pg" => judge(stats.pgreat),
        "defaultNotesProcessingCounter_gr" => judge(stats.great),
        "defaultNotesProcessingCounter_gd" => judge(stats.good),
        "defaultNotesProcessingCounter_bd" => judge(stats.bad),
        "defaultNotesProcessingCounter_pr" => judge(stats.poor),
        "defaultNotesProcessingCounter_notes" | "defaultNotesProcessingCounter_stroke" => {
            notes.to_string()
        }
        "defaultNotesProcessingCounter_cp" => {
            format!("{}/{}", stats.clear_count, stats.play_count)
        }
        "defaultNotesProcessingCounter_rank" => m_select_daily_rank(stats).to_string(),
        "defaultNotesProcessingCounter_rate" => rounded_percent(
            stats.pgreat.saturating_mul(2).saturating_add(stats.great),
            notes.saturating_mul(2),
        )
        .to_string(),
        _ => return None,
    })
}

pub(in crate::skin) fn operating_time_seconds(state: &SkinDrawState) -> i64 {
    i64::from(state.operating_time_ms.max(0)) / 1_000
}

pub(in crate::skin) fn select_folder_lamp_counts_available(state: &SkinDrawState) -> bool {
    state.select_screen
        && matches!(
            state.select_row_kind,
            SelectRowKind::Folder
                | SelectRowKind::SearchFolder
                | SelectRowKind::TableFolder
                | SelectRowKind::Command
                | SelectRowKind::NewSongFolder
        )
}

pub(in crate::skin) fn select_row_folder_song_count(row: &SelectRowSnapshot) -> Option<u32> {
    (row.is_folder
        && matches!(
            row.kind,
            SelectRowKind::Folder
                | SelectRowKind::SearchFolder
                | SelectRowKind::TableFolder
                | SelectRowKind::Command
                | SelectRowKind::NewSongFolder
        ))
    .then(|| row.folder_lamp_counts.iter().copied().sum())
}

pub(in crate::skin) fn select_folder_lamp_count(ref_id: i32, counts: &[u32; 11]) -> Option<i64> {
    let index = match ref_id {
        320 => 0,
        321 => 1,
        322 => 2,
        323 => 3,
        324 => 4,
        325 => 5,
        326 => 6,
        327 => 7,
        328 => 8,
        329 => 9,
        330 => 10,
        _ => return None,
    };
    Some(counts[index] as i64)
}
