//! The play result an OpenLR2 CustomIR module (such as Stellaverse) receives,
//! built from LEGACY judgement events.
//!
//! OpenLR2 fills `IRScoreV1` from `PLAYERSTATUS` and a few play-scene tables
//! (`LR2_customir.cpp`). Their rules differ from BMZ's own score:
//!
//! - Early/late and FAST/SLOW are counted per branch of `ProcSinglenote` /
//!   `ProcLongnote` (`Scene04_Play.cpp`). PGREAT is never FAST/SLOW, a passed
//!   POOR is late and a combo break, an EMPTY POOR is early and not a note.
//!   A long note counts FAST/SLOW when its head is hit and is scored at its
//!   end with the head's early/late. The object after a BAD is judged by
//!   `ProcSinglenote` even when it is a long note, so it counts as a tap.
//! - All six gauges move on every judgement and mine (`ApplyJudgeNote`,
//!   `ApplyJudgeMine`); a play that ends with notes left zeroes all of them.
//! - The graphs keep 1000 samples over `song_runtime`, written on each poll
//!   (`LogGraphPlayData`) and filled to the end with the final state.
//! - The ghost is the judgement sequence in LR2's run-length format
//!   (`PLAYSCORE::EncodeGhostData`).
//!
//! The recorder sees the events in the order the engine applies them, so it
//! must be fed every event of an outcome, including the long-note head events
//! that do not affect the score.

use std::collections::HashMap;

use bmz_chart::model::{NoteKind, PlayableChart};
use bmz_core::{
    ids::NoteId,
    judge::{Judge, TimingSide},
    lane::Lane,
};

use crate::{gauge::legacy_judge_damage, judge::model::JudgementEvent};

/// Number of graph samples.
pub const LR2_GRAPH_SAMPLES: usize = 1000;
/// OpenLR2 gauge indices.
pub const LR2_GAUGE_GROOVE: usize = 0;
pub const LR2_GAUGE_HARD: usize = 1;
pub const LR2_GAUGE_DEATH: usize = 2;
pub const LR2_GAUGE_EASY: usize = 3;
pub const LR2_GAUGE_PATTACK: usize = 4;
pub const LR2_GAUGE_GATTACK: usize = 5;

/// `IRScoreV1::JUDGEMENTS`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Lr2Judgements {
    pub epg: u32,
    pub lpg: u32,
    pub egr: u32,
    pub lgr: u32,
    pub egd: u32,
    pub lgd: u32,
    pub ebd: u32,
    pub lbd: u32,
    pub epr: u32,
    pub lpr: u32,
    pub cb: u32,
    pub fast: u32,
    pub slow: u32,
    pub notes_played: u32,
    pub notes_total: u32,
}

impl Lr2Judgements {
    /// The fields in `IRScoreV1` order.
    pub fn values(&self) -> [u32; 15] {
        [
            self.epg,
            self.lpg,
            self.egr,
            self.lgr,
            self.egd,
            self.lgd,
            self.ebd,
            self.lbd,
            self.epr,
            self.lpr,
            self.cb,
            self.fast,
            self.slow,
            self.notes_played,
            self.notes_total,
        ]
    }

    fn scored(&mut self, judge: usize, early: bool) {
        let (e, l) = match judge {
            5 => (&mut self.epg, &mut self.lpg),
            4 => (&mut self.egr, &mut self.lgr),
            3 => (&mut self.egd, &mut self.lgd),
            _ => (&mut self.ebd, &mut self.lbd),
        };
        *if early { e } else { l } += 1;
        if judge == 2 {
            self.cb += 1;
        }
        self.notes_played += 1;
    }

    fn fast_slow(&mut self, early: bool) {
        *if early { &mut self.fast } else { &mut self.slow } += 1;
    }
}

