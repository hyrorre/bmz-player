use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

#[derive(Debug, Clone, Default)]
pub struct RenderSnapshot {
    pub time: TimeUs,
    pub combo: u32,
    pub gauge: f32,
    pub visible_notes: [Vec<VisibleNote>; LANE_COUNT],
    pub recent_judgements: Vec<DisplayJudgement>,
    pub bar_lines: Vec<VisibleBarLine>,
}

#[derive(Debug, Clone)]
pub struct VisibleNote {
    pub lane: Lane,
    pub time: TimeUs,
    pub y: f32,
}

#[derive(Debug, Clone)]
pub struct DisplayJudgement {
    pub text: String,
    pub time: TimeUs,
}

#[derive(Debug, Clone)]
pub struct VisibleBarLine {
    pub time: TimeUs,
    pub y: f32,
}
