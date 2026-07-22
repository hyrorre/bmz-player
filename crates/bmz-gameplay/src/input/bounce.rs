use std::collections::HashMap;

use bmz_core::input::InputKind;

use super::backend::{
    DeviceId, DeviceInputEvent, DeviceTimestamp, InputBouncePolicy, PhysicalControl,
};

/// チャタリング抑制のデバイス種別ごとの閾値。
///
/// 値が 0 のデバイス種別は、重複状態を含めて入力をそのまま通す。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputBounceConfig {
    pub keyboard_threshold_us: u64,
    pub controller_threshold_us: u64,
}

impl InputBounceConfig {
    pub const fn threshold_for(self, control: &PhysicalControl) -> u64 {
        match control {
            PhysicalControl::KeyboardKey(_) => self.keyboard_threshold_us,
            PhysicalControl::GamepadButton(_) | PhysicalControl::HidButton(_) => {
                self.controller_threshold_us
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct AcceptedState {
    kind: InputKind,
    timestamp_ns: Option<u128>,
}

/// `(DeviceId, PhysicalControl)` ごとに受理済み状態を追跡するチャタリングフィルタ。
#[derive(Debug, Default)]
pub struct InputBounceFilter {
    config: InputBounceConfig,
    accepted: HashMap<(DeviceId, PhysicalControl), AcceptedState>,
}

impl InputBounceFilter {
    pub fn new(config: InputBounceConfig) -> Self {
        Self { config, accepted: HashMap::new() }
    }

    pub fn config(&self) -> InputBounceConfig {
        self.config
    }

    pub fn set_config(&mut self, config: InputBounceConfig) {
        if self.config != config {
            self.config = config;
            self.accepted.clear();
        }
    }

    pub fn clear(&mut self) {
        self.accepted.clear();
    }

    /// イベントを受理するべき場合に返す。
    ///
    /// monotonic timestamp が双方にある場合だけ、Release 直後の Press を閾値で
    /// 抑制する。時刻が欠落または逆行している場合は、状態重複の除外だけを行う。
    pub fn accept(&mut self, event: DeviceInputEvent) -> Option<DeviceInputEvent> {
        if event.bounce_policy == InputBouncePolicy::Bypass {
            return Some(event);
        }
        let threshold_us = self.config.threshold_for(&event.control);
        if threshold_us == 0 {
            return Some(event);
        }
        let key = (event.device, event.control.clone());
        let timestamp_ns = monotonic_timestamp(&event.timestamp);
        if let Some(previous) = self.accepted.get(&key).copied() {
            if previous.kind == event.kind {
                return None;
            }
            if previous.kind == InputKind::Release
                && event.kind == InputKind::Press
                && let Some(elapsed_us) = elapsed_us(previous.timestamp_ns, timestamp_ns)
                && elapsed_us <= threshold_us
            {
                let device_kind = match &event.control {
                    PhysicalControl::KeyboardKey(_) => "keyboard",
                    PhysicalControl::GamepadButton(_) | PhysicalControl::HidButton(_) => {
                        "controller"
                    }
                };
                tracing::info!(
                    device = ?event.device,
                    control = ?event.control,
                    device_kind,
                    elapsed_us,
                    threshold_us,
                    "suppressed input bounce candidate"
                );
                return None;
            }
        }

        self.accepted.insert(key, AcceptedState { kind: event.kind, timestamp_ns });
        Some(event)
    }
}

fn monotonic_timestamp(timestamp: &DeviceTimestamp) -> Option<u128> {
    match timestamp {
        DeviceTimestamp::MonotonicNs(timestamp) => Some(*timestamp),
        DeviceTimestamp::Unknown | DeviceTimestamp::BackendTicks(_) => None,
    }
}

fn elapsed_us(previous_ns: Option<u128>, current_ns: Option<u128>) -> Option<u64> {
    let (previous_ns, current_ns) = (previous_ns?, current_ns?);
    let elapsed_ns = current_ns.checked_sub(previous_ns)?;
    Some((elapsed_ns / 1_000).min(u128::from(u64::MAX)) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(
        device: u32,
        control: PhysicalControl,
        kind: InputKind,
        timestamp_ns: u128,
    ) -> DeviceInputEvent {
        DeviceInputEvent {
            device: DeviceId(device),
            control,
            kind,
            timestamp: DeviceTimestamp::MonotonicNs(timestamp_ns),
            bounce_policy: InputBouncePolicy::Apply,
        }
    }

    fn keyboard() -> PhysicalControl {
        PhysicalControl::KeyboardKey("Z".to_string())
    }

    fn controller() -> PhysicalControl {
        PhysicalControl::GamepadButton("Button1".to_string())
    }

    #[test]
    fn suppresses_press_after_release_within_keyboard_threshold() {
        let mut filter = InputBounceFilter::new(InputBounceConfig {
            keyboard_threshold_us: 5_000,
            controller_threshold_us: 0,
        });

        assert!(filter.accept(event(0, keyboard(), InputKind::Press, 1_000_000)).is_some());
        assert!(filter.accept(event(0, keyboard(), InputKind::Release, 2_000_000)).is_some());
        assert!(filter.accept(event(0, keyboard(), InputKind::Press, 6_000_000)).is_none());
    }

    #[test]
    fn accepts_press_after_release_outside_threshold() {
        let mut filter = InputBounceFilter::new(InputBounceConfig {
            keyboard_threshold_us: 5_000,
            controller_threshold_us: 0,
        });

        assert!(filter.accept(event(0, keyboard(), InputKind::Press, 1_000_000)).is_some());
        assert!(filter.accept(event(0, keyboard(), InputKind::Release, 2_000_000)).is_some());
        assert!(filter.accept(event(0, keyboard(), InputKind::Press, 8_000_000)).is_some());
    }

    #[test]
    fn applies_thresholds_per_device_kind() {
        let mut filter = InputBounceFilter::new(InputBounceConfig {
            keyboard_threshold_us: 0,
            controller_threshold_us: 5_000,
        });

        for kind in [InputKind::Press, InputKind::Release] {
            assert!(filter.accept(event(0, controller(), kind, 1_000_000)).is_some());
        }
        assert!(filter.accept(event(0, controller(), InputKind::Press, 5_000_000)).is_none());
        assert!(filter.accept(event(0, keyboard(), InputKind::Press, 5_000_000)).is_some());
    }

    #[test]
    fn bypass_policy_passes_controller_event_without_tracking_state() {
        let config =
            InputBounceConfig { keyboard_threshold_us: 0, controller_threshold_us: 20_000 };
        let mut filter = InputBounceFilter::new(config);
        let mut release = event(0, controller(), InputKind::Release, 1_000_000);
        release.bounce_policy = InputBouncePolicy::Bypass;
        let mut press = event(0, controller(), InputKind::Press, 1_001_000);
        press.bounce_policy = InputBouncePolicy::Bypass;

        assert!(filter.accept(release).is_some());
        assert!(filter.accept(press).is_some());
        assert!(filter.accept(event(0, controller(), InputKind::Release, 2_000_000)).is_some());
        assert!(filter.accept(event(0, controller(), InputKind::Press, 2_001_000)).is_none());
    }

    #[test]
    fn does_not_cross_device_or_control_boundaries() {
        let mut filter = InputBounceFilter::new(InputBounceConfig {
            keyboard_threshold_us: 5_000,
            controller_threshold_us: 5_000,
        });

        assert!(filter.accept(event(0, keyboard(), InputKind::Release, 1_000_000)).is_some());
        assert!(filter.accept(event(1, keyboard(), InputKind::Press, 2_000_000)).is_some());
        assert!(filter.accept(event(0, controller(), InputKind::Press, 2_000_000)).is_some());
    }

    #[test]
    fn zero_threshold_disables_time_based_suppression() {
        let mut filter = InputBounceFilter::new(InputBounceConfig::default());

        assert!(filter.accept(event(0, keyboard(), InputKind::Release, 1_000_000)).is_some());
        assert!(filter.accept(event(0, keyboard(), InputKind::Press, 1_001_000)).is_some());
    }

    #[test]
    fn zero_threshold_passes_through_duplicate_states() {
        let mut filter = InputBounceFilter::new(InputBounceConfig::default());

        assert!(filter.accept(event(0, keyboard(), InputKind::Press, 1_000_000)).is_some());
        assert!(filter.accept(event(0, keyboard(), InputKind::Press, 1_001_000)).is_some());
        assert!(filter.accept(event(0, keyboard(), InputKind::Release, 1_002_000)).is_some());
        assert!(filter.accept(event(0, keyboard(), InputKind::Release, 1_003_000)).is_some());
    }

    #[test]
    fn drops_extra_release_after_suppressed_bounce_press() {
        let mut filter = InputBounceFilter::new(InputBounceConfig {
            keyboard_threshold_us: 5_000,
            controller_threshold_us: 0,
        });

        assert!(filter.accept(event(0, keyboard(), InputKind::Press, 1_000_000)).is_some());
        assert!(filter.accept(event(0, keyboard(), InputKind::Release, 2_000_000)).is_some());
        assert!(filter.accept(event(0, keyboard(), InputKind::Press, 6_000_000)).is_none());
        assert!(filter.accept(event(0, keyboard(), InputKind::Release, 7_000_000)).is_none());
    }

    #[test]
    fn unknown_timestamps_only_filter_duplicate_states() {
        let mut filter = InputBounceFilter::new(InputBounceConfig {
            keyboard_threshold_us: 5_000,
            controller_threshold_us: 0,
        });
        let mut release = event(0, keyboard(), InputKind::Release, 0);
        release.timestamp = DeviceTimestamp::Unknown;
        let mut press = event(0, keyboard(), InputKind::Press, 0);
        press.timestamp = DeviceTimestamp::Unknown;

        assert!(filter.accept(release).is_some());
        assert!(filter.accept(press).is_some());
        let mut duplicate_press = event(0, keyboard(), InputKind::Press, 0);
        duplicate_press.timestamp = DeviceTimestamp::Unknown;
        assert!(filter.accept(duplicate_press).is_none());
    }
}