/// `IRScoreV1::GRAPHDATA` (the rate graph is the high score's, not the play's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lr2Graphs {
    pub hp: [[i32; LR2_GRAPH_SAMPLES]; 6],
    pub combo: [i32; LR2_GRAPH_SAMPLES],
    pub exscore: [i32; LR2_GRAPH_SAMPLES],
}

/// The play-derived part of `IRScoreV1` for a single song.
#[derive(Debug, Clone, PartialEq)]
pub struct Lr2Score {
    pub judgements_total: Lr2Judgements,
    pub judgements_column: [Lr2Judgements; 20],
    pub max_combo: u32,
    pub hp: [f64; 6],
    pub gauge_type: usize,
    pub moneyscore: i32,
    pub exscore: i32,
    pub rate: f64,
    /// 1 FAILED, 2 EASY, 3 GROOVE, 4 HARD, 5 FULL COMBO.
    pub clear_type: i32,
    pub notes_total: u32,
    pub song_runtime: f64,
    pub graphs: Box<Lr2Graphs>,
    pub ghost: String,
}

/// OpenLR2 lane (`bmsobj_note` index): P1 0..=7, P2 10..=17.
pub fn lr2_column(lane: Lane) -> usize {
    match lane {
        Lane::Scratch2 => 10,
        lane if lane.index() < 8 => lane.index(),
        lane => 11 + lane.index() - Lane::Key8.index(),
    }
}

/// LR2 judgement index: 0 EMPTY POOR .. 5 PGREAT.
pub fn lr2_judge(judge: Judge) -> usize {
    match judge {
        Judge::EmptyPoor => 0,
        Judge::Poor => 1,
        Judge::Bad => 2,
        Judge::Good => 3,
        Judge::Great => 4,
        Judge::PGreat => 5,
    }
}

pub struct Lr2ScoreRecorder {
    kinds: HashMap<NoteId, NoteKind>,
    head_early: HashMap<NoteId, bool>,
    damage: [[f64; 6]; 6],
    total: Lr2Judgements,
    columns: [Lr2Judgements; 20],
    judgecount: [i32; 6],
    note_current: u32,
    totalnotes: u32,
    now_combo: i32,
    max_combo: i32,
    hp: [f64; 6],
    gauge_type: usize,
    moneyscore: i32,
    exscore: i32,
    rate: f64,
    song_runtime: f64,
    graphs: Box<Lr2Graphs>,
    cursor: usize,
    ghost: Vec<i8>,
}

impl Lr2ScoreRecorder {
    /// `gauge_type` is the selected OpenLR2 gauge, `total` the gauge TOTAL.
    pub fn new(chart: &PlayableChart, gauge_type: usize, total: f64) -> Self {
        let mut kinds = HashMap::new();
        let mut columns = [Lr2Judgements::default(); 20];
        for note in chart.lane_notes.iter().flatten() {
            kinds.insert(note.id, note.kind);
            // `bmsobj_note[lane].count`: notes, long notes and mines.
            if matches!(note.kind, NoteKind::Tap | NoteKind::LongStart | NoteKind::Mine) {
                columns[lr2_column(note.lane)].notes_total += 1;
            }
        }
        let totalnotes = chart.total_notes;
        let hp = [20.0, 100.0, 100.0, 20.0, 100.0, 100.0];
        let mut graphs = Box::new(Lr2Graphs {
            hp: [[0; LR2_GRAPH_SAMPLES]; 6],
            combo: [0; 1000],
            exscore: [0; 1000],
        });
        for (gauge, value) in hp.iter().enumerate() {
            graphs.hp[gauge][0] = *value as i32;
        }
        Self {
            kinds,
            head_early: HashMap::new(),
            damage: legacy_judge_damage(total, totalnotes).unwrap_or_default(),
            total: Lr2Judgements { notes_total: totalnotes, ..Default::default() },
            columns,
            judgecount: [0; 6],
            note_current: 0,
            totalnotes,
            now_combo: 0,
            max_combo: 0,
            hp,
            gauge_type,
            moneyscore: 0,
            exscore: 0,
            rate: 0.0,
            song_runtime: chart.metadata.legacy_song_runtime_ms.unwrap_or(0.0),
            graphs,
            cursor: 0,
            ghost: Vec::new(),
        }
    }

