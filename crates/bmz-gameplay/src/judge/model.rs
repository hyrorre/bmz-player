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
    /// Mine がヒットしたと判定する押下時刻の許容幅（前後）。
    /// beatoraja 準拠だと「踏んだ瞬間に一致」だが、フレームレート由来の
    /// 揺れを吸収するため小さな窓を設ける。
    pub mine_hit_us: i64,
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

/// Mine ノーツがプレイヤーの押下によってヒットしたイベント。
/// 通常の判定 (`JudgementEvent`) とは別ライフサイクルで、コンボ/スコアには影響せず
/// ゲージのみを `damage` 分だけ削る。
#[derive(Debug, Clone, Copy)]
pub struct MineHitEvent {
    pub note_id: NoteId,
    pub lane: Lane,
    pub damage: u16,
    pub time: TimeUs,
}

#[derive(Debug, Clone, Default)]
pub struct JudgeOutcome {
    pub events: Vec<JudgementEvent>,
    pub mine_hits: Vec<MineHitEvent>,
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
    /// HCN モードで終点前に離したあと、終点までゲージを継続減衰させる。
    pub hcn_draining: bool,
    pub last_press_time: Option<TimeUs>,
    /// 直近にヒットした Mine の time。同一 Mine への二重ヒットを防ぐ簡易ガード。
    /// Mine は密集しないという前提で「直近1個」だけ覚えておけば十分。
    pub last_mine_hit_time: Option<TimeUs>,
}
