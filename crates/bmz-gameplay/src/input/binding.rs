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
            .find(|entry| {
                entry.control == *control && entry.device.map(|id| id == device).unwrap_or(true)
            })
            .map(|entry| BindingResolution {
                lane: entry.lane,
                scratch_direction: entry.scratch_direction,
            })
    }
}