    /// The counts of one OpenLR2 lane so far (`notes_total` is the lane's).
    pub fn column(&self, lane: usize) -> &Lr2Judgements {
        &self.columns[lane]
    }

    /// One judgement event, in the order the engine emitted it.
    pub fn apply_event(&mut self, event: &JudgementEvent) {
        let kind = event.note_id.and_then(|id| self.kinds.get(&id).copied());
        let column = lr2_column(event.lane);
        let early = event.side == TimingSide::Fast;
        let judge = lr2_judge(event.judge);
        if !event.affects_score {
            // A long-note head: ProcLongnote counts GREAT..BAD as FAST/SLOW now
            // and remembers whether the head was early for the end.
            if kind == Some(NoteKind::LongStart) && judge >= 2 {
                if judge <= 4 {
                    self.total.fast_slow(early);
                    self.columns[column].fast_slow(early);
                }
                if let Some(id) = event.note_id {
                    self.head_early.insert(id, early);
                }
            }
            return;
        }
        // A long note whose head was hit is scored at its end with the head's
        // early/late. One judged without a head (the next object of a BAD,
        // which ProcSinglenote judges as a single note) counts as a tap.
        let head_early = event.note_id.and_then(|id| self.head_early.remove(&id));
        for stats in [&mut self.total, &mut self.columns[column]] {
            match judge {
                0 => stats.epr += 1,
                1 => {
                    stats.lpr += 1;
                    stats.cb += 1;
                    stats.notes_played += 1;
                }
                _ if head_early.is_some() => stats.scored(judge, head_early == Some(true)),
                _ => {
                    stats.scored(judge, early);
                    if judge <= 4 {
                        stats.fast_slow(early);
                    }
                }
            }
        }
        self.apply_judge_note(judge);
    }

    /// `ApplyJudgeNote` for player 1 of a single song.
    fn apply_judge_note(&mut self, judge: usize) {
        if judge >= 1 {
            self.ghost.push(judge as i8);
            self.note_current += 1;
        } else {
            self.ghost.push(0);
        }
        self.judgecount[judge.max(1)] += 1;
        for gauge in 0..6 {
            let hp = (self.hp[gauge] as i32 / 2) * 2;
            let survival = !(gauge == LR2_GAUGE_GROOVE || gauge == LR2_GAUGE_EASY);
            let mut damage = self.damage[gauge][judge];
            if hp <= 30 && survival && judge <= 2 {
                damage *= 0.6;
            }
            let value = &mut self.hp[gauge];
            *value += damage;
            if *value <= 2.0 && !survival {
                *value = 2.0;
            }
            *value = value.clamp(0.0, 100.0);
            if hp <= 0 {
                *value = 0.0;
            }
        }
        let jc = &self.judgecount;
        if self.totalnotes > 0 {
            let points = (jc[3] + (jc[4] + jc[5] * 2) * 2) as u64;
            self.moneyscore = (points * 50000 / u64::from(self.totalnotes)) as i32;
        }
        self.exscore = jc[4] + jc[5] * 2;
        if self.note_current > 0 {
            self.rate = f64::from(self.exscore * 100) / f64::from(self.note_current * 2);
        }
        match judge {
            1 | 2 => self.now_combo = 0,
            3..=5 => {
                self.now_combo += 1;
                self.max_combo = self.max_combo.max(self.now_combo);
            }
            _ => {}
        }
    }

