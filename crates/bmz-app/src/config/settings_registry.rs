use super::profile_config::ProfileConfig;

/// ゲーム内設定で編集可能な profile.toml 項目 (最小セット)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsEntryId {
    MasterVolume,
    KeyVolume,
    BgmVolume,
    PreviewVolume,
    InputOffsetMs,
    VisualOffsetMs,
}

impl SettingsEntryId {
    pub const VOLUME_ENTRIES: &'static [Self] =
        &[Self::MasterVolume, Self::KeyVolume, Self::BgmVolume, Self::PreviewVolume];

    pub const JUDGE_ENTRIES: &'static [Self] = &[Self::InputOffsetMs, Self::VisualOffsetMs];

    pub fn label(self) -> &'static str {
        match self {
            Self::MasterVolume => "MASTER",
            Self::KeyVolume => "KEY",
            Self::BgmVolume => "BGM",
            Self::PreviewVolume => "PREVIEW",
            Self::InputOffsetMs => "INPUT OFFSET",
            Self::VisualOffsetMs => "VISUAL OFFSET",
        }
    }
}

/// 設定値 1 ステップの増減量。
pub fn settings_adjust_step(id: SettingsEntryId) -> i32 {
    match id {
        SettingsEntryId::InputOffsetMs | SettingsEntryId::VisualOffsetMs => 1,
        _ => 5,
    }
}

pub fn format_settings_value(profile: &ProfileConfig, id: SettingsEntryId) -> String {
    match id {
        SettingsEntryId::MasterVolume => format!("{}", profile.audio_mix.master_volume),
        SettingsEntryId::KeyVolume => format!("{}", profile.audio_mix.key_volume),
        SettingsEntryId::BgmVolume => format!("{}", profile.audio_mix.bgm_volume),
        SettingsEntryId::PreviewVolume => format!("{}", profile.audio_mix.preview_volume),
        SettingsEntryId::InputOffsetMs => {
            format!("{} ms", profile.judge.input_offset_us / 1_000)
        }
        SettingsEntryId::VisualOffsetMs => {
            format!("{} ms", profile.judge.visual_offset_us / 1_000)
        }
    }
}

/// 設定値を 1 ステップ変更する。変更があった場合 `true`。
pub fn adjust_settings_value(profile: &mut ProfileConfig, id: SettingsEntryId, delta: i32) -> bool {
    if delta == 0 {
        return false;
    }
    match id {
        SettingsEntryId::MasterVolume => {
            adjust_u32(&mut profile.audio_mix.master_volume, delta, 0, 100)
        }
        SettingsEntryId::KeyVolume => adjust_u32(&mut profile.audio_mix.key_volume, delta, 0, 100),
        SettingsEntryId::BgmVolume => adjust_u32(&mut profile.audio_mix.bgm_volume, delta, 0, 100),
        SettingsEntryId::PreviewVolume => {
            adjust_u32(&mut profile.audio_mix.preview_volume, delta, 0, 100)
        }
        SettingsEntryId::InputOffsetMs => {
            let before = profile.judge.input_offset_us;
            let ms = (profile.judge.input_offset_us / 1_000).saturating_add(delta as i64);
            let ms = ms.clamp(-500, 500);
            profile.judge.input_offset_us = ms * 1_000;
            profile.judge.input_offset_us != before
        }
        SettingsEntryId::VisualOffsetMs => {
            let before = profile.judge.visual_offset_us;
            let ms = (profile.judge.visual_offset_us / 1_000).saturating_add(delta as i64);
            let ms = ms.clamp(-500, 500);
            profile.judge.visual_offset_us = ms * 1_000;
            profile.judge.visual_offset_us != before
        }
    }
}

fn adjust_u32(value: &mut u32, delta: i32, min: u32, max: u32) -> bool {
    let before = *value;
    let next = (*value as i32).saturating_add(delta).clamp(min as i32, max as i32) as u32;
    *value = next;
    *value != before
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile_config::ProfileConfig;

    #[test]
    fn adjust_volume_clamps_to_range() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        profile.audio_mix.master_volume = 98;
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::MasterVolume, 5));
        assert_eq!(profile.audio_mix.master_volume, 100);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::MasterVolume, -200));
        assert_eq!(profile.audio_mix.master_volume, 0);
    }

    #[test]
    fn adjust_judge_offset_in_millisecond_steps() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::InputOffsetMs, 3));
        assert_eq!(profile.judge.input_offset_us, 3_000);
    }
}
