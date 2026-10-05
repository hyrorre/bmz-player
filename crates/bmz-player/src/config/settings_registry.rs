use super::play::{
    CONSTANT_FADE_MAX_MS, CONSTANT_FADE_MIN_MS, TARGET_GREEN_NUMBER_MAX, TARGET_GREEN_NUMBER_MIN,
    clamp_hispeed,
};
use super::profile_config::{
    AssistLongNoteMode, AssistMineMode, AssistOptionConfig, AssistScrollMode, BgaExpandConfig,
    BgaModeConfig, BottomShiftableGaugeConfig, DifficultyTableLevelDisplay, DoubleOptionConfig,
    FastSlowDisplayScope, GaugeAutoShiftConfig, GaugeTypeConfig, HISPEED_STEP_MAX,
    HISPEED_STEP_MIN, HispeedConfigPreset, HispeedDirectionConfig, HsFixConfig,
    JudgeAlgorithmConfig, KeyModeConversionConfig, LaneConfig, ProfileConfig,
    RELEASE_BOUNCE_MS_MAX, RandomOptionConfig, ReplaySlotRule, SelectInputModeConfig,
    SevenToNinePattern, SevenToNineRuleMode, SevenToNineType, TargetOptionConfig,
    default_classic_hispeed_step, normalize_hispeed_step,
};
use bmz_core::lane::KeyMode;
use bmz_gameplay::rule::RuleMode;

use crate::i18n::AppLocale;
use crate::ln_policy::LnPolicySetting;
use crate::select_options::SessionMode;

