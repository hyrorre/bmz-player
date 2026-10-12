use super::*;

#[test]
fn lr2_select_buttons_cycle_lr2_values_without_opening_settings() {
    use Lr2SelectButtonAction::*;
    for (button, action) in [
        (1, TogglePanel),
        (10, SkinEvent(10)),
        (11, KeyFilter),
        (12, Sort),
        (40, Gauge),
        (42, Arrange1P),
        (43, Arrange2P),
        (50, LaneEffect),
        (54, DpFlip),
        (55, HsFix),
        (57, SkinEvent(57)),
        (72, SkinEvent(72)),
        (74, SkinEvent(74)),
        (75, JudgeAutoAdjust),
        (77, Target),
        (93, DifficultyTab(2)),
    ] {
        assert_eq!(lr2_select_button_action(button), action, "button {button}");
    }
    // LR2 options without an identical BMZ setting are ignored instead of opening BMZ UI.
    for button in [2, 8, 18, 20, 29, 33, 41, 44, 45, 46, 51, 56, 58, 70, 71, 73, 76, 80, 83, 90] {
        assert_eq!(lr2_select_button_action(button), Unsupported, "button {button}");
    }
}

#[test]
fn lr2_select_button_direction_follows_lr2_click_rules() {
    // Difficulty / key / sort filters always advance in LR2.
    for button in 10..=12 {
        assert_eq!(lr2_select_button_direction(button, -1), 1);
    }
    assert_eq!(lr2_select_button_direction(40, -1), -1);
    assert_eq!(lr2_select_button_direction(40, 1), 1);
    let lr2_gauge = bmz_render::skin::LR2_BUTTON_BASE + 40;
    assert!(select_skin_click_allowed(lr2_gauge, MouseButton::Left));
    assert!(!select_skin_click_allowed(lr2_gauge, MouseButton::Right));
    assert!(!select_skin_click_allowed(lr2_gauge, MouseButton::Middle));
    // beatoraja events keep right-click as the previous value.
    assert!(select_skin_click_allowed(40, MouseButton::Right));
}

#[test]
fn lr2_select_option_values_cycle_in_lr2_order() {
    let mut gauge = GaugeTypeConfig::Normal;
    let mut seen = Vec::new();
    for _ in 0..4 {
        gauge = cycle_lr2_value(LR2_GAUGES, gauge, 1);
        seen.push(gauge);
    }
    assert_eq!(
        seen,
        [
            GaugeTypeConfig::Hard,
            GaugeTypeConfig::Hazard,
            GaugeTypeConfig::Easy,
            GaugeTypeConfig::Normal
        ]
    );
    assert_eq!(cycle_lr2_value(LR2_GAUGES, GaugeTypeConfig::Normal, -1), GaugeTypeConfig::Easy);
    // BMZ-only values enter the LR2 list at its edge instead of opening settings.
    assert_eq!(cycle_lr2_value(LR2_GAUGES, GaugeTypeConfig::ExHard, 1), GaugeTypeConfig::Normal);
    assert_eq!(cycle_lr2_value(LR2_GAUGES, GaugeTypeConfig::ExHard, -1), GaugeTypeConfig::Easy);

    assert_eq!(cycle_lr2_value(LR2_ARRANGES, ArrangeOption::SRandom, 1), ArrangeOption::HRandom);
    assert_eq!(cycle_lr2_value(LR2_ARRANGES, ArrangeOption::HRandom, 1), ArrangeOption::AllScratch);
    assert_eq!(cycle_lr2_value(LR2_ARRANGES, ArrangeOption::AllScratch, 1), ArrangeOption::Normal);
    assert_eq!(cycle_lr2_value(LR2_ARRANGES, ArrangeOption::Spiral, 1), ArrangeOption::Normal);

    assert_eq!(
        cycle_lr2_value(LR2_LANE_EFFECTS, LaneEffectConfig::Off, 1),
        LaneEffectConfig::Hidden
    );
    assert_eq!(
        cycle_lr2_value(LR2_LANE_EFFECTS, LaneEffectConfig::Hidden, 1),
        LaneEffectConfig::Sudden
    );
    assert_eq!(
        cycle_lr2_value(LR2_LANE_EFFECTS, LaneEffectConfig::Off, -1),
        LaneEffectConfig::HiddenSudden
    );

    assert_eq!(cycle_lr2_value(LR2_DP_OPTIONS, DoubleOption::Off, 1), DoubleOption::Flip);
    assert_eq!(cycle_lr2_value(LR2_DP_OPTIONS, DoubleOption::Flip, 1), DoubleOption::Off);
    assert_eq!(cycle_lr2_value(LR2_DP_OPTIONS, DoubleOption::Battle, 1), DoubleOption::Off);

    assert_eq!(cycle_lr2_value(LR2_HS_FIXES, HsFixOption::MinBpm, 1), HsFixOption::MainBpm);
    assert_eq!(cycle_lr2_value(LR2_HS_FIXES, HsFixOption::StartBpm, 1), HsFixOption::Off);

    assert_eq!(cycle_lr2_value(LR2_TARGETS, TargetOption::None, 1), TargetOption::RankAaa);
    assert_eq!(cycle_lr2_value(LR2_TARGETS, TargetOption::RankA, 1), TargetOption::IrTop);
    assert_eq!(cycle_lr2_value(LR2_TARGETS, TargetOption::IrNext, 1), TargetOption::None);
    assert_eq!(cycle_lr2_value(LR2_TARGETS, TargetOption::Max, -1), TargetOption::IrNext);

    assert_eq!(cycle_lr2_value(LR2_SORTS, SelectSort::Score, 1), SelectSort::Level);
    assert_eq!(cycle_lr2_value(LR2_SORTS, SelectSort::Artist, 1), SelectSort::Level);
    assert_eq!(cycle_lr2_value(LR2_KEY_FILTERS, SelectModeFilter::K9, 1), SelectModeFilter::All);
}
