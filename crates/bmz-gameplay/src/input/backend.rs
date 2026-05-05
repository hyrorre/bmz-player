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

pub trait InputEventSink {
    fn push_event(&mut self, event: DeviceInputEvent);

    fn extend_events(&mut self, events: impl IntoIterator<Item = DeviceInputEvent>) {
        for event in events {
            self.push_event(event);
        }
    }
}

#[derive(Debug, Default)]
pub struct NullInputBackend;

impl InputBackend for NullInputBackend {
    fn drain_events(&mut self) -> Vec<DeviceInputEvent> {
        Vec::new()
    }
}

#[derive(Debug, Default)]
pub struct BufferedInputBackend {
    events: Vec<DeviceInputEvent>,
}

impl BufferedInputBackend {
    pub fn push(&mut self, event: DeviceInputEvent) {
        self.push_event(event);
    }

    pub fn extend(&mut self, events: impl IntoIterator<Item = DeviceInputEvent>) {
        self.extend_events(events);
    }
}

impl InputBackend for BufferedInputBackend {
    fn drain_events(&mut self) -> Vec<DeviceInputEvent> {
        std::mem::take(&mut self.events)
    }
}

impl InputEventSink for BufferedInputBackend {
    fn push_event(&mut self, event: DeviceInputEvent) {
        self.events.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffered_backend_drains_queued_events() {
        let mut backend = BufferedInputBackend::default();
        backend.push_event(DeviceInputEvent {
            device: DeviceId(1),
            control: PhysicalControl::KeyboardKey("Z".to_string()),
            kind: InputKind::Press,
            timestamp: DeviceTimestamp::Unknown,
        });

        assert_eq!(backend.drain_events().len(), 1);
        assert!(backend.drain_events().is_empty());
    }

    #[test]
    fn input_event_sink_extends_buffered_backend() {
        let mut backend = BufferedInputBackend::default();

        backend.extend_events([
            DeviceInputEvent {
                device: DeviceId(1),
                control: PhysicalControl::KeyboardKey("Z".to_string()),
                kind: InputKind::Press,
                timestamp: DeviceTimestamp::Unknown,
            },
            DeviceInputEvent {
                device: DeviceId(1),
                control: PhysicalControl::KeyboardKey("Z".to_string()),
                kind: InputKind::Release,
                timestamp: DeviceTimestamp::Unknown,
            },
        ]);

        assert_eq!(backend.drain_events().len(), 2);
    }
}
