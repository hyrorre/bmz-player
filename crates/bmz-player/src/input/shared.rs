use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use bmz_core::input::InputKind;
use bmz_gameplay::input::backend::{
    DeviceId, DeviceInputEvent, DeviceTimestamp, InputBackend, InputEventSink, PhysicalControl,
    monotonic_timestamp_ns,
};

type ControlKey = (DeviceId, PhysicalControl);
#[derive(Debug, Default)]
struct InputBuffer {
    events: VecDeque<(DeviceInputEvent, Option<std::time::Instant>)>,
    delivered: HashMap<ControlKey, DeviceInputEvent>,
    blocked: HashSet<ControlKey>,
}

#[derive(Debug, Clone)]
pub struct SharedInputBackend {
    generation: u64,
    buffer: Arc<Mutex<InputBuffer>>,
    delivery: Arc<bmz_core::latency::AtomicLatencyHistogram>,
    waker: Arc<Mutex<Option<std::thread::Thread>>>,
    overflow: Arc<AtomicU64>,
}

impl Default for SharedInputBackend {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            generation: NEXT.fetch_add(1, Ordering::Relaxed),
            buffer: Default::default(),
            delivery: Default::default(),
            waker: Default::default(),
            overflow: Default::default(),
        }
    }
}

impl SharedInputBackend {
    pub fn generation(&self) -> u64 {
        self.generation
    }
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
            let key = (event.device, event.control.clone());
            if buffer.blocked.contains(&key) {
                if event.kind == InputKind::Release {
                    buffer.blocked.remove(&key);
                }
            } else if buffer.events.len() == Self::CAPACITY {
                self.overflow.fetch_add(1, Ordering::Relaxed);
                // History is lost. Release everything gameplay actually saw,
                // discard the incomplete history and inhibit holds until key-up.
                // This recovery is independent of the OS SYN_DROPPED mechanism.
                // Keep delivered state until gameplay drains the releases, so
                // another overflow cannot discard a pending recovery release.
                let delivered = buffer.delivered.clone();
                let queued = std::mem::take(&mut buffer.events);
                let mut held: HashSet<_> = delivered.keys().cloned().collect();
                for pending in queued.into_iter().map(|(event, _)| event).chain([event]) {
                    let key = (pending.device, pending.control);
                    match pending.kind {
                        InputKind::Press => {
                            held.insert(key);
                        }
                        InputKind::Release => {
                            held.remove(&key);
                        }
                    }
                }
                // Earlier blocked holds survive synthesized releases in queued;
                // only a real incoming key-up clears them above.
                buffer.blocked.extend(held);
                for (_, mut release) in delivered {
                    release.kind = InputKind::Release;
                    release.timestamp = DeviceTimestamp::MonotonicNs(monotonic_timestamp_ns());
                    buffer.events.push_back((release, enqueued));
                }
            } else {
                buffer.events.push_back((event, enqueued));
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
                let events: Vec<_> = buffer
                    .events
                    .drain(..)
                    .map(|(event, enqueued)| {
                        if let Some(enqueued) = enqueued {
                            self.delivery
                                .record(enqueued.elapsed().as_nanos().min(u64::MAX as u128) as u64);
                        }
                        event
                    })
                    .collect();
                for event in &events {
                    let key = (event.device, event.control.clone());
                    match event.kind {
                        InputKind::Press => {
                            buffer.delivered.insert(key, event.clone());
                        }
                        InputKind::Release => {
                            buffer.delivered.remove(&key);
                        }
                    }
                }
                events
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

    fn button_event(button: u32, kind: InputKind) -> DeviceInputEvent {
        DeviceInputEvent {
            device: DeviceId(0),
            control: PhysicalControl::HidButton(button),
            kind,
            timestamp: DeviceTimestamp::MonotonicNs(123),
            bounce_policy: Default::default(),
        }
    }

    #[test]
    fn overflow_preserves_fresh_presses_after_queued_or_triggering_releases() {
        for delivered in [false, true] {
            for triggering_release in [false, true] {
                let mut input = SharedInputBackend::default();
                input.push_shared_event(button_event(1, InputKind::Press));
                if delivered {
                    assert_eq!(input.drain_events().len(), 1);
                }
                if !triggering_release {
                    input.push_shared_event(button_event(1, InputKind::Release));
                }
                let queued = usize::from(!delivered) + usize::from(!triggering_release);
                for _ in queued..SharedInputBackend::CAPACITY {
                    input.push_shared_event(button_event(2, InputKind::Press));
                }
                input.push_shared_event(if triggering_release {
                    button_event(1, InputKind::Release)
                } else {
                    button_event(2, InputKind::Press)
                });
                assert_eq!(input.overflow_count(), 1);
                let releases = input.drain_events();
                assert_eq!(releases.len(), usize::from(delivered));
                if delivered {
                    assert_eq!(releases[0].kind, InputKind::Release);
                    assert_eq!(releases[0].control, PhysicalControl::HidButton(1));
                }
                input.push_shared_event(button_event(1, InputKind::Press));
                let fresh = input.drain_events();
                assert_eq!(fresh.len(), 1, "delivered={delivered}, trigger={triggering_release}");
                assert_eq!(fresh[0].kind, InputKind::Press);
                assert_eq!(fresh[0].timestamp, DeviceTimestamp::MonotonicNs(123));
            }
        }
    }

    #[test]
    fn overflow_inhibits_a_repress_after_a_queued_release_until_its_own_release() {
        let mut input = SharedInputBackend::default();
        input.push_shared_event(button_event(1, InputKind::Press));
        input.drain_events();
        input.push_shared_event(button_event(1, InputKind::Release));
        input.push_shared_event(button_event(1, InputKind::Press));
        for _ in 2..=SharedInputBackend::CAPACITY {
            input.push_shared_event(button_event(2, InputKind::Press));
        }
        assert_eq!(input.drain_events()[0].kind, InputKind::Release);
        input.push_shared_event(button_event(1, InputKind::Press));
        assert!(input.drain_events().is_empty());
        input.push_shared_event(button_event(1, InputKind::Release));
        input.push_shared_event(button_event(1, InputKind::Press));
        assert_eq!(input.drain_events()[0].kind, InputKind::Press);
    }

    #[test]
    fn repeated_overflow_keeps_pending_recovery_releases_and_blocks_real_holds() {
        let mut input = SharedInputBackend::default();
        input.push_shared_event(button_event(1, InputKind::Press));
        input.drain_events();
        for _ in 0..=SharedInputBackend::CAPACITY {
            input.push_shared_event(button_event(2, InputKind::Press));
        }
        // Overflow again before gameplay has drained the synthesized release.
        for _ in 1..=SharedInputBackend::CAPACITY {
            input.push_shared_event(button_event(3, InputKind::Press));
        }
        assert_eq!(input.overflow_count(), 2);
        let releases = input.drain_events();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].kind, InputKind::Release);
        assert_eq!(releases[0].control, PhysicalControl::HidButton(1));
        input.push_shared_event(button_event(1, InputKind::Press));
        assert!(input.drain_events().is_empty());
        input.push_shared_event(button_event(1, InputKind::Release));
        input.push_shared_event(button_event(1, InputKind::Press));
        assert_eq!(input.drain_events()[0].kind, InputKind::Press);
    }

    #[test]
    fn overflow_releases_delivered_holds_and_inhibits_missing_history_until_release() {
        let mut input = SharedInputBackend::default();
        let event = |kind| DeviceInputEvent {
            device: DeviceId(0),
            control: PhysicalControl::KeyboardKey("Z".into()),
            kind,
            timestamp: DeviceTimestamp::MonotonicNs(123),
            bounce_policy: Default::default(),
        };
        input.push_shared_event(event(InputKind::Press));
        assert_eq!(input.drain_events()[0].kind, InputKind::Press);
        for _ in 0..=SharedInputBackend::CAPACITY {
            input.push_shared_event(event(InputKind::Press));
        }
        assert_eq!(input.overflow_count(), 1);
        let releases = input.drain_events();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].kind, InputKind::Release);
        input.push_shared_event(event(InputKind::Press));
        assert!(input.drain_events().is_empty());
        input.push_shared_event(event(InputKind::Release));
        input.push_shared_event(event(InputKind::Press));
        assert_eq!(input.drain_events()[0].timestamp, DeviceTimestamp::MonotonicNs(123));
    }

    #[test]
    fn diagnostic_receipt_time_does_not_replace_judgement_timestamp() {
        let mut input = SharedInputBackend::default();
        input.buffer.lock().unwrap().events.push_back((
            DeviceInputEvent {
                device: DeviceId(0),
                control: PhysicalControl::KeyboardKey("Z".into()),
                kind: InputKind::Press,
                timestamp: DeviceTimestamp::MonotonicNs(123),
                bounce_policy: Default::default(),
            },
            Some(std::time::Instant::now() - std::time::Duration::from_millis(10)),
        ));
        assert_eq!(input.drain_events()[0].timestamp, DeviceTimestamp::MonotonicNs(123));
        assert_eq!(input.delivery_summary().count, 1);
        assert!(input.delivery_summary().max >= 10_000_000);
    }

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
        // The incomplete history was discarded; no fabricated fresh presses.
        assert!(old.clone().drain_events().is_empty());
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
