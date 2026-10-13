use super::*;

/// LR2 number refs (`LR2_NUMBER_BASE + id`) whose meaning differs from beatoraja.
/// Definitions follow LR2's `GetNumber` for the 1P side.
pub(in crate::skin) fn lr2_number(id: i32, state: &SkinDrawState) -> Option<i64> {
    let counts = state.judge_counts;
    let ex = i64::from(state.ex_score);
    match id {
        // LR2 shows the judge timing here even while the sort is changed.
        12 => Some(i64::from(state.judge_timing_offset_ms)),
        // Levels of the song's difficulties in its folder. Without the folder data
        // only the selected chart's own difficulty slot shows a level.
        45..=49 => {
            if !select_chart_metadata_available(state) {
                return None;
            }
            if let Some(slots) = state.select_same_folder {
                return slots[(id - 45) as usize].level;
            }
            if state.difficulty != i64::from(id - 44) {
                return None;
            }
            Some(if state.play_level != 0 { state.play_level } else { state.select_play_level })
        }
        // EQ, FX and pitch settings do not exist in BMZ.
        50..=56 | 60..=66 => None,
        // My best score and rate of the selected song.
        70 | 73 if !select_score_metadata_available(state) => None,
        70 => Some(lr2_point_score(
            counts.pgreat,
            counts.great,
            counts.good,
            state.select_total_notes,
            state.select_chart_key_mode.unwrap_or(state.key_mode),
        )),
        73 => {
            let max = i64::from(state.select_total_notes) * 2;
            (max > 0).then(|| i64::from(state.select_ex_score.unwrap_or(0)) * 100 / max)
        }
        // IR players / rates per LR2 clear class: FAILED, EASY, CLEAR, HARD, FULLCOMBO.
        210..=219 => lr2_ir_clear_class(&state.ir_ranking, id),
        // Selected song's IR rank / players / clear rate.
        92..=94 if !select_score_metadata_available(state) => None,
        92 => state.ir_ranking.rank,
        93 => state.ir_ranking.total_player,
        94 => lr2_ir_clear_rate(&state.ir_ranking),
        // Selected song's best EX score minus the rival's.
        95 => {
            if !select_score_metadata_available(state) {
                return None;
            }
            Some(i64::from(state.select_ex_score?) - state.rival_ex_score?)
        }
        100 => Some(lr2_point_score(
            counts.pgreat,
            counts.great,
            counts.good,
            state.total_notes,
            state.key_mode,
        )),
        107 => Some(lr2_gauge(state.gauge)),
        // EX difference to the 2P player; in single play LR2's 2P is the target ghost.
        108 => lr2_2p_ex(state).map(|other| ex - other),
        // Best / target ghost EX at the current progress, and their rates.
        150 => projected_best_score_at_progress(state).map(i64::from),
        151 => lr2_target_ghost_ex(state),
        155..=158 => {
            let ghost = if id <= 156 {
                projected_best_score_at_progress(state).map(i64::from)
            } else {
                lr2_target_ghost_ex(state)
            }?;
            let max = i64::from(state.total_notes) * 2;
            if max == 0 {
                return None;
            }
            Some(if id % 2 == 1 { ghost * 100 / max } else { ghost * 10_000 / max % 100 })
        }
        // Rival's max EX / rate / notes on the selected chart.
        272..=274 => {
            let rival = state.rival_ex_score?;
            let notes = i64::from(state.select_total_notes);
            match id {
                272 => Some(notes * 2),
                273 => (notes > 0).then(|| rival * 100 / (notes * 2)),
                _ => Some(notes),
            }
        }
        109 => Some(lr2_next_rank_diff(ex, i64::from(state.total_notes) * 2)),
        // LR2 counts empty POOR into POOR.
        114 => Some(i64::from(poor_plus_miss(counts))),
        _ => None,
    }
}

/// LR2's level bar limit per key mode (`levelbarflash_5/7/9` defaults): levels above
/// it flash, and the bar graphs reach full length at it.
pub(in crate::skin) fn lr2_level_bar_limit(state: &SkinDrawState) -> i64 {
    match state.select_chart_key_mode.unwrap_or(state.key_mode) {
        KeyMode::K5 | KeyMode::K10 => 9,
        KeyMode::K9 => 42,
        _ => 12,
    }
}

/// Level of a same-folder difficulty slot, or -1 when the folder lacks it (LR2's value).
pub(in crate::skin) fn lr2_same_folder_level(state: &SkinDrawState, slot: usize) -> Option<i64> {
    if !state.select_screen || !select_chart_metadata_available(state) {
        return None;
    }
    let slot = state.select_same_folder?[slot];
    Some(if slot.charts == 0 { -1 } else { slot.level.unwrap_or(0) })
}

/// Target ghost EX at the current progress.
fn lr2_target_ghost_ex(state: &SkinDrawState) -> Option<i64> {
    state.target_ex_score.map(|target| i64::from(projected_score_at_progress(target, state)))
}

