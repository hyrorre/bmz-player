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
}

impl LaneBinding {
    pub fn resolve(&self, device: DeviceId, control: &PhysicalControl) -> Option<Lane> {
        self.entries
            .iter()
            .find(|entry| {
                entry.control == *control && entry.device.map(|id| id == device).unwrap_or(true)
            })
            .map(|entry| entry.lane)
    }
}
