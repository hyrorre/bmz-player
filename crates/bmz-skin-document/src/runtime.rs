//! `SkinDocument` の `#[serde(skip)]` ランタイムフィールドが参照する plain data 型。
//!
//! プレイ/リザルト描画時に plan 側が計算して document へ差し込む graph 値で、
//! 描画依存を持たないため document crate に置く。`bmz-render` の
//! `chart_graph` / `snapshot` モジュールからは re-export でパスを維持する。

use bmz_core::judge::Judge;

#[derive(Debug, Clone, PartialEq)]
pub struct BpmGraphSegment {
    pub start_ratio: f32,
    pub end_ratio: f32,
    pub bpm: f32,
    pub is_stop: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ResultGaugeGraphPoint {
    pub time_ms: i32,
    pub value: f32,
    pub max: f32,
    pub border: f32,
    pub gauge_type: i32,
    /// Course result で、この点から次の曲の履歴が始まることを示す。
    pub course_section_start: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultTimingPoint {
    pub time_ms: i32,
    /// beatoraja `Note.getPlayTime()` 相当。正が FAST/EARLY、負が SLOW/LATE。
    pub delta_us: i64,
    pub judge: Judge,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResultJudgeGraphBucket {
    /// beatoraja `SkinNoteDistributionGraph.TYPE_JUDGE` の state 0..5。
    /// 0=unjudged, 1=PG, 2=GR, 3=GD, 4=BD, 5=PR/MS。
    pub values: [u32; 6],
}

impl ResultJudgeGraphBucket {
    pub fn total(self) -> u32 {
        self.values.iter().copied().sum()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResultNoteGraphBucket {
    /// beatoraja `SkinNoteDistributionGraph.TYPE_NORMAL` の state 0..6。
    /// 0/3=LN終点、1/4=LN本体、2=Scratch、5=通常ノート、6=Mine。
    pub values: [u32; 7],
}

impl ResultNoteGraphBucket {
    pub fn total(self) -> u32 {
        self.values.iter().copied().sum()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResultEarlyLateGraphBucket {
    /// beatoraja `SkinNoteDistributionGraph.TYPE_EARLYLATE` の state 0..9。
    /// 0=unjudged, 1=PG, 2..5=FAST/EARLY GR..PR, 6..9=SLOW/LATE GR..PR。
    pub values: [u32; 10],
}

impl ResultEarlyLateGraphBucket {
    pub fn total(self) -> u32 {
        self.values.iter().copied().sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultTimingDistribution {
    pub range_ms: i32,
    pub counts: Vec<u32>,
}

impl Default for ResultTimingDistribution {
    fn default() -> Self {
        Self::new(150)
    }
}

impl ResultTimingDistribution {
    pub fn new(range_ms: i32) -> Self {
        let range_ms = range_ms.max(1);
        Self { range_ms, counts: vec![0; (range_ms * 2 + 1) as usize] }
    }

    pub fn add(&mut self, timing_ms: i32) {
        if (-self.range_ms..=self.range_ms).contains(&timing_ms) {
            let index = (timing_ms + self.range_ms) as usize;
            if let Some(count) = self.counts.get_mut(index) {
                *count = count.saturating_add(1);
            }
        }
    }

    pub fn total(&self) -> u32 {
        self.counts.iter().copied().sum()
    }

    pub fn stats(&self) -> Option<(f32, f32)> {
        let count = self.total();
        if count == 0 {
            return None;
        }
        let count_f = count as f32;
        let average_ms = self
            .counts
            .iter()
            .enumerate()
            .map(|(index, count)| (index as i32 - self.range_ms) as f32 * *count as f32)
            .sum::<f32>()
            / count_f;
        let variance = self
            .counts
            .iter()
            .enumerate()
            .map(|(index, count)| {
                let diff = (index as i32 - self.range_ms) as f32 - average_ms;
                diff * diff * *count as f32
            })
            .sum::<f32>()
            / count_f;
        Some((average_ms, variance.sqrt()))
    }
}

/// Side effect requested by Lua while running an `act` or a `customEvents` action.
///
/// The Lua runtime (`bmz-skin`) produces these and the app applies them after the
/// frame, so the skin VM never touches audio devices or settings directly.
#[derive(Debug, Clone, PartialEq)]
pub enum SkinLuaEffect {
    /// A `customEvents` action given as a built-in event ID instead of a function.
    Event { id: i32, arg1: i32, arg2: i32 },
    /// `main_state.audio_play` / `audio_loop` / `audio_stop`. The path is relative
    /// to the skin entry directory, like declarative skin audio.
    Audio(crate::SkinAudioActionDef),
    /// `main_state.set_volume_sys` / `set_volume_key` / `set_volume_bg`.
    SetVolume { channel: SkinVolumeChannel, value: f32 },
}

/// beatoraja's three configurable volumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinVolumeChannel {
    System,
    Key,
    Bgm,
}
