pub(in crate::ui) fn profile_selection_label(
    profiles: &[crate::storage::profile::ProfileSummary],
    profile_id: &str,
) -> String {
    profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .map(|profile| format!("{}: {}", profile.id, profile.display_name))
        .unwrap_or_else(|| profile_id.to_string())
}

pub(in crate::ui) fn trimmed_non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

pub(in crate::ui) fn profile_id_text_edit(ui: &mut egui::Ui, value: &mut String) {
    if ui.text_edit_singleline(value).changed() {
        sanitize_profile_id_input(value);
    }
}

pub(in crate::ui) fn sanitize_profile_id_input(value: &mut String) {
    value.retain(is_profile_id_char);
    if value.len() > 64 {
        value.truncate(64);
    }
}

pub(in crate::ui) fn is_profile_id_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

pub(in crate::ui) fn volume_slider(ui: &mut egui::Ui, value: &mut u32, label: &str) {
    ui.add(egui::Slider::new(value, 0..=100).text(label));
}

pub(in crate::ui) fn lane_unit_slider(ui: &mut egui::Ui, value: &mut u32, label: &str) {
    lane_unit_slider_with_max(ui, value, label, 1000);
}

pub(in crate::ui) fn lane_unit_slider_with_max(
    ui: &mut egui::Ui,
    value: &mut u32,
    label: &str,
    max: u32,
) {
    *value = (*value).min(max);
    ui.add(egui::Slider::new(value, 0..=max).text(label));
}

const OFFSET_SLIDER_MIN_MS: i64 = -500;
const OFFSET_SLIDER_MAX_MS: i64 = 500;
const OFFSET_SLIDER_STEP_MS: f64 = 1.0;

pub(in crate::ui) fn offset_ms_slider(ui: &mut egui::Ui, value_us: &mut i64, label: &str) {
    let mut value_ms = (*value_us / 1_000).clamp(OFFSET_SLIDER_MIN_MS, OFFSET_SLIDER_MAX_MS);
    let response = ui.add(
        egui::Slider::new(&mut value_ms, OFFSET_SLIDER_MIN_MS..=OFFSET_SLIDER_MAX_MS)
            // smart_aim は 5/10/50 などの値へ吸着するため、オフセットでは使わない。
            .smart_aim(false)
            .step_by(OFFSET_SLIDER_STEP_MS)
            .text(format!("{label} (ms)")),
    );
    if response.changed() {
        *value_us = value_ms * 1_000;
    }
}

pub(in crate::ui) fn judge_algorithm_label(value: JudgeAlgorithmConfig) -> &'static str {
    match value {
        JudgeAlgorithmConfig::Combo => "COMBO",
        JudgeAlgorithmConfig::Duration => "DURATION",
        JudgeAlgorithmConfig::Lowest => "LOWEST",
    }
}

pub(in crate::ui) fn fast_slow_scope_label(text: Localizer, value: FastSlowDisplayScope) -> String {
    match value {
        FastSlowDisplayScope::Auto => tr!(text, "profile-fast-slow-auto"),
        FastSlowDisplayScope::ThresholdMs => tr!(text, "profile-fast-slow-threshold-mode"),
    }
}

pub(in crate::ui) fn rule_mode_label(value: RuleMode) -> &'static str {
    match value {
        RuleMode::Beatoraja => "BEATORAJA",
        RuleMode::Lr2Oraja => "LR2ORAJA",
        RuleMode::Dx => "DX",
    }
}

pub(in crate::ui) fn gauge_label(value: GaugeTypeConfig) -> &'static str {
    match value {
        GaugeTypeConfig::AssistEasy => "ASSIST EASY",
        GaugeTypeConfig::Easy => "EASY",
        GaugeTypeConfig::Normal => "NORMAL",
        GaugeTypeConfig::Hard => "HARD",
        GaugeTypeConfig::ExHard => "EX HARD",
        GaugeTypeConfig::AutoShift => "AUTO SHIFT",
        GaugeTypeConfig::Hazard => "HAZARD",
    }
}

