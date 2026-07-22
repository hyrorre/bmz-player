use std::cell::Cell;

use bmz_core::input::InputEvent;

use super::backend::{DeviceInputEvent, DeviceTimestamp, InputBackend};
use super::bounce::InputBounceFilter;
use super::translator::{InputTimingContext, InputTranslator};

thread_local! {
    static INPUT_COLLECTION_SEQUENCE: Cell<u64> = const { Cell::new(0) };
    static LAST_INPUT_COLLECTION_DIAGNOSTICS: Cell<InputCollectionDiagnostics> =
        const { Cell::new(InputCollectionDiagnostics::empty()) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputCollectionDiagnostics {
    pub sequence: u64,
    pub drained_events: usize,
    pub translated_events: usize,
    pub dropped_events: usize,
    pub timestamped_events: usize,
    pub min_event_age_us: Option<u64>,
    pub max_event_age_us: Option<u64>,
    pub max_future_event_us: Option<u64>,
}

impl InputCollectionDiagnostics {
    pub const fn empty() -> Self {
        Self {
            sequence: 0,
            drained_events: 0,
            translated_events: 0,
            dropped_events: 0,
            timestamped_events: 0,
            min_event_age_us: None,
            max_event_age_us: None,
            max_future_event_us: None,
        }
    }
}

pub struct InputSystem {
    pub backend: Box<dyn InputBackend>,
    pub translator: Box<dyn InputTranslator>,
    pub bounce_filter: InputBounceFilter,
}

impl InputSystem {
    pub fn collect_game_inputs(&mut self, ctx: &InputTimingContext<'_>) -> Vec<InputEvent> {
        self.backend.update();
        let events = self.backend.drain_events();
        let mut diagnostics = input_collection_diagnostics(&events, ctx);
        let events = events
            .into_iter()
            .filter_map(|event| self.bounce_filter.accept(event))
            .collect::<Vec<_>>();
        let inputs = events
            .into_iter()
            .filter_map(|event| self.translator.translate(event, ctx))
            .collect::<Vec<_>>();
        diagnostics.translated_events = inputs.len();
        diagnostics.dropped_events =
            diagnostics.drained_events.saturating_sub(diagnostics.translated_events);
        publish_input_collection_diagnostics(diagnostics);
        inputs
    }
}

pub fn last_input_collection_diagnostics() -> InputCollectionDiagnostics {
    LAST_INPUT_COLLECTION_DIAGNOSTICS.with(Cell::get)
}

fn publish_input_collection_diagnostics(mut diagnostics: InputCollectionDiagnostics) {
    diagnostics.sequence = INPUT_COLLECTION_SEQUENCE.with(|sequence| {
        let next = sequence.get().wrapping_add(1).max(1);
        sequence.set(next);
        next
    });
    LAST_INPUT_COLLECTION_DIAGNOSTICS.with(|last| last.set(diagnostics));
}

fn input_collection_diagnostics(
    events: &[DeviceInputEvent],
    ctx: &InputTimingContext<'_>,
) -> InputCollectionDiagnostics {
    let mut diagnostics = InputCollectionDiagnostics {
        drained_events: events.len(),
        ..InputCollectionDiagnostics::empty()
    };
    let Some(anchor) = ctx.timestamp_anchor else {
        return diagnostics;
    };
    for event in events {
        let DeviceTimestamp::MonotonicNs(event_ns) = event.timestamp else {
            continue;
        };
        diagnostics.timestamped_events += 1;
        if event_ns <= anchor.monotonic_ns {
            let age_us = u128_to_u64_saturating((anchor.monotonic_ns - event_ns) / 1_000);
            update_min(&mut diagnostics.min_event_age_us, age_us);
            update_max(&mut diagnostics.max_event_age_us, age_us);
        } else {
            let future_us = u128_to_u64_saturating((event_ns - anchor.monotonic_ns) / 1_000);
            update_max(&mut diagnostics.max_future_event_us, future_us);
        }
    }
    diagnostics
}

fn update_min(target: &mut Option<u64>, value: u64) {
    *target = Some(target.map_or(value, |current| current.min(value)));
}

fn update_max(target: &mut Option<u64>, value: u64) {
    *target = Some(target.map_or(value, |current| current.max(value)));
}

fn u128_to_u64_saturating(value: u128) -> u64 {
    value.min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    use bmz_audio::clock::AudioClock;
    use bmz_core::input::InputKind;
    use bmz_core::lane::Lane;

    use super::super::backend::{DeviceId, PhysicalControl};
    use super::super::binding::BindingEntry;
    use super::super::translator::InputTimestampAnchor;
    use super::*;
    use crate::session::PlayOffsets;

    fn test_clock() -> AudioClock {
        AudioClock {
            sample_rate: 48_000,
            start_output_frame: 0,
            chart_zero_time_us: 0,
            current_frame: Arc::new(AtomicU64::new(0)),
            running: false,
        }
    }

    fn test_ctx<'a>(clock: &'a AudioClock) -> InputTimingContext<'a> {
        InputTimingContext {
            audio_clock: clock,
            offsets: PlayOffsets { input_offset_us: 0, visual_offset_us: 0 },
            timestamp_anchor: Some(InputTimestampAnchor {
                monotonic_ns: 2_000_000,
                audio_time: bmz_core::time::TimeUs(0),
            }),
        }
    }

    fn input_event(ns: u128) -> DeviceInputEvent {
        DeviceInputEvent {
            device: DeviceId(1),
            control: PhysicalControl::KeyboardKey("Z".to_string()),
            kind: InputKind::Press,
            timestamp: DeviceTimestamp::MonotonicNs(ns),
        }
    }

    #[test]
    fn collection_diagnostics_measure_event_age() {
        let clock = test_clock();
        let ctx = test_ctx(&clock);
        let diagnostics = input_collection_diagnostics(
            &[
                input_event(1_500_000),
                input_event(1_750_000),
                DeviceInputEvent { timestamp: DeviceTimestamp::Unknown, ..input_event(1_900_000) },
            ],
            &ctx,
        );

        assert_eq!(diagnostics.drained_events, 3);
        assert_eq!(diagnostics.timestamped_events, 2);
        assert_eq!(diagnostics.min_event_age_us, Some(250));
        assert_eq!(diagnostics.max_event_age_us, Some(500));
        assert_eq!(diagnostics.max_future_event_us, None);
    }

    #[test]
    fn collection_diagnostics_measure_future_events() {
        let clock = test_clock();
        let ctx = test_ctx(&clock);
        let diagnostics = input_collection_diagnostics(&[input_event(2_400_000)], &ctx);

        assert_eq!(diagnostics.timestamped_events, 1);
        assert_eq!(diagnostics.min_event_age_us, None);
        assert_eq!(diagnostics.max_event_age_us, None);
        assert_eq!(diagnostics.max_future_event_us, Some(400));
    }

    #[test]
    fn collect_game_inputs_publishes_queue_diagnostics() {
        let clock = test_clock();
        let ctx = test_ctx(&clock);
        let mut backend = super::super::backend::BufferedInputBackend::default();
        backend.extend([
            input_event(1_500_000),
            DeviceInputEvent {
                control: PhysicalControl::KeyboardKey("unmapped".to_string()),
                ..input_event(1_750_000)
            },
        ]);
        let mut system = InputSystem {
            backend: Box::new(backend),
            translator: Box::new(super::super::translator::DefaultInputTranslator {
                binding: super::super::binding::LaneBinding {
                    entries: vec![BindingEntry {
                        device: None,
                        control: PhysicalControl::KeyboardKey("Z".to_string()),
                        lane: Lane::Key1,
                        scratch_direction: None,
                    }],
                },
            }),
            bounce_filter: Default::default(),
        };

        let inputs = system.collect_game_inputs(&ctx);
        let diagnostics = last_input_collection_diagnostics();

        assert_eq!(inputs.len(), 1);
        assert_ne!(diagnostics.sequence, 0);
        assert_eq!(diagnostics.drained_events, 2);
        assert_eq!(diagnostics.translated_events, 1);
        assert_eq!(diagnostics.dropped_events, 1);
        assert_eq!(diagnostics.max_event_age_us, Some(500));
    }
}
