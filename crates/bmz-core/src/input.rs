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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputDeviceKind {
    #[default]
    Keyboard,
    Controller,
}

impl InputDeviceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Keyboard => "keyboard",
            Self::Controller => "controller",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputEvent {
    pub lane: Lane,
    pub kind: InputKind,
    pub time: TimeUs,
    pub source: InputSource,
    #[serde(default)]
    pub device_kind: InputDeviceKind,
}