impl SettingsEntryId {
    pub const VOLUME_ENTRIES: &'static [Self] = &[
        Self::NormalizeChartVolume,
        Self::NormalizeSystemBgmVolume,
        Self::MasterVolume,
        Self::KeyVolume,
        Self::BgmVolume,
        Self::PreviewVolume,
        Self::SystemBgmVolume,
        Self::SystemSeVolume,
    ];

    pub const JUDGE_ENTRIES: &'static [Self] = &[
        Self::InputOffsetMs,
        Self::VisualOffsetMs,
        Self::VisualOffsetAutoAdjust,
        Self::JudgeAlgorithm,
        Self::FastSlowDisplayScope,
        Self::FastSlowDisplayThresholdMs,
    ];

    // 旧単一行の `Assist` はLEGACY NOTEだけを切り替えるため一覧から除外し、
    // 個別項目は下位のASSISTフォルダで公開する。
    pub const PLAY_ENTRIES: &'static [Self] = &[
        Self::Gauge,
        Self::RuleMode,
        Self::LnModePolicy,
        Self::GaugeAutoShift,
        Self::BottomShiftableGauge,
        Self::Random,
        Self::Random2,
        Self::DoubleOption,
        Self::HsFix,
        Self::Target,
        Self::BgaMode,
        Self::BgaExpand,
        Self::SessionMode,
        Self::KeyModeConversion,
        Self::MisslayerDurationMs,
        Self::PlayExitHoldMs,
        Self::NoteRetention,
        Self::ShowLnTailCap,
        Self::GuideSe,
    ];

    pub const SEVEN_TO_NINE_ENTRIES: &'static [Self] =
        &[Self::SevenToNinePattern, Self::SevenToNineType, Self::SevenToNineRuleMode];

    pub const ASSIST_ENTRIES: &'static [Self] = &[
        Self::AssistExpandJudge,
        Self::AssistJudgeArea,
        Self::AssistMarkNote,
        Self::AssistBpmGuide,
        Self::AssistScrollMode,
        Self::AssistLongNoteMode,
        Self::AssistMineMode,
    ];

    pub const ASSIST_NOTE_ENTRIES: &'static [Self] = &[
        Self::AssistScrollSection,
        Self::AssistScrollRate,
        Self::AssistLongNoteRate,
        Self::AssistExtraNoteDepth,
        Self::AssistExtraNoteScratch,
        Self::AssistExtraNoteType,
    ];

    pub const ASSIST_JUDGE_ENTRIES: &'static [Self] = &[
        Self::AssistKeyPgreatRate,
        Self::AssistKeyGreatRate,
        Self::AssistKeyGoodRate,
        Self::AssistScratchPgreatRate,
        Self::AssistScratchGreatRate,
        Self::AssistScratchGoodRate,
        Self::AssistLongNoteMarginRate,
    ];

    pub const DISPLAY_ENTRIES: &'static [Self] = &[
        Self::HispeedMode,
        Self::NormalHispeedLevel,
        Self::Hispeed,
        Self::ClassicHispeedStep,
        Self::FloatingHispeedStep,
        Self::TargetGreenNumber,
        Self::NoteDisplayDurationMs,
        Self::HispeedAutoAdjust,
        Self::SuddenEnabled,
        Self::Sudden,
        Self::LiftEnabled,
        Self::Lift,
        Self::HiddenEnabled,
        Self::Hidden,
        Self::Constant,
        Self::ConstantFadeMs,
    ];

    pub const fn visible_for_hispeed_config(self, preset: HispeedConfigPreset) -> bool {
        match self {
            Self::NormalHispeedLevel => preset.supports_normal(),
            Self::Hispeed | Self::ClassicHispeedStep => preset.supports_classic(),
            Self::FloatingHispeedStep
            | Self::TargetGreenNumber
            | Self::NoteDisplayDurationMs
            | Self::HispeedAutoAdjust => preset.supports_floating(),
            _ => true,
        }
    }

    pub const INPUT_ENTRIES: &'static [Self] = &[
        Self::SelectInputMode,
        Self::AnalogScratch1P,
        Self::AnalogScratchSensitivity1P,
        Self::AnalogScratchThreshold1P,
        Self::AnalogScratch2P,
        Self::AnalogScratchSensitivity2P,
        Self::AnalogScratchThreshold2P,
        Self::AnalogTicksPerScroll,
        Self::KeyboardReleaseBounceMs,
        Self::ControllerReleaseBounceMs,
    ];

    pub const SELECT_ENTRIES: &'static [Self] = &[
        Self::DifficultyTableLevelDisplay,
        Self::SelectRandomSelect,
        Self::SelectRandomNoPlay,
        Self::SelectRandomFailed,
        Self::SelectRandomNotEasy,
        Self::SelectRandomNotClear,
        Self::SelectRandomNotHard,
        Self::SelectRandomNotExHard,
        Self::SelectRandomNotFullCombo,
        Self::RandomMixTargetLevel,
        Self::RandomMixMaxLevel,
        Self::RandomMixMinLevel,
        Self::RandomMixBpmRange,
        Self::RandomMixMaxBpm,
        Self::RandomMixMinBpm,
        Self::RandomMixStages,
    ];

    pub const HISPEED_8K_ENTRIES: &'static [Self] = &[
        Self::Hispeed8Key1,
        Self::Hispeed8Key2,
        Self::Hispeed8Key3,
        Self::Hispeed8Key4,
        Self::Hispeed8Key5,
        Self::Hispeed8Key6,
        Self::Hispeed8Key7,
        Self::Hispeed8Key8,
    ];

    pub const REPLAY_ENTRIES: &'static [Self] = &[
        Self::ReplayAutoSave,
        Self::ReplayCompress,
        Self::ReplaySlot1Rule,
        Self::ReplaySlot2Rule,
        Self::ReplaySlot3Rule,
        Self::ReplaySlot4Rule,
    ];

    pub const UI_ENTRIES: &'static [Self] = &[Self::Language, Self::ShowFps];

    pub fn is_play_setting(self) -> bool {
        Self::PLAY_ENTRIES.contains(&self)
            || Self::SEVEN_TO_NINE_ENTRIES.contains(&self)
            || Self::ASSIST_ENTRIES.contains(&self)
            || Self::ASSIST_NOTE_ENTRIES.contains(&self)
            || Self::ASSIST_JUDGE_ENTRIES.contains(&self)
            || matches!(self, Self::SuddenEnabled | Self::HiddenEnabled)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NormalizeChartVolume => "NORMALIZE",
            Self::NormalizeSystemBgmVolume => "SYS BGM NORM",
            Self::MasterVolume => "MASTER",
            Self::KeyVolume => "KEY",
            Self::BgmVolume => "BGM",
            Self::PreviewVolume => "PREVIEW",
            Self::SystemBgmVolume => "SYS BGM",
            Self::SystemSeVolume => "SYS SE",
            Self::InputOffsetMs => "INPUT OFFSET",
            Self::VisualOffsetMs => "VISUAL OFFSET",
            Self::VisualOffsetAutoAdjust => "AUTO ADJUST",
            Self::JudgeAlgorithm => "JUDGE ALGO",
            Self::FastSlowDisplayScope => "FAST/SLOW MODE",
            Self::FastSlowDisplayThresholdMs => "FAST/SLOW LIMIT",
            Self::RuleMode => "RULE MODE",
            Self::LnModePolicy => "LN MODE",
            Self::Gauge => "GAUGE",
            Self::GaugeAutoShift => "GAUGE SHIFT",
            Self::BottomShiftableGauge => "GAS BOTTOM",
            Self::Random => "RANDOM",
            Self::Random2 => "RANDOM 2P",
            Self::DoubleOption => "DP OPTION",
            Self::HsFix => "HS-FIX",
            Self::Target => "TARGET",
            Self::Assist => "ASSIST",
            Self::BgaMode => "BGA",
            Self::BgaExpand => "BGA FIT",
            Self::SessionMode => "SESSION MODE",
            Self::KeyModeConversion => "KEY CONVERSION",
            Self::SevenToNinePattern => "7K TO 9K PATTERN",
            Self::SevenToNineType => "7K TO 9K TYPE",
            Self::SevenToNineRuleMode => "7K TO 9K RULE",
            Self::PlayExitHoldMs => "PLAY EXIT HOLD",
            Self::AssistExpandJudge => "EXPAND JUDGE",
            Self::AssistJudgeArea => "JUDGE AREA",
            Self::AssistMarkNote => "MARK NOTE",
            Self::AssistBpmGuide => "BPM GUIDE",
            Self::AssistScrollMode => "SCROLL",
            Self::AssistLongNoteMode => "LONGNOTE",
            Self::AssistMineMode => "MINE",
            Self::AssistScrollSection => "SCROLL SECTION",
            Self::AssistScrollRate => "SCROLL RATE",
            Self::AssistLongNoteRate => "LONGNOTE RATE",
            Self::AssistExtraNoteDepth => "EXTRA NOTE DEPTH",
            Self::AssistExtraNoteScratch => "EXTRA NOTE SCRATCH",
            Self::AssistExtraNoteType => "EXTRA NOTE TYPE",
            Self::AssistKeyPgreatRate => "KEY PGREAT",
            Self::AssistKeyGreatRate => "KEY GREAT",
            Self::AssistKeyGoodRate => "KEY GOOD",
            Self::AssistScratchPgreatRate => "SCRATCH PGREAT",
            Self::AssistScratchGreatRate => "SCRATCH GREAT",
            Self::AssistScratchGoodRate => "SCRATCH GOOD",
            Self::AssistLongNoteMarginRate => "LN MARGIN",
            Self::MisslayerDurationMs => "MISSLAYER",
            Self::NoteRetention => "NOTE RETENTION",
            Self::ShowLnTailCap => "LN TAIL CAP",
            Self::GuideSe => "GUIDE SE",
            Self::Hispeed => "CLASSIC HS",
            Self::HispeedMode => "HS CONFIG",
            Self::NormalHispeedLevel => "NORMAL HS",
            Self::ClassicHispeedStep => "CLASSIC HS STEP",
            Self::FloatingHispeedStep => "FLOATING HS STEP",
            Self::SuddenEnabled => "SUDDEN+ ENABLED",
            Self::Sudden => "SUDDEN+",
            Self::LiftEnabled => "LIFT ENABLED",
            Self::Lift => "LIFT",
            Self::HispeedAutoAdjust => "HS AUTO ADJUST",
            Self::HiddenEnabled => "HIDDEN+ ENABLED",
            Self::Hidden => "HIDDEN",
            Self::TargetGreenNumber => "GREEN NO.",
            Self::NoteDisplayDurationMs => "DURATION",
            Self::Constant => "CONSTANT",
            Self::ConstantFadeMs => "CONSTANT FADE",
            Self::SelectInputMode => "SELECT INPUT DEVICE",
            Self::AnalogScratch1P => "1P ANALOG SCRATCH",
            Self::AnalogScratchSensitivity1P => "1P ANALOG SENS",
            Self::AnalogScratchThreshold1P => "1P ANALOG STOP",
            Self::AnalogScratch2P => "2P ANALOG SCRATCH",
            Self::AnalogScratchSensitivity2P => "2P ANALOG SENS",
            Self::AnalogScratchThreshold2P => "2P ANALOG STOP",
            Self::AnalogTicksPerScroll => "ANALOG SCROLL",
            Self::KeyboardReleaseBounceMs => "KEYBOARD BOUNCE",
            Self::ControllerReleaseBounceMs => "CONTROLLER BOUNCE",
            Self::Hispeed8Key1 => "KEY 1 HS DIRECTION",
            Self::Hispeed8Key2 => "KEY 2 HS DIRECTION",
            Self::Hispeed8Key3 => "KEY 3 HS DIRECTION",
            Self::Hispeed8Key4 => "KEY 4 HS DIRECTION",
            Self::Hispeed8Key5 => "KEY 5 HS DIRECTION",
            Self::Hispeed8Key6 => "KEY 6 HS DIRECTION",
            Self::Hispeed8Key7 => "KEY 7 HS DIRECTION",
            Self::Hispeed8Key8 => "KEY 8 HS DIRECTION",
            Self::DifficultyTableLevelDisplay => "TABLE LEVEL DISPLAY",
            Self::SelectRandomSelect => "RANDOM SELECT",
            Self::SelectRandomNoPlay => "NO PLAY RANDOM SELECT",
            Self::SelectRandomFailed => "FAILED RANDOM SELECT",
            Self::SelectRandomNotEasy => "NOT EASY RANDOM SELECT",
            Self::SelectRandomNotClear => "NOT CLEAR RANDOM SELECT",
            Self::SelectRandomNotHard => "NOT HARD RANDOM SELECT",
            Self::SelectRandomNotExHard => "NOT EX-HARD RANDOM SELECT",
            Self::SelectRandomNotFullCombo => "NOT FULL COMBO RANDOM SELECT",
            Self::RandomMixTargetLevel => "MIX TARGET LEVEL",
            Self::RandomMixMaxLevel => "MIX MAX LEVEL",
            Self::RandomMixMinLevel => "MIX MIN LEVEL",
            Self::RandomMixBpmRange => "MIX BPM RANGE",
            Self::RandomMixMaxBpm => "MIX MAX BPM",
            Self::RandomMixMinBpm => "MIX MIN BPM",
            Self::RandomMixStages => "MIX STAGES",
            Self::ReplayAutoSave => "REPLAY SAVE",
            Self::ReplayCompress => "REPLAY COMPRESS",
            Self::ReplaySlot1Rule => "REPLAY 1",
            Self::ReplaySlot2Rule => "REPLAY 2",
            Self::ReplaySlot3Rule => "REPLAY 3",
            Self::ReplaySlot4Rule => "REPLAY 4",
            Self::Language => "LANGUAGE",
            Self::ShowFps => "SHOW FPS",
        }
    }

    /// 選曲スキンの詳細欄へ表示する、設定項目自体の説明文キー。
    pub const fn description_key(self) -> &'static str {
        match self {
            Self::NormalizeChartVolume => "settings-entry-description-normalize-chart-volume",
            Self::NormalizeSystemBgmVolume => {
                "settings-entry-description-normalize-system-bgm-volume"
            }
            Self::MasterVolume => "settings-entry-description-master-volume",
            Self::KeyVolume => "settings-entry-description-key-volume",
            Self::BgmVolume => "settings-entry-description-bgm-volume",
            Self::PreviewVolume => "settings-entry-description-preview-volume",
            Self::SystemBgmVolume => "settings-entry-description-system-bgm-volume",
            Self::SystemSeVolume => "settings-entry-description-system-se-volume",
            Self::InputOffsetMs => "settings-entry-description-input-offset",
            Self::VisualOffsetMs => "settings-entry-description-visual-offset",
            Self::VisualOffsetAutoAdjust => "settings-entry-description-visual-offset-auto-adjust",
            Self::JudgeAlgorithm => "settings-entry-description-judge-algorithm",
            Self::FastSlowDisplayScope => "settings-entry-description-fast-slow-display-scope",
            Self::FastSlowDisplayThresholdMs => {
                "settings-entry-description-fast-slow-display-threshold"
            }
            Self::RuleMode => "settings-entry-description-rule-mode",
            Self::LnModePolicy => "settings-entry-description-ln-mode-policy",
            Self::Gauge => "settings-entry-description-gauge",
            Self::GaugeAutoShift => "settings-entry-description-gauge-auto-shift",
            Self::BottomShiftableGauge => "settings-entry-description-bottom-shiftable-gauge",
            Self::Random | Self::Random2 => "settings-entry-description-random",
            Self::DoubleOption => "settings-entry-description-double-option",
            Self::HsFix => "settings-entry-description-hs-fix",
            Self::Target => "settings-entry-description-target",
            Self::Assist => "settings-entry-description-assist",
            Self::BgaMode => "settings-entry-description-bga-mode",
            Self::BgaExpand => "settings-entry-description-bga-expand",
            Self::SessionMode => "settings-entry-description-session-mode",
            Self::KeyModeConversion => "settings-entry-description-key-mode-conversion",
            Self::SevenToNinePattern => "settings-entry-description-seven-to-nine-pattern",
            Self::SevenToNineType => "settings-entry-description-seven-to-nine-type",
            Self::SevenToNineRuleMode => "settings-entry-description-seven-to-nine-rule-mode",
            Self::PlayExitHoldMs => "settings-entry-description-play-exit-hold",
            Self::AssistExpandJudge => "settings-entry-description-assist-expand-judge",
            Self::AssistJudgeArea => "settings-entry-description-assist-judge-area",
            Self::AssistMarkNote => "settings-entry-description-assist-mark-note",
            Self::AssistBpmGuide => "settings-entry-description-assist-bpm-guide",
            Self::AssistScrollMode => "settings-entry-description-assist-scroll-mode",
            Self::AssistLongNoteMode => "settings-entry-description-assist-long-note-mode",
            Self::AssistMineMode => "settings-entry-description-assist-mine-mode",
            Self::AssistScrollSection => "settings-entry-description-assist-scroll-section",
            Self::AssistScrollRate => "settings-entry-description-assist-scroll-rate",
            Self::AssistLongNoteRate => "settings-entry-description-assist-long-note-rate",
            Self::AssistExtraNoteDepth => "settings-entry-description-assist-extra-note-depth",
            Self::AssistExtraNoteScratch => "settings-entry-description-assist-extra-note-scratch",
            Self::AssistExtraNoteType => "settings-entry-description-assist-extra-note-type",
            Self::AssistKeyPgreatRate | Self::AssistKeyGreatRate | Self::AssistKeyGoodRate => {
                "settings-entry-description-assist-key-judge-rate"
            }
            Self::AssistScratchPgreatRate
            | Self::AssistScratchGreatRate
            | Self::AssistScratchGoodRate => "settings-entry-description-assist-scratch-judge-rate",
            Self::AssistLongNoteMarginRate => {
                "settings-entry-description-assist-long-note-margin-rate"
            }
            Self::MisslayerDurationMs => "settings-entry-description-misslayer-duration",
            Self::NoteRetention => "settings-entry-description-note-retention",
            Self::ShowLnTailCap => "settings-entry-description-show-ln-tail-cap",
            Self::GuideSe => "settings-entry-description-guide-se",
            Self::Hispeed => "settings-entry-description-hispeed",
            Self::HispeedMode => "settings-entry-description-hispeed-mode",
            Self::NormalHispeedLevel => "settings-entry-description-normal-hispeed-level",
            Self::ClassicHispeedStep => "settings-entry-description-classic-hispeed-step",
            Self::FloatingHispeedStep => "settings-entry-description-floating-hispeed-step",
            Self::SuddenEnabled => "settings-entry-description-sudden-enabled",
            Self::Sudden => "settings-entry-description-sudden",
            Self::LiftEnabled => "settings-entry-description-lift-enabled",
            Self::Lift => "settings-entry-description-lift",
            Self::HispeedAutoAdjust => "settings-entry-description-hispeed-auto-adjust",
            Self::HiddenEnabled => "settings-entry-description-hidden-enabled",
            Self::Hidden => "settings-entry-description-hidden",
            Self::TargetGreenNumber => "settings-entry-description-target-green-number",
            Self::NoteDisplayDurationMs => "settings-entry-description-note-display-duration",
            Self::Constant => "settings-entry-description-constant",
            Self::ConstantFadeMs => "settings-entry-description-constant-fade",
            Self::SelectInputMode => "settings-entry-description-select-input-mode",
            Self::AnalogScratch1P | Self::AnalogScratch2P => {
                "settings-entry-description-analog-scratch"
            }
            Self::AnalogScratchSensitivity1P | Self::AnalogScratchSensitivity2P => {
                "settings-entry-description-analog-scratch-sensitivity"
            }
            Self::AnalogScratchThreshold1P | Self::AnalogScratchThreshold2P => {
                "settings-entry-description-analog-scratch-threshold"
            }
            Self::AnalogTicksPerScroll => "settings-entry-description-analog-ticks-per-scroll",
            Self::KeyboardReleaseBounceMs => "settings-entry-description-keyboard-release-bounce",
            Self::ControllerReleaseBounceMs => {
                "settings-entry-description-controller-release-bounce"
            }
            Self::Hispeed8Key1
            | Self::Hispeed8Key2
            | Self::Hispeed8Key3
            | Self::Hispeed8Key4
            | Self::Hispeed8Key5
            | Self::Hispeed8Key6
            | Self::Hispeed8Key7
            | Self::Hispeed8Key8 => "settings-entry-description-hispeed-direction",
            Self::DifficultyTableLevelDisplay => {
                "settings-entry-description-difficulty-table-level-display"
            }
            Self::SelectRandomSelect => "settings-entry-description-random-select",
            Self::SelectRandomNoPlay => "settings-entry-description-random-select",
            Self::SelectRandomFailed => "settings-entry-description-random-select",
            Self::SelectRandomNotEasy => "settings-entry-description-random-select",
            Self::SelectRandomNotClear => "settings-entry-description-random-select",
            Self::SelectRandomNotHard => "settings-entry-description-random-select",
            Self::SelectRandomNotExHard => "settings-entry-description-random-select",
            Self::SelectRandomNotFullCombo => "settings-entry-description-random-select",
            Self::RandomMixTargetLevel => "settings-entry-description-random-mix-target-level",
            Self::RandomMixMaxLevel => "settings-entry-description-random-mix-max-level",
            Self::RandomMixMinLevel => "settings-entry-description-random-mix-min-level",
            Self::RandomMixBpmRange => "settings-entry-description-random-mix-bpm-range",
            Self::RandomMixMaxBpm => "settings-entry-description-random-mix-max-bpm",
            Self::RandomMixMinBpm => "settings-entry-description-random-mix-min-bpm",
            Self::RandomMixStages => "settings-entry-description-random-mix-stages",
            Self::ReplayAutoSave => "settings-entry-description-replay-auto-save",
            Self::ReplayCompress => "settings-entry-description-replay-compress",
            Self::ReplaySlot1Rule
            | Self::ReplaySlot2Rule
            | Self::ReplaySlot3Rule
            | Self::ReplaySlot4Rule => "settings-entry-description-replay-slot-rule",
            Self::Language => "settings-entry-description-language",
            Self::ShowFps => "settings-entry-description-show-fps",
        }
    }
}

