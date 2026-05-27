pub const SKIN_OFFSET_VALUE_COUNT: usize = 200;
pub const SKIN_OFFSET_BAR_LINE: i32 = 34;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkinOffsetValue {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub r: i32,
    pub a: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkinOffsetValues {
    values: [SkinOffsetValue; SKIN_OFFSET_VALUE_COUNT],
}

impl Default for SkinOffsetValues {
    fn default() -> Self {
        Self { values: [SkinOffsetValue::default(); SKIN_OFFSET_VALUE_COUNT] }
    }
}

impl SkinOffsetValues {
    pub fn get(&self, id: i32) -> Option<SkinOffsetValue> {
        self.values.get(offset_index(id)?).copied()
    }

    pub fn set(&mut self, id: i32, value: SkinOffsetValue) -> bool {
        let Some(index) = offset_index(id) else {
            return false;
        };
        self.values[index] = value;
        true
    }
}

fn offset_index(id: i32) -> Option<usize> {
    let index = usize::try_from(id).ok()?;
    (index < SKIN_OFFSET_VALUE_COUNT).then_some(index)
}
