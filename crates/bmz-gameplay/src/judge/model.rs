use bmz_chart::model::LongNoteMode;
use bmz_core::ids::NoteId;
use bmz_core::judge::{Judge, TimingSide};
use bmz_core::lane::Lane;
use bmz_core::time::{ChartTick, TimeUs};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum JudgeAlgorithm {
    // Keep the serialized names stable for saved replays.
    #[default]
    #[serde(rename = "Combo")]
    Combo,
    #[serde(rename = "Duration")]
    Duration,
    #[serde(rename = "Lowest")]
    Lowest,
    #[serde(rename = "Score")]
    Score,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JudgeWindow {
    pub pgreat_us: i64,
    pub great_us: i64,
    pub good_us: i64,
    pub bad_fast_us: i64,
    pub bad_slow_us: i64,
    pub empty_poor_fast_us: i64,
    pub empty_poor_slow_us: i64,
    /// Optional FAST-side PGREAT, GREAT and GOOD widths. The main fields describe SLOW.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fast_bands: Option<[i64; 3]>,
}

impl JudgeWindow {
    pub const fn symmetric(
        pgreat_us: i64,
        great_us: i64,
        good_us: i64,
        bad_us: i64,
        empty_poor_fast_us: i64,
        empty_poor_slow_us: i64,
    ) -> Self {
        Self {
            pgreat_us,
            great_us,
            good_us,
            bad_fast_us: bad_us,
            bad_slow_us: bad_us,
            empty_poor_fast_us,
            empty_poor_slow_us,
            fast_bands: None,
        }
    }

    pub const fn bands(self, fast: bool) -> [i64; 3] {
        if fast && let Some(bands) = self.fast_bands {
            return bands;
        }
        [self.pgreat_us, self.great_us, self.good_us]
    }

    pub const fn bad_us(self) -> i64 {
        if self.bad_fast_us > self.bad_slow_us { self.bad_fast_us } else { self.bad_slow_us }
    }

    /// 最終ノーツ後に入力がスコアへ影響し得る SLOW 側の最大時間。
    ///
    /// 見逃し Poor は `bad_slow_us` を過ぎた時点で確定し、判定済みノーツに
    /// 対する追加入力は `empty_poor_slow_us` まで Empty Poor になり得る。
    /// Mine は通過時点で確定する。
    pub const fn result_settle_margin_us(self) -> i64 {
        if self.bad_slow_us > self.empty_poor_slow_us {
            self.bad_slow_us
        } else {
            self.empty_poor_slow_us
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JudgeWindows {
    pub note: JudgeWindow,
    pub scratch: JudgeWindow,
    pub long_note_end: JudgeWindow,
    pub long_scratch_end: JudgeWindow,
    /// LN 終端を早離し (FAST) したときだけ使う窓。`None` なら `long_note_end` を両側に使う。
    ///
    /// PG/GR/GD 幅が FAST/SLOW で異なるルール用。FAST 側の値だけが判定に使われるため、
    /// SLOW 側のフィールドも FAST 側と同じ値にしておく。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_note_end_fast: Option<JudgeWindow>,
    /// スクラッチ LN 終端を早離し (FAST) したときだけ使う窓。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub long_scratch_end_fast: Option<JudgeWindow>,
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
            long_note_end_fast: None,
            long_scratch_end_fast: None,
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

    pub(crate) fn press_scan_limits(self) -> (i64, i64) {
        (
            self.note
                .bad_slow_us
                .max(self.note.empty_poor_slow_us)
                .max(self.scratch.bad_slow_us)
                .max(self.scratch.empty_poor_slow_us),
            self.note
                .bad_fast_us
                .max(self.note.empty_poor_fast_us)
                .max(self.scratch.bad_fast_us)
                .max(self.scratch.empty_poor_fast_us),
        )
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

    fn window(bad_slow_us: i64, empty_poor_slow_us: i64) -> JudgeWindow {
        JudgeWindow {
            pgreat_us: 10,
            great_us: 20,
            good_us: 30,
            bad_fast_us: 40,
            bad_slow_us,
            empty_poor_fast_us: 50,
            empty_poor_slow_us,
            fast_bands: None,
        }
    }

    #[test]
    fn result_settle_margin_uses_largest_slow_window() {
        assert_eq!(window(280, 150).result_settle_margin_us(), 280);
        assert_eq!(window(120, 180).result_settle_margin_us(), 180);
        assert_eq!(window(12, 15).result_settle_margin_us(), 15);
    }

    #[test]
    fn window_set_result_margin_includes_scratch_and_long_end() {
        let windows = JudgeWindows {
            note: window(100, 90),
            scratch: window(110, 140),
            long_note_end: window(180, 120),
            long_scratch_end: window(150, 130),
            long_note_end_fast: None,
            long_scratch_end_fast: None,
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
    /// Advances the processed-note count and writes one ghost entry.
    pub advances_score: bool,
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
    /// HLN BODY の符号付き時間積分で発生したゲージ専用イベント。
    pub hold_ticks: Vec<HoldGaugeEvent>,
    /// HLN 専用音声のゲート。音量係数であり、スコアには作用しない。
    pub hold_sounds: Vec<HoldSoundEvent>,
    pub events: Vec<JudgementEvent>,
    pub mine_hits: Vec<MineHitEvent>,
    pub potion_hits: Vec<PotionHitEvent>,
    /// Input-triggered key sounds.  This is intentionally separate from
    /// `JudgementEvent::note_id`: EmptyPoor does not consume a note, and LN mode
    /// scores the start note at the end while beatoraja plays the end note sound.
    pub keysounds: Vec<KeySoundEvent>,
    /// Per-sound volume changes emitted by judgement side effects.  beatoraja
    /// mutes the held start sound when an LN/CN is released early badly.
    pub keysound_volumes: Vec<(bmz_core::ids::SoundId, f32)>,
    pub consumed_input: bool,
}

impl JudgeOutcome {
    pub(crate) fn append(&mut self, mut other: Self) {
        self.hold_ticks.append(&mut other.hold_ticks);
        self.hold_sounds.append(&mut other.hold_sounds);
        self.events.append(&mut other.events);
        self.mine_hits.append(&mut other.mine_hits);
        self.potion_hits.append(&mut other.potion_hits);
        self.keysounds.append(&mut other.keysounds);
        self.keysound_volumes.append(&mut other.keysound_volumes);
        self.consumed_input |= other.consumed_input;
    }
    pub(crate) fn stamp_judgement_time(&mut self, time: TimeUs) {
        for event in &mut self.events {
            event.time = time;
        }
        for hit in &mut self.mine_hits {
            hit.time = time;
        }
        for sound in &mut self.keysounds {
            sound.time = time;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoldGaugeEvent {
    pub lane: Lane,
    pub time: TimeUs,
    pub increase: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PotionHitEvent {
    pub note_id: NoteId,
    pub lane: Lane,
    pub time: TimeUs,
    pub recovery: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct HlnBodyVisualState {
    pub pair_index: usize,
    pub activated: TimeUs,
    pub held: bool,
    pub reactive: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct HoldSoundEvent {
    pub note_id: NoteId,
    pub time: TimeUs,
    /// 初回の見逃し回復時だけ、始点からのサンプル位置で再生を開始する。
    pub start_at: Option<TimeUs>,
    pub audible: bool,
}

/// キー音イベントの発生要因。自動キー音モードでどれを抑制/許可するかの判定に使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySoundTrigger {
    NoteJudged,
    /// Optional no-input POOR playback, including missed long-note heads.
    Miss,
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
    pub(crate) scratch_press_absorbed_at: Option<TimeUs>,
}
