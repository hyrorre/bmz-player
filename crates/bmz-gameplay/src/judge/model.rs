use bmz_chart::model::LongNoteMode;
use bmz_core::ids::NoteId;
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::Lane;
use bmz_core::time::{ChartTick, TimeUs};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JudgeAlgorithm {
    #[default]
    Combo,
    Duration,
    Lowest,
    Score,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JudgeWindow {
    pub pgreat_us: i64,
    pub great_us: i64,
    pub good_us: i64,
    pub bad_fast_us: i64,
    pub bad_slow_us: i64,
    pub empty_poor_fast_us: i64,
    pub empty_poor_slow_us: i64,
    /// Mine がヒットしたと判定する押下時刻の許容幅（前後）。
    /// beatoraja 準拠だと「踏んだ瞬間に一致」だが、フレームレート由来の
    /// 揺れを吸収するため小さな窓を設ける。
    pub mine_hit_us: i64,
}

impl JudgeWindow {
    pub const fn symmetric(
        pgreat_us: i64,
        great_us: i64,
        good_us: i64,
        bad_us: i64,
        empty_poor_fast_us: i64,
        empty_poor_slow_us: i64,
        mine_hit_us: i64,
    ) -> Self {
        Self {
            pgreat_us,
            great_us,
            good_us,
            bad_fast_us: bad_us,
            bad_slow_us: bad_us,
            empty_poor_fast_us,
            empty_poor_slow_us,
            mine_hit_us,
        }
    }

    pub const fn bad_us(self) -> i64 {
        if self.bad_fast_us > self.bad_slow_us { self.bad_fast_us } else { self.bad_slow_us }
    }

    /// 最終ノーツ後に入力がスコアへ影響し得る SLOW 側の最大時間。
    ///
    /// 見逃し Poor は `bad_slow_us` を過ぎた時点で確定し、判定済みノーツに
    /// 対する追加入力は `empty_poor_slow_us` まで Empty Poor になり得る。
    /// Mine も同じ終了判定で取りこぼさないように含める。
    pub const fn result_settle_margin_us(self) -> i64 {
        let judge_margin = if self.bad_slow_us > self.empty_poor_slow_us {
            self.bad_slow_us
        } else {
            self.empty_poor_slow_us
        };
        if judge_margin > self.mine_hit_us { judge_margin } else { self.mine_hit_us }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JudgeWindows {
    pub note: JudgeWindow,
    pub scratch: JudgeWindow,
    pub long_note_end: JudgeWindow,
    pub long_scratch_end: JudgeWindow,
    /// 通常レーンの LN/CN/HCN を早離しした後、押し直しを許す猶予。
    pub long_note_release_margin_us: i64,
    /// スクラッチ LN の早離し後、押し直しを許す猶予。
    pub long_scratch_release_margin_us: i64,
}

impl JudgeWindows {
    pub const fn uniform(window: JudgeWindow) -> Self {
        Self {
            note: window,
            scratch: window,
            long_note_end: window,
            long_scratch_end: window,
            long_note_release_margin_us: 0,
            long_scratch_release_margin_us: 0,
        }
    }

    pub const fn press_window(self, lane: Lane) -> JudgeWindow {
        match lane {
            Lane::Scratch | Lane::Scratch2 => self.scratch,
            _ => self.note,
        }
    }

    pub const fn long_end_window(self, lane: Lane) -> JudgeWindow {
        match lane {
            Lane::Scratch | Lane::Scratch2 => self.long_scratch_end,
            _ => self.long_note_end,
        }
    }

    pub const fn long_release_margin_us(self, lane: Lane) -> i64 {
        match lane {
            Lane::Scratch | Lane::Scratch2 => self.long_scratch_release_margin_us,
            _ => self.long_note_release_margin_us,
        }
    }

    /// 通常ノーツ・スクラッチ・LN 終端を含む、リザルト確定までの最大 SLOW 窓。
    pub const fn result_settle_margin_us(self) -> i64 {
        let note = self.note.result_settle_margin_us();
        let scratch = self.scratch.result_settle_margin_us();
        let long_note_end = self.long_note_end.result_settle_margin_us();
        let long_scratch_end = self.long_scratch_end.result_settle_margin_us();
        let press = if note > scratch { note } else { scratch };
        let long_end =
            if long_note_end > long_scratch_end { long_note_end } else { long_scratch_end };
        if press > long_end { press } else { long_end }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(bad_slow_us: i64, empty_poor_slow_us: i64, mine_hit_us: i64) -> JudgeWindow {
        JudgeWindow {
            pgreat_us: 10,
            great_us: 20,
            good_us: 30,
            bad_fast_us: 40,
            bad_slow_us,
            empty_poor_fast_us: 50,
            empty_poor_slow_us,
            mine_hit_us,
        }
    }

    #[test]
    fn result_settle_margin_uses_largest_slow_window() {
        assert_eq!(window(280, 150, 16).result_settle_margin_us(), 280);
        assert_eq!(window(120, 180, 16).result_settle_margin_us(), 180);
        assert_eq!(window(12, 15, 20).result_settle_margin_us(), 20);
    }

    #[test]
    fn window_set_result_margin_includes_scratch_and_long_end() {
        let windows = JudgeWindows {
            note: window(100, 90, 16),
            scratch: window(110, 140, 16),
            long_note_end: window(180, 120, 16),
            long_scratch_end: window(150, 130, 16),
            long_note_release_margin_us: 0,
            long_scratch_release_margin_us: 0,
        };

        assert_eq!(windows.result_settle_margin_us(), 180);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JudgementEvent {
    pub note_id: Option<NoteId>,
    pub lane: Lane,
    pub judge: Judge,
    pub side: TimingSide,
    pub delta: TimeUs,
    pub time: TimeUs,
    pub affects_score: bool,
}

/// Mine ノーツがプレイヤーの押下によってヒットしたイベント。
/// 通常の判定 (`JudgementEvent`) とは別ライフサイクルで、コンボ/スコアには影響せず
/// ゲージのみを `damage` 分だけ削る。
#[derive(Debug, Clone, Copy)]
pub struct MineHitEvent {
    pub note_id: NoteId,
    pub lane: Lane,
    pub damage: f64,
    pub sound: Option<bmz_core::ids::SoundId>,
    pub time: TimeUs,
}

#[derive(Debug, Clone, Default)]
pub struct JudgeOutcome {
    pub events: Vec<JudgementEvent>,
    pub mine_hits: Vec<MineHitEvent>,
    /// Input-triggered key sounds.  This is intentionally separate from
    /// `JudgementEvent::note_id`: EmptyPoor does not consume a note, and LN mode
    /// scores the start note at the end while beatoraja plays the end note sound.
    pub keysounds: Vec<KeySoundEvent>,
    /// Per-sound volume changes emitted by judgement side effects.  beatoraja
    /// mutes the held start sound when an LN/CN is released early badly.
    pub keysound_volumes: Vec<(bmz_core::ids::SoundId, f32)>,
    pub consumed_input: bool,
}

/// キー音イベントの発生要因。自動キー音モードでどれを抑制/許可するかの判定に使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySoundTrigger {
    NoteJudged,
    Fallback,
    Mine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySoundEvent {
    pub note_id: NoteId,
    pub time: TimeUs,
    pub trigger: KeySoundTrigger,
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
    pub mode: LongNoteMode,
    pub start_note_id: NoteId,
    pub start_judge: Judge,
    pub start_delta: TimeUs,
    pub end: LongNoteEndRef,
    /// HEAD が判定されてホールドが始まった時刻。beatoraja の TIMER_HOLD 相当の起点。
    pub started_at: TimeUs,
    /// Direction that started a long scratch. None for key lanes and legacy
    /// replays that predate directional scratch events.
    pub scratch_direction: Option<bmz_core::input::ScratchDirection>,
    /// BAD/POOR 相当の早離しを、release margin 中だけ保留する。
    pub pending_release: Option<PendingLongRelease>,
}

#[derive(Debug, Clone, Copy)]
pub struct PendingLongRelease {
    pub released_at: TimeUs,
    pub judge: Judge,
    pub delta: TimeUs,
}

#[derive(Debug, Clone, Copy)]
pub struct ScratchPressSuppression {
    pub direction: bmz_core::input::ScratchDirection,
    pub started_at: TimeUs,
    pub expires_at: TimeUs,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LaneJudgeState {
    pub next_note_index: usize,
    pub active_long: Option<ActiveLongNote>,
    pub scratch_press_suppression: Option<ScratchPressSuppression>,
    pub last_press_time: Option<TimeUs>,
    pub next_mine_index: usize,
    /// 直近にヒットした Mine の time。同一 Mine への二重ヒットを防ぐ簡易ガード。
    /// Mine は密集しないという前提で「直近1個」だけ覚えておけば十分。
    pub last_mine_hit_time: Option<TimeUs>,
}