pub(in crate::ui) fn gauge_auto_shift_label(value: GaugeAutoShiftConfig) -> &'static str {
    match value {
        GaugeAutoShiftConfig::Off => "OFF",
        GaugeAutoShiftConfig::Continue => "CONTINUE",
        GaugeAutoShiftConfig::HardToGroove => "HARD->GROOVE",
        GaugeAutoShiftConfig::BestClear => "BEST CLEAR",
        GaugeAutoShiftConfig::SelectToUnder => "SELECT UNDER",
    }
}

pub(in crate::ui) fn bottom_shiftable_gauge_label(
    value: BottomShiftableGaugeConfig,
) -> &'static str {
    match value {
        BottomShiftableGaugeConfig::AssistEasy => "ASSIST EASY",
        BottomShiftableGaugeConfig::Easy => "EASY",
        BottomShiftableGaugeConfig::Normal => "NORMAL",
    }
}

pub(in crate::ui) fn random_label(value: RandomOptionConfig) -> &'static str {
    match value {
        RandomOptionConfig::Off => "OFF",
        RandomOptionConfig::Mirror => "MIRROR",
        RandomOptionConfig::Random => "RANDOM",
        RandomOptionConfig::RRandom => "R-RANDOM",
        RandomOptionConfig::SRandom => "S-RANDOM",
        RandomOptionConfig::Spiral => "SPIRAL",
        RandomOptionConfig::HRandom => "H-RANDOM",
        RandomOptionConfig::AllScratch => "ALL-SCR",
        RandomOptionConfig::RandomEx => "RANDOM-EX",
        RandomOptionConfig::SRandomEx => "S-RANDOM-EX",
        RandomOptionConfig::FRandom => "F-RANDOM",
        RandomOptionConfig::MFRandom => "MF-RANDOM",
    }
}

pub(in crate::ui) fn random_options() -> [(RandomOptionConfig, &'static str); 12] {
    [
        (RandomOptionConfig::Off, "OFF"),
        (RandomOptionConfig::Mirror, "MIRROR"),
        (RandomOptionConfig::Random, "RANDOM"),
        (RandomOptionConfig::RRandom, "R-RANDOM"),
        (RandomOptionConfig::SRandom, "S-RANDOM"),
        (RandomOptionConfig::Spiral, "SPIRAL"),
        (RandomOptionConfig::HRandom, "H-RANDOM"),
        (RandomOptionConfig::AllScratch, "ALL-SCR"),
        (RandomOptionConfig::RandomEx, "RANDOM-EX"),
        (RandomOptionConfig::SRandomEx, "S-RANDOM-EX"),
        (RandomOptionConfig::FRandom, "F-RANDOM"),
        (RandomOptionConfig::MFRandom, "MF-RANDOM"),
    ]
}

pub(in crate::ui) fn double_option_label(value: DoubleOptionConfig) -> &'static str {
    match value {
        DoubleOptionConfig::Off => "OFF",
        DoubleOptionConfig::Flip => "FLIP",
        DoubleOptionConfig::Battle => "BATTLE",
        DoubleOptionConfig::BattleAutoScratch => "BATTLE AS",
    }
}

pub(in crate::ui) fn hs_fix_label(value: HsFixConfig) -> &'static str {
    match value {
        HsFixConfig::Off => "OFF",
        HsFixConfig::StartBpm => "START BPM",
        HsFixConfig::MinBpm => "MIN BPM",
        HsFixConfig::MaxBpm => "MAX BPM",
        HsFixConfig::MainBpm => "MAIN BPM",
    }
}

