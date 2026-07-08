use std::sync::{Arc, Mutex};

use bmz_gameplay::input::backend::{
    BufferedInputBackend, DeviceInputEvent, InputBackend, InputEventSink,
};

#[derive(Debug, Clone, Default)]
pub struct SharedInputBackend {
    buffer: Arc<Mutex<BufferedInputBackend>>,
}

impl SharedInputBackend {
    pub fn push_shared_event(&self, event: DeviceInputEvent) {
        if let Ok(mut buffer) = self.buffer.lock() {
            buffer.push_event(event);
        }
    }
}

impl InputBackend for SharedInputBackend {
    fn drain_events(&mut self) -> Vec<DeviceInputEvent> {
        self.buffer.lock().map(|mut buffer| buffer.drain_events()).unwrap_or_default()
    }
}

impl InputEventSink for SharedInputBackend {
    fn push_event(&mut self, event: DeviceInputEvent) {
        self.push_shared_event(event);
    }
}

#[cfg(test)]
mod tests {
    use bmz_core::input::InputKind;
    use bmz_gameplay::input::backend::{DeviceId, DeviceTimestamp, PhysicalControl};

    use super::*;

    #[test]
    fn cloned_shared_input_backend_drains_events_once() {
        let event_source = SharedInputBackend::default();
        let mut game_backend = event_source.clone();

        event_source.push_shared_event(DeviceInputEvent {
            device: DeviceId(0),
            control: PhysicalControl::KeyboardKey("Z".to_string()),
            kind: InputKind::Press,
            timestamp: DeviceTimestamp::Unknown,
        });

        let events = game_backend.drain_events();
        assert_eq!(events.len(), 1);
        assert!(game_backend.drain_events().is_empty());
    }
}
