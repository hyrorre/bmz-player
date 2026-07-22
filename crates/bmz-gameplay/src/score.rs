use bmz_chart::model::{LongNoteMode, PlayableChart};
use bmz_core::clear::ClearType;
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::KeyMode;

use crate::gauge::GaugeState;
use crate::judge::model::JudgementEvent;
use crate::rule::RuleMode;

/// Returns the number of judgement events that contribute to score for an
/// already-resolved chart.
///
/// `PlayableChart::total_notes` counts taps and long-note starts. CN/HCN ends
/// are scored independently, while LN pairs are scored once as a whole, so
/// each effective CN/HCN pair contributes one additional scored note.
pub fn scored_note_count(chart: &PlayableChart) -> u32 {
    let scored_long_ends = chart.long_notes.iter().fold(0u32, |count, pair| {
        let mode = pair.mode.unwrap_or(chart.metadata.long_note_mode);
        count.saturating_add(u32::from(matches!(mode, LongNoteMode::Cn | LongNoteMode::Hcn)))
    });
    chart.total_notes.saturating_add(scored_long_ends)
}

#[derive(Debug, Clone, Default)]
pub struct JudgeCounts {
    pub fast_pgreat: u32,
    pub slow_pgreat: u32,
    pub fast_great: u32,
    pub slow_great: u32,
    pub fast_good: u32,
    pub slow_good: u32,
    pub fast_bad: u32,
    pub slow_bad: u32,
    pub fast_poor: u32,
    pub slow_poor: u32,
    pub fast_empty_poor: u32,
    pub slow_empty_poor: u32,
}

#[derive(Debug, Clone, Default)]
pub struct ScoreState {
    pub judges: JudgeCounts,
    pub combo: u32,
    pub max_combo: u32,
    pub past_notes: u32,
    /// beatoraja 互換 ghost。各スコア対象ノーツの判定 bucket を 1 byte で保持する。
    pub ghost: Vec<u8>,
    pub empty_poor_breaks_combo: bool,
}

impl ScoreState {
    pub fn for_rule_mode(key_mode: KeyMode, rule_mode: RuleMode) -> Self {
        Self {
            empty_poor_breaks_combo: empty_poor_breaks_combo_for_rule_mode(key_mode, rule_mode),
            ..Default::default()
        }
    }

    pub fn apply(&mut self, event: &JudgementEvent) {
        if !event.affects_score {
            return;
        }

        self.increment_judge(event.judge, event.side);

        if event.note_id.is_some() {
            self.past_notes += 1;
            self.ghost.push(ghost_judge_code(event.judge));
        }

        match event.judge {
            Judge::PGreat | Judge::Great | Judge::Good => {
                self.combo += 1;
                self.max_combo = self.max_combo.max(self.combo);
            }
            Judge::Bad | Judge::Poor => {
                self.combo = 0;
            }
            Judge::EmptyPoor if self.empty_poor_breaks_combo => {
                self.combo = 0;
            }
            Judge::EmptyPoor => {}
        }
    }

    pub fn ex_score(&self) -> u32 {
        (self.judges.fast_pgreat + self.judges.slow_pgreat) * 2
            + self.judges.fast_great
            + self.judges.slow_great
    }

    pub fn total_great(&self) -> u32 {
        self.judges.fast_great + self.judges.slow_great
    }

    pub fn total_good(&self) -> u32 {
        self.judges.fast_good + self.judges.slow_good
    }

    pub fn bp(&self) -> u32 {
        self.cb() + self.judges.fast_empty_poor + self.judges.slow_empty_poor
    }

    pub fn cb(&self) -> u32 {
        self.judges.fast_bad + self.judges.slow_bad + self.judges.fast_poor + self.judges.slow_poor
    }

    pub fn bp_with_unprocessed_notes(&self, total_notes: u32) -> u32 {
        self.bp().saturating_add(total_notes.saturating_sub(self.past_notes))
    }

    pub fn cb_with_unprocessed_notes(&self, total_notes: u32) -> u32 {
        self.cb().saturating_add(total_notes.saturating_sub(self.past_notes))
    }

    pub fn ex_score_rate(&self, total_notes: u32) -> f32 {
        if total_notes == 0 { 1.0 } else { self.ex_score() as f32 / (total_notes * 2) as f32 }
    }

    fn increment_judge(&mut self, judge: Judge, side: TimingSide) {
        match (judge, side) {
            (Judge::PGreat, TimingSide::Fast) => self.judges.fast_pgreat += 1,
            (Judge::PGreat, TimingSide::Slow) => self.judges.slow_pgreat += 1,
            (Judge::Great, TimingSide::Fast) => self.judges.fast_great += 1,
            (Judge::Great, TimingSide::Slow) => self.judges.slow_great += 1,
            (Judge::Good, TimingSide::Fast) => self.judges.fast_good += 1,
            (Judge::Good, TimingSide::Slow) => self.judges.slow_good += 1,
            (Judge::Bad, TimingSide::Fast) => self.judges.fast_bad += 1,
            (Judge::Bad, TimingSide::Slow) => self.judges.slow_bad += 1,
            (Judge::Poor, TimingSide::Fast) => self.judges.fast_poor += 1,
            (Judge::Poor, TimingSide::Slow) => self.judges.slow_poor += 1,
            (Judge::EmptyPoor, TimingSide::Fast) => self.judges.fast_empty_poor += 1,
            (Judge::EmptyPoor, TimingSide::Slow) => self.judges.slow_empty_poor += 1,
        }
    }
}

