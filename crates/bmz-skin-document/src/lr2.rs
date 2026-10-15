use serde::Deserialize;

/// Internal event namespace for CSV button semantics, separate from beatoraja IDs.
pub const LR2_BUTTON_BASE: i32 = 90_000;
pub const LR2_BUTTON_LAST: i32 = 90_999;
/// Result number refs 100..136, with LR2's 1P/2P flip semantics.
pub const LR2_RESULT_NUMBER_BASE: i32 = 91_000;
pub const LR2_RESULT_NUMBER_LAST: i32 = 91_036;
pub const LR2_RESULT_RANK_BASE: i32 = 92_000;
pub const LR2_RESULT_RANK_LAST: i32 = 92_018;
/// LR2 number IDs whose meaning differs from beatoraja's ref with the same number.
/// The renderer evaluates `LR2_NUMBER_BASE + id` with LR2's definition.
pub const LR2_NUMBER_BASE: i32 = 93_000;
pub const LR2_NUMBER_LAST: i32 = 93_999;
/// LR2 option IDs whose meaning differs from beatoraja's option with the same number.
/// Negated options keep their sign: `-(LR2_OPTION_BASE + id)`.
pub const LR2_OPTION_BASE: i32 = 94_000;
pub const LR2_OPTION_LAST: i32 = 94_999;
/// LR2 text IDs whose meaning differs from beatoraja's text with the same number.
pub const LR2_TEXT_BASE: i32 = 95_000;
pub const LR2_TEXT_LAST: i32 = 95_999;

pub fn lr2_gauge_index(index: usize) -> Option<usize> {
    match index {
        1 => Some(3),
        2 => Some(0),
        3 => Some(1),
        5 => Some(2),
        _ => None,
    }
}

/// LR2 RANDOM: NORMAL / MIRROR / RANDOM / S-RANDOM / H-RANDOM / ALL-SCR.
pub fn lr2_arrange_index(index: usize) -> Option<usize> {
    match index {
        0..=2 => Some(index),
        4 => Some(3),
        6 => Some(4),
        7 => Some(5),
        _ => None,
    }
}

