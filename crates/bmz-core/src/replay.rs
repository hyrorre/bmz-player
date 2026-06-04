use serde::{Deserialize, Serialize};

use crate::input::{InputDeviceKind, InputKind};
use crate::lane::Lane;
use crate::time::TimeUs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayEvent {
    pub lane: Lane,
    pub kind: InputKind,
    pub time: TimeUs,
    #[serde(default)]
    pub device_kind: InputDeviceKind,
}
