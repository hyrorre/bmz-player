use super::*;
use bmz_gameplay::input::{backend::InputBackend, binding::LaneBinding};

fn route(input: SharedInputBackend) -> InputRoute {
    InputRoute {
        input,
        binding: LaneBinding { entries: Vec::new() },
        focused: true,
        keyboard_enabled: true,
    }
}
fn event(kind: InputKind) -> DeviceInputEvent {
    DeviceInputEvent {
        device: super::super::winit::W_KEYBOARD_DEVICE_ID,
        control: PhysicalControl::KeyboardKey("Z".into()),
        kind,
        timestamp: DeviceTimestamp::MonotonicNs(123),
        bounce_policy: Default::default(),
    }
}

#[test]
fn repeat_and_snapshot_holds_never_become_new_presses() {
    let mut keys = Keys::default();
    keys.baseline([44]);
    assert_eq!(keys.change(44, 1), None);
    assert_eq!(keys.change(44, 2), None);
    assert_eq!(keys.change(44, 0), None);
    assert_eq!(keys.change(44, 1), Some(InputKind::Press));
    assert_eq!(keys.change(44, 2), None);
    assert_eq!(keys.change(44, 0), Some(InputKind::Release));
}

#[test]
fn source_switch_is_serialized_with_delivery_and_releases_old_holds() {
    let mut input = SharedInputBackend::default();
    let mut routing = Routing::default();
    routing.set_route(Some(route(input.clone())));
    assert!(routing.window_event(event(InputKind::Press), &input));
    routing.set_native(true);
    assert!(!routing.window_event(event(InputKind::Press), &input));
    let events = input.drain_events();
    assert_eq!(
        events.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [InputKind::Press, InputKind::Release]
    );
    routing.set_native(false);
    assert!(!routing.window_event(event(InputKind::Press), &input));
    assert!(!routing.window_event(event(InputKind::Release), &input));
    assert!(routing.window_event(event(InputKind::Press), &input));
}

#[test]
fn route_change_focus_loss_and_lease_expiry_release_and_inhibit() {
    let mut input = SharedInputBackend::default();
    let mut next = SharedInputBackend::default();
    let mut routing = Routing::default();
    routing.set_route(Some(route(input.clone())));
    routing.set_native(true);
    routing.push(event(InputKind::Press));
    assert!(routing.leased(Instant::now() + Duration::from_millis(100)));
    assert!(!routing.leased(Instant::now() + Duration::from_millis(300)));
    routing.set_route(Some(route(next.clone())));
    assert_eq!(input.drain_events().len(), 2);
    assert!(next.drain_events().is_empty());
    routing.push(event(InputKind::Press));
    let mut unfocused = route(next.clone());
    unfocused.focused = false;
    routing.set_route(Some(unfocused));
    assert_eq!(next.drain_events().last().unwrap().kind, InputKind::Release);
}

#[test]
fn missing_device_is_distinct_from_invalid_unstable_path() {
    assert_eq!(
        open_devices(&["/dev/input/bmz-definitely-missing".into()]).err().unwrap().kind(),
        io::ErrorKind::NotFound
    );
    assert!(!valid_path(Path::new("/dev/input/event0")));
    assert!(!valid_path(Path::new("/etc/passwd")));
}

#[test]
fn permission_loss_releases_native_holds_and_restores_window_delivery() {
    let mut input = SharedInputBackend::default();
    let mut routing = Routing::default();
    routing.set_route(Some(route(input.clone())));
    routing.set_native(true);
    routing.push(event(InputKind::Press));
    assert!(!routing.window_event(event(InputKind::Press), &input));
    assert_eq!(routing.unavailable(&io::Error::from_raw_os_error(libc::EACCES)), 4);
    assert!(!routing.native);
    assert_eq!(input.drain_events().last().unwrap().kind, InputKind::Release);
    assert!(!routing.window_event(event(InputKind::Press), &input));
    assert!(!routing.window_event(event(InputKind::Release), &input));
    assert!(routing.window_event(event(InputKind::Press), &input));
    assert_eq!(routing.unavailable(&io::Error::from_raw_os_error(libc::ENOENT)), 5);
    assert_eq!(routing.unavailable(&io::Error::from_raw_os_error(libc::EIO)), 6);
}