impl crate::SkinDocument {
    /// Rows of the embedded skin settings this LR2 select skin shows: the last row
    /// with a customize button (220..229) or row text (100..109 / 110..119), plus one.
    /// LR2 skins show five to ten rows; `None` when the skin has none.
    pub fn lr2_skin_settings_rows(&self) -> Option<usize> {
        let buttons = self
            .image
            .iter()
            .filter_map(|image| image.act)
            .filter(|act| (LR2_BUTTON_BASE + 220..=LR2_BUTTON_BASE + 229).contains(act))
            .map(|act| (act - LR2_BUTTON_BASE - 220) as usize);
        let texts = self
            .text
            .iter()
            .map(|text| text.ref_id)
            .filter(|id| (LR2_TEXT_BASE + 100..=LR2_TEXT_BASE + 119).contains(id))
            .map(|id| ((id - LR2_TEXT_BASE - 100) % 10) as usize);
        buttons.chain(texts).max().map(|row| row + 1)
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct SkinLr2ResultDef {
    pub rank_wait: i32,
    pub update_wait: i32,
    pub graph_start: i32,
    pub graph_end: i32,
    pub flip: bool,
    /// The skin draws on the update timer (152). Without it the press after the
    /// rank stage leaves the Result, as on LR2 W-MIX / ECSE.
    pub update_stage: bool,
}

impl Default for SkinLr2ResultDef {
    fn default() -> Self {
        Self {
            rank_wait: 0,
            update_wait: 0,
            graph_start: 0,
            graph_end: 0,
            flip: false,
            update_stage: true,
        }
    }
}

impl SkinLr2ResultDef {
    /// beatoraja starts the update timer with the scene when the rank time is 0,
    /// so its single press after input start leaves the Result.
    fn update_starts_with_scene(&self) -> bool {
        self.update_stage && self.rank_wait <= 0
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SkinLr2ChartDef {
    pub id: String,
    pub score: bool,
    pub player: i32,
    pub index: i32,
    pub width: i32,
    pub height: i32,
    pub start: i32,
    pub end: i32,
}

/// Absolute scene times. Saving scores is independent of these presentation stages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Lr2ResultProgress {
    pub graph_end: Option<i32>,
    pub update: Option<i32>,
}

impl Lr2ResultProgress {
    pub fn timers(self, spec: &SkinLr2ResultDef, elapsed: i32) -> [Option<i32>; 3] {
        let elapsed_since = |start: i32| elapsed.checked_sub(start).filter(|v| *v >= 0);
        [
            elapsed_since(spec.graph_start),
            elapsed_since(self.graph_end.unwrap_or(spec.graph_end)),
            self.update_start(spec).and_then(elapsed_since),
        ]
    }

    fn update_start(self, spec: &SkinLr2ResultDef) -> Option<i32> {
        self.update.or_else(|| spec.update_starts_with_scene().then_some(0))
    }

    /// Returns true only when the next input may leave the Result scene.
    pub fn press(
        &mut self,
        spec: &SkinLr2ResultDef,
        elapsed: i32,
        score_save_enabled: bool,
    ) -> bool {
        let end = self.graph_end.unwrap_or(spec.graph_end);
        if elapsed < end {
            self.graph_end = Some(elapsed);
            return false;
        }
        if elapsed.saturating_sub(end) < spec.rank_wait.max(0) {
            return false;
        }
        if !score_save_enabled || !spec.update_stage {
            return true;
        }
        if let Some(update) = self.update_start(spec) {
            elapsed.saturating_sub(update) >= spec.update_wait.max(0)
        } else {
            self.update = Some(elapsed);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_skip_wait_update_and_exit_are_separate_stages() {
        let spec = SkinLr2ResultDef {
            graph_start: 500,
            graph_end: 2000,
            rank_wait: 1600,
            update_wait: 500,
            ..Default::default()
        };
        let mut progress = Lr2ResultProgress::default();
        assert_eq!(progress.timers(&spec, 499), [None, None, None]);
        assert_eq!(progress.timers(&spec, 700), [Some(200), None, None]);
        assert!(!progress.press(&spec, 700, true));
        assert_eq!(progress.timers(&spec, 800), [Some(300), Some(100), None]);
        assert!(!progress.press(&spec, 2299, true));
        assert_eq!(progress.update, None);
        assert!(!progress.press(&spec, 2300, true));
        assert_eq!(progress.timers(&spec, 2400), [Some(1900), Some(1700), Some(100)]);
        assert!(!progress.press(&spec, 2799, true));
        assert!(progress.press(&spec, 2800, true));
        assert!(Lr2ResultProgress::default().press(&spec, 3600, false));
        assert_eq!(Lr2ResultProgress::default().timers(&spec, 2000), [Some(1500), Some(0), None]);
    }

    #[test]
    fn result_without_update_timer_or_rank_time_leaves_in_one_stage() {
        // W-MIX / ECSE: no timer 152 object, so the press after the rank stage exits.
        let no_update = SkinLr2ResultDef {
            graph_end: 2000,
            rank_wait: 500,
            update_wait: 500,
            update_stage: false,
            ..Default::default()
        };
        let mut progress = Lr2ResultProgress::default();
        assert!(!progress.press(&no_update, 2499, true));
        assert!(progress.press(&no_update, 2500, true));
        assert_eq!(progress.timers(&no_update, 2600)[2], None);

        // beatoraja: rank time 0 starts the update timer with the scene.
        let rank_zero = SkinLr2ResultDef {
            graph_end: 2000,
            rank_wait: 0,
            update_wait: 500,
            ..Default::default()
        };
        let mut progress = Lr2ResultProgress::default();
        assert_eq!(progress.timers(&rank_zero, 100)[2], Some(100));
        assert!(progress.press(&rank_zero, 2000, true));
        // A press during the graph still skips the graph first.
        let mut progress = Lr2ResultProgress::default();
        assert!(!progress.press(&rank_zero, 300, true));
        assert!(!progress.press(&rank_zero, 400, true));
        assert!(progress.press(&rank_zero, 500, true));
    }

    #[test]
    fn lr2_option_mapping_does_not_clamp_extended_settings() {
        assert_eq!((lr2_gauge_index(2), lr2_gauge_index(1)), (Some(0), Some(3)));
        assert_eq!(lr2_gauge_index(4), None);
        assert_eq!(lr2_arrange_index(4), Some(3));
        assert_eq!((lr2_arrange_index(6), lr2_arrange_index(7)), (Some(4), Some(5)));
        assert_eq!((lr2_arrange_index(3), lr2_arrange_index(5)), (None, None));
        assert_eq!(lr2_arrange_index(9), None);
        assert_eq!(lr2_arrange_index(10), None);
    }
}
