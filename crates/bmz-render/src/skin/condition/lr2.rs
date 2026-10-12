use super::*;

/// LR2's five clear lamps folded from beatoraja's clear index, matching the song bar lamps.
const LR2_CLEAR_FROM_BEATORAJA: [i64; 11] = [0, 1, 2, 2, 2, 3, 4, 4, 5, 5, 5];

/// LR2 options (`LR2_OPTION_BASE + id`) whose meaning differs from beatoraja.
/// Definitions follow LR2's `GetOptionFlag_dst`.
pub(super) fn lr2_option(id: i32, state: &SkinDrawState) -> bool {
    match id {
        // EXTRA mode and auto scratch are off; the IR ranking view and ghost battle are closed.
        52 | 54 | 56 | 620 | 622 => true,
        53 | 55 | 57 | 621 | 623 => false,
        // Selected song's clear lamp, false on folders.
        100..=105 => lr2_select_clear(state) == Some(i64::from(id - 100)),
        // Selected song's rank, only after the song has been played.
        110..=117 => lr2_select_rank(state).is_some_and(|rank| match id {
            110 => rank > 7,
            117 => rank < 2,
            _ => rank == i64::from(118 - id),
        }),
        // Rank of the score just played.
        340..=347 => {
            lr2_rank(i64::from(state.ex_score), state.total_notes).is_some_and(|rank| match id {
                340 => rank >= 8,
                347 => rank <= 1,
                _ => rank == i64::from(348 - id),
            })
        }
        _ => false,
    }
}

fn lr2_select_clear(state: &SkinDrawState) -> Option<i64> {
    if !state.select_screen || !select_chart_metadata_available(state) {
        return None;
    }
    LR2_CLEAR_FROM_BEATORAJA.get(usize::try_from(state.select_clear_index).ok()?).copied()
}

fn lr2_select_rank(state: &SkinDrawState) -> Option<i64> {
    if lr2_select_clear(state)? < 1 {
        return None;
    }
    lr2_rank(i64::from(state.select_ex_score?), state.select_total_notes)
}

/// LR2's rank index `EX * 9 / max`: 8 or more is AAA, 1 or less is F.
fn lr2_rank(ex: i64, total_notes: u32) -> Option<i64> {
    (total_notes > 0).then(|| ex * 9 / (i64::from(total_notes) * 2))
}