pub(in crate::ui) fn target_label(value: TargetOptionConfig) -> String {
    match value {
        TargetOptionConfig::None => "NONE".to_string(),
        TargetOptionConfig::RankA => "RANK_A".to_string(),
        TargetOptionConfig::RankAaMinus => "RANK_AA-".to_string(),
        TargetOptionConfig::RankAa => "RANK_AA".to_string(),
        TargetOptionConfig::RankAaaMinus => "RANK_AAA-".to_string(),
        TargetOptionConfig::RankAaa => "RANK_AAA".to_string(),
        TargetOptionConfig::RankMaxMinus => "RANK_MAX-".to_string(),
        TargetOptionConfig::Max => "MAX".to_string(),
        TargetOptionConfig::RankNext => "RANK_NEXT".to_string(),
        TargetOptionConfig::IrTop => "IR_TOP".to_string(),
        TargetOptionConfig::IrNext => "IR_NEXT".to_string(),
        TargetOptionConfig::RivalTop => "RIVAL TOP".to_string(),
        TargetOptionConfig::RivalNext => "RIVAL NEXT".to_string(),
        TargetOptionConfig::RivalIndex(index) => format!("RIVAL_{index}"),
    }
}

pub(in crate::ui) fn bga_mode_label(value: BgaModeConfig) -> &'static str {
    match value {
        BgaModeConfig::On => "ON",
        BgaModeConfig::Auto => "AUTO",
        BgaModeConfig::Off => "OFF",
    }
}

pub(in crate::ui) fn bga_expand_label(value: BgaExpandConfig) -> &'static str {
    match value {
        BgaExpandConfig::Full => "FULL",
        BgaExpandConfig::KeepAspect => "KEEP ASPECT",
        BgaExpandConfig::Off => "OFF",
    }
}

pub(in crate::ui) fn hispeed_mode_label(value: HispeedConfigPreset) -> &'static str {
    match value {
        HispeedConfigPreset::Normal => "NORMAL",
        HispeedConfigPreset::Classic => "CLASSIC",
        HispeedConfigPreset::Floating => "FLOATING",
        HispeedConfigPreset::NormalFloating => "NORMAL+FLOATING",
        HispeedConfigPreset::ClassicFloating => "CLASSIC+FLOATING",
        HispeedConfigPreset::Lr2 => "LR2",
    }
}

pub(in crate::ui) fn replay_slot_rule_label(value: ReplaySlotRule) -> &'static str {
    match value {
        ReplaySlotRule::Disabled => "DISABLED",
        ReplaySlotRule::Always => "ALWAYS",
        ReplaySlotRule::ScoreUpdate => "SCORE UPDATE",
        ReplaySlotRule::BpUpdate => "BP UPDATE",
        ReplaySlotRule::MaxComboUpdate => "MAX COMBO UPDATE",
        ReplaySlotRule::ClearUpdate => "CLEAR UPDATE",
    }
}

pub(in crate::ui) fn system_sound_path_row(
    ui: &mut egui::Ui,
    text: Localizer,
    label: &str,
    value: &mut String,
) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::TextEdit::singleline(value).desired_width(260.0));
        if ui.button(tr!(text, "common-choose-folder")).clicked()
            && let Some(folder) = rfd::FileDialog::new().pick_folder()
        {
            *value = folder.to_string_lossy().into_owned();
        }
    });
}

pub(in crate::ui) fn ir_provider_text_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
    });
}

pub(in crate::ui) fn ir_provider_family(provider: &str) -> &'static str {
    if crate::ir::bms_ir::is_bms_ir_provider(provider) {
        crate::ir::bms_ir::BMS_IR_PROVIDER
    } else if crate::ir::rian_ir::is_rian_ir_provider(provider) {
        crate::ir::rian_ir::RIAN_IR_PROVIDER
    } else {
        crate::ir::bmz_official::BMZ_IR_PROVIDER
    }
}

