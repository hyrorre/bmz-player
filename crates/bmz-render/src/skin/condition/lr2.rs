use super::*;

/// LR2's five clear lamps folded from beatoraja's clear index, matching the song bar lamps.
const LR2_CLEAR_FROM_BEATORAJA: [i64; 11] = [0, 1, 2, 2, 2, 3, 4, 4, 5, 5, 5];

/// LR2 options (`LR2_OPTION_BASE + id`) whose meaning differs from beatoraja.
/// Definitions follow LR2's `GetOptionFlag_dst`.
pub(super) fn lr2_option(id: i32, state: &SkinDrawState) -> bool {
    match id {
        100..=105 if state.result_failed.is_some() => {
            let clear = state.best_clear_index.unwrap_or(state.select_clear_index);
            LR2_CLEAR_FROM_BEATORAJA.get(usize::try_from(clear).unwrap_or(0))
                == Some(&(i64::from(id) - 100))
        }
        // LR2's selected bar classification is independent of playability.
        1 => {
            matches!(
                state.select_row_kind,
                SelectRowKind::Folder
                    | SelectRowKind::TableFolder
                    | SelectRowKind::SearchFolder
                    | SelectRowKind::CourseFolder
                    | SelectRowKind::NewSongFolder
                    | SelectRowKind::Command
                    | SelectRowKind::Container
                    | SelectRowKind::SettingsRoot
                    | SelectRowKind::SettingsFolder
                    | SelectRowKind::SettingsBack
                    | SelectRowKind::SettingsClose
            )
        }
        2 => {
            matches!(state.select_row_kind, SelectRowKind::Song | SelectRowKind::NoSong)
        }
        3 => state.select_row_kind == SelectRowKind::Course,
        4 => {
            matches!(state.select_row_kind, SelectRowKind::NewCourse | SelectRowKind::RandomCourse)
        }
        5 => {
            !state.in_settings
                && matches!(
                    state.select_row_kind,
                    SelectRowKind::Song
                        | SelectRowKind::NoSong
                        | SelectRowKind::Course
                        | SelectRowKind::RandomCourse
                        | SelectRowKind::Executable
                )
        }
        // LR2 Select uses the STYLE filter, not the selected chart's key mode, or a
        // BATTLE that plays on both sides (D-BATTLE / SP-TO-DP). BMZ has no
        // two-player BATTLE or course editor setting.
        10 | 12 => {
            matches!(state.lr2_select_mode_index, 4..=6)
                || matches!(state.lr2_select_battle_index, 2 | 3)
        }
        11 | 571 => false,
        572 => true,
        // AUTO SCRATCH off / on for 1P (54 / 55) and 2P (56 / 57). Play shows the
        // scratch lanes actually played automatically; other scenes show the setting
        // like LR2's `p1_assist`.
        54..=57 => {
            let lane = if id < 56 { Lane::Scratch } else { Lane::Scratch2 };
            let on = if state.play_screen {
                state.auto_note_lanes[lane.index()]
            } else {
                state.auto_scratch_setting
            };
            on == (id % 2 == 1)
        }
        // EXTRA mode is off; the IR ranking view and ghost battle are closed.
        52 | 620 | 622 => true,
        53 | 621 | 623 => false,
        // No option panel is open.
        20 => state.select_option_panel == 0,
        21..=29 => i32::from(state.select_option_panel) == id - 20,
        // Selected song's clear lamp, false on folders.
        100..=105 => lr2_select_clear(state) == Some(i64::from(id - 100)),
        // Selected song's rank, only after the song has been played.
        110..=117 => lr2_select_rank(state).is_some_and(|rank| match id {
            110 => rank > 7,
            117 => rank < 2,
            _ => rank == i64::from(118 - id),
        }),
        // Level bar: normal (70..74) or above the flash limit (75..79).
        70..=79 => {
            let slot = (id - 70) as usize % 5;
            lr2_same_folder_level(state, slot)
                .is_some_and(|level| (level <= lr2_level_bar_limit(state)) == (id < 75))
        }
        // Difficulty missing (500..504) / present (505..509) / one chart (510..514) /
        // several charts (515..519) in the selected song's folder.
        500..=519 => {
            lr2_same_folder_slot(state, (id - 500) as usize % 5).is_some_and(|slot| match id {
                500..=504 => slot.charts == 0,
                505..=509 => slot.charts > 0,
                510..=514 => slot.charts == 1,
                _ => slot.charts > 1,
            })
        }
        // Level bar lamp per difficulty: 520 + 10 * slot + LR2 lamp (0..5).
        520..=565 if (id - 520) % 10 <= 5 => {
            lr2_same_folder_slot(state, ((id - 520) / 10) as usize).is_some_and(|slot| {
                LR2_CLEAR_FROM_BEATORAJA.get(usize::try_from(slot.clear_index).unwrap_or(0))
                    == Some(&i64::from((id - 520) % 10))
            })
        }
        // Gauges / RANDOM / assist used in the selected song's past clears.
        118..=149 => {
            state.select_screen
                && select_chart_metadata_available(state)
                && state.select_lr2_option_history.is_some_and(|bits| bits & (1 << (id - 118)) != 0)
        }
        // Best lamp the current settings can earn.
        62..=66 => lr2_clear_lamp_challenge(state) == i64::from(id - 62),
        // LR2's IR status: exactly one of 600..608 holds (`IRstatus`), unlike
        // beatoraja's overlapping IR options.
        600..=608 => lr2_ir_status(state) == id,
        // Selected rival's clear lamp and rank on the selected song.
        640..=645 => lr2_rival_clear(state) == Some(i64::from(id - 640)),
        650..=657 => lr2_rival_rank(state).is_some_and(|rank| match id {
            650 => rank >= 8,
            657 => rank < 2,
            _ => rank == i64::from(658 - id),
        }),
        // Win / lose / draw against the 2P player (the target ghost in single play).
        352..=354 => lr2_2p_ex(state).is_some_and(|other| {
            let ex = i64::from(state.ex_score);
            match id {
                352 => ex > other,
                353 => ex < other,
                _ => ex == other,
            }
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

/// The option of LR2's `IRstatus` for BMZ's ranking state. LR2 moves from waiting
/// (3, option 606) to connecting (4, 607) and then to a ranking with players (2, 602),
/// no players (-1, 603) or a connection failure (-3, 604); 0 (600) is no IR target.
/// BMZ has no momentary reading (601), banned song (605) or rate-limit busy (608).
fn lr2_ir_status(state: &SkinDrawState) -> i32 {
    use crate::scene::ResultIrState;
    match state.ir_ranking.state {
        ResultIrState::Offline => 600,
        ResultIrState::Waiting => 606,
        ResultIrState::Loading => 607,
        ResultIrState::Loaded if state.ir_ranking.total_player == Some(0) => 603,
        ResultIrState::Loaded => 602,
        ResultIrState::Failed => 604,
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

fn lr2_same_folder_slot(
    state: &SkinDrawState,
    slot: usize,
) -> Option<crate::scene::SelectSameFolderDifficulty> {
    if !state.select_screen || !select_chart_metadata_available(state) {
        return None;
    }
    state.select_same_folder.map(|slots| slots[slot])
}

fn lr2_rival_clear(state: &SkinDrawState) -> Option<i64> {
    if !state.select_screen || !select_chart_metadata_available(state) {
        return None;
    }
    let clear = state.rival_clear_index.unwrap_or(0);
    LR2_CLEAR_FROM_BEATORAJA.get(usize::try_from(clear).ok()?).copied()
}

fn lr2_rival_rank(state: &SkinDrawState) -> Option<i64> {
    if lr2_rival_clear(state)? < 1 {
        return None;
    }
    lr2_rank(state.rival_ex_score?, state.select_total_notes)
}

/// LR2's `CheckClearLampChallenge`: 0 no clear, 1 EASY, 2 NORMAL, 3 HARD, 4 FULLCOMBO.
/// Autoplay and practice save nothing; H-RANDOM / ALL-SCR and constant speed are
/// limited to EASY as LR2's irregular options.
fn lr2_clear_lamp_challenge(state: &SkinDrawState) -> i64 {
    // Session modes: PRACTICE / AUTOPLAY / AUTOPLAY BATTLE.
    // SP-TO-DP saves nothing in BMZ.
    if state.select_assist_index != 0
        || matches!(state.select_session_mode_index, 1..=3)
        || state.lr2_select_battle_index == 3
    {
        return 0;
    }
    let irregular_arrange = |index: usize| lr2_arrange_index(index).is_some_and(|lr2| lr2 >= 4);
    // AUTO SCRATCH is an assist except in D-BATTLE, as in LR2.
    if irregular_arrange(state.select_extended_arrange_index)
        || irregular_arrange(state.select_extended_arrange_2p_index)
        || state.constant_enabled
        || (state.auto_scratch_setting && state.lr2_select_battle_index != 2)
    {
        return 1;
    }
    // A-EASY / EASY / NORMAL / HARD / EX-HARD / HAZARD.
    match state.select_gauge_index {
        0 | 1 => 1,
        3 | 4 => 3,
        5 => 4,
        _ => 2,
    }
}

/// LR2's rank index `EX * 9 / max`: 8 or more is AAA, 1 or less is F.
fn lr2_rank(ex: i64, total_notes: u32) -> Option<i64> {
    (total_notes > 0).then(|| ex * 9 / (i64::from(total_notes) * 2))
}
