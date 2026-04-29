use bmz_core::input::InputKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PhysicalControl {
    KeyboardKey(String),
    GamepadButton(String),
    HidButton(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceTimestamp {
    Unknown,
    MonotonicNs(u128),
    BackendTicks(u64),
}

#[derive(Debug, Clone)]
pub struct DeviceInputEvent {
    pub device: DeviceId,
    pub control: PhysicalControl,
    pub kind: InputKind,
    pub timestamp: DeviceTimestamp,
}

pub trait InputBackend {
    fn update(&mut self) {}
    fn drain_events(&mut self) -> Vec<DeviceInputEvent>;
}