pub(in crate::ui) fn ir_send_policy_label(value: IrSendPolicyConfig) -> &'static str {
    match value {
        IrSendPolicyConfig::UpdateScore => "UPDATE SCORE",
        IrSendPolicyConfig::Always => "ALWAYS",
        IrSendPolicyConfig::CompleteSong => "COMPLETE SONG",
    }
}

pub(in crate::ui) fn ir_primary_provider_label(
    provider: &IrProviderConfig,
    provider_key: &str,
) -> String {
    let account = provider.account_display_name.trim();
    if account.is_empty() {
        format!("{provider_key} ({})", provider.base_url)
    } else {
        format!("{provider_key} - {account} ({})", provider.base_url)
    }
}

pub(in crate::ui) fn sync_ir_provider_roles(ir_config: &mut IrConfig) -> bool {
    let mut changed = false;
    let primary_provider = ir_config.primary_provider.trim();
    for provider in &mut ir_config.providers {
        let next_role = if !primary_provider.is_empty()
            && crate::ir::provider_key::configured_provider_key(provider)
                .is_some_and(|provider_key| provider_key == primary_provider)
        {
            IrProviderRoleConfig::Primary
        } else {
            IrProviderRoleConfig::SubmitOnly
        };
        if provider.role != next_role {
            provider.role = next_role;
            changed = true;
        }
    }
    changed
}

pub(in crate::ui) fn format_optional_timestamp(value: Option<i64>) -> String {
    value.map(format_unix_local_timestamp).unwrap_or_else(|| "-".to_string())
}

fn format_unix_local_timestamp(seconds: i64) -> String {
    let (year, month, day, hour, minute) = unix_seconds_to_local_datetime(seconds)
        .unwrap_or_else(|| unix_seconds_to_utc_datetime(seconds));
    format_datetime_minute(year, month, day, hour, minute)
}

pub(in crate::ui) fn format_datetime_minute(
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
) -> String {
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

#[cfg(unix)]
fn unix_seconds_to_local_datetime(seconds: i64) -> Option<(i64, u32, u32, u32, u32)> {
    let raw_time = libc::time_t::try_from(seconds).ok()?;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: `raw_time` and `tm` remain valid for the duration of the call.
    let result = unsafe { libc::localtime_r(&raw_time, tm.as_mut_ptr()) };
    if result.is_null() {
        return None;
    }
    // SAFETY: A non-null result means `tm` was initialized by `localtime_r`.
    Some(datetime_minute_from_tm(unsafe { tm.assume_init() }))
}

#[cfg(windows)]
fn unix_seconds_to_local_datetime(seconds: i64) -> Option<(i64, u32, u32, u32, u32)> {
    let raw_time = libc::time_t::try_from(seconds).ok()?;
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: `raw_time` and `tm` remain valid for the duration of the call.
    let result = unsafe { libc::localtime_s(tm.as_mut_ptr(), &raw_time) };
    if result != 0 {
        return None;
    }
    // SAFETY: A zero result means `tm` was initialized by `localtime_s`.
    Some(datetime_minute_from_tm(unsafe { tm.assume_init() }))
}

#[cfg(not(any(unix, windows)))]
fn unix_seconds_to_local_datetime(_seconds: i64) -> Option<(i64, u32, u32, u32, u32)> {
    None
}

#[cfg(any(unix, windows))]
fn datetime_minute_from_tm(tm: libc::tm) -> (i64, u32, u32, u32, u32) {
    (
        i64::from(tm.tm_year) + 1900,
        (tm.tm_mon + 1).clamp(1, 12) as u32,
        tm.tm_mday.clamp(1, 31) as u32,
        tm.tm_hour.clamp(0, 23) as u32,
        tm.tm_min.clamp(0, 59) as u32,
    )
}

fn unix_seconds_to_utc_datetime(seconds: i64) -> (i64, u32, u32, u32, u32) {
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400) as u32;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month as u32, day as u32, seconds_of_day / 3_600, (seconds_of_day % 3_600) / 60)
}
use super::*;
