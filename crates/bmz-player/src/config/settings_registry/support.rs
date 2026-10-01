use super::*;

pub(super) fn format_hispeed_direction(value: HispeedDirectionConfig) -> String {
    match value {
        HispeedDirectionConfig::Down => "DOWN",
        HispeedDirectionConfig::Up => "UP",
    }
    .to_string()
}

pub(super) fn adjust_u32(value: &mut u32, delta: i32, min: u32, max: u32) -> bool {
    let before = *value;
    let next = (*value as i32).saturating_add(delta).clamp(min as i32, max as i32) as u32;
    *value = next;
    *value != before
}

pub(super) fn adjust_u16(value: &mut u16, delta: i32, min: u16, max: u16) -> bool {
    let before = *value;
    *value = (*value as i32).saturating_add(delta).clamp(min as i32, max as i32) as u16;
    *value != before
}

pub(super) fn adjust_u8(value: &mut u8, delta: i32, min: u8, max: u8) -> bool {
    let before = *value;
    *value = (*value as i32).saturating_add(delta).clamp(min as i32, max as i32) as u8;
    *value != before
}

pub(super) fn adjust_f64_percent(value: &mut f64, delta: i32, min: f64, max: f64) -> bool {
    let before = *value;
    let next_percent = ((*value * 100.0).round() as i32)
        .saturating_add(delta)
        .clamp((min * 100.0).round() as i32, (max * 100.0).round() as i32);
    *value = next_percent as f64 / 100.0;
    (*value - before).abs() > f64::EPSILON
}

pub(super) fn adjust_offset_ms(value: &mut i64, delta: i32) -> bool {
    let before = *value;
    let ms = (*value / 1_000).saturating_add(delta as i64).clamp(-500, 500);
    *value = ms * 1_000;
    *value != before
}

pub(super) fn adjust_hispeed(value: &mut f32, delta: i32, step: f32, default_step: f32) -> bool {
    let before = *value;
    let step = normalize_hispeed_step(step, default_step);
    *value = clamp_hispeed(*value + step * delta.signum() as f32);
    (*value - before).abs() > f32::EPSILON
}

pub(super) fn adjust_hispeed_step(value: &mut f32, delta: i32) -> bool {
    let before = *value;
    let current = normalize_hispeed_step(*value, 0.25);
    let next = ((current / 0.05).round() as i32 + delta)
        .clamp((HISPEED_STEP_MIN / 0.05).round() as i32, (HISPEED_STEP_MAX / 0.05).round() as i32);
    *value = next as f32 / 20.0;
    (*value - before).abs() > f32::EPSILON
}

pub(super) fn adjust_f32_tenths(value: &mut f32, delta: i32, min: f32, max: f32) -> bool {
    let before = *value;
    let next = ((*value * 10.0).round() as i32 + delta)
        .clamp((min * 10.0).round() as i32, (max * 10.0).round() as i32);
    *value = next as f32 / 10.0;
    (*value - before).abs() > f32::EPSILON
}

pub(super) fn adjust_replay_slot_rule(value: &mut ReplaySlotRule, delta: i32) -> bool {
    let forward = delta >= 0;
    let steps = delta.unsigned_abs().max(1) as usize;
    let mut next = *value;
    for _ in 0..steps {
        next = next.cycle(forward);
    }
    if next == *value {
        return false;
    }
    *value = next;
    true
}

pub(super) fn format_lane_unit(value: u32) -> String {
    format!("{}", value.min(1000))
}

pub(super) fn format_bool_on_off(value: bool) -> String {
    if value { "ON".to_string() } else { "OFF".to_string() }
}

pub(super) fn format_gauge(value: GaugeTypeConfig) -> String {
    match value {
        GaugeTypeConfig::AssistEasy => "ASSIST EASY".to_string(),
        GaugeTypeConfig::Easy => "EASY".to_string(),
        GaugeTypeConfig::Normal => "NORMAL".to_string(),
        GaugeTypeConfig::Hard => "HARD".to_string(),
        GaugeTypeConfig::ExHard => "EX HARD".to_string(),
        GaugeTypeConfig::AutoShift => "AUTO SHIFT".to_string(),
        GaugeTypeConfig::Hazard => "HAZARD".to_string(),
    }
}

