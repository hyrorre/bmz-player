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