    /// `ApplyJudgeMine`: OpenLR2 passes the damage as an `int`.
    pub fn apply_mine(&mut self, damage: f64) {
        let damage = f64::from(damage as i32);
        for (gauge, value) in self.hp.iter_mut().enumerate() {
            *value -= damage;
            if *value <= 2.0 && (gauge == LR2_GAUGE_GROOVE || gauge == LR2_GAUGE_EASY) {
                *value = 2.0;
            }
            *value = value.clamp(0.0, 100.0);
        }
    }

    /// `LogGraphPlayData` after the polls at `ms`. Both arguments are `int` in
    /// OpenLR2, and `time * 1000` wraps like the C multiplication.
    pub fn log_graph(&mut self, ms: i64) {
        let time = ms as i32;
        let endtime = msvc_int(self.song_runtime);
        if self.cursor >= LR2_GRAPH_SAMPLES || time > endtime || endtime <= 0 {
            return;
        }
        while self.cursor < LR2_GRAPH_SAMPLES {
            if (time.wrapping_mul(1000) / endtime) < self.cursor as i32 {
                return;
            }
            self.write_sample(self.cursor);
            self.cursor += 1;
        }
    }

    fn write_sample(&mut self, index: usize) {
        for gauge in 0..6 {
            self.graphs.hp[gauge][index] = self.hp[gauge] as i32;
        }
        self.graphs.combo[index] = self.now_combo;
        self.graphs.exscore[index] = self.exscore;
    }

    /// Ends the play: zeroes the gauges when notes are left, fills the graphs
    /// (`LogGraphPlayerDataToEnd`) and decides the clear (`CheckClear`).
    pub fn finish(mut self) -> Lr2Score {
        if self.totalnotes > self.note_current {
            self.hp = [0.0; 6];
        }
        if self.cursor >= 1 {
            for index in self.cursor..LR2_GRAPH_SAMPLES {
                self.write_sample(index);
            }
        } else {
            // Nothing was logged (song_runtime <= 0): every sample repeats the
            // first one, which holds the starting gauges.
            let graphs = &mut *self.graphs;
            for gauge in 0..6 {
                let first = graphs.hp[gauge][0];
                graphs.hp[gauge].fill(first);
            }
            let (combo, exscore) = (graphs.combo[0], graphs.exscore[0]);
            graphs.combo.fill(combo);
            graphs.exscore.fill(exscore);
        }
        let gauge = self.gauge_type;
        let hp = self.hp[gauge];
        let mut clear_type = 1;
        if self.totalnotes as i32 == self.max_combo {
            clear_type = 5;
        } else if matches!(gauge, LR2_GAUGE_HARD | LR2_GAUGE_GATTACK | LR2_GAUGE_PATTACK) {
            if self.note_current == self.totalnotes && hp >= 2.0 {
                clear_type = 4;
            }
        } else if self.note_current == self.totalnotes && hp >= 80.0 {
            clear_type = if gauge == LR2_GAUGE_EASY { 2 } else { 3 };
        }
        Lr2Score {
            judgements_total: self.total,
            judgements_column: self.columns,
            max_combo: self.max_combo as u32,
            hp: self.hp,
            gauge_type: gauge,
            moneyscore: self.moneyscore,
            exscore: self.exscore,
            rate: self.rate,
            clear_type,
            notes_total: self.totalnotes,
            song_runtime: self.song_runtime,
            graphs: self.graphs,
            ghost: encode_ghost(&self.ghost),
        }
    }
}

/// A C `(int)` conversion of a double as MSVC x64 compiles it (`cvttsd2si`):
/// truncation, and `INT_MIN` for NaN, infinities and values out of range.
/// A chart whose `song_runtime` is infinite therefore logs no graph samples.
fn msvc_int(value: f64) -> i32 {
    if value.is_finite() && value > -2_147_483_649.0 && value < 2_147_483_648.0 {
        value as i32
    } else {
        i32::MIN
    }
}

