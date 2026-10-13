//! LR2 numbers, options and texts whose meaning differs from beatoraja's ref with
//! the same ID. They are moved to the LR2 namespaces so the renderer evaluates them
//! with LR2's definitions; matching IDs keep their beatoraja refs.

use bmz_skin_document::{LR2_NUMBER_BASE, LR2_OPTION_BASE, LR2_TEXT_BASE};

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
        // 1P score, gauge, target diff, next rank diff, POOR including empty POOR,
        // and the best / target ghost EX and rates at the current progress.
        100 | 107 | 108 | 109 | 114 | 150 | 151 | 155..=158 if is_play(skin_type) => {
            Some(LR2_NUMBER_BASE + id)
        }
        // Judge timing, per-difficulty levels, my best score / rate, the selected
        // song's IR rank / players / clear rate / rival diff, IR clear classes and
        // the rival's max EX / rate / notes.
        12 | 45..=49 | 70 | 73 | 92..=95 | 210..=219 | 250..=259 | 272..=274
            if is_song_scene(skin_type) =>
        {
            Some(LR2_NUMBER_BASE + id)
        }
        // Extended LR2 result FAST / SLOW / combo break, as on the play screen.
        212 if is_result(skin_type) => Some(423),
        214 if is_result(skin_type) => Some(424),
        216 if is_result(skin_type) => Some(425),
        // EQ, FX and pitch have no BMZ setting; beatoraja uses 54/55 for other options.
        50..=56 | 60..=66 => Some(LR2_NUMBER_BASE + id),
        // LR2 duplicates the max/min BPM at 290/291.
        290 | 291 => Some(id - 200),
        _ => None,
    }
}

/// LR2 text refs: the embedded skin settings (skin name / author, customize rows)
/// and random course settings, which have no BMZ counterpart.
pub(super) fn lr2_text_ref(skin_type: i32, id: i32) -> Option<i32> {
    (is_song_scene(skin_type) && matches!(id, 50 | 51 | 100..=119 | 190..=199))
        .then_some(LR2_TEXT_BASE + id)
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

/// Song-scene options with an LR2-only meaning, evaluated per draw.
fn is_lr2_song_scene_option(skin_type: i32, id: i32) -> bool {
    is_song_scene(skin_type)
        && matches!(
            id,
            20 | 62..=66
                | 70..=79
                | 100..=105
                | 110..=117
                | 118..=149
                | 500..=519
                | 520..=525
                | 530..=535
                | 540..=545
                | 550..=555
                | 560..=565
                | 640..=645
                | 650..=657
        )
}

/// Options that must be evaluated per draw instead of at load. The selected song,
/// the open panel and the IR connection change while the scene is shown.
pub(super) fn is_lr2_scene_runtime_option(skin_type: i32, id: i32) -> bool {
    is_lr2_song_scene_option(skin_type, id) || (is_song_scene(skin_type) && matches!(id, 50 | 51))
}

/// Destination ops evaluated with LR2's definition.
pub(super) fn lr2_option_ref(skin_type: i32, op: i32) -> Option<i32> {
    let id = op.abs();
    let divergent = LR2_FIXED_OPTIONS.iter().any(|&(fixed, _)| fixed == id)
        || is_lr2_song_scene_option(skin_type, id)
        || (is_result(skin_type) && matches!(id, 340..=347))
        // Win / lose / draw against the 2P player, which is the target ghost in single play.
        || (is_play(skin_type) && matches!(id, 352..=354));
    divergent.then(|| op.signum() * (LR2_OPTION_BASE + id))
}
