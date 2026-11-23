use serde::{Deserialize, Serialize};

use crate::input::{InputDeviceKind, InputKind, ScratchDirection};
use crate::lane::Lane;
use crate::time::TimeUs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchDecision {
    pub block: usize,
    pub branch: usize,
    pub time: TimeUs,
}

/// The observed state and the selected path are separate evidence. A stored path
/// can reproduce a play even when verification against the state fails.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchEvaluation {
    pub decision: BranchDecision,
    /// Position in the scoring timeline when the audio reached this decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_position: Option<ReplayScorePosition>,
    pub values: std::collections::BTreeMap<String, f64>,
    pub calculated_branch: usize,
    pub from_replay: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayScorePosition {
    pub time: TimeUs,
    /// Two inputs can be delivered on opposite sides of a decision at the same time.
    pub input_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayEvent {
    /// Inclusive scoring boundary already processed when a late input arrived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_after: Option<TimeUs>,
    pub lane: Lane,
    pub kind: InputKind,
    pub time: TimeUs,
    #[serde(default)]
    pub device_kind: InputDeviceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratch_direction: Option<ScratchDirection>,
}

impl ReplayEvent {
    pub fn delivery_time(&self) -> TimeUs {
        self.applied_after.unwrap_or(self.time).max(self.time)
    }
}
