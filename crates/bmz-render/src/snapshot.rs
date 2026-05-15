use bmz_core::lane::{LANE_COUNT, Lane};
use bmz_core::time::TimeUs;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderSnapshot {
    pub time: TimeUs,
    pub duration: TimeUs,
    pub title: String,
    pub subtitle: String,
    pub artist: String,
    pub subartist: String,
    pub genre: String,
    pub combo: u32,
    pub max_combo: u32,
    pub ex_score: u32,
    pub total_notes: u32,
    pub past_notes: u32,
    pub judge_counts: DisplayJudgeCounts,
    pub gauge: f32,
    pub hispeed: f32,
    pub lift: f32,
    pub lane_cover: f32,
    pub now_bpm: f32,
    pub min_bpm: f32,
    pub max_bpm: f32,
    pub best_ex_score: Option<u32>,
    pub target_ex_score: Option<u32>,
    pub judge_timing_offset_ms: i32,
    pub visible_notes: [Vec<VisibleNote>; LANE_COUNT],
    pub recent_inputs: Vec<DisplayInput>,
    pub recent_judgements: Vec<DisplayJudgement>,
    pub bar_lines: Vec<VisibleBarLine>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisplayJudgeCounts {
    pub pgreat: u32,
    pub great: u32,
    pub good: u32,
    pub bad: u32,
    pub poor: u32,
    pub empty_poor: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisibleNote {
    pub lane: Lane,
    pub time: TimeUs,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayJudgement {
    pub lane: Lane,
    pub text: String,
    pub delta_us: i64,
    pub time: TimeUs,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayInput {
    pub lane: Lane,
    pub time: TimeUs,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VisibleBarLine {
    pub time: TimeUs,
    pub y: f32,
}