/// 設定値 1 ステップの増減量。
mod adjust;
mod entries;
mod support;
mod value;

pub use adjust::eight_key_hispeed_lane;
pub(crate) use entries::SettingsSnapshot;
pub use entries::{SettingsEntryId, adjust_settings_value, format_settings_value};
pub(crate) use support::{format_bottom_shiftable_gauge, format_hispeed_mode};
pub use value::settings_adjust_step;

use support::*;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::profile_config::{FloatingPolicyConfig, LaneEffectConfig, ProfileConfig};

    #[test]
    fn adjust_volume_clamps_to_range() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert!(profile.audio_mix.normalize_chart_volume);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::NormalizeChartVolume, 1));
        assert!(!profile.audio_mix.normalize_chart_volume);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::NormalizeChartVolume), "OFF");
        assert!(profile.audio_mix.normalize_system_bgm_volume);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::NormalizeSystemBgmVolume, 1,));
        assert!(!profile.audio_mix.normalize_system_bgm_volume);
        assert_eq!(
            format_settings_value(&profile, SettingsEntryId::NormalizeSystemBgmVolume),
            "OFF"
        );
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

    #[test]
    fn visual_offset_auto_adjust_toggles() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert!(SettingsEntryId::JUDGE_ENTRIES.contains(&SettingsEntryId::VisualOffsetAutoAdjust));
        assert_eq!(format_settings_value(&profile, SettingsEntryId::VisualOffsetAutoAdjust), "OFF");
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::VisualOffsetAutoAdjust, 1));
        assert!(profile.judge.visual_offset_auto_adjust);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::VisualOffsetAutoAdjust), "ON");
    }

    #[test]
    fn cycle_judge_algorithm_uses_beatoraja_order() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);

        assert_eq!(format_settings_value(&profile, SettingsEntryId::JudgeAlgorithm), "COMBO");
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::JudgeAlgorithm, 1));
        assert_eq!(profile.judge.judge_algorithm, JudgeAlgorithmConfig::Duration);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::JudgeAlgorithm, 1));
        assert_eq!(profile.judge.judge_algorithm, JudgeAlgorithmConfig::Lowest);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::JudgeAlgorithm, 1));
        assert_eq!(profile.judge.judge_algorithm, JudgeAlgorithmConfig::Combo);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::JudgeAlgorithm), "COMBO");
    }

    #[test]
    fn adjust_classic_hispeed_uses_classic_step() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::Hispeed, 1));
        assert!((profile.lane.hispeed - 2.25).abs() < f32::EPSILON);

        profile.lane.floating_policy = FloatingPolicyConfig::Locked;
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::Hispeed, 1));
        assert!((profile.lane.hispeed - 2.50).abs() < f32::EPSILON);
    }

    #[test]
    fn adjust_hispeed_step_settings_increments_by_five_hundredths() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::ClassicHispeedStep), "0.25");
        assert_eq!(format_settings_value(&profile, SettingsEntryId::FloatingHispeedStep), "0.50");

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::ClassicHispeedStep, 1));
        assert!((profile.lane.classic_hispeed_step - 0.30).abs() < f32::EPSILON);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::FloatingHispeedStep, -1));
        assert!((profile.lane.floating_hispeed_step - 0.45).abs() < f32::EPSILON);
    }

    #[test]
    fn adjust_lane_cover_and_lift_keep_combined_range() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        profile.lane.sudden = 900;
        profile.lane.lift = 200;

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::Sudden, 1));
        assert_eq!(profile.lane.sudden, 800);

        profile.lane.sudden = 300;
        profile.lane.lift = 700;
        assert!(!adjust_settings_value(&mut profile, SettingsEntryId::Lift, 1));
        assert_eq!(profile.lane.lift, 700);
    }

    #[test]
    fn independent_cover_enable_entries_preserve_the_other_flag() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);

        assert_eq!(format_settings_value(&profile, SettingsEntryId::SuddenEnabled), "OFF");
        assert_eq!(format_settings_value(&profile, SettingsEntryId::HiddenEnabled), "OFF");
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::SuddenEnabled, 1));
        assert_eq!(profile.play.lane_effect, LaneEffectConfig::Sudden);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HiddenEnabled, 1));
        assert_eq!(profile.play.lane_effect, LaneEffectConfig::HiddenSudden);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::SuddenEnabled, -1));
        assert_eq!(profile.play.lane_effect, LaneEffectConfig::Hidden);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::HiddenEnabled), "ON");
    }

    #[test]
    fn cycle_gauge_wraps() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        profile.play.gauge = GaugeTypeConfig::Hazard;
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::Gauge, 1));
        assert_eq!(profile.play.gauge, GaugeTypeConfig::AutoShift);
    }

    #[test]
    fn cycle_rule_mode_and_format_value() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::RuleMode), "BEATORAJA");

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::RuleMode, 1));
        assert_eq!(profile.play.rule_mode, RuleMode::Lr2Oraja);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::RuleMode), "LR2ORAJA");

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::RuleMode, 1));
        assert_eq!(profile.play.rule_mode, RuleMode::Dx);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::RuleMode), "DX");
    }

    #[test]
    fn cycle_hs_fix_uses_beatoraja_order() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::HsFix), "OFF");

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HsFix, 1));
        assert_eq!(profile.play.hs_fix, HsFixConfig::StartBpm);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HsFix, 1));
        assert_eq!(profile.play.hs_fix, HsFixConfig::MaxBpm);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HsFix, 1));
        assert_eq!(profile.play.hs_fix, HsFixConfig::MainBpm);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HsFix, 1));
        assert_eq!(profile.play.hs_fix, HsFixConfig::MinBpm);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HsFix, 1));
        assert_eq!(profile.play.hs_fix, HsFixConfig::Off);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HsFix, -1));
        assert_eq!(profile.play.hs_fix, HsFixConfig::MinBpm);
    }

    #[test]
    fn session_mode_cycles_and_mirrors_legacy_auto_play() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert!(!profile.play.auto_play);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::SessionMode, 1));
        assert_eq!(profile.play.session_mode, Some(SessionMode::Practice));
        assert!(!profile.play.auto_play);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::SessionMode, 1));
        assert_eq!(profile.play.session_mode, Some(SessionMode::Autoplay));
        assert!(profile.play.auto_play);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::SessionMode), "AUTOPLAY");
    }

    #[test]
    fn key_mode_conversion_clears_dp_option_and_cycles_conversion_details() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        profile.play.double_option = DoubleOptionConfig::Battle;

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::KeyModeConversion, 1));
        assert_eq!(profile.play.key_mode_conversion, KeyModeConversionConfig::SpToDp);
        assert_eq!(profile.play.double_option, DoubleOptionConfig::Off);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::KeyModeConversion), "SP TO DP");

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::SevenToNinePattern, 1));
        assert_eq!(profile.play.seven_to_nine_pattern, SevenToNinePattern::Sc9Key2To8);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::SevenToNineType, 1));
        assert_eq!(profile.play.seven_to_nine_type, SevenToNineType::NoMashing);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::SevenToNineRuleMode, 1));
        assert_eq!(profile.play.seven_to_nine_rule_mode, SevenToNineRuleMode::Keys9);
    }

    #[test]
    fn assist_entries_edit_individual_fields() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::AssistExpandJudge, 1));
        assert!(profile.play.assist.expand_judge);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::AssistScrollMode, 1));
        assert_eq!(profile.play.assist.scroll_mode, AssistScrollMode::Remove);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::AssistScrollRate, 5));
        assert!((profile.play.assist.scroll_rate - 0.55).abs() < f64::EPSILON);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::AssistScrollRate), "55%");
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::AssistKeyPgreatRate, -5));
        assert_eq!(profile.play.assist.key_pgreat_rate, 395);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::AssistKeyPgreatRate), "395%");
    }

    #[test]
    fn additional_scalar_settings_adjust_and_format() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::FastSlowDisplayScope, 1));
        assert_eq!(profile.judge.fast_slow_display_scope, FastSlowDisplayScope::ThresholdMs);
        assert!(adjust_settings_value(
            &mut profile,
            SettingsEntryId::FastSlowDisplayThresholdMs,
            5,
        ));
        assert_eq!(
            format_settings_value(&profile, SettingsEntryId::FastSlowDisplayThresholdMs),
            "5 ms"
        );

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::LiftEnabled, 1));
        assert!(!profile.lane.lift_enabled);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HispeedAutoAdjust, 1));
        assert!(!profile.lane.hispeed_auto_adjust);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::ReplayCompress, 1));
        assert!(profile.replay.compress);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::ShowFps, 1));
        assert!(profile.ui.show_fps);

        let initial_locale = profile.ui.locale();
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::Language, 1));
        assert_ne!(profile.ui.locale(), initial_locale);
        assert_eq!(
            format_settings_value(&profile, SettingsEntryId::Language),
            profile.ui.locale().native_name()
        );
    }

    #[test]
    fn cycle_ln_mode_policy_and_hispeed_mode() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::LnModePolicy), "AUTO(LN)");
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::LnModePolicy, 1));
        assert_eq!(profile.play.ln_mode_policy, crate::ln_policy::LnPolicySetting::AutoCn);

        assert_eq!(
            format_settings_value(&profile, SettingsEntryId::HispeedMode),
            "CLASSIC+FLOATING"
        );
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::HispeedMode, 1));
        assert_eq!(profile.lane.hispeed_config(), HispeedConfigPreset::Normal);
    }

    #[test]
    fn adjust_green_number_misslayer_and_analog_settings() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        profile.lane.target_green_number = 5_995;
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::TargetGreenNumber, 10));
        assert_eq!(profile.lane.target_green_number, 6_000);
        assert_eq!(
            format_settings_value(&profile, SettingsEntryId::NoteDisplayDurationMs),
            "10000 ms"
        );

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::NoteDisplayDurationMs, -10,));
        assert_eq!(profile.lane.target_green_number, 5_994);
        assert_eq!(
            format_settings_value(&profile, SettingsEntryId::NoteDisplayDurationMs),
            "9990 ms"
        );
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::Constant, 1));
        assert!(profile.lane.constant_enabled);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::GuideSe, 1));
        assert!(profile.play.guide_se);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::NoteRetention), "OFF");
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::NoteRetention, 1));
        assert!(profile.play.note_retention);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::NoteRetention), "ON");

        profile.play.misslayer_duration_ms = 4_980;
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::MisslayerDurationMs, 50));
        assert_eq!(profile.play.misslayer_duration_ms, 5_000);

        assert!(adjust_settings_value(
            &mut profile,
            SettingsEntryId::AnalogScratchSensitivity1P,
            1,
        ));
        assert!((profile.input.gamepad1.analog_scratch_sensitivity - 1.1).abs() < f32::EPSILON);

        profile.input.gamepad1.analog_scratch_threshold = 995;
        assert!(
            adjust_settings_value(&mut profile, SettingsEntryId::AnalogScratchThreshold1P, 10,)
        );
        assert_eq!(profile.input.gamepad1.analog_scratch_threshold, 1_000);
        assert_eq!(
            format_settings_value(&profile, SettingsEntryId::AnalogScratchThreshold1P),
            "1000 ticks"
        );

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::AnalogScratch2P, 1));
        assert!(!profile.input.gamepad2.analog_scratch);

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::KeyboardReleaseBounceMs, 5,));
        assert_eq!(profile.input.keyboard_release_bounce_ms, 5);
        assert_eq!(
            format_settings_value(&profile, SettingsEntryId::KeyboardReleaseBounceMs),
            "5 ms"
        );

        profile.input.controller_release_bounce_ms = RELEASE_BOUNCE_MS_MAX;
        assert!(!adjust_settings_value(
            &mut profile,
            SettingsEntryId::ControllerReleaseBounceMs,
            1,
        ));
        assert_eq!(profile.input.controller_release_bounce_ms, RELEASE_BOUNCE_MS_MAX);
    }

    #[test]
    fn cycle_input_and_replay_settings() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::SelectInputMode), "7K/14K");
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::SelectInputMode, 1));
        assert_eq!(
            profile.input.select_input_mode,
            crate::config::profile_config::SelectInputModeConfig::Key9
        );

        assert!(profile.replay.auto_save);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::ReplayAutoSave, 1));
        assert!(!profile.replay.auto_save);

        assert_eq!(
            format_settings_value(&profile, SettingsEntryId::ReplaySlot2Rule),
            "SCORE UPDATE"
        );
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::ReplaySlot2Rule, 1));
        assert_eq!(
            profile.replay.slot_rules[1],
            crate::config::profile_config::ReplaySlotRule::BpUpdate
        );
    }

    #[test]
    fn random_mix_settings_use_lr2_ranges_and_labels() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::RandomMixTargetLevel), "OFF");
        assert_eq!(
            format_settings_value(&profile, SettingsEntryId::RandomMixBpmRange),
            "+- 10 BPM"
        );
        assert_eq!(format_settings_value(&profile, SettingsEntryId::RandomMixStages), "5 STAGE");

        assert_eq!(settings_adjust_step(SettingsEntryId::RandomMixMaxBpm), 10);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::RandomMixTargetLevel, 120,));
        assert_eq!(profile.select.random_mix.target_level, 99);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::RandomMixMaxBpm, 1_000,));
        assert_eq!(profile.select.random_mix.max_bpm, 990);
        assert!(adjust_settings_value(&mut profile, SettingsEntryId::RandomMixStages, -5,));
        assert_eq!(format_settings_value(&profile, SettingsEntryId::RandomMixStages), "RANDOM");
    }

    #[test]
    fn difficulty_table_level_display_cycles_between_table_and_chart() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert_eq!(
            format_settings_value(&profile, SettingsEntryId::DifficultyTableLevelDisplay),
            "TABLE LEVEL"
        );

        assert!(adjust_settings_value(
            &mut profile,
            SettingsEntryId::DifficultyTableLevelDisplay,
            1,
        ));
        assert_eq!(
            profile.select.difficulty_table_level_display,
            DifficultyTableLevelDisplay::Chart
        );
        assert_eq!(
            format_settings_value(&profile, SettingsEntryId::DifficultyTableLevelDisplay),
            "CHART LEVEL"
        );
    }

    #[test]
    fn eight_key_hispeed_direction_rows_toggle_independently() {
        let mut profile = ProfileConfig::new_default("default", "Default", 0);
        assert_eq!(format_settings_value(&profile, SettingsEntryId::Hispeed8Key1), "UP");
        assert_eq!(format_settings_value(&profile, SettingsEntryId::Hispeed8Key2), "DOWN");

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::Hispeed8Key1, 1));
        assert_eq!(format_settings_value(&profile, SettingsEntryId::Hispeed8Key1), "DOWN");
        assert_eq!(format_settings_value(&profile, SettingsEntryId::Hispeed8Key2), "DOWN");
        assert_eq!(
            profile.input.play[KeyMode::K8.play_map_key()].hispeed.get(&LaneConfig::Key1),
            Some(&HispeedDirectionConfig::Down),
        );

        assert!(adjust_settings_value(&mut profile, SettingsEntryId::Hispeed8Key1, -1));
        assert!(profile.input.play[KeyMode::K8.play_map_key()].hispeed.is_empty());
    }
}
