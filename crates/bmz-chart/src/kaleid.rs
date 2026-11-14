//! Source provenance is independent of the filename and of the selected branch.
use serde::{Deserialize, Serialize};

use crate::model::{LongNoteMode, LongNoteStarts, NoteKind, PlayableChart};

pub const INTERPRETATION_VERSION: u32 = 1;

/// Per-play evidence is retained without adding a ranking or score-key dimension.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KaleidPlayEvidence {
    pub provenance: KaleidProvenance,
    pub reference_total: u32,
    pub actual_total: u32,
    pub preplay_lamp: u8,
    pub branches: Vec<bmz_core::replay::BranchDecision>,
    pub evaluations: Vec<bmz_core::replay::BranchEvaluation>,
    pub ended_at: bmz_core::time::TimeUs,
    pub manually_stopped: bool,
    pub options: std::collections::BTreeMap<String, serde_json::Value>,
}

pub fn features_for_chart(chart: &PlayableChart) -> KaleidFeatures {
    features_for_chart_with_starts(chart, &chart.long_note_starts())
}

pub(crate) fn features_for_chart_with_starts(
    chart: &PlayableChart,
    starts: &LongNoteStarts,
) -> KaleidFeatures {
    KaleidFeatures {
        hln: chart.long_notes.iter().any(|p| {
            p.mode.unwrap_or(chart.metadata.long_note_mode) == LongNoteMode::Hln
                && !starts.is_dummy(p)
        }),
        typed_end: chart.metadata.kaleid.source.typed_end,
        conditional: chart.metadata.conditional.is_some(),
        free_zone: !chart.metadata.free_zones.is_empty(),
        potion: chart.lane_notes.iter().flatten().any(|n| n.kind == NoteKind::Potion),
        dummy: chart.lane_notes.iter().flatten().any(|n| n.kind.is_dummy()),
        bar_line: chart.metadata.kaleid.source.bar_line,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KaleidFeatures {
    pub hln: bool,
    pub typed_end: bool,
    pub conditional: bool,
    pub free_zone: bool,
    pub potion: bool,
    pub dummy: bool,
    pub bar_line: bool,
}

impl KaleidFeatures {
    pub fn any(self) -> bool {
        self.hln
            || self.typed_end
            || self.conditional
            || self.free_zone
            || self.potion
            || self.dummy
            || self.bar_line
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            hln: self.hln || other.hln,
            typed_end: self.typed_end || other.typed_end,
            conditional: self.conditional || other.conditional,
            free_zone: self.free_zone || other.free_zone,
            potion: self.potion || other.potion,
            dummy: self.dummy || other.dummy,
            bar_line: self.bar_line || other.bar_line,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KaleidProvenance {
    /// Zero denotes a chart imported before feature detection was available.
    pub version: u32,
    pub declared: KaleidFeatures,
    /// Objects remaining after static RANDOM, including all possible dynamic branches.
    pub source: KaleidFeatures,
    /// Selected, transformed objects in this materialized chart.
    pub effective: KaleidFeatures,
    /// Diagnostics describe partial interpretation; they do not disable local persistence.
    pub diagnostics: Vec<String>,
}

impl KaleidProvenance {
    pub fn imported() -> Self {
        Self { version: INTERPRETATION_VERSION, ..Self::default() }
    }

    pub fn partial(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}
