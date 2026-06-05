use bmz_core::clear::ClearType;
use bmz_core::judge::{Judge, TimingSide};

use crate::gauge::GaugeState;
use crate::judge::model::JudgementEvent;

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
}

impl ScoreState {
    pub fn apply(&mut self, event: &JudgementEvent) {
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
    use bmz_core::lane::Lane;
    use bmz_core::time::TimeUs;

    use super::*;

    fn event(judge: Judge, side: TimingSide, note_id: Option<NoteId>) -> JudgementEvent {
        JudgementEvent { note_id, lane: Lane::Key1, judge, side, delta: TimeUs(0), time: TimeUs(0) }
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
}
