use super::*;

pub const fn eight_key_hispeed_lane(id: SettingsEntryId) -> Option<LaneConfig> {
    match id {
        SettingsEntryId::Hispeed8Key1 => Some(LaneConfig::Key1),
        SettingsEntryId::Hispeed8Key2 => Some(LaneConfig::Key2),
        SettingsEntryId::Hispeed8Key3 => Some(LaneConfig::Key3),
        SettingsEntryId::Hispeed8Key4 => Some(LaneConfig::Key4),
        SettingsEntryId::Hispeed8Key5 => Some(LaneConfig::Key5),
        SettingsEntryId::Hispeed8Key6 => Some(LaneConfig::Key6),
        SettingsEntryId::Hispeed8Key7 => Some(LaneConfig::Key7),
        SettingsEntryId::Hispeed8Key8 => Some(LaneConfig::Key8),
        _ => None,
    }
}