pub(super) fn format_rule_mode(value: RuleMode) -> String {
    match value {
        RuleMode::Beatoraja => "BEATORAJA".to_string(),
        RuleMode::Lr2Oraja => "LR2ORAJA".to_string(),
        RuleMode::Dx => "DX".to_string(),
    }
}

pub(super) fn format_gauge_auto_shift(value: GaugeAutoShiftConfig) -> String {
    match value {
        GaugeAutoShiftConfig::Off => "OFF".to_string(),
        GaugeAutoShiftConfig::Continue => "CONTINUE".to_string(),
        GaugeAutoShiftConfig::HardToGroove => "HARD->GROOVE".to_string(),
        GaugeAutoShiftConfig::BestClear => "BEST CLEAR".to_string(),
        GaugeAutoShiftConfig::SelectToUnder => "SELECT UNDER".to_string(),
    }
}

pub(crate) fn format_bottom_shiftable_gauge(value: BottomShiftableGaugeConfig) -> String {
    match value {
        BottomShiftableGaugeConfig::AssistEasy => "ASSIST EASY".to_string(),
        BottomShiftableGaugeConfig::Easy => "EASY".to_string(),
        BottomShiftableGaugeConfig::Normal => "NORMAL".to_string(),
    }
}

pub(super) fn format_random(value: RandomOptionConfig) -> String {
    match value {
        RandomOptionConfig::Off => "OFF".to_string(),
        RandomOptionConfig::Mirror => "MIRROR".to_string(),
        RandomOptionConfig::Random => "RANDOM".to_string(),
        RandomOptionConfig::RRandom => "R-RANDOM".to_string(),
        RandomOptionConfig::SRandom => "S-RANDOM".to_string(),
        RandomOptionConfig::Spiral => "SPIRAL".to_string(),
        RandomOptionConfig::HRandom => "H-RANDOM".to_string(),
        RandomOptionConfig::AllScratch => "ALL-SCR".to_string(),
        RandomOptionConfig::RandomEx => "RANDOM-EX".to_string(),
        RandomOptionConfig::SRandomEx => "S-RANDOM-EX".to_string(),
        RandomOptionConfig::FRandom => "F-RANDOM".to_string(),
        RandomOptionConfig::MFRandom => "MF-RANDOM".to_string(),
    }
}

pub(super) fn format_double_option(value: DoubleOptionConfig) -> String {
    match value {
        DoubleOptionConfig::Off => "OFF".to_string(),
        DoubleOptionConfig::Flip => "FLIP".to_string(),
        DoubleOptionConfig::Battle => "BATTLE".to_string(),
        DoubleOptionConfig::BattleAutoScratch => "BATTLE AS".to_string(),
    }
}

pub(super) fn format_hs_fix(value: HsFixConfig) -> String {
    match value {
        HsFixConfig::Off => "OFF".to_string(),
        HsFixConfig::StartBpm => "START BPM".to_string(),
        HsFixConfig::MinBpm => "MIN BPM".to_string(),
        HsFixConfig::MaxBpm => "MAX BPM".to_string(),
        HsFixConfig::MainBpm => "MAIN BPM".to_string(),
    }
}