/// EX of LR2's 2P player: the battle opponent, or the target ghost in single play.
pub(in crate::skin) fn lr2_2p_ex(state: &SkinDrawState) -> Option<i64> {
    match state.lr2_opponent.as_ref() {
        Some(opponent) => Some(i64::from(opponent.ex_score)),
        None => lr2_target_ghost_ex(state),
    }
}

/// 1P numbers 100..116 that use the LR2 definition on result screens too.
pub(in crate::skin) fn lr2_player_number_diverges(id: i32) -> bool {
    matches!(id, 100 | 107 | 108 | 109 | 114)
}

/// LR2's score: (PG*4 + GR*2 + GD) * 50000 / notes, halved in 10-point steps
/// outside 7/14 keys.
fn lr2_point_score(pgreat: u32, great: u32, good: u32, total_notes: u32, key_mode: KeyMode) -> i64 {
    if total_notes == 0 {
        return 0;
    }
    let points = (i64::from(pgreat) * 4 + i64::from(great) * 2 + i64::from(good)) * 50_000
        / i64::from(total_notes);
    if matches!(key_mode, KeyMode::K7 | KeyMode::K14) { points } else { points / 20 * 10 }
}

/// LR2 shows the gauge in steps of 2%.
fn lr2_gauge(gauge: f32) -> i64 {
    (gauge as i64 / 2) * 2
}

/// Points to the next rank boundary as a non-positive number; 0 at MAX.
fn lr2_next_rank_diff(ex: i64, max: i64) -> i64 {
    if max == 0 || ex == max {
        return 0;
    }
    let rank = (ex * 9 / max).clamp(1, 8);
    ex - max * (rank + 1) / 9
}

/// LR2's IR clear rate: players with EASY or better among all players.
fn lr2_ir_clear_rate(ranking: &crate::scene::ResultIrSnapshot) -> Option<i64> {
    let Some(counts) = ranking.clear_counts else {
        return ranking.clear_rate;
    };
    let total = counts.iter().map(|&count| i64::from(count)).sum::<i64>();
    let cleared = counts[2..].iter().map(|&count| i64::from(count)).sum::<i64>();
    (total > 0).then(|| cleared * 100 / total)
}

/// LR2 folds clears into five classes; beatoraja's assist clears count as EASY,
/// EX-HARD as HARD and PERFECT / MAX as FULLCOMBO.
fn lr2_ir_clear_class(ranking: &crate::scene::ResultIrSnapshot, id: i32) -> Option<i64> {
    if ranking.state != crate::scene::ResultIrState::Loaded {
        return None;
    }
    let counts = ranking.clear_counts?;
    let classes: [&[usize]; 5] = [&[1], &[2, 3, 4], &[5], &[6, 7], &[8, 9, 10]];
    let players = classes[((id - 210) / 2) as usize]
        .iter()
        .map(|&index| i64::from(counts[index]))
        .sum::<i64>();
    if id % 2 == 0 {
        return Some(players);
    }
    let total = counts.iter().map(|&count| i64::from(count)).sum::<i64>();
    (total > 0).then(|| players * 100 / total)
}

pub(in crate::skin) fn lr2_opponent_number(ref_id: i32, state: &SkinDrawState) -> i64 {
    // In single play LR2's 2P is the target ghost; BMZ projects only its EX score.
    let ghost =
        state.lr2_opponent.is_none().then(|| lr2_target_ghost_ex(state)).flatten().map(|ex| {
            crate::snapshot::OpponentRenderSnapshot {
                ex_score: ex as u32,
                total_notes: state.total_notes,
                past_notes: state.past_notes,
                ..Default::default()
            }
        });
    let empty = crate::snapshot::OpponentRenderSnapshot::default();
    let opponent = state.lr2_opponent.as_ref().or(ghost.as_ref()).unwrap_or(&empty);
    let ex = i64::from(opponent.ex_score);
    let total = i64::from(opponent.total_notes) * 2;
    let current = i64::from(opponent.past_notes) * 2;
    let current_rate = if current > 0 { ex * 10_000 / current } else { 0 };
    let final_rate = if total > 0 { ex * 10_000 / total } else { 0 };
    let counts = opponent.judge_counts;
    match ref_id - SKIN_REF_BMZ_LR2_2P_BASE + 120 {
        120 => lr2_point_score(
            counts.pgreat,
            counts.great,
            counts.good,
            opponent.total_notes,
            state.key_mode,
        ),
        121 => ex,
        122 => current_rate / 100,
        123 => current_rate % 100,
        124 => i64::from(opponent.combo),
        125 => i64::from(opponent.max_combo),
        126 => i64::from(opponent.total_notes),
        127 => lr2_gauge(opponent.gauge),
        128 => ex - i64::from(state.ex_score),
        129 => lr2_next_rank_diff(ex, total),
        130 => i64::from(counts.pgreat),
        131 => i64::from(counts.great),
        132 => i64::from(counts.good),
        133 => i64::from(counts.bad),
        134 => i64::from(counts.poor) + i64::from(counts.empty_poor),
        135 => final_rate / 100,
        136 => final_rate % 100,
        _ => 0,
    }
}
