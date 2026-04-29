use serde::{Deserialize, Serialize};

pub const LANE_COUNT: usize = 8;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lane {
    Scratch = 0,
    Key1 = 1,
    Key2 = 2,
    Key3 = 3,
    Key4 = 4,
    Key5 = 5,
    Key6 = 6,
    Key7 = 7,
}

impl Lane {
    pub const ALL: [Lane; LANE_COUNT] = [
        Lane::Scratch,
        Lane::Key1,
        Lane::Key2,
        Lane::Key3,
        Lane::Key4,
        Lane::Key5,
        Lane::Key6,
        Lane::Key7,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }
}
