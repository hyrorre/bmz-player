use serde::{Deserialize, Serialize};

use crate::lane::Lane;
use crate::time::TimeUs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputKind {
    Press,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputSource {
    Human,
    Auto,
    Replay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputEvent {
    pub lane: Lane,
    pub kind: InputKind,
    pub time: TimeUs,
    pub source: InputSource,
}
