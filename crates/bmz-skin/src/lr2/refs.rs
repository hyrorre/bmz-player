//! LR2 numbers and options whose meaning differs from beatoraja's ref with the
//! same ID. They are moved to the LR2 namespaces so the renderer evaluates them
//! with LR2's definitions; matching IDs keep their beatoraja refs.

use bmz_skin_document::{LR2_NUMBER_BASE, LR2_OPTION_BASE};

fn is_play(skin_type: i32) -> bool {
    matches!(skin_type, 0 | 1 | 2 | 3 | 4 | 12 | 13)
}

fn is_result(skin_type: i32) -> bool {
    matches!(skin_type, 7 | 15)
}

fn is_song_scene(skin_type: i32) -> bool {
    matches!(skin_type, 5 | 6)
}

/// LR2 number refs for scenes other than the result. Result 100..136 use
/// `LR2_RESULT_NUMBER_BASE` and reach the same definitions after the flip.
pub(super) fn lr2_number_ref(skin_type: i32, id: i32) -> Option<i32> {
    match id {
        // 1P score, gauge, target diff, next rank diff and POOR including empty POOR.
        100 | 107 | 108 | 109 | 114 if is_play(skin_type) => Some(LR2_NUMBER_BASE + id),
        // Selected song's IR rank, players, clear rate and difference to the rival.
        92..=95 if is_song_scene(skin_type) => Some(LR2_NUMBER_BASE + id),
        // LR2 duplicates the max/min BPM at 290/291.
        290 | 291 => Some(id - 200),
        _ => None,
    }
}

/// Options LR2 always resolves to one value because BMZ has no such setting:
/// EXTRA mode and auto scratch are off, the IR ranking view and ghost battle are closed.
pub(super) const LR2_FIXED_OPTIONS: [(i32, bool); 10] = [
    (52, true),
    (53, false),
    (54, true),
    (55, false),
    (56, true),
    (57, false),
    (620, true),
    (621, false),
    (622, true),
    (623, false),
];

/// Song-dependent options that must be evaluated per draw instead of at load.
pub(super) fn is_lr2_scene_runtime_option(skin_type: i32, id: i32) -> bool {
    is_song_scene(skin_type) && matches!(id, 100..=105 | 110..=117)
}

/// Destination ops evaluated with LR2's definition.
pub(super) fn lr2_option_ref(skin_type: i32, op: i32) -> Option<i32> {
    let id = op.abs();
    let divergent = LR2_FIXED_OPTIONS.iter().any(|&(fixed, _)| fixed == id)
        || is_lr2_scene_runtime_option(skin_type, id)
        || (is_result(skin_type) && matches!(id, 340..=347));
    divergent.then(|| op.signum() * (LR2_OPTION_BASE + id))
}
