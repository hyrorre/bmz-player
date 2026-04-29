use bmz_core::ids::NoteId;
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::Lane;
use bmz_core::time::{ChartTick, TimeUs};

#[derive(Debug, Clone, Copy)]
pub struct JudgeWindow {
    pub pgreat_us: i64,
    pub great_us: i64,
    pub good_us: i64,
    pub bad_us: i64,
    pub empty_poor_fast_us: i64,
    pub empty_poor_slow_us: i64,
}

#[derive(Debug, Clone)]
pub struct JudgementEvent {
    pub note_id: Option<NoteId>,
    pub lane: Lane,
    pub judge: Judge,
    pub side: TimingSide,
    pub delta: TimeUs,
    pub time: TimeUs,
}

#[derive(Debug, Clone, Default)]
pub struct JudgeOutcome {
    pub events: Vec<JudgementEvent>,
    pub consumed_input: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct LongNoteEndRef {
    pub end_note_id: NoteId,
    pub end_tick: ChartTick,
    pub end_time: TimeUs,
}

#[derive(Debug, Clone, Copy)]
pub struct ActiveLongNote {
    pub pair_index: usize,
    pub start_note_id: NoteId,
    pub end: LongNoteEndRef,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LaneJudgeState {
    pub next_note_index: usize,
    pub active_long: Option<ActiveLongNote>,
    pub last_press_time: Option<TimeUs>,
}
