use bmz_core::input::ScratchDirection;
use bmz_core::lane::Lane;

use super::backend::{DeviceId, PhysicalControl};

#[derive(Debug, Clone)]
pub struct LaneBinding {
    pub entries: Vec<BindingEntry>,
}

#[derive(Debug, Clone)]
pub struct BindingEntry {
    pub device: Option<DeviceId>,
    pub control: PhysicalControl,
    pub lane: Lane,
    pub scratch_direction: Option<ScratchDirection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BindingResolution {
    pub lane: Lane,
    pub scratch_direction: Option<ScratchDirection>,
}

impl LaneBinding {
    pub fn resolve(&self, device: DeviceId, control: &PhysicalControl) -> Option<Lane> {
        self.resolve_entry(device, control).map(|entry| entry.lane)
    }

    pub fn resolve_entry(
        &self,
        device: DeviceId,
        control: &PhysicalControl,
    ) -> Option<BindingResolution> {
        self.entries
            .iter()
            .find(|entry| entry.control == *control && entry.device == Some(device))
            .or_else(|| {
                self.entries
                    .iter()
                    .find(|entry| entry.control == *control && entry.device.is_none())
            })
            .map(|entry| BindingResolution {
                lane: entry.lane,
                scratch_direction: entry.scratch_direction,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button(name: &str) -> PhysicalControl {
        PhysicalControl::GamepadButton(name.to_string())
    }

    fn entry(device: Option<DeviceId>, lane: Lane) -> BindingEntry {
        BindingEntry { device, control: button("Button1"), lane, scratch_direction: None }
    }

    #[test]
    fn device_specific_binding_takes_priority_over_earlier_wildcard() {
        let binding = LaneBinding {
            entries: vec![entry(None, Lane::Key8), entry(Some(DeviceId(16)), Lane::Key1)],
        };

        assert_eq!(binding.resolve(DeviceId(16), &button("Button1")), Some(Lane::Key1));
        assert_eq!(binding.resolve(DeviceId(17), &button("Button1")), Some(Lane::Key8));
    }

    #[test]
    fn wildcard_binding_still_matches_when_no_specific_binding_exists() {
        let binding = LaneBinding { entries: vec![entry(None, Lane::Key1)] };

        assert_eq!(binding.resolve(DeviceId(16), &button("Button1")), Some(Lane::Key1));
        assert_eq!(binding.resolve(DeviceId(42), &button("Button1")), Some(Lane::Key1));
    }
}
