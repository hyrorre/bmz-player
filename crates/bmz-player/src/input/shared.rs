use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use bmz_gameplay::input::backend::{DeviceInputEvent, InputBackend, InputEventSink};

#[derive(Debug, Clone, Default)]
pub struct SharedInputBackend {
    buffer: Arc<Mutex<VecDeque<(DeviceInputEvent, Option<std::time::Instant>)>>>,
    delivery: Arc<bmz_core::latency::AtomicLatencyHistogram>,
    waker: Arc<Mutex<Option<std::thread::Thread>>>,
    overflow: Arc<AtomicU64>,
}

impl SharedInputBackend {
    pub const CAPACITY: usize = 16_384;
    pub fn same_source(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.buffer, &other.buffer)
    }
    pub fn overflow_count(&self) -> u64 {
        self.overflow.load(Ordering::Relaxed)
    }
    pub fn push_shared_event(&self, event: DeviceInputEvent) {
        let enqueued = bmz_core::latency::diagnostics_enabled().then(std::time::Instant::now);
        if let Ok(mut buffer) = self.buffer.lock() {
            if buffer.len() == Self::CAPACITY {
                if self.overflow.fetch_add(1, Ordering::Relaxed) == 0 {
                    tracing::error!(
                        capacity = Self::CAPACITY,
                        "timestamped gameplay input queue overflow"
                    );
                }
            } else {
                buffer.push_back((event, enqueued));
            }
        }
        if let Ok(waker) = self.waker.lock()
            && let Some(waker) = &*waker
        {
            waker.unpark();
        }
    }
    pub fn delivery_summary(&self) -> bmz_core::latency::DistributionSummary {
        self.delivery.summary()
    }
}

impl InputBackend for SharedInputBackend {
    fn set_waker(&mut self, waker: Option<std::thread::Thread>) {
        if let Ok(mut target) = self.waker.lock() {
            *target = waker;
        }
    }
    fn drain_events(&mut self) -> Vec<DeviceInputEvent> {
        self.buffer
            .lock()
            .map(|mut buffer| {
                buffer
                    .drain(..)
                    .map(|(event, enqueued)| {
                        if let Some(enqueued) = enqueued {
                            self.delivery
                                .record(enqueued.elapsed().as_nanos().min(u64::MAX as u128) as u64);
                        }
                        event
                    })
                    .collect()
            })
            .unwrap_or_default()
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
    fn input_generations_have_independent_bounded_queues() {
        let old = SharedInputBackend::default();
        let mut current = SharedInputBackend::default();
        assert!(!old.same_source(&current));
        for _ in 0..=SharedInputBackend::CAPACITY {
            old.push_shared_event(DeviceInputEvent {
                device: DeviceId(0),
                control: PhysicalControl::HidButton(1),
                kind: InputKind::Press,
                timestamp: DeviceTimestamp::MonotonicNs(123),
                bounce_policy: Default::default(),
            });
        }
        assert_eq!(old.overflow_count(), 1);
        assert!(current.drain_events().is_empty());
        assert_eq!(old.clone().drain_events().len(), SharedInputBackend::CAPACITY);
    }

    #[test]
    fn cloned_shared_input_backend_drains_events_once() {
        let event_source = SharedInputBackend::default();
        let mut game_backend = event_source.clone();

        event_source.push_shared_event(DeviceInputEvent {
            device: DeviceId(0),
            control: PhysicalControl::KeyboardKey("Z".to_string()),
            kind: InputKind::Press,
            timestamp: DeviceTimestamp::Unknown,
            bounce_policy: Default::default(),
        });

        let events = game_backend.drain_events();
        assert_eq!(events.len(), 1);
        assert!(game_backend.drain_events().is_empty());
    }
}
