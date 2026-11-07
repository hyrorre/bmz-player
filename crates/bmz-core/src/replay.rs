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
    pub values: std::collections::BTreeMap<String, f64>,
    pub calculated_branch: usize,
    pub from_replay: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayEvent {
    pub lane: Lane,
    pub kind: InputKind,
    pub time: TimeUs,
    #[serde(default)]
    pub device_kind: InputDeviceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratch_direction: Option<ScratchDirection>,
}
