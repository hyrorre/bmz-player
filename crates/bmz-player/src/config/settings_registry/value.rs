use super::*;

pub fn settings_adjust_step(id: SettingsEntryId) -> i32 {
    match id {
        SettingsEntryId::InputOffsetMs | SettingsEntryId::VisualOffsetMs => 1,
        SettingsEntryId::PlayExitHoldMs => 100,
        SettingsEntryId::Sudden | SettingsEntryId::Lift | SettingsEntryId::Hidden => 25,
        SettingsEntryId::TargetGreenNumber => 10,
        SettingsEntryId::NoteDisplayDurationMs => 10,
        SettingsEntryId::ConstantFadeMs => 10,
        SettingsEntryId::MisslayerDurationMs => 50,
        SettingsEntryId::AnalogScratchThreshold1P | SettingsEntryId::AnalogScratchThreshold2P => 10,
        SettingsEntryId::RandomMixMaxBpm | SettingsEntryId::RandomMixMinBpm => 10,
        SettingsEntryId::AssistScrollRate
        | SettingsEntryId::AssistLongNoteRate
        | SettingsEntryId::AssistKeyPgreatRate
        | SettingsEntryId::AssistKeyGreatRate
        | SettingsEntryId::AssistKeyGoodRate
        | SettingsEntryId::AssistScratchPgreatRate
        | SettingsEntryId::AssistScratchGreatRate
        | SettingsEntryId::AssistScratchGoodRate
        | SettingsEntryId::AssistLongNoteMarginRate => 5,
        _ => 1,
    }
}

pub(super) fn format_random_mix_level(value: u32, zero: &str) -> String {
    if value == 0 { zero.to_string() } else { format!("LEVEL {value}") }
}

pub(super) fn format_random_mix_bpm(value: u32) -> String {
    if value == 0 { "NO LIMIT".to_string() } else { format!("{value} BPM") }
}