/// `PLAYSCORE::EncodeGhostData`: run-length letters (`@` EMPTY POOR, `A`..`E`
/// POOR..PGREAT, a count after runs of two or more), then two passes that
/// replace common pairs with single letters. Each pass drops the last
/// character, which is why the `ZZZ` terminator ends as `Z`.
pub fn encode_ghost(queue: &[i8]) -> String {
    let Some(&first) = queue.first() else {
        return String::new();
    };
    let letter = |judge: i8| match judge {
        0 => b'@',
        1 => b'A',
        2 => b'B',
        3 => b'C',
        4 => b'D',
        5 => b'E',
        _ => b'-',
    };
    let mut data = Vec::with_capacity(queue.len() + 3);
    let push_run = |data: &mut Vec<u8>, judge: i8, run: usize| {
        data.push(letter(judge));
        if run >= 2 {
            data.extend_from_slice(run.to_string().as_bytes());
        }
    };
    let mut last = first;
    let mut run = 0;
    for &judge in queue {
        if judge == last {
            run += 1;
        } else {
            push_run(&mut data, last, run);
            last = judge;
            run = 1;
        }
    }
    push_run(&mut data, last, run);
    data.extend_from_slice(b"ZZZ");

    const FIRST: &[(&[u8; 2], u8)] = &[
        (b"E1", b'F'),
        (b"E2", b'G'),
        (b"E3", b'H'),
        (b"E4", b'I'),
        (b"E5", b'J'),
        (b"E6", b'K'),
        (b"E7", b'L'),
        (b"E8", b'M'),
        (b"E9", b'N'),
        (b"EC", b'P'),
        (b"EB", b'Q'),
        (b"EA", b'R'),
        (b"D2", b'S'),
        (b"D3", b'T'),
        (b"D4", b'U'),
        (b"D5", b'V'),
        (b"D6", b'W'),
        (b"DE", b'X'),
        (b"DC", b'Y'),
        (b"DB", b'a'),
        (b"DA", b'b'),
        (b"C2", b'c'),
        (b"C3", b'd'),
        (b"C4", b'e'),
        (b"C5", b'f'),
        (b"CE", b'g'),
        (b"CD", b'h'),
        (b"CB", b'i'),
        (b"CA", b'j'),
        (b"AB", b'k'),
        (b"AC", b'l'),
        (b"AD", b'm'),
        (b"AE", b'n'),
        (b"A2", b'o'),
        (b"A3", b'p'),
    ];
    const SECOND: &[(&[u8; 2], u8)] = &[
        (b"XX", b'q'),
        (b"X1", b'r'),
        (b"X2", b's'),
        (b"X3", b't'),
        (b"X4", b'u'),
        (b"X5", b'v'),
        (b"X6", b'w'),
        (b"X7", b'x'),
        (b"X8", b'y'),
        (b"X9", b'z'),
    ];
    let pass = |data: &[u8], table: &[(&[u8; 2], u8)]| {
        let mut out = Vec::with_capacity(data.len());
        let mut i = 1;
        while i < data.len() {
            let pair = [data[i - 1], data[i]];
            match table.iter().find(|(from, _)| **from == pair) {
                Some(&(_, to)) => {
                    out.push(to);
                    i += 1;
                }
                None => out.push(data[i - 1]),
            }
            i += 1;
        }
        out
    };
    let data = pass(&data, FIRST);
    let data = pass(&data, SECOND);
    String::from_utf8(data).expect("ghost letters are ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ghost_encoding_follows_lr2_pairs() {
        // E E E D D C A @ -> "E3D2CA@ZZZ" -> "HSj@ZZ" -> "HSj@Z": each pass
        // drops the last character.
        assert_eq!(encode_ghost(&[5, 5, 5, 4, 4, 3, 1, 0]), "HSj@Z");
        assert_eq!(encode_ghost(&[]), "");
        // D E D E -> "DEDEZZZ" -> X X Z Z -> q Z.
        assert_eq!(encode_ghost(&[4, 5, 4, 5]), "qZ");
    }
}