#[test]
fn multi_keyboard_holds_emit_first_press_and_last_release_only() {
    let mut keys = [Keys::default(), Keys::default()];
    let mut result = Vec::new();
    for (index, value) in [(0, 1), (1, 1), (0, 2), (0, 0), (1, 0)] {
        if let Some(kind) = keys[index].change(44, value)
            && !other_key_held(keys.iter().enumerate(), index, 44)
        {
            result.push(kind);
        }
    }
    assert_eq!(result, [InputKind::Press, InputKind::Release]);
}

#[test]
fn ready_devices_merge_by_time_with_stable_ties_and_preserve_syn_order() {
    let first = evdev::InputEvent::new(1, 44, 1);
    let later = evdev::InputEvent::new_now(1, 44, 0);
    let syn = evdev::InputEvent::new_now(0, 0, 0);
    let mut batches = vec![VecDeque::from([first, later, syn]), VecDeque::from([first])];
    let mut order = Vec::new();
    while let Some((index, event)) = pop_earliest(&mut batches) {
        order.push((index, event.event_type().0, event.value()));
    }
    assert_eq!(order, [(0, 1, 1), (1, 1, 1), (0, 1, 0), (0, 0, 0)]);
}

#[test]
fn lost_history_and_reconnect_release_held_keys_without_fabricating_presses() {
    let mut input = SharedInputBackend::default();
    let mut routing = Routing::default();
    routing.set_route(Some(route(input.clone())));
    routing.set_native(true);
    let mut keys = Keys::default();
    assert_eq!(keys.change(44, 1), Some(InputKind::Press));
    routing.push(event(InputKind::Press));
    keys.dropped = true;
    routing.release();
    assert_eq!(keys.change(44, 0), None); // Ignore history through SYN_REPORT.
    keys.baseline([44]); // Kernel snapshot; age of hold is unknown.
    assert!(keys.dropped); // A focus/queue resync cannot terminate SYN_DROPPED.
    keys.dropped = false; // Only the following SYN_REPORT ends discarded history.
    assert_eq!(keys.change(44, 1), None);
    assert_eq!(keys.change(44, 0), None);
    assert_eq!(
        input.drain_events().iter().map(|e| e.kind).collect::<Vec<_>>(),
        [InputKind::Press, InputKind::Release]
    );
    assert_eq!(keys.change(44, 1), Some(InputKind::Press));
    routing.push(event(InputKind::Press));
    routing.set_native(false); // Disconnect / clock failure / permission loss.
    assert_eq!(input.drain_events().last().unwrap().kind, InputKind::Release);
    keys.baseline([44]);
    routing.set_native(true);
    assert_eq!(keys.change(44, 2), None);
    assert!(input.drain_events().is_empty());
}

#[test]
fn arbiter_preserves_the_exact_input_sequence_used_by_gameplay() {
    let mut direct = SharedInputBackend::default();
    let mut native = SharedInputBackend::default();
    let mut routing = Routing::default();
    routing.set_route(Some(route(native.clone())));
    routing.set_native(true);
    for kind in [InputKind::Press, InputKind::Release, InputKind::Press, InputKind::Release] {
        let e = event(kind);
        direct.push_shared_event(e.clone());
        routing.push(e);
    }
    let describe = |events: Vec<DeviceInputEvent>| {
        events
            .into_iter()
            .map(|e| (e.device, e.control, e.kind, e.timestamp, e.bounce_policy))
            .collect::<Vec<_>>()
    };
    assert_eq!(describe(direct.drain_events()), describe(native.drain_events()));
}

#[test]
fn lapsed_lease_blocks_new_presses_but_keeps_releases_of_holds() {
    assert!(admits(InputKind::Press, true, true));
    assert!(admits(InputKind::Release, true, true));
    // A redraw stall must not cut a long note that is already held.
    assert!(!admits(InputKind::Press, true, false));
    assert!(admits(InputKind::Release, true, false));
    // Focus/clock failures still deny everything; held keys are released by the reset.
    assert!(!admits(InputKind::Press, false, true));
    assert!(!admits(InputKind::Release, false, true));
}