pub(super) fn format_target(value: TargetOptionConfig) -> String {
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

pub(super) fn format_assist(value: AssistOptionConfig) -> String {
    let labels = value
        .flags()
        .into_iter()
        .zip([
            "EXPAND JUDGE",
            "CONSTANT",
            "JUDGE AREA",
            "LEGACY NOTE",
            "MARK NOTE",
            "BPM GUIDE",
            "NO MINE",
        ])
        .filter_map(|(enabled, label)| enabled.then_some(label))
        .collect::<Vec<_>>();
    if labels.is_empty() { "NONE".to_string() } else { labels.join(" + ") }
}

pub(super) fn format_bga_mode(value: BgaModeConfig) -> String {
    match value {
        BgaModeConfig::On => "ON".to_string(),
        BgaModeConfig::Auto => "AUTO".to_string(),
        BgaModeConfig::Off => "OFF".to_string(),
    }
}

pub(super) fn format_bga_expand(value: BgaExpandConfig) -> String {
    match value {
        BgaExpandConfig::Full => "FULL".to_string(),
        BgaExpandConfig::KeepAspect => "KEEP ASPECT".to_string(),
        BgaExpandConfig::Off => "OFF".to_string(),
    }
}

pub(super) fn format_judge_algorithm(value: JudgeAlgorithmConfig) -> String {
    match value {
        JudgeAlgorithmConfig::Combo => "COMBO".to_string(),
        JudgeAlgorithmConfig::Duration => "DURATION".to_string(),
        JudgeAlgorithmConfig::Lowest => "LOWEST".to_string(),
    }
}

pub(crate) fn format_hispeed_mode(value: HispeedConfigPreset) -> String {
    match value {
        HispeedConfigPreset::Normal => "NORMAL".to_string(),
        HispeedConfigPreset::Classic => "CLASSIC".to_string(),
        HispeedConfigPreset::Floating => "FLOATING".to_string(),
        HispeedConfigPreset::NormalFloating => "NORMAL+FLOATING".to_string(),
        HispeedConfigPreset::ClassicFloating => "CLASSIC+FLOATING".to_string(),
    }
}

pub(super) fn format_replay_slot_rule(value: ReplaySlotRule) -> String {
    match value {
        ReplaySlotRule::Disabled => "DISABLED".to_string(),
        ReplaySlotRule::Always => "ALWAYS".to_string(),
        ReplaySlotRule::ScoreUpdate => "SCORE UPDATE".to_string(),
        ReplaySlotRule::BpUpdate => "BP UPDATE".to_string(),
        ReplaySlotRule::MaxComboUpdate => "MAX COMBO UPDATE".to_string(),
        ReplaySlotRule::ClearUpdate => "CLEAR UPDATE".to_string(),
    }
}

pub(super) fn cycle_enum<T: Copy + PartialEq>(
    delta: i32,
    current: T,
    cycle: fn(T, bool) -> T,
) -> Option<T> {
    if delta == 0 {
        return None;
    }
    let forward = delta > 0;
    Some(cycle(current, forward))
}

pub(super) fn cycle_judge_algorithm(
    current: JudgeAlgorithmConfig,
    forward: bool,
) -> JudgeAlgorithmConfig {
    cycle_in_slice(&JudgeAlgorithmConfig::ORDER, current, forward)
}

pub(super) fn cycle_fast_slow_display_scope(
    current: FastSlowDisplayScope,
    forward: bool,
) -> FastSlowDisplayScope {
    cycle_in_slice(
        &[FastSlowDisplayScope::Auto, FastSlowDisplayScope::ThresholdMs],
        current,
        forward,
    )
}

pub(super) fn cycle_session_mode(current: SessionMode, forward: bool) -> SessionMode {
    cycle_in_slice(&SessionMode::VALUES, current, forward)
}

pub(super) fn cycle_key_mode_conversion(
    current: KeyModeConversionConfig,
    forward: bool,
) -> KeyModeConversionConfig {
    cycle_in_slice(&KeyModeConversionConfig::VALUES, current, forward)
}

pub(super) fn cycle_seven_to_nine_pattern(
    current: SevenToNinePattern,
    forward: bool,
) -> SevenToNinePattern {
    cycle_in_slice(&SevenToNinePattern::VALUES, current, forward)
}

pub(super) fn cycle_seven_to_nine_type(current: SevenToNineType, forward: bool) -> SevenToNineType {
    cycle_in_slice(&SevenToNineType::VALUES, current, forward)
}

pub(super) fn cycle_seven_to_nine_rule_mode(
    current: SevenToNineRuleMode,
    forward: bool,
) -> SevenToNineRuleMode {
    cycle_in_slice(&SevenToNineRuleMode::VALUES, current, forward)
}

pub(super) fn cycle_assist_scroll_mode(
    current: AssistScrollMode,
    forward: bool,
) -> AssistScrollMode {
    cycle_in_slice(
        &[AssistScrollMode::Off, AssistScrollMode::Remove, AssistScrollMode::Add],
        current,
        forward,
    )
}

pub(super) fn cycle_assist_long_note_mode(
    current: AssistLongNoteMode,
    forward: bool,
) -> AssistLongNoteMode {
    cycle_in_slice(
        &[
            AssistLongNoteMode::Off,
            AssistLongNoteMode::Remove,
            AssistLongNoteMode::AddLn,
            AssistLongNoteMode::AddCn,
            AssistLongNoteMode::AddHcn,
            AssistLongNoteMode::AddAll,
        ],
        current,
        forward,
    )
}

pub(super) fn cycle_assist_mine_mode(current: AssistMineMode, forward: bool) -> AssistMineMode {
    cycle_in_slice(
        &[
            AssistMineMode::Off,
            AssistMineMode::Remove,
            AssistMineMode::AddRandom,
            AssistMineMode::AddNear,
            AssistMineMode::AddBlank,
        ],
        current,
        forward,
    )
}

pub(super) fn cycle_language(current: AppLocale, forward: bool) -> AppLocale {
    cycle_in_slice(&AppLocale::SUPPORTED, current, forward)
}

pub(super) fn cycle_rule_mode(current: RuleMode, forward: bool) -> RuleMode {
    const VALUES: [RuleMode; 3] = [RuleMode::Beatoraja, RuleMode::Lr2Oraja, RuleMode::Dx];
    cycle_in_slice(&VALUES, current, forward)
}

pub(super) fn cycle_ln_mode_policy(current: LnPolicySetting, forward: bool) -> LnPolicySetting {
    cycle_in_slice(&LnPolicySetting::ORDER, current, forward)
}

pub(super) fn cycle_gauge(current: GaugeTypeConfig, forward: bool) -> GaugeTypeConfig {
    const VALUES: [GaugeTypeConfig; 7] = [
        GaugeTypeConfig::AssistEasy,
        GaugeTypeConfig::Easy,
        GaugeTypeConfig::Normal,
        GaugeTypeConfig::Hard,
        GaugeTypeConfig::ExHard,
        GaugeTypeConfig::Hazard,
        GaugeTypeConfig::AutoShift,
    ];
    cycle_in_slice(&VALUES, current, forward)
}

pub(super) fn cycle_gauge_auto_shift(
    current: GaugeAutoShiftConfig,
    forward: bool,
) -> GaugeAutoShiftConfig {
    const VALUES: [GaugeAutoShiftConfig; 5] = [
        GaugeAutoShiftConfig::Off,
        GaugeAutoShiftConfig::Continue,
        GaugeAutoShiftConfig::HardToGroove,
        GaugeAutoShiftConfig::BestClear,
        GaugeAutoShiftConfig::SelectToUnder,
    ];
    cycle_in_slice(&VALUES, current, forward)
}

pub(super) fn cycle_bottom_shiftable_gauge(
    current: BottomShiftableGaugeConfig,
    forward: bool,
) -> BottomShiftableGaugeConfig {
    const VALUES: [BottomShiftableGaugeConfig; 3] = [
        BottomShiftableGaugeConfig::AssistEasy,
        BottomShiftableGaugeConfig::Easy,
        BottomShiftableGaugeConfig::Normal,
    ];
    cycle_in_slice(&VALUES, current, forward)
}

pub(super) fn cycle_random(current: RandomOptionConfig, forward: bool) -> RandomOptionConfig {
    const VALUES: [RandomOptionConfig; 12] = [
        RandomOptionConfig::Off,
        RandomOptionConfig::Mirror,
        RandomOptionConfig::Random,
        RandomOptionConfig::RRandom,
        RandomOptionConfig::SRandom,
        RandomOptionConfig::Spiral,
        RandomOptionConfig::HRandom,
        RandomOptionConfig::AllScratch,
        RandomOptionConfig::RandomEx,
        RandomOptionConfig::SRandomEx,
        RandomOptionConfig::FRandom,
        RandomOptionConfig::MFRandom,
    ];
    cycle_in_slice(&VALUES, current, forward)
}

pub(super) fn cycle_double_option(
    current: DoubleOptionConfig,
    forward: bool,
) -> DoubleOptionConfig {
    const VALUES: [DoubleOptionConfig; 4] = [
        DoubleOptionConfig::Off,
        DoubleOptionConfig::Flip,
        DoubleOptionConfig::Battle,
        DoubleOptionConfig::BattleAutoScratch,
    ];
    cycle_in_slice(&VALUES, current, forward)
}

pub(super) fn cycle_hs_fix(current: HsFixConfig, forward: bool) -> HsFixConfig {
    const VALUES: [HsFixConfig; 5] = [
        HsFixConfig::Off,
        HsFixConfig::StartBpm,
        HsFixConfig::MaxBpm,
        HsFixConfig::MainBpm,
        HsFixConfig::MinBpm,
    ];
    cycle_in_slice(&VALUES, current, forward)
}

pub(super) fn cycle_target(current: TargetOptionConfig, forward: bool) -> TargetOptionConfig {
    const VALUES: [TargetOptionConfig; 13] = [
        TargetOptionConfig::None,
        TargetOptionConfig::RankA,
        TargetOptionConfig::RankAaMinus,
        TargetOptionConfig::RankAa,
        TargetOptionConfig::RankAaaMinus,
        TargetOptionConfig::RankAaa,
        TargetOptionConfig::RankMaxMinus,
        TargetOptionConfig::Max,
        TargetOptionConfig::RankNext,
        TargetOptionConfig::IrTop,
        TargetOptionConfig::IrNext,
        TargetOptionConfig::RivalTop,
        TargetOptionConfig::RivalNext,
    ];
    let current = if matches!(current, TargetOptionConfig::RivalIndex(_)) {
        TargetOptionConfig::None
    } else {
        current
    };
    cycle_in_slice(&VALUES, current, forward)
}

pub(super) fn cycle_assist(current: AssistOptionConfig, forward: bool) -> AssistOptionConfig {
    let mut next = current;
    // 旧設定一覧から編集された場合の互換操作。独立した7トグルは選曲スキンと
    // profile UI で扱い、一覧の単一行では LEGACY NOTE を切り替える。
    let _ = forward;
    next.toggle_beatoraja_button(304);
    next
}

pub(super) fn cycle_bga_mode(current: BgaModeConfig, forward: bool) -> BgaModeConfig {
    const VALUES: [BgaModeConfig; 3] = [BgaModeConfig::On, BgaModeConfig::Auto, BgaModeConfig::Off];
    cycle_in_slice(&VALUES, current, forward)
}

pub(super) fn cycle_bga_expand(current: BgaExpandConfig, forward: bool) -> BgaExpandConfig {
    const VALUES: [BgaExpandConfig; 3] =
        [BgaExpandConfig::KeepAspect, BgaExpandConfig::Full, BgaExpandConfig::Off];
    cycle_in_slice(&VALUES, current, forward)
}

pub(super) fn cycle_hispeed_mode(
    current: HispeedConfigPreset,
    forward: bool,
) -> HispeedConfigPreset {
    cycle_in_slice(&HispeedConfigPreset::ORDER, current, forward)
}

pub(super) fn cycle_select_input_mode(
    current: SelectInputModeConfig,
    forward: bool,
) -> SelectInputModeConfig {
    const VALUES: [SelectInputModeConfig; 2] =
        [SelectInputModeConfig::Key7Key14, SelectInputModeConfig::Key9];
    cycle_in_slice(&VALUES, current, forward)
}

pub(super) fn cycle_in_slice<T: Copy + PartialEq>(values: &[T], current: T, forward: bool) -> T {
    let index = values.iter().position(|value| *value == current).unwrap_or(0);
    if forward {
        values[(index + 1) % values.len()]
    } else {
        values[(index + values.len() - 1) % values.len()]
    }
}
