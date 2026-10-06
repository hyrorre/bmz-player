use super::value::{format_random_mix_bpm, format_random_mix_level};
use super::*;
use crate::config::profile_config::LaneEffectConfig;

// One entry owns its value binding, adjustment and display. The same definition
// generates the edit snapshot, so adding a setting cannot omit its undo path.
macro_rules! read_setting {
    ($profile:ident, field $($field:tt)+) => { $profile.$($field)+ };
    ($profile:ident, cloned $($field:tt)+) => { $profile.$($field)+.clone() };
    ($profile:ident, access |$p:ident| $get:expr, |$q:ident, $v:ident| $set:block) => {{
        let $p = $profile;
        $get
    }};
}
macro_rules! restore_setting {
    ($profile:ident, $value:ident, field $($field:tt)+) => { $profile.$($field)+ = *$value };
    ($profile:ident, $value:ident, cloned $($field:tt)+) => { $profile.$($field)+.clone_from($value) };
    ($profile:ident, $value:ident, access |$p:ident| $get:expr, |$q:ident, $v:ident| $set:block) => {{
        let $q = $profile;
        let $v = $value;
        $set
    }};
}
macro_rules! define_settings {
    ($($name:ident($ty:ty) {
        value: [$($binding:tt)+],
        adjust: |$p:ident, $delta:ident| $adjust:expr,
        format: |$q:ident| $format:expr,
    })+) => {
        /// Profile settings editable from the in-game settings screen.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum SettingsEntryId { $($name,)+ }

        impl SettingsEntryId {
            pub const ALL: &'static [Self] = &[$(Self::$name,)+];

            pub(crate) fn capture(self, profile: &ProfileConfig) -> SettingsSnapshot {
                match self {
                    $(Self::$name => SettingsSnapshot::$name(read_setting!(profile, $($binding)+)),)+
                }
            }
        }

        #[derive(Debug, Clone)]
        pub(crate) enum SettingsSnapshot { $($name($ty),)+ }

        impl SettingsSnapshot {
            pub(crate) fn entry_id(&self) -> SettingsEntryId {
                match self { $(Self::$name(_) => SettingsEntryId::$name,)+ }
            }

            pub(crate) fn restore(&self, profile: &mut ProfileConfig) {
                match self {
                    $(Self::$name(value) => { restore_setting!(profile, value, $($binding)+); },)+
                }
            }
        }

        pub fn adjust_settings_value(profile: &mut ProfileConfig, id: SettingsEntryId, delta: i32) -> bool {
            if delta == 0 { return false; }
            match id {
                $(SettingsEntryId::$name => {
                    let $p = profile;
                    let $delta = delta;
                    $adjust
                },)+
            }
        }

        pub fn format_settings_value(profile: &ProfileConfig, id: SettingsEntryId) -> String {
            match id {
                $(SettingsEntryId::$name => { let $q = profile; $format },)+
            }
        }
    };
}

