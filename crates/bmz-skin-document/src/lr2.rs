use serde::Deserialize;

/// Internal event namespace for CSV button semantics, separate from beatoraja IDs.
pub const LR2_BUTTON_BASE: i32 = 90_000;
pub const LR2_BUTTON_LAST: i32 = 90_999;
/// Result number refs 100..136, with LR2's 1P/2P flip semantics.
pub const LR2_RESULT_NUMBER_BASE: i32 = 91_000;
pub const LR2_RESULT_NUMBER_LAST: i32 = 91_036;
pub const LR2_RESULT_RANK_BASE: i32 = 92_000;
pub const LR2_RESULT_RANK_LAST: i32 = 92_018;

pub fn lr2_gauge_index(index: usize) -> Option<usize> {
    match index {
        1 => Some(3),
        2 => Some(0),
        3 => Some(1),
        5 => Some(2),
        _ => None,
    }
}

pub fn lr2_arrange_index(index: usize) -> Option<usize> {
    match index {
        0..=2 => Some(index),
        4 => Some(3),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct SkinLr2ResultDef {
    pub rank_wait: i32,
    pub update_wait: i32,
    pub graph_start: i32,
    pub graph_end: i32,
    pub flip: bool,
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
            self.update.and_then(elapsed_since),
        ]
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
        if !score_save_enabled {
            return true;
        }
        if let Some(update) = self.update {
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
    fn lr2_option_mapping_does_not_clamp_extended_settings() {
        assert_eq!((lr2_gauge_index(2), lr2_gauge_index(1)), (Some(0), Some(3)));
        assert_eq!(lr2_gauge_index(4), None);
        assert_eq!(lr2_arrange_index(4), Some(3));
        assert_eq!(lr2_arrange_index(9), None);
        assert_eq!(lr2_arrange_index(10), None);
    }
}
