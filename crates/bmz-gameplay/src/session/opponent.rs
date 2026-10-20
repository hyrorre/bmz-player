use super::*;

/// Battle presentation can retain display-only state alongside an independent
/// opponent. Independent judging remains the authority for opponent display.
#[derive(Default)]
pub struct OpponentRuntime {
    pub score: Option<ScoreState>,
    pub gauge: Option<GaugeState>,
    pub independent: Option<BattleOpponentSession>,
    pub gauge_increase_started_at: Option<TimeUs>,
    pub gauge_max_started_at: Option<TimeUs>,
    pub full_combo_started_at: Option<TimeUs>,
}

pub struct OpponentDisplayState<'a> {
    pub score: &'a ScoreState,
    pub gauge: &'a GaugeState,
    pub total_notes: u32,
    pub gauge_increase_started_at: Option<TimeUs>,
    pub gauge_max_started_at: Option<TimeUs>,
    full_combo_started_at: Option<TimeUs>,
    independent: bool,
}

impl OpponentRuntime {
    pub fn display_state(&self, primary_total_notes: u32) -> Option<OpponentDisplayState<'_>> {
        if let Some(opponent) = &self.independent {
            return Some(OpponentDisplayState {
                score: &opponent.score,
                gauge: &opponent.gauge,
                total_notes: opponent.scored_total_notes,
                gauge_increase_started_at: opponent.gauge_increase_started_at,
                gauge_max_started_at: opponent.gauge_max_started_at,
                full_combo_started_at: opponent.full_combo_started_at,
                independent: true,
            });
        }
        Some(OpponentDisplayState {
            score: self.score.as_ref()?,
            gauge: self.gauge.as_ref()?,
            total_notes: primary_total_notes,
            gauge_increase_started_at: self.gauge_increase_started_at,
            gauge_max_started_at: self.gauge_max_started_at,
            full_combo_started_at: self.full_combo_started_at,
            independent: false,
        })
    }
}

impl OpponentDisplayState<'_> {
    pub fn full_combo_elapsed_ms(&self, now: TimeUs) -> Option<i32> {
        let started_at = self.full_combo_started_at?;
        let elapsed = now.0 - started_at.0;
        // Preserve the display-only future-timer gate and independent timer clamp.
        (self.independent || elapsed >= 0)
            .then_some((elapsed / 1_000).clamp(0, i32::MAX as i64) as i32)
    }
}
