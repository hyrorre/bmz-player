use super::*;
use crate::input::shared::SharedInputBackend;
use bmz_gameplay::input::{backend::InputBackend, binding::LaneBinding};

fn state(modern: bool) -> (PadState, SharedInputBackend) {
    let input = SharedInputBackend::default();
    let mut state = PadState::new([GamepadScratchConfig::default(); 2], 1_000_000_000, 1, 1);
    state.clock = MachClock::new(1_000_000_000, 1_000_000_000, 1, 1);
    state.route_started = 1_000_000_000;
    state.route = Some(InputRoute {
        input: input.clone(),
        binding: LaneBinding { entries: vec![] },
        focused: true,
        keyboard_enabled: false,
    });
    send(&mut state, 1, u32::from(modern), 0.0, 1.0, true);
    (state, input)
}
fn send(
    state: &mut PadState,
    kind: i32,
    control: u32,
    value: f32,
    timestamp: f64,
    foreground: bool,
) {
    state.receive(
        Event {
            generation: state.generation,
            device: 1,
            kind,
            control,
            value,
            timestamp,
            name: Some("Test pad".into()),
        },
        1_100_000_000,
        Instant::now(),
        1_100_000_000,
        foreground,
    );
}
#[test]
fn buffered_short_edges_keep_host_timestamps_during_ui_stall() {
    let (mut state, mut input) = state(true);
    send(&mut state, 3, 0, 1.0, 1.001, true);
    send(&mut state, 3, 0, 0.0, 1.002, true);
    let events = input.drain_events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].timestamp, DeviceTimestamp::MonotonicNs(1_001_000_000));
    assert_eq!(events[1].timestamp, DeviceTimestamp::MonotonicNs(1_002_000_000));
    assert_eq!(state.output.buttons.len(), 2);
    assert_eq!(state.invalid, 0);
}
#[test]
fn legacy_edges_use_receipt_time_and_opposite_dpad_buttons_coexist() {
    let (mut state, mut input) = state(false);
    send(&mut state, 3, 13, 1.0, 0.0, true);
    send(&mut state, 3, 14, 1.0, 0.0, true);
    let events = input.drain_events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].timestamp, DeviceTimestamp::MonotonicNs(1_100_000_000));
    assert_eq!(state.pressed_buttons().len(), 2);
    assert_eq!(state.status(), 2);
}
#[test]
fn history_gap_releases_and_requires_fresh_press_after_baseline() {
    let (mut state, mut input) = state(true);
    send(&mut state, 3, 0, 1.0, 1.001, true);
    send(&mut state, 5, 0, 0.0, 1.002, true);
    send(&mut state, 8, 0, 1.0, 1.002, true);
    send(&mut state, 3, 0, 0.0, 1.003, true);
    let events = input.drain_events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].kind, bmz_core::input::InputKind::Release);
    send(&mut state, 3, 0, 1.0, 1.004, true);
    assert_eq!(input.drain_events().len(), 1);
    assert_eq!(state.gaps, 1);
}
#[test]
fn disconnect_and_background_input_do_not_leave_gameplay_holds() {
    let (mut state, mut input) = state(false);
    send(&mut state, 3, 0, 1.0, 0.0, true);
    send(&mut state, 3, 1, 1.0, 0.0, false);
    let events = input.drain_events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].kind, bmz_core::input::InputKind::Release);
    send(&mut state, 2, 0, 0.0, 0.0, false);
    assert!(input.drain_events().is_empty());
    assert!(state.devices.is_empty());
}
#[test]
fn stale_generation_and_invalid_host_time_are_not_retimed_to_now() {
    let (mut state, mut input) = state(true);
    state.receive(
        Event {
            generation: 999,
            device: 1,
            kind: 3,
            control: 0,
            value: 1.0,
            timestamp: 1.001,
            name: None,
        },
        1_100_000_000,
        Instant::now(),
        1_100_000_000,
        true,
    );
    send(&mut state, 3, 0, 1.0, f64::NAN, true);
    assert_eq!(state.invalid, 1);
    assert!(
        !input.drain_events().iter().any(|event| event.kind == bmz_core::input::InputKind::Press)
    );
}
#[test]
fn invalid_release_releases_the_last_valid_gameplay_hold() {
    let (mut state, mut input) = state(true);
    send(&mut state, 3, 0, 1.0, 1.001, true);
    send(&mut state, 3, 0, 0.0, f64::NAN, true);
    let events = input.drain_events();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].kind, bmz_core::input::InputKind::Press);
    assert_eq!(events[1].kind, bmz_core::input::InputKind::Release);
    assert_eq!(state.invalid, 1);
    assert!(state.suppressed.contains(&(1, 0)));
    send(&mut state, 3, 0, 0.0, 1.003, true);
    send(&mut state, 3, 0, 1.0, 1.004, true);
    assert_eq!(input.drain_events().len(), 1);
}
#[test]
fn hotplug_between_route_generation_and_native_fence_is_not_lost() {
    let (mut state, mut input) = state(true);
    state.devices.clear();
    state.generation = 1;
    for kind in [1, 2] {
        state.receive(
            Event {
                generation: 0,
                device: 2,
                kind,
                control: 1,
                value: 0.0,
                timestamp: 0.0,
                name: Some("Hotplug pad".into()),
            },
            1_100_000_000,
            Instant::now(),
            1_100_000_000,
            true,
        );
        assert_eq!(state.devices.contains_key(&2), kind == 1);
    }
    assert!(input.drain_events().is_empty());
}
#[test]
fn digital_axis_baseline_requires_neutral_before_a_new_press() {
    let (mut state, mut input) = state(true);
    let mut configs = [GamepadScratchConfig::default(); 2];
    configs[0].analog_scratch = false;
    state
        .analog
        .set_config(configs, GamepadSlotMap::from_device_ids([Some(DeviceId(0x4000_0001)), None]));
    send(&mut state, 9, 0, 0.95, 0.0, true);
    send(&mut state, 4, 0, 0.96, 1.001, true);
    send(&mut state, 4, 0, 0.0, 1.002, true);
    assert!(input.drain_events().is_empty());
    send(&mut state, 4, 0, 0.95, 1.003, true);
    assert_eq!(input.drain_events().len(), 1);
}
#[test]
fn analog_scratch_baseline_allows_fresh_movement_without_returning_to_neutral() {
    let (mut state, mut input) = state(true);
    let mut configs = [GamepadScratchConfig::default(); 2];
    configs[0].analog_scratch = true;
    state
        .analog
        .set_config(configs, GamepadSlotMap::from_device_ids([Some(DeviceId(0x4000_0001)), None]));
    send(&mut state, 9, 0, 0.5, 0.0, true);
    assert!(input.drain_events().is_empty());
    send(&mut state, 4, 0, 0.75, 1.001, true);
    assert_eq!(input.drain_events().len(), 1);
    assert!(!state.output.axis_ticks.is_empty());
}
#[test]
fn native_pad_open_close_fences_callback_storage() {
    if let Some(pads) = Pads::start([GamepadScratchConfig::default(); 2]) {
        drop(pads);
    }
    assert_eq!(pad_status(), 0);
}