define_settings! {
    NormalizeChartVolume(bool) {
        value: [field audio_mix.normalize_chart_volume],
        adjust: |profile, _delta| {
            profile.audio_mix.normalize_chart_volume = !profile.audio_mix.normalize_chart_volume;
            true
        },
        format: |profile| {
            format_bool_on_off(profile.audio_mix.normalize_chart_volume)
        },
    }
    NormalizeSystemBgmVolume(bool) {
        value: [field audio_mix.normalize_system_bgm_volume],
        adjust: |profile, _delta| {
            profile.audio_mix.normalize_system_bgm_volume =
                !profile.audio_mix.normalize_system_bgm_volume;
            true
        },
        format: |profile| {
            format_bool_on_off(profile.audio_mix.normalize_system_bgm_volume)
        },
    }
    MasterVolume(u32) {
        value: [field audio_mix.master_volume],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.audio_mix.master_volume, delta, 0, 100)
        },
        format: |profile| format!("{}", profile.audio_mix.master_volume),
    }
    KeyVolume(u32) {
        value: [field audio_mix.key_volume],
        adjust: |profile, delta| adjust_u32(&mut profile.audio_mix.key_volume, delta, 0, 100),
        format: |profile| format!("{}", profile.audio_mix.key_volume),
    }
    BgmVolume(u32) {
        value: [field audio_mix.bgm_volume],
        adjust: |profile, delta| adjust_u32(&mut profile.audio_mix.bgm_volume, delta, 0, 100),
        format: |profile| format!("{}", profile.audio_mix.bgm_volume),
    }
    PreviewVolume(u32) {
        value: [field audio_mix.preview_volume],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.audio_mix.preview_volume, delta, 0, 100)
        },
        format: |profile| format!("{}", profile.audio_mix.preview_volume),
    }
    SystemBgmVolume(u32) {
        value: [field audio_mix.system_bgm_volume],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.audio_mix.system_bgm_volume, delta, 0, 100)
        },
        format: |profile| format!("{}", profile.audio_mix.system_bgm_volume),
    }
    SystemSeVolume(u32) {
        value: [field audio_mix.system_se_volume],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.audio_mix.system_se_volume, delta, 0, 100)
        },
        format: |profile| format!("{}", profile.audio_mix.system_se_volume),
    }
    InputOffsetMs(i64) {
        value: [field judge.input_offset_us],
        adjust: |profile, delta| {
            adjust_offset_ms(&mut profile.judge.input_offset_us, delta)
        },
        format: |profile| {
            format!("{} ms", profile.judge.input_offset_us / 1_000)
        },
    }
    VisualOffsetMs(i64) {
        value: [field judge.visual_offset_us],
        adjust: |profile, delta| {
            adjust_offset_ms(&mut profile.judge.visual_offset_us, delta)
        },
        format: |profile| {
            format!("{} ms", profile.judge.visual_offset_us / 1_000)
        },
    }
    VisualOffsetAutoAdjust(bool) {
        value: [field judge.visual_offset_auto_adjust],
        adjust: |profile, _delta| {
            profile.judge.visual_offset_auto_adjust = !profile.judge.visual_offset_auto_adjust;
            true
        },
        format: |profile| {
            format_bool_on_off(profile.judge.visual_offset_auto_adjust)
        },
    }
    JudgeAlgorithm(JudgeAlgorithmConfig) {
        value: [field judge.judge_algorithm],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.judge.judge_algorithm, cycle_judge_algorithm)
                .map(|next| profile.judge.judge_algorithm = next)
                .is_some()
        },
        format: |profile| format_judge_algorithm(profile.judge.judge_algorithm),
    }
    FastSlowDisplayScope(FastSlowDisplayScope) {
        value: [field judge.fast_slow_display_scope],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.judge.fast_slow_display_scope, cycle_fast_slow_display_scope)
                .map(|next| profile.judge.fast_slow_display_scope = next)
                .is_some()
        },
        format: |profile| match profile.judge.fast_slow_display_scope {
            FastSlowDisplayScope::Auto => "AUTO".to_string(),
            FastSlowDisplayScope::ThresholdMs => "THRESHOLD".to_string(),
        },
    }
    FastSlowDisplayThresholdMs(u32) {
        value: [field judge.fast_slow_display_threshold_ms],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.judge.fast_slow_display_threshold_ms, delta, 0, 50)
        },
        format: |profile| {
            format!("{} ms", profile.judge.fast_slow_display_threshold_ms)
        },
    }
    RuleMode(RuleMode) {
        value: [field play.rule_mode],
        adjust: |profile, delta| cycle_enum(delta, profile.play.rule_mode, cycle_rule_mode)
            .map(|next| profile.play.rule_mode = next)
            .is_some(),
        format: |profile| format_rule_mode(profile.play.rule_mode),
    }
    LnModePolicy(LnPolicySetting) {
        value: [field play.ln_mode_policy],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.play.ln_mode_policy, cycle_ln_mode_policy)
                .map(|next| profile.play.ln_mode_policy = next)
                .is_some()
        },
        format: |profile| profile.play.ln_mode_policy.display_label().to_string(),
    }
    Gauge(GaugeTypeConfig) {
        value: [field play.gauge],
        adjust: |profile, delta| cycle_enum(delta, profile.play.gauge, cycle_gauge)
            .map(|next| profile.play.gauge = next)
            .is_some(),
        format: |profile| format_gauge(profile.play.gauge),
    }
    GaugeAutoShift(GaugeAutoShiftConfig) {
        value: [field play.gauge_auto_shift],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.play.gauge_auto_shift, cycle_gauge_auto_shift)
                .map(|next| profile.play.gauge_auto_shift = next)
                .is_some()
        },
        format: |profile| format_gauge_auto_shift(profile.play.gauge_auto_shift),
    }
    BottomShiftableGauge(BottomShiftableGaugeConfig) {
        value: [field play.bottom_shiftable_gauge],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.play.bottom_shiftable_gauge, cycle_bottom_shiftable_gauge)
                .map(|next| profile.play.bottom_shiftable_gauge = next)
                .is_some()
        },
        format: |profile| {
            format_bottom_shiftable_gauge(profile.play.bottom_shiftable_gauge)
        },
    }
    Random(RandomOptionConfig) {
        value: [field play.random],
        adjust: |profile, delta| cycle_enum(delta, profile.play.random, cycle_random)
            .map(|next| profile.play.random = next)
            .is_some(),
        format: |profile| format_random(profile.play.random),
    }
    Random2(RandomOptionConfig) {
        value: [field play.random2],
        adjust: |profile, delta| cycle_enum(delta, profile.play.random2, cycle_random)
            .map(|next| profile.play.random2 = next)
            .is_some(),
        format: |profile| format_random(profile.play.random2),
    }
    DoubleOption(DoubleOptionConfig) {
        value: [field play.double_option],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.play.double_option, cycle_double_option)
                .map(|next| profile.play.double_option = next)
                .is_some()
        },
        format: |profile| format_double_option(profile.play.double_option),
    }
    HsFix(HsFixConfig) {
        value: [field play.hs_fix],
        adjust: |profile, delta| cycle_enum(delta, profile.play.hs_fix, cycle_hs_fix)
            .map(|next| profile.play.hs_fix = next)
            .is_some(),
        format: |profile| format_hs_fix(profile.play.hs_fix),
    }
    Target(TargetOptionConfig) {
        value: [field play.target],
        adjust: |profile, delta| cycle_enum(delta, profile.play.target, cycle_target)
            .map(|next| profile.play.target = next)
            .is_some(),
        format: |profile| format_target(profile.play.target),
    }
    Assist(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| cycle_enum(delta, profile.play.assist, cycle_assist)
            .map(|next| profile.play.assist = next)
            .is_some(),
        format: |profile| format_assist(profile.play.assist),
    }
    BgaMode(BgaModeConfig) {
        value: [field play.bga],
        adjust: |profile, delta| cycle_enum(delta, profile.play.bga, cycle_bga_mode)
            .map(|next| profile.play.bga = next)
            .is_some(),
        format: |profile| format_bga_mode(profile.play.bga),
    }
    BgaExpand(BgaExpandConfig) {
        value: [field play.bga_expand],
        adjust: |profile, delta| cycle_enum(delta, profile.play.bga_expand, cycle_bga_expand)
            .map(|next| profile.play.bga_expand = next)
            .is_some(),
        format: |profile| format_bga_expand(profile.play.bga_expand),
    }
    SessionMode((Option<SessionMode>, bool)) {
        value: [access |profile| (profile.play.session_mode, profile.play.auto_play), |profile, value| { profile.play.session_mode = value.0; profile.play.auto_play = value.1; }],
        adjust: |profile, delta| {
            let current = profile.play.session_mode.unwrap_or(if profile.play.auto_play {
                SessionMode::Autoplay
            } else {
                SessionMode::Normal
            });
            cycle_enum(delta, current, cycle_session_mode)
                .map(|next| {
                    profile.play.session_mode = Some(next);
                    profile.play.auto_play = next.primary_autoplay();
                })
                .is_some()
        },
        format: |profile| profile
            .play
            .session_mode
            .unwrap_or(if profile.play.auto_play {
                SessionMode::Autoplay
            } else {
                SessionMode::Normal
            })
            .as_str()
            .to_string(),
    }
    KeyModeConversion((KeyModeConversionConfig, DoubleOptionConfig)) {
        value: [access |profile| (profile.play.key_mode_conversion, profile.play.double_option), |profile, value| { profile.play.key_mode_conversion = value.0; profile.play.double_option = value.1; }],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.play.key_mode_conversion, cycle_key_mode_conversion)
                .map(|next| {
                    profile.play.key_mode_conversion = next;
                    if next != KeyModeConversionConfig::Off {
                        profile.play.double_option = DoubleOptionConfig::Off;
                    }
                })
                .is_some()
        },
        format: |profile| profile.play.key_mode_conversion.as_str().to_string(),
    }
    SevenToNinePattern(SevenToNinePattern) {
        value: [field play.seven_to_nine_pattern],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.play.seven_to_nine_pattern, cycle_seven_to_nine_pattern)
                .map(|next| profile.play.seven_to_nine_pattern = next)
                .is_some()
        },
        format: |profile| {
            profile.play.seven_to_nine_pattern.label().to_string()
        },
    }
    SevenToNineType(SevenToNineType) {
        value: [field play.seven_to_nine_type],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.play.seven_to_nine_type, cycle_seven_to_nine_type)
                .map(|next| profile.play.seven_to_nine_type = next)
                .is_some()
        },
        format: |profile| profile.play.seven_to_nine_type.label().to_string(),
    }
    SevenToNineRuleMode(SevenToNineRuleMode) {
        value: [field play.seven_to_nine_rule_mode],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.play.seven_to_nine_rule_mode, cycle_seven_to_nine_rule_mode)
                .map(|next| profile.play.seven_to_nine_rule_mode = next)
                .is_some()
        },
        format: |profile| {
            profile.play.seven_to_nine_rule_mode.as_str().to_string()
        },
    }
    PlayExitHoldMs(u32) {
        value: [field play.play_exit_hold_ms],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.play.play_exit_hold_ms, delta, 100, 5000)
        },
        format: |profile| format!("{} ms", profile.play.play_exit_hold_ms),
    }
    AssistExpandJudge(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, _delta| {
            profile.play.assist.expand_judge = !profile.play.assist.expand_judge;
            true
        },
        format: |profile| format_bool_on_off(profile.play.assist.expand_judge),
    }
    AssistJudgeArea(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, _delta| {
            profile.play.assist.judge_area = !profile.play.assist.judge_area;
            true
        },
        format: |profile| format_bool_on_off(profile.play.assist.judge_area),
    }
    AssistMarkNote(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, _delta| {
            profile.play.assist.mark_note = !profile.play.assist.mark_note;
            true
        },
        format: |profile| format_bool_on_off(profile.play.assist.mark_note),
    }
    AssistBpmGuide(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, _delta| {
            profile.play.assist.bpm_guide = !profile.play.assist.bpm_guide;
            true
        },
        format: |profile| format_bool_on_off(profile.play.assist.bpm_guide),
    }
    AssistScrollMode(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.play.assist.scroll_mode, cycle_assist_scroll_mode)
                .map(|next| profile.play.assist.scroll_mode = next)
                .is_some()
        },
        format: |profile| match profile.play.assist.scroll_mode {
            AssistScrollMode::Off => "OFF",
            AssistScrollMode::Remove => "REMOVE (CONSTANT)",
            AssistScrollMode::Add => "ADD",
        }
        .to_string(),
    }
    AssistLongNoteMode(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.play.assist.long_note_mode, cycle_assist_long_note_mode)
                .map(|next| profile.play.assist.long_note_mode = next)
                .is_some()
        },
        format: |profile| match profile.play.assist.long_note_mode {
            AssistLongNoteMode::Off => "OFF",
            AssistLongNoteMode::Remove => "REMOVE (LEGACY NOTE)",
            AssistLongNoteMode::AddLn => "ADD LN",
            AssistLongNoteMode::AddCn => "ADD CN",
            AssistLongNoteMode::AddHcn => "ADD HCN",
            AssistLongNoteMode::AddAll => "ADD ALL",
        }
        .to_string(),
    }
    AssistMineMode(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.play.assist.mine_mode, cycle_assist_mine_mode)
                .map(|next| profile.play.assist.mine_mode = next)
                .is_some()
        },
        format: |profile| match profile.play.assist.mine_mode {
            AssistMineMode::Off => "OFF",
            AssistMineMode::Remove => "REMOVE (NO MINE)",
            AssistMineMode::AddRandom => "ADD RANDOM",
            AssistMineMode::AddNear => "ADD NEAR",
            AssistMineMode::AddBlank => "ADD BLANK",
        }
        .to_string(),
    }
    AssistScrollSection(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            adjust_u16(&mut profile.play.assist.scroll_section, delta, 1, 64)
        },
        format: |profile| profile.play.assist.scroll_section.to_string(),
    }
    AssistScrollRate(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            adjust_f64_percent(&mut profile.play.assist.scroll_rate, delta, 0.0, 1.0)
        },
        format: |profile| {
            format!("{}%", (profile.play.assist.scroll_rate * 100.0).round() as i32)
        },
    }
    AssistLongNoteRate(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            adjust_f64_percent(&mut profile.play.assist.long_note_rate, delta, 0.0, 1.0)
        },
        format: |profile| {
            format!("{}%", (profile.play.assist.long_note_rate * 100.0).round() as i32)
        },
    }
    AssistExtraNoteDepth(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            adjust_u8(&mut profile.play.assist.extra_note_depth, delta, 0, 16)
        },
        format: |profile| profile.play.assist.extra_note_depth.to_string(),
    }
    AssistExtraNoteScratch(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, _delta| {
            profile.play.assist.extra_note_scratch = !profile.play.assist.extra_note_scratch;
            true
        },
        format: |profile| {
            format_bool_on_off(profile.play.assist.extra_note_scratch)
        },
    }
    AssistExtraNoteType(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            adjust_u8(&mut profile.play.assist.extra_note_type, delta, 0, 2)
        },
        format: |profile| profile.play.assist.extra_note_type.to_string(),
    }
    AssistKeyPgreatRate(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            adjust_u16(&mut profile.play.assist.key_pgreat_rate, delta, 0, 400)
        },
        format: |profile| {
            format!("{}%", profile.play.assist.key_pgreat_rate)
        },
    }
    AssistKeyGreatRate(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            adjust_u16(&mut profile.play.assist.key_great_rate, delta, 0, 400)
        },
        format: |profile| {
            format!("{}%", profile.play.assist.key_great_rate)
        },
    }
    AssistKeyGoodRate(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            adjust_u16(&mut profile.play.assist.key_good_rate, delta, 0, 400)
        },
        format: |profile| {
            format!("{}%", profile.play.assist.key_good_rate)
        },
    }
    AssistScratchPgreatRate(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            adjust_u16(&mut profile.play.assist.scratch_pgreat_rate, delta, 0, 400)
        },
        format: |profile| {
            format!("{}%", profile.play.assist.scratch_pgreat_rate)
        },
    }
    AssistScratchGreatRate(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            adjust_u16(&mut profile.play.assist.scratch_great_rate, delta, 0, 400)
        },
        format: |profile| {
            format!("{}%", profile.play.assist.scratch_great_rate)
        },
    }
    AssistScratchGoodRate(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            adjust_u16(&mut profile.play.assist.scratch_good_rate, delta, 0, 400)
        },
        format: |profile| {
            format!("{}%", profile.play.assist.scratch_good_rate)
        },
    }
    AssistLongNoteMarginRate(AssistOptionConfig) {
        value: [field play.assist],
        adjust: |profile, delta| {
            adjust_u16(&mut profile.play.assist.long_note_margin_rate, delta, 0, 400)
        },
        format: |profile| {
            format!("{}%", profile.play.assist.long_note_margin_rate)
        },
    }
    MisslayerDurationMs(u32) {
        value: [field play.misslayer_duration_ms],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.play.misslayer_duration_ms, delta, 0, 5000)
        },
        format: |profile| {
            format!("{} ms", profile.play.misslayer_duration_ms)
        },
    }
    WaitAllNotesResult(bool) {
        value: [field play.wait_all_notes_result],
        adjust: |profile, _delta| {
            profile.play.wait_all_notes_result = !profile.play.wait_all_notes_result;
            true
        },
        format: |profile| format_bool_on_off(profile.play.wait_all_notes_result),
    }
    HideMisslayerOnGood(bool) {
        value: [field play.hide_misslayer_on_good],
        adjust: |profile, _delta| {
            profile.play.hide_misslayer_on_good = !profile.play.hide_misslayer_on_good;
            true
        },
        format: |profile| format_bool_on_off(profile.play.hide_misslayer_on_good),
    }
    PlayKeysoundOnMiss(bool) {
        value: [field play.play_keysound_on_miss],
        adjust: |profile, _delta| {
            profile.play.play_keysound_on_miss = !profile.play.play_keysound_on_miss;
            true
        },
        format: |profile| format_bool_on_off(profile.play.play_keysound_on_miss),
    }
    NoteRetention(bool) {
        value: [field play.note_retention],
        adjust: |profile, _delta| {
                profile.play.note_retention = !profile.play.note_retention;
                true
            },
        format: |profile| format_bool_on_off(profile.play.note_retention),
    }
    ShowLnTailCap(bool) {
        value: [field play.show_ln_tail_cap],
        adjust: |profile, _delta| {
                profile.play.show_ln_tail_cap = !profile.play.show_ln_tail_cap;
                true
            },
        format: |profile| format_bool_on_off(profile.play.show_ln_tail_cap),
    }
    GuideSe(bool) {
        value: [field play.guide_se],
        adjust: |profile, _delta| {
            profile.play.guide_se = !profile.play.guide_se;
            true
        },
        format: |profile| format_bool_on_off(profile.play.guide_se),
    }
    Hispeed(f32) {
        value: [field lane.hispeed],
        adjust: |profile, delta| adjust_hispeed(
            &mut profile.lane.hispeed,
            delta,
            profile.lane.classic_hispeed_step,
            default_classic_hispeed_step(),
        ),
        format: |profile| format!("{:.2}", profile.lane.hispeed),
    }
    HispeedMode(HispeedConfigPreset) {
        value: [access |profile| profile.lane.hispeed_config(), |profile, value| {
                profile.lane.set_hispeed_config(*value);
            }],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.lane.hispeed_config(), cycle_hispeed_mode)
                .map(|next| profile.lane.set_hispeed_config(next))
                .is_some()
        },
        format: |profile| format_hispeed_mode(profile.lane.hispeed_config()),
    }
    NormalHispeedLevel(u32) {
        value: [access |profile| u32::from(profile.lane.normal_hispeed_level), |profile, value| {
                profile.lane.normal_hispeed_level = *value as u8;
            }],
        adjust: |profile, delta| {
            let before = profile.lane.normal_hispeed_level;
            profile.lane.normal_hispeed_level = (i32::from(before) + delta).clamp(
                i32::from(crate::config::play::NORMAL_HISPEED_LEVEL_MIN),
                i32::from(crate::config::play::NORMAL_HISPEED_LEVEL_MAX),
            ) as u8;
            profile.lane.normal_hispeed_level != before
        },
        format: |profile| {
            format!("{}", profile.lane.normal_hispeed_level)
        },
    }
    ClassicHispeedStep(f32) {
        value: [field lane.classic_hispeed_step],
        adjust: |profile, delta| {
            adjust_hispeed_step(&mut profile.lane.classic_hispeed_step, delta)
        },
        format: |profile| format!("{:.2}", profile.lane.classic_hispeed_step),
    }
    FloatingHispeedStep(f32) {
        value: [field lane.floating_hispeed_step],
        adjust: |profile, delta| {
            adjust_hispeed_step(&mut profile.lane.floating_hispeed_step, delta)
        },
        format: |profile| {
            format!("{:.2}", profile.lane.floating_hispeed_step)
        },
    }
    SuddenEnabled(LaneEffectConfig) {
        value: [field play.lane_effect],
        adjust: |profile, _delta| {
            profile.play.lane_effect = profile
                .play
                .lane_effect
                .with_sudden_enabled(!profile.play.lane_effect.sudden_enabled());
            true
        },
        format: |profile| {
            format_bool_on_off(profile.play.lane_effect.sudden_enabled())
        },
    }
    Sudden(u32) {
        value: [field lane.sudden],
        adjust: |profile, delta| adjust_u32(
            &mut profile.lane.sudden,
            delta,
            0,
            crate::config::play::lane_unit_max_for_other(profile.lane.lift),
        ),
        format: |profile| format_lane_unit(profile.lane.sudden),
    }
    LiftEnabled(bool) {
        value: [field lane.lift_enabled],
        adjust: |profile, _delta| {
            profile.lane.lift_enabled = !profile.lane.lift_enabled;
            true
        },
        format: |profile| format_bool_on_off(profile.lane.lift_enabled),
    }
    Lift(u32) {
        value: [field lane.lift],
        adjust: |profile, delta| adjust_u32(
            &mut profile.lane.lift,
            delta,
            0,
            crate::config::play::lane_unit_max_for_other(profile.lane.sudden),
        ),
        format: |profile| format_lane_unit(profile.lane.lift),
    }
    HispeedAutoAdjust(bool) {
        value: [field lane.hispeed_auto_adjust],
        adjust: |profile, _delta| {
            profile.lane.hispeed_auto_adjust = !profile.lane.hispeed_auto_adjust;
            true
        },
        format: |profile| format_bool_on_off(profile.lane.hispeed_auto_adjust),
    }
    HiddenEnabled(LaneEffectConfig) {
        value: [field play.lane_effect],
        adjust: |profile, _delta| {
            profile.play.lane_effect = profile
                .play
                .lane_effect
                .with_hidden_enabled(!profile.play.lane_effect.hidden_enabled());
            true
        },
        format: |profile| {
            format_bool_on_off(profile.play.lane_effect.hidden_enabled())
        },
    }
    Hidden(u32) {
        value: [field lane.hidden],
        adjust: |profile, delta| adjust_u32(&mut profile.lane.hidden, delta, 0, 1000),
        format: |profile| format_lane_unit(profile.lane.hidden),
    }
    TargetGreenNumber(u32) {
        value: [field lane.target_green_number],
        adjust: |profile, delta| adjust_u32(
            &mut profile.lane.target_green_number,
            delta,
            TARGET_GREEN_NUMBER_MIN,
            TARGET_GREEN_NUMBER_MAX,
        ),
        format: |profile| format!("{}", profile.lane.target_green_number),
    }
    NoteDisplayDurationMs(u32) {
        value: [field lane.target_green_number],
        adjust: |profile, delta| {
            let current = profile.lane.target_green_number;
            let next = crate::config::play::adjust_green_number_by_duration_ms(current, delta);
            profile.lane.target_green_number = next;
            next != current
        },
        format: |profile| {
            format!(
                "{} ms",
                crate::config::play::duration_ms_from_green_number(
                    profile.lane.target_green_number.max(1),
                )
            )
        },
    }
    Constant(bool) {
        value: [field lane.constant_enabled],
        adjust: |profile, _delta| {
            profile.lane.constant_enabled = !profile.lane.constant_enabled;
            true
        },
        format: |profile| format_bool_on_off(profile.lane.constant_enabled),
    }
    ConstantFadeMs(i32) {
        value: [field lane.constant_fade_ms],
        adjust: |profile, delta| {
            let before = profile.lane.constant_fade_ms;
            profile.lane.constant_fade_ms = profile
                .lane
                .constant_fade_ms
                .saturating_add(delta)
                .clamp(CONSTANT_FADE_MIN_MS, CONSTANT_FADE_MAX_MS);
            profile.lane.constant_fade_ms != before
        },
        format: |profile| format!("{} ms", profile.lane.constant_fade_ms),
    }
    SelectInputMode(SelectInputModeConfig) {
        value: [field input.select_input_mode],
        adjust: |profile, delta| {
            cycle_enum(delta, profile.input.select_input_mode, cycle_select_input_mode)
                .map(|next| profile.input.select_input_mode = next)
                .is_some()
        },
        format: |profile| {
            profile.input.select_input_mode.display_label().to_string()
        },
    }
    AnalogScratch1P(bool) {
        value: [field input.gamepad1.analog_scratch],
        adjust: |profile, _delta| {
            profile.input.gamepad1.analog_scratch = !profile.input.gamepad1.analog_scratch;
            true
        },
        format: |profile| {
            format_bool_on_off(profile.input.gamepad1.analog_scratch)
        },
    }
    AnalogScratchSensitivity1P(f32) {
        value: [field input.gamepad1.analog_scratch_sensitivity],
        adjust: |profile, delta| adjust_f32_tenths(
            &mut profile.input.gamepad1.analog_scratch_sensitivity,
            delta,
            0.1,
            5.0,
        ),
        format: |profile| {
            format!("{:.1}", profile.input.gamepad1.analog_scratch_sensitivity)
        },
    }
    AnalogScratchThreshold1P(u32) {
        value: [field input.gamepad1.analog_scratch_threshold],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.input.gamepad1.analog_scratch_threshold, delta, 1, 1000)
        },
        format: |profile| {
            format!("{} ticks", profile.input.gamepad1.analog_scratch_threshold)
        },
    }
    AnalogScratch2P(bool) {
        value: [field input.gamepad2.analog_scratch],
        adjust: |profile, _delta| {
            profile.input.gamepad2.analog_scratch = !profile.input.gamepad2.analog_scratch;
            true
        },
        format: |profile| {
            format_bool_on_off(profile.input.gamepad2.analog_scratch)
        },
    }
    AnalogScratchSensitivity2P(f32) {
        value: [field input.gamepad2.analog_scratch_sensitivity],
        adjust: |profile, delta| adjust_f32_tenths(
            &mut profile.input.gamepad2.analog_scratch_sensitivity,
            delta,
            0.1,
            5.0,
        ),
        format: |profile| {
            format!("{:.1}", profile.input.gamepad2.analog_scratch_sensitivity)
        },
    }
    AnalogScratchThreshold2P(u32) {
        value: [field input.gamepad2.analog_scratch_threshold],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.input.gamepad2.analog_scratch_threshold, delta, 1, 1000)
        },
        format: |profile| {
            format!("{} ticks", profile.input.gamepad2.analog_scratch_threshold)
        },
    }
    AnalogTicksPerScroll(u32) {
        value: [field input.analog_ticks_per_scroll],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.input.analog_ticks_per_scroll, delta, 1, 100)
        },
        format: |profile| {
            format!("{} ticks", profile.input.analog_ticks_per_scroll)
        },
    }
    KeyboardReleaseBounceMs(u32) {
        value: [field input.keyboard_release_bounce_ms],
        adjust: |profile, delta| adjust_u32(
            &mut profile.input.keyboard_release_bounce_ms,
            delta,
            0,
            RELEASE_BOUNCE_MS_MAX,
        ),
        format: |profile| {
            format!("{} ms", profile.input.keyboard_release_bounce_ms)
        },
    }
    ControllerReleaseBounceMs(u32) {
        value: [field input.controller_release_bounce_ms],
        adjust: |profile, delta| adjust_u32(
            &mut profile.input.controller_release_bounce_ms,
            delta,
            0,
            RELEASE_BOUNCE_MS_MAX,
        ),
        format: |profile| {
            format!("{} ms", profile.input.controller_release_bounce_ms)
        },
    }
    Hispeed8Key1(HispeedDirectionConfig) {
        value: [access |profile| crate::config::play_input::hispeed_direction_for_lane(&profile.input, KeyMode::K8, crate::config::play::lane_from_config(LaneConfig::Key1)).expect("8K key lane has a hispeed direction"), |profile, value| { crate::config::play_input::set_eight_key_hispeed_direction(&mut profile.input, LaneConfig::Key1, *value); }],
        adjust: |profile, _delta| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key1).expect("guarded 8K hispeed setting");
            let current = crate::config::play_input::hispeed_direction_for_lane(
                &profile.input,
                KeyMode::K8,
                crate::config::play::lane_from_config(lane),
            )
            .expect("8K key lane has a hispeed direction");
            let next = match current {
                HispeedDirectionConfig::Down => HispeedDirectionConfig::Up,
                HispeedDirectionConfig::Up => HispeedDirectionConfig::Down,
            };
            crate::config::play_input::set_eight_key_hispeed_direction(
                &mut profile.input,
                lane,
                next,
            )
        },
        format: |profile| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key1).expect("guarded 8K hispeed setting");
            format_hispeed_direction(
                crate::config::play_input::hispeed_direction_for_lane(
                    &profile.input,
                    KeyMode::K8,
                    crate::config::play::lane_from_config(lane),
                )
                .expect("8K key lane has a hispeed direction"),
            )
        },
    }
    Hispeed8Key2(HispeedDirectionConfig) {
        value: [access |profile| crate::config::play_input::hispeed_direction_for_lane(&profile.input, KeyMode::K8, crate::config::play::lane_from_config(LaneConfig::Key2)).expect("8K key lane has a hispeed direction"), |profile, value| { crate::config::play_input::set_eight_key_hispeed_direction(&mut profile.input, LaneConfig::Key2, *value); }],
        adjust: |profile, _delta| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key2).expect("guarded 8K hispeed setting");
            let current = crate::config::play_input::hispeed_direction_for_lane(
                &profile.input,
                KeyMode::K8,
                crate::config::play::lane_from_config(lane),
            )
            .expect("8K key lane has a hispeed direction");
            let next = match current {
                HispeedDirectionConfig::Down => HispeedDirectionConfig::Up,
                HispeedDirectionConfig::Up => HispeedDirectionConfig::Down,
            };
            crate::config::play_input::set_eight_key_hispeed_direction(
                &mut profile.input,
                lane,
                next,
            )
        },
        format: |profile| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key2).expect("guarded 8K hispeed setting");
            format_hispeed_direction(
                crate::config::play_input::hispeed_direction_for_lane(
                    &profile.input,
                    KeyMode::K8,
                    crate::config::play::lane_from_config(lane),
                )
                .expect("8K key lane has a hispeed direction"),
            )
        },
    }
    Hispeed8Key3(HispeedDirectionConfig) {
        value: [access |profile| crate::config::play_input::hispeed_direction_for_lane(&profile.input, KeyMode::K8, crate::config::play::lane_from_config(LaneConfig::Key3)).expect("8K key lane has a hispeed direction"), |profile, value| { crate::config::play_input::set_eight_key_hispeed_direction(&mut profile.input, LaneConfig::Key3, *value); }],
        adjust: |profile, _delta| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key3).expect("guarded 8K hispeed setting");
            let current = crate::config::play_input::hispeed_direction_for_lane(
                &profile.input,
                KeyMode::K8,
                crate::config::play::lane_from_config(lane),
            )
            .expect("8K key lane has a hispeed direction");
            let next = match current {
                HispeedDirectionConfig::Down => HispeedDirectionConfig::Up,
                HispeedDirectionConfig::Up => HispeedDirectionConfig::Down,
            };
            crate::config::play_input::set_eight_key_hispeed_direction(
                &mut profile.input,
                lane,
                next,
            )
        },
        format: |profile| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key3).expect("guarded 8K hispeed setting");
            format_hispeed_direction(
                crate::config::play_input::hispeed_direction_for_lane(
                    &profile.input,
                    KeyMode::K8,
                    crate::config::play::lane_from_config(lane),
                )
                .expect("8K key lane has a hispeed direction"),
            )
        },
    }
    Hispeed8Key4(HispeedDirectionConfig) {
        value: [access |profile| crate::config::play_input::hispeed_direction_for_lane(&profile.input, KeyMode::K8, crate::config::play::lane_from_config(LaneConfig::Key4)).expect("8K key lane has a hispeed direction"), |profile, value| { crate::config::play_input::set_eight_key_hispeed_direction(&mut profile.input, LaneConfig::Key4, *value); }],
        adjust: |profile, _delta| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key4).expect("guarded 8K hispeed setting");
            let current = crate::config::play_input::hispeed_direction_for_lane(
                &profile.input,
                KeyMode::K8,
                crate::config::play::lane_from_config(lane),
            )
            .expect("8K key lane has a hispeed direction");
            let next = match current {
                HispeedDirectionConfig::Down => HispeedDirectionConfig::Up,
                HispeedDirectionConfig::Up => HispeedDirectionConfig::Down,
            };
            crate::config::play_input::set_eight_key_hispeed_direction(
                &mut profile.input,
                lane,
                next,
            )
        },
        format: |profile| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key4).expect("guarded 8K hispeed setting");
            format_hispeed_direction(
                crate::config::play_input::hispeed_direction_for_lane(
                    &profile.input,
                    KeyMode::K8,
                    crate::config::play::lane_from_config(lane),
                )
                .expect("8K key lane has a hispeed direction"),
            )
        },
    }
    Hispeed8Key5(HispeedDirectionConfig) {
        value: [access |profile| crate::config::play_input::hispeed_direction_for_lane(&profile.input, KeyMode::K8, crate::config::play::lane_from_config(LaneConfig::Key5)).expect("8K key lane has a hispeed direction"), |profile, value| { crate::config::play_input::set_eight_key_hispeed_direction(&mut profile.input, LaneConfig::Key5, *value); }],
        adjust: |profile, _delta| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key5).expect("guarded 8K hispeed setting");
            let current = crate::config::play_input::hispeed_direction_for_lane(
                &profile.input,
                KeyMode::K8,
                crate::config::play::lane_from_config(lane),
            )
            .expect("8K key lane has a hispeed direction");
            let next = match current {
                HispeedDirectionConfig::Down => HispeedDirectionConfig::Up,
                HispeedDirectionConfig::Up => HispeedDirectionConfig::Down,
            };
            crate::config::play_input::set_eight_key_hispeed_direction(
                &mut profile.input,
                lane,
                next,
            )
        },
        format: |profile| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key5).expect("guarded 8K hispeed setting");
            format_hispeed_direction(
                crate::config::play_input::hispeed_direction_for_lane(
                    &profile.input,
                    KeyMode::K8,
                    crate::config::play::lane_from_config(lane),
                )
                .expect("8K key lane has a hispeed direction"),
            )
        },
    }
    Hispeed8Key6(HispeedDirectionConfig) {
        value: [access |profile| crate::config::play_input::hispeed_direction_for_lane(&profile.input, KeyMode::K8, crate::config::play::lane_from_config(LaneConfig::Key6)).expect("8K key lane has a hispeed direction"), |profile, value| { crate::config::play_input::set_eight_key_hispeed_direction(&mut profile.input, LaneConfig::Key6, *value); }],
        adjust: |profile, _delta| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key6).expect("guarded 8K hispeed setting");
            let current = crate::config::play_input::hispeed_direction_for_lane(
                &profile.input,
                KeyMode::K8,
                crate::config::play::lane_from_config(lane),
            )
            .expect("8K key lane has a hispeed direction");
            let next = match current {
                HispeedDirectionConfig::Down => HispeedDirectionConfig::Up,
                HispeedDirectionConfig::Up => HispeedDirectionConfig::Down,
            };
            crate::config::play_input::set_eight_key_hispeed_direction(
                &mut profile.input,
                lane,
                next,
            )
        },
        format: |profile| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key6).expect("guarded 8K hispeed setting");
            format_hispeed_direction(
                crate::config::play_input::hispeed_direction_for_lane(
                    &profile.input,
                    KeyMode::K8,
                    crate::config::play::lane_from_config(lane),
                )
                .expect("8K key lane has a hispeed direction"),
            )
        },
    }
    Hispeed8Key7(HispeedDirectionConfig) {
        value: [access |profile| crate::config::play_input::hispeed_direction_for_lane(&profile.input, KeyMode::K8, crate::config::play::lane_from_config(LaneConfig::Key7)).expect("8K key lane has a hispeed direction"), |profile, value| { crate::config::play_input::set_eight_key_hispeed_direction(&mut profile.input, LaneConfig::Key7, *value); }],
        adjust: |profile, _delta| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key7).expect("guarded 8K hispeed setting");
            let current = crate::config::play_input::hispeed_direction_for_lane(
                &profile.input,
                KeyMode::K8,
                crate::config::play::lane_from_config(lane),
            )
            .expect("8K key lane has a hispeed direction");
            let next = match current {
                HispeedDirectionConfig::Down => HispeedDirectionConfig::Up,
                HispeedDirectionConfig::Up => HispeedDirectionConfig::Down,
            };
            crate::config::play_input::set_eight_key_hispeed_direction(
                &mut profile.input,
                lane,
                next,
            )
        },
        format: |profile| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key7).expect("guarded 8K hispeed setting");
            format_hispeed_direction(
                crate::config::play_input::hispeed_direction_for_lane(
                    &profile.input,
                    KeyMode::K8,
                    crate::config::play::lane_from_config(lane),
                )
                .expect("8K key lane has a hispeed direction"),
            )
        },
    }
    Hispeed8Key8(HispeedDirectionConfig) {
        value: [access |profile| crate::config::play_input::hispeed_direction_for_lane(&profile.input, KeyMode::K8, crate::config::play::lane_from_config(LaneConfig::Key8)).expect("8K key lane has a hispeed direction"), |profile, value| { crate::config::play_input::set_eight_key_hispeed_direction(&mut profile.input, LaneConfig::Key8, *value); }],
        adjust: |profile, _delta| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key8).expect("guarded 8K hispeed setting");
            let current = crate::config::play_input::hispeed_direction_for_lane(
                &profile.input,
                KeyMode::K8,
                crate::config::play::lane_from_config(lane),
            )
            .expect("8K key lane has a hispeed direction");
            let next = match current {
                HispeedDirectionConfig::Down => HispeedDirectionConfig::Up,
                HispeedDirectionConfig::Up => HispeedDirectionConfig::Down,
            };
            crate::config::play_input::set_eight_key_hispeed_direction(
                &mut profile.input,
                lane,
                next,
            )
        },
        format: |profile| {
            let lane = eight_key_hispeed_lane(SettingsEntryId::Hispeed8Key8).expect("guarded 8K hispeed setting");
            format_hispeed_direction(
                crate::config::play_input::hispeed_direction_for_lane(
                    &profile.input,
                    KeyMode::K8,
                    crate::config::play::lane_from_config(lane),
                )
                .expect("8K key lane has a hispeed direction"),
            )
        },
    }
    DifficultyTableLevelDisplay(DifficultyTableLevelDisplay) {
        value: [field select.difficulty_table_level_display],
        adjust: |profile, _delta| {
            profile.select.difficulty_table_level_display =
                match profile.select.difficulty_table_level_display {
                    DifficultyTableLevelDisplay::Table => DifficultyTableLevelDisplay::Chart,
                    DifficultyTableLevelDisplay::Chart => DifficultyTableLevelDisplay::Table,
                };
            true
        },
        format: |profile| {
            match profile.select.difficulty_table_level_display {
                DifficultyTableLevelDisplay::Table => "TABLE LEVEL",
                DifficultyTableLevelDisplay::Chart => "CHART LEVEL",
            }
            .to_string()
        },
    }
    SelectRandomSelect(bool) {
        value: [field select.random_select],
        adjust: |profile, _delta| {
                profile.select.random_select = !profile.select.random_select;
                true
            },
        format: |profile| format_bool_on_off(profile.select.random_select),
    }
    SelectRandomNoPlay(bool) {
        value: [field select.random_select_no_play],
        adjust: |profile, _delta| {
                profile.select.random_select_no_play = !profile.select.random_select_no_play;
                true
            },
        format: |profile| {
            format_bool_on_off(profile.select.random_select_no_play)
        },
    }
    SelectRandomFailed(bool) {
        value: [field select.random_select_failed],
        adjust: |profile, _delta| {
                profile.select.random_select_failed = !profile.select.random_select_failed;
                true
            },
        format: |profile| {
            format_bool_on_off(profile.select.random_select_failed)
        },
    }
    SelectRandomNotEasy(bool) {
        value: [field select.random_select_not_easy],
        adjust: |profile, _delta| {
                profile.select.random_select_not_easy = !profile.select.random_select_not_easy;
                true
            },
        format: |profile| {
            format_bool_on_off(profile.select.random_select_not_easy)
        },
    }
    SelectRandomNotClear(bool) {
        value: [field select.random_select_not_clear],
        adjust: |profile, _delta| {
                profile.select.random_select_not_clear = !profile.select.random_select_not_clear;
                true
            },
        format: |profile| {
            format_bool_on_off(profile.select.random_select_not_clear)
        },
    }
    SelectRandomNotHard(bool) {
        value: [field select.random_select_not_hard],
        adjust: |profile, _delta| {
                profile.select.random_select_not_hard = !profile.select.random_select_not_hard;
                true
            },
        format: |profile| {
            format_bool_on_off(profile.select.random_select_not_hard)
        },
    }
    SelectRandomNotExHard(bool) {
        value: [field select.random_select_not_ex_hard],
        adjust: |profile, _delta| {
                profile.select.random_select_not_ex_hard =
                    !profile.select.random_select_not_ex_hard;
                true
            },
        format: |profile| {
            format_bool_on_off(profile.select.random_select_not_ex_hard)
        },
    }
    SelectRandomNotFullCombo(bool) {
        value: [field select.random_select_not_full_combo],
        adjust: |profile, _delta| {
                profile.select.random_select_not_full_combo =
                    !profile.select.random_select_not_full_combo;
                true
            },
        format: |profile| {
            format_bool_on_off(profile.select.random_select_not_full_combo)
        },
    }
    RandomMixTargetLevel(u32) {
        value: [field select.random_mix.target_level],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.select.random_mix.target_level, delta, 0, 99)
        },
        format: |profile| {
            format_random_mix_level(profile.select.random_mix.target_level, "OFF")
        },
    }
    RandomMixMaxLevel(u32) {
        value: [field select.random_mix.max_level],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.select.random_mix.max_level, delta, 0, 99)
        },
        format: |profile| {
            format_random_mix_level(profile.select.random_mix.max_level, "NO LIMIT")
        },
    }
    RandomMixMinLevel(u32) {
        value: [field select.random_mix.min_level],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.select.random_mix.min_level, delta, 0, 99)
        },
        format: |profile| {
            format_random_mix_level(profile.select.random_mix.min_level, "NO LIMIT")
        },
    }
    RandomMixBpmRange(u32) {
        value: [field select.random_mix.bpm_range],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.select.random_mix.bpm_range, delta, 0, 99)
        },
        format: |profile| {
            let value = profile.select.random_mix.bpm_range;
            if value == 0 { "NO LIMIT".to_string() } else { format!("+- {value} BPM") }
        },
    }
    RandomMixMaxBpm(u32) {
        value: [field select.random_mix.max_bpm],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.select.random_mix.max_bpm, delta, 0, 990)
        },
        format: |profile| {
            format_random_mix_bpm(profile.select.random_mix.max_bpm)
        },
    }
    RandomMixMinBpm(u32) {
        value: [field select.random_mix.min_bpm],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.select.random_mix.min_bpm, delta, 0, 990)
        },
        format: |profile| {
            format_random_mix_bpm(profile.select.random_mix.min_bpm)
        },
    }
    RandomMixStages(u32) {
        value: [field select.random_mix.stages],
        adjust: |profile, delta| {
            adjust_u32(&mut profile.select.random_mix.stages, delta, 0, 5)
        },
        format: |profile| {
            let value = profile.select.random_mix.stages;
            if value == 0 { "RANDOM".to_string() } else { format!("{value} STAGE") }
        },
    }
    ReplayAutoSave(bool) {
        value: [field replay.auto_save],
        adjust: |profile, _delta| {
                profile.replay.auto_save = !profile.replay.auto_save;
                true
            },
        format: |profile| format_bool_on_off(profile.replay.auto_save),
    }
    ReplayCompress(bool) {
        value: [field replay.compress],
        adjust: |profile, _delta| {
            profile.replay.compress = !profile.replay.compress;
            true
        },
        format: |profile| format_bool_on_off(profile.replay.compress),
    }
    ReplaySlot1Rule(ReplaySlotRule) {
        value: [field replay.slot_rules[0]],
        adjust: |profile, delta| {
            adjust_replay_slot_rule(&mut profile.replay.slot_rules[0], delta)
        },
        format: |profile| format_replay_slot_rule(profile.replay.slot_rules[0]),
    }
    ReplaySlot2Rule(ReplaySlotRule) {
        value: [field replay.slot_rules[1]],
        adjust: |profile, delta| {
            adjust_replay_slot_rule(&mut profile.replay.slot_rules[1], delta)
        },
        format: |profile| format_replay_slot_rule(profile.replay.slot_rules[1]),
    }
    ReplaySlot3Rule(ReplaySlotRule) {
        value: [field replay.slot_rules[2]],
        adjust: |profile, delta| {
            adjust_replay_slot_rule(&mut profile.replay.slot_rules[2], delta)
        },
        format: |profile| format_replay_slot_rule(profile.replay.slot_rules[2]),
    }
    ReplaySlot4Rule(ReplaySlotRule) {
        value: [field replay.slot_rules[3]],
        adjust: |profile, delta| {
            adjust_replay_slot_rule(&mut profile.replay.slot_rules[3], delta)
        },
        format: |profile| format_replay_slot_rule(profile.replay.slot_rules[3]),
    }
    Language(String) {
        value: [cloned ui.language],
        adjust: |profile, delta| {
            let current = profile.ui.locale();
            cycle_enum(delta, current, cycle_language)
                .map(|next| profile.ui.set_locale(next))
                .is_some()
        },
        format: |profile| profile.ui.locale().native_name().to_string(),
    }
    ShowFps(bool) {
        value: [field ui.show_fps],
        adjust: |profile, _delta| {
            profile.ui.show_fps = !profile.ui.show_fps;
            true
        },
        format: |profile| format_bool_on_off(profile.ui.show_fps),
    }
}
