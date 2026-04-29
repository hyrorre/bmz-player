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
}

impl ScoreState {
    pub fn apply(&mut self, event: &JudgementEvent) {
        self.increment_judge(event.judge, event.side);

        if event.note_id.is_some() {
            self.past_notes += 1;
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

pub fn compute_clear_type(failed_state: bool, score: &ScoreState, gauge: &GaugeState) -> ClearType {
    if failed_state || !gauge.current().is_qualified() {
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

    gauge.current_clear_type().unwrap_or(ClearType::Failed)
}