pub fn empty_poor_breaks_combo_for_rule_mode(key_mode: KeyMode, rule_mode: RuleMode) -> bool {
    match rule_mode {
        RuleMode::Beatoraja => matches!(key_mode, KeyMode::K5 | KeyMode::K10 | KeyMode::K9),
        RuleMode::Dx => key_mode == KeyMode::K9,
        RuleMode::Lr2Oraja => false,
    }
}

fn ghost_judge_code(judge: Judge) -> u8 {
    match judge {
        Judge::PGreat => 0,
        Judge::Great => 1,
        Judge::Good => 2,
        Judge::Bad => 3,
        Judge::Poor | Judge::EmptyPoor => 4,
    }
}

pub fn compute_clear_type(failed_state: bool, score: &ScoreState, gauge: &GaugeState) -> ClearType {
    let result_gauge = gauge.result_gauge();
    if failed_state || !result_gauge.is_qualified() {
        return ClearType::Failed;
    }

    if score.past_notes == score.combo {
        if score.total_good() == 0 {
            if score.total_great() == 0 {
                return ClearType::Max;
            }
            return ClearType::Perfect;
        }
        return ClearType::FullCombo;
    }

    result_gauge.definition.clear_type.unwrap_or(ClearType::Failed)
}

#[cfg(test)]
mod tests {
    use bmz_core::ids::NoteId;
    use bmz_core::judge::{Judge, TimingSide};
    use bmz_core::lane::{KeyMode, Lane};
    use bmz_core::time::TimeUs;

    use super::*;

    fn event(judge: Judge, side: TimingSide, note_id: Option<NoteId>) -> JudgementEvent {
        JudgementEvent {
            note_id,
            lane: Lane::Key1,
            judge,
            side,
            delta: TimeUs(0),
            time: TimeUs(0),
            affects_score: true,
        }
    }

    #[test]
    fn bp_counts_empty_poor_but_cb_does_not() {
        let mut score = ScoreState::default();

        score.apply(&event(Judge::Bad, TimingSide::Fast, Some(NoteId(1))));
        score.apply(&event(Judge::Poor, TimingSide::Slow, Some(NoteId(2))));
        score.apply(&event(Judge::EmptyPoor, TimingSide::Slow, None));

        assert_eq!(score.cb(), 2);
        assert_eq!(score.bp(), 3);
    }

    #[test]
    fn failed_record_counts_unprocessed_notes_as_bp_and_cb() {
        let mut score = ScoreState::default();

        score.apply(&event(Judge::PGreat, TimingSide::Fast, Some(NoteId(1))));
        score.apply(&event(Judge::Bad, TimingSide::Slow, Some(NoteId(2))));
        score.apply(&event(Judge::EmptyPoor, TimingSide::Slow, None));

        assert_eq!(score.past_notes, 2);
        assert_eq!(score.cb_with_unprocessed_notes(10), 9);
        assert_eq!(score.bp_with_unprocessed_notes(10), 10);
    }

    #[test]
    fn default_empty_poor_keeps_combo() {
        let mut score = ScoreState::default();

        score.apply(&event(Judge::PGreat, TimingSide::Slow, Some(NoteId(1))));
        score.apply(&event(Judge::EmptyPoor, TimingSide::Slow, None));

        assert_eq!(score.combo, 1);
    }

    #[test]
    fn non_scoring_event_does_not_change_score_state() {
        let mut score = ScoreState::default();
        let mut event = event(Judge::PGreat, TimingSide::Slow, Some(NoteId(1)));
        event.affects_score = false;

        score.apply(&event);

        assert_eq!(score.past_notes, 0);
        assert_eq!(score.combo, 0);
        assert_eq!(score.ex_score(), 0);
    }

    #[test]
    fn fivekey_empty_poor_breaks_combo() {
        let mut score = ScoreState::for_rule_mode(KeyMode::K5, RuleMode::Beatoraja);

        score.apply(&event(Judge::PGreat, TimingSide::Slow, Some(NoteId(1))));
        score.apply(&event(Judge::EmptyPoor, TimingSide::Slow, None));

        assert_eq!(score.combo, 0);
    }

    #[test]
    fn beatoraja_empty_poor_combo_policy_matches_keymode() {
        assert!(empty_poor_breaks_combo_for_rule_mode(KeyMode::K5, RuleMode::Beatoraja));
        assert!(empty_poor_breaks_combo_for_rule_mode(KeyMode::K10, RuleMode::Beatoraja));
        assert!(empty_poor_breaks_combo_for_rule_mode(KeyMode::K9, RuleMode::Beatoraja));
        assert!(!empty_poor_breaks_combo_for_rule_mode(KeyMode::K7, RuleMode::Beatoraja));
        assert!(!empty_poor_breaks_combo_for_rule_mode(KeyMode::K6, RuleMode::Beatoraja));
        assert!(!empty_poor_breaks_combo_for_rule_mode(KeyMode::K5, RuleMode::Lr2Oraja));
        assert!(empty_poor_breaks_combo_for_rule_mode(KeyMode::K9, RuleMode::Dx));
        assert!(!empty_poor_breaks_combo_for_rule_mode(KeyMode::K7, RuleMode::Dx));
    }
}
