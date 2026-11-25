use super::*;

#[test]
fn default_gauge_total_matches_beatoraja_formula() {
    assert_eq!(default_gauge_total(0), 260.0);
    assert_eq!(default_gauge_total(100), 260.0);
    let dense = default_gauge_total(2000);
    assert!(dense > 260.0);
}

#[test]
fn lr2oraja_default_gauge_total_matches_endlessdream_formula() {
    assert_eq!(lr2oraja_default_gauge_total(0), 160.0);
    assert_eq!(lr2oraja_default_gauge_total(400), 224.0);
    assert_eq!(lr2oraja_default_gauge_total(500), 256.0);
    assert_eq!(lr2oraja_default_gauge_total(600), 288.0);
    assert_eq!(lr2oraja_default_gauge_total(1000), 352.0);
}

#[test]
fn gauge_total_for_chart_uses_metadata_when_positive() {
    assert_eq!(gauge_total_for_chart(Some(320.0), 500), 320.0);
    assert_eq!(gauge_total_for_chart(Some(0.0), 500), default_gauge_total(500));
    assert_eq!(gauge_total_for_chart(None, 500), default_gauge_total(500));
}

#[test]
fn gauge_total_for_chart_and_rule_mode_uses_lr2oraja_default() {
    assert_eq!(
        gauge_total_for_chart_and_rule_mode(None, 500, RuleMode::Beatoraja),
        default_gauge_total(500)
    );
    assert_eq!(
        gauge_total_for_chart_and_rule_mode(None, 500, RuleMode::Dx),
        default_gauge_total(500)
    );
    assert_eq!(
        gauge_total_for_chart_and_rule_mode(None, 500, RuleMode::Lr2Oraja),
        lr2oraja_default_gauge_total(500)
    );
    assert_eq!(gauge_total_for_chart_and_rule_mode(Some(320.0), 500, RuleMode::Lr2Oraja), 320.0);
}

#[test]
fn bmson_total_percent_is_resolved_against_the_selected_rules_scored_note_count() {
    use bmz_chart::model::ChartMetadata;
    for (percent, expected) in [(100.0, 352.0), (0.0, 352.0), (-1.0, 352.0), (150.0, 528.0)] {
        let metadata = ChartMetadata {
            bmson_total_percent: Some(percent),
            total: Some(460.909),
            ..Default::default()
        };
        let total = gauge_total_for_play(&metadata, 1000, RuleMode::Lr2Oraja);
        assert_eq!(total, expected);
        let gauge = GaugeState::new_with_property_and_rule_mode(
            GaugeType::Normal,
            total,
            1000,
            GaugeProperty::Lr2,
            RuleMode::Lr2Oraja,
        );
        assert_eq!(gauge.current().definition.values[0], (expected / 1000.0) as f32);
    }
    let bms = ChartMetadata { total: Some(100.0), ..Default::default() };
    assert_eq!(gauge_total_for_play(&bms, 1000, RuleMode::Lr2Oraja), 100.0);
}

#[test]
fn beatoraja_modify_damage_matches_upstream_thresholds_and_signed_note_terms() {
    let gauge = definition_for_property(GaugeType::ExHard, GaugeProperty::FiveKeys);
    for (total, notes, expected) in
        [(200.0, 1000, -15.0), (128.0, 1000, -50.0), (260.0, 100, -10.0)]
    {
        let compiled = compile_gauge_definition(&gauge, total, notes);
        assert_eq!(compiled.values[GaugeJudgeIndex::Bd as usize], expected);
    }
}

#[test]
fn beatoraja_lr2_guts_start_below_30_percent() {
    for ty in [GaugeType::Hard, GaugeType::Class, GaugeType::ExClass] {
        for (value, rate) in [(29.999, 0.6), (30.0, 1.0), (30.001, 1.0)] {
            let mut gauge = GaugeState::new_with_property(ty, 260.0, 1000, GaugeProperty::Lr2);
            gauge.set_initial_value(value);
            let damage = gauge.current().definition.values[GaugeJudgeIndex::Bd as usize];
            gauge.apply_judge(Judge::Bad, 1.0);
            assert!((gauge.current().value - (value + damage * rate)).abs() < 0.0001);
        }
    }
}

#[test]
fn lr2oraja_exhardclass_closes_below_but_not_at_two_percent() {
    for (value, expected) in [(3.5, 0.0), (3.999, 0.0), (4.0, 2.0), (4.001, 2.001)] {
        let mut gauge = GaugeState::new_with_property_and_rule_mode(
            GaugeType::ExHardClass,
            352.0,
            1000,
            GaugeProperty::Lr2,
            RuleMode::Lr2Oraja,
        );
        gauge.set_initial_value(value);
        gauge.apply_judge(Judge::EmptyPoor, 1.0);
        assert!((gauge.current().value - expected).abs() < 0.0001);
    }
    assert_eq!(definition_for_property(GaugeType::ExHardClass, GaugeProperty::Lr2).death, 0.0);
    assert_eq!(
        definition_for_rule_mode_and_keymode(GaugeType::ExHardClass, RuleMode::Dx, KeyMode::K9)
            .death,
        0.0
    );
}

#[test]
fn modify_damage_scale_matches_lr2oraja_formula() {
    assert!((modify_damage_scale(225.0, 1000) - 10.0 / 9.0).abs() < 0.000_1);
    assert!((modify_damage_scale(205.0, 1000) - 10.0 / 7.0).abs() < 0.000_1);
    assert!((modify_damage_scale(115.0, 1000) - 5.0).abs() < 0.000_1);
    assert!((modify_damage_scale(260.0, 20) - 10.0).abs() < 0.000_1);
    assert!((modify_damage_scale(260.0, 25) - 9.0).abs() < 0.000_1);
    assert!((modify_damage_scale(260.0, 45) - 6.0).abs() < 0.000_1);
}

#[test]
fn creates_selected_gauge_state_from_defaults() {
    let gauge = GaugeState::new(GaugeType::Hard, 160.0, 1000);

    assert_eq!(gauge.selected, GaugeType::Hard);
    assert_eq!(gauge.current().definition.gauge_type, GaugeType::Hard);
    assert_eq!(gauge.current().value, 100.0);
}

#[test]
fn hazard_fails_on_any_damage_judge() {
    let mut gauge = GaugeState::new(GaugeType::Hazard, 160.0, 1000);

    gauge.apply_judge(Judge::Bad, 1.0);

    assert_eq!(gauge.current().value, 0.0);
    assert!(!gauge.current().is_qualified());
    assert!(gauge.current_closes_play_on_zero());
}

#[test]
fn auto_shift_starts_from_hazard_and_falls_back_to_exhard() {
    let mut gauge = GaugeState::new_auto_shift(160.0, 1000);

    // Poor at rate 1.0 drains Hazard (value -100) in one hit, then
    // ExHard only loses -16 and stays alive.
    gauge.apply_judge(Judge::Poor, 1.0);

    assert!(gauge.auto_shift);
    assert_eq!(gauge.original, GaugeType::Hazard);
    assert_eq!(gauge.selected, GaugeType::ExHard);
}

#[test]
fn auto_shift_result_uses_highest_qualified_gauge() {
    let mut gauge = GaugeState::new_auto_shift(160.0, 1000);

    gauge
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::Hazard)
        .unwrap()
        .value = 0.0;
    gauge
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::ExHard)
        .unwrap()
        .value = 0.0;
    gauge
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::Hard)
        .unwrap()
        .value = 0.0;
    gauge
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::Normal)
        .unwrap()
        .value = 70.0;
    gauge
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::Easy)
        .unwrap()
        .value = 82.0;
    gauge.selected = GaugeType::Normal;

    let result_gauge = gauge.result_gauge();

    assert_eq!(result_gauge.definition.gauge_type, GaugeType::Easy);
}

#[test]
fn continue_mode_does_not_fail_or_shift_at_zero() {
    let mut gauge =
        GaugeState::new_with_auto_shift(GaugeType::Hard, GaugeAutoShiftMode::Continue, 160.0, 1000);

    gauge.apply_judge(Judge::Poor, 20.0);

    assert_eq!(gauge.selected, GaugeType::Hard);
    assert_eq!(gauge.current().value, 0.0);
    assert!(!gauge.current_closes_play_on_zero());
}

#[test]
fn hard_to_groove_shifts_survival_gauge_to_normal() {
    let mut gauge = GaugeState::new_with_auto_shift(
        GaugeType::ExHard,
        GaugeAutoShiftMode::HardToGroove,
        160.0,
        1000,
    );

    gauge.apply_judge(Judge::Poor, 20.0);

    assert_eq!(gauge.selected, GaugeType::Normal);
}

#[test]
fn hard_to_groove_keeps_course_gauge_at_zero() {
    let mut gauge = GaugeState::new_with_auto_shift(
        GaugeType::ExHardClass,
        GaugeAutoShiftMode::HardToGroove,
        160.0,
        1000,
    );

    gauge.apply_judge(Judge::Poor, 200.0);

    assert_eq!(gauge.selected, GaugeType::ExHardClass);
    assert_eq!(gauge.current().value, 0.0);
}

fn definition_for(gauge_type: GaugeType) -> GaugeDefinition {
    default_gauge_definitions()
        .into_iter()
        .find(|def| def.gauge_type == gauge_type)
        .expect("definition exists")
}

fn definition_for_property(gauge_type: GaugeType, property: GaugeProperty) -> GaugeDefinition {
    gauge_definitions_for(property)
        .into_iter()
        .find(|def| def.gauge_type == gauge_type)
        .expect("definition exists")
}

fn definition_for_rule_mode(gauge_type: GaugeType, rule_mode: RuleMode) -> GaugeDefinition {
    gauge_definitions_for_rule_mode(
        GaugeProperty::for_rule_mode(KeyMode::K7, rule_mode, None),
        rule_mode,
    )
    .into_iter()
    .find(|def| def.gauge_type == gauge_type)
    .expect("definition exists")
}

fn definition_for_rule_mode_and_keymode(
    gauge_type: GaugeType,
    rule_mode: RuleMode,
    key_mode: KeyMode,
) -> GaugeDefinition {
    gauge_definitions_for_rule_mode_and_keymode(GaugeProperty::SevenKeys, rule_mode, key_mode)
        .into_iter()
        .find(|def| def.gauge_type == gauge_type)
        .expect("definition exists")
}

#[test]
fn class_gauges_start_full_and_clear_above_zero() {
    for &(ty, expected_clear) in &[
        (GaugeType::Class, Some(ClearType::Normal)),
        (GaugeType::ExClass, Some(ClearType::Hard)),
        (GaugeType::ExHardClass, Some(ClearType::ExHard)),
    ] {
        let def = definition_for(ty);
        assert_eq!(def.init, 100.0, "{ty:?} init");
        assert_eq!(def.max, 100.0, "{ty:?} max");
        assert_eq!(def.min, 0.0, "{ty:?} min");
        assert_eq!(def.border, 0.0, "{ty:?} border");
        assert_eq!(def.clear_type, expected_clear, "{ty:?} clear_type");
    }
}

#[test]
fn class_gauge_fails_at_zero_like_survival_gauges() {
    for ty in [GaugeType::Class, GaugeType::ExClass, GaugeType::ExHardClass] {
        let mut gauge = GaugeState::new(ty, 160.0, 1000);
        gauge.apply_judge(Judge::Poor, 200.0);
        assert_eq!(gauge.current().value, 0.0, "{ty:?} should drain to zero");
        assert!(!gauge.current().is_qualified(), "{ty:?} not qualified at zero");
        assert!(gauge.current_closes_play_on_zero(), "{ty:?} closes play on zero");
    }
}

#[test]
fn class_gauges_drain_strictly_more_than_normal() {
    let class = definition_for(GaugeType::Class);
    let exclass = definition_for(GaugeType::ExClass);
    let exhardclass = definition_for(GaugeType::ExHardClass);
    // Bad index = 3. Each tier should drain at least as hard as Class.
    assert!(class.values[3] >= exclass.values[3]);
    assert!(exclass.values[3] >= exhardclass.values[3]);
}

#[test]
fn gauge_property_from_keymode_matches_beatoraja_player_rule() {
    assert_eq!(GaugeProperty::from_keymode(KeyMode::K5), GaugeProperty::FiveKeys);
    assert_eq!(GaugeProperty::from_keymode(KeyMode::K10), GaugeProperty::FiveKeys);
    assert_eq!(GaugeProperty::from_keymode(KeyMode::K7), GaugeProperty::SevenKeys);
    assert_eq!(GaugeProperty::from_keymode(KeyMode::K14), GaugeProperty::SevenKeys);
}

#[test]
fn class_gauge_values_differ_per_property() {
    // beatoraja FIVEKEYS の CLASS は SEVENKEYS よりはるかにマイルド。
    let class_5 = definition_for_property(GaugeType::Class, GaugeProperty::FiveKeys);
    let class_7 = definition_for_property(GaugeType::Class, GaugeProperty::SevenKeys);
    assert_eq!(class_5.values, [0.01, 0.01, 0.0, -0.5, -1.0, -0.5]);
    assert_eq!(class_7.values, [0.15, 0.12, 0.06, -1.5, -3.0, -1.5]);

    // PMS の CLASS は SEVENKEYS と回復は同じだが EmptyPoor (idx 5) が厳しい (-3 vs -1.5)。
    let class_pms = definition_for_property(GaugeType::Class, GaugeProperty::Pms);
    assert_eq!(class_pms.values[5], -3.0);

    // LR2 EXHARDCLASS は突き抜けて重い (-12 BAD)。
    let exhardclass_lr2 = definition_for_property(GaugeType::ExHardClass, GaugeProperty::Lr2);
    assert_eq!(exhardclass_lr2.values[3], -12.0);
    let class_lr2 = definition_for_property(GaugeType::Class, GaugeProperty::Lr2);
    assert_eq!(class_lr2.guts, BEATORAJA_LR2_GUTS);

    // KEYBOARD CLASS は PG/GR 回復が 0.20 と高い。
    let class_kb = definition_for_property(GaugeType::Class, GaugeProperty::Keyboard);
    assert_eq!(class_kb.values[0], 0.20);
}

#[test]
fn groove_gauge_values_match_beatoraja_gauge_property() {
    let normal_7 = definition_for_property(GaugeType::Normal, GaugeProperty::SevenKeys);
    assert_eq!(normal_7.min, 2.0);
    assert_eq!(normal_7.max, 100.0);
    assert_eq!(normal_7.init, 20.0);
    assert_eq!(normal_7.border, 80.0);
    assert_eq!(normal_7.values, [1.0, 1.0, 0.5, -3.0, -6.0, -2.0]);

    let hard_7 = definition_for_property(GaugeType::Hard, GaugeProperty::SevenKeys);
    assert_eq!(hard_7.values, [0.15, 0.12, 0.03, -5.0, -10.0, -5.0]);
    assert_eq!(hard_7.guts, HARD_GUTS);

    let exhard_7 = definition_for_property(GaugeType::ExHard, GaugeProperty::SevenKeys);
    assert_eq!(exhard_7.values, [0.15, 0.06, 0.0, -8.0, -16.0, -8.0]);

    let normal_pms = definition_for_property(GaugeType::Normal, GaugeProperty::Pms);
    assert_eq!(normal_pms.max, 120.0);
    assert_eq!(normal_pms.init, 30.0);
    assert_eq!(normal_pms.border, 85.0);
    assert_eq!(normal_pms.values, [1.0, 1.0, 0.5, -2.0, -6.0, -6.0]);

    let normal_kb = definition_for_property(GaugeType::Normal, GaugeProperty::Keyboard);
    assert_eq!(normal_kb.border, 70.0);
    assert_eq!(normal_kb.values, [1.0, 1.0, 0.5, -2.0, -4.0, -2.0]);

    let lr2_hard = definition_for_property(GaugeType::Hard, GaugeProperty::Lr2);
    assert_eq!(lr2_hard.modifier, GaugeModifier::ModifyDamage);
    assert_eq!(lr2_hard.values, [0.1, 0.1, 0.05, -6.0, -10.0, -2.0]);
}

#[test]
fn lr2oraja_hard_gauge_uses_32_percent_guts_and_2_percent_death() {
    let hard = definition_for_rule_mode(GaugeType::Hard, RuleMode::Lr2Oraja);
    assert_eq!(hard.guts, LR2_HARD_GUTS);
    assert_eq!(hard.death, 2.0);

    let mut at_threshold = GaugeState::new_with_property_and_rule_mode(
        GaugeType::Hard,
        160.0,
        1000,
        GaugeProperty::Lr2,
        RuleMode::Lr2Oraja,
    );
    let hard_at_threshold = at_threshold
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::Hard)
        .unwrap();
    hard_at_threshold.value = 32.0;
    let damage_without_guts = hard_at_threshold.definition.values[GaugeJudgeIndex::Pr as usize];
    hard_at_threshold.apply(GaugeJudgeIndex::Pr, 1.0);
    assert_eq!(hard_at_threshold.value, 32.0 + damage_without_guts);

    let mut below_threshold = GaugeState::new_with_property_and_rule_mode(
        GaugeType::Hard,
        160.0,
        1000,
        GaugeProperty::Lr2,
        RuleMode::Lr2Oraja,
    );
    let hard_below_threshold = below_threshold
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::Hard)
        .unwrap();
    hard_below_threshold.value = 31.9;
    hard_below_threshold.apply(GaugeJudgeIndex::Pr, 1.0);
    assert!((hard_below_threshold.value - (31.9 + damage_without_guts * 0.6)).abs() < 0.000_1);

    hard_below_threshold.value = 2.1;
    hard_below_threshold.apply(GaugeJudgeIndex::Epr, 0.1);
    assert_eq!(hard_below_threshold.value, 0.0);
}

#[test]
fn dx_gauge_definitions_match_lr2oraja_iidx_mode() {
    let normal = definition_for_rule_mode(GaugeType::Normal, RuleMode::Dx);
    assert_eq!(normal.modifier, GaugeModifier::Iidx);
    assert_eq!(normal.init, 22.0);
    assert_eq!(normal.values, [1.0, 1.0, 0.5, -2.0, -6.0, -2.0]);

    let hard = definition_for_rule_mode(GaugeType::Hard, RuleMode::Dx);
    assert_eq!(hard.modifier, GaugeModifier::None);
    assert_eq!(hard.values, [0.16, 0.16, 0.0, -4.5, -9.0, -4.5]);
    assert_eq!(hard.guts, DX_HARD_GUTS);

    let mut gauge = GaugeState::new_with_property_and_rule_mode(
        GaugeType::Normal,
        999.0,
        1000,
        GaugeProperty::SevenKeys,
        RuleMode::Dx,
    );
    let start = gauge.current().value;
    gauge.apply_judge(Judge::PGreat, 1.0);
    assert!((gauge.current().value - (start + iidx_total_value(1000) / 1000.0)).abs() < 0.000_1);
}

#[test]
fn dx_iidx_hard_and_class_gauges_close_below_2_percent() {
    let death_types = [
        GaugeType::Hard,
        GaugeType::ExHard,
        GaugeType::Hazard,
        GaugeType::Class,
        GaugeType::ExClass,
        GaugeType::ExHardClass,
    ];
    for gauge_type in death_types {
        assert_eq!(definition_for_rule_mode(gauge_type, RuleMode::Dx).death, 2.0, "{gauge_type:?}");

        let mut state = GaugeState::new_with_property_and_rule_mode(
            gauge_type,
            160.0,
            1000,
            GaugeProperty::SevenKeys,
            RuleMode::Dx,
        );
        let gauge = state
            .gauges
            .iter_mut()
            .find(|gauge| gauge.definition.gauge_type == gauge_type)
            .unwrap();
        gauge.value = 3.0;
        gauge.apply_mine(0.9);
        assert!((gauge.value - 2.1).abs() < 0.000_1, "{gauge_type:?}");
        gauge.apply_mine(0.2);
        assert_eq!(gauge.value, 0.0, "{gauge_type:?}");
    }
    for gauge_type in [GaugeType::AssistEasy, GaugeType::Easy, GaugeType::Normal] {
        assert_eq!(definition_for_rule_mode(gauge_type, RuleMode::Dx).death, 0.0, "{gauge_type:?}");
    }

    // 判定によるダメージでも 2% 未満で閉店する。
    let mut state = GaugeState::new_with_property_and_rule_mode(
        GaugeType::ExHard,
        160.0,
        1000,
        GaugeProperty::SevenKeys,
        RuleMode::Dx,
    );
    let exhard = state
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::ExHard)
        .unwrap();
    exhard.value = 11.5;
    exhard.apply(GaugeJudgeIndex::Bd, 1.0);
    assert!((exhard.value - 2.5).abs() < 0.000_1);
    exhard.value = 10.5;
    exhard.apply(GaugeJudgeIndex::Bd, 1.0);
    assert_eq!(exhard.value, 0.0);
}

#[test]
fn dx_9key_pop_gauges_keep_zero_death_border() {
    for gauge_type in [
        GaugeType::AssistEasy,
        GaugeType::Easy,
        GaugeType::Normal,
        GaugeType::Hard,
        GaugeType::ExHard,
        GaugeType::Hazard,
        GaugeType::Class,
        GaugeType::ExClass,
        GaugeType::ExHardClass,
    ] {
        let definition =
            definition_for_rule_mode_and_keymode(gauge_type, RuleMode::Dx, KeyMode::K9);
        assert_eq!(definition.death, 0.0, "{gauge_type:?}");
    }
}

#[test]
fn dx_9key_gauge_definitions_match_endless_dream_pop_mode() {
    let assist =
        definition_for_rule_mode_and_keymode(GaugeType::AssistEasy, RuleMode::Dx, KeyMode::K9);
    assert_eq!(assist.modifier, GaugeModifier::Pop);
    assert_eq!((assist.min, assist.max, assist.init, assist.border), (2.0, 120.0, 30.0, 65.0));
    assert_eq!(assist.values, [1.2, 1.2, 0.6, -0.937, -2.929, -2.929]);

    let easy = definition_for_rule_mode_and_keymode(GaugeType::Easy, RuleMode::Dx, KeyMode::K9);
    assert_eq!((easy.min, easy.max, easy.init, easy.border), (2.0, 120.0, 30.0, 85.0));
    assert_eq!(easy.values, [1.2, 1.2, 0.6, -0.937, -2.929, -2.929]);

    let normal = definition_for_rule_mode_and_keymode(GaugeType::Normal, RuleMode::Dx, KeyMode::K9);
    assert_eq!(normal.modifier, GaugeModifier::Pop);
    assert_eq!((normal.min, normal.max, normal.init, normal.border), (2.0, 120.0, 30.0, 85.0));
    assert_eq!(normal.values, [1.2, 1.2, 0.6, -1.992, -5.976, -5.976]);

    let hard = definition_for_rule_mode_and_keymode(GaugeType::Hard, RuleMode::Dx, KeyMode::K9);
    assert_eq!(hard.modifier, GaugeModifier::Pop);
    assert_eq!(hard.values, [1.2, 1.2, 0.6, -3.984, -11.952, -11.952]);

    let exhard = definition_for_rule_mode_and_keymode(GaugeType::ExHard, RuleMode::Dx, KeyMode::K9);
    assert_eq!(exhard.values, [1.2, 1.2, 0.6, -7.968, -23.906, -23.906]);

    let hazard = definition_for_rule_mode_and_keymode(GaugeType::Hazard, RuleMode::Dx, KeyMode::K9);
    assert_eq!((hazard.min, hazard.max, hazard.init, hazard.border), (0.0, 100.0, 100.0, 0.0));
    assert_eq!(hazard.values, [0.15, 0.06, 0.0, -100.0, -100.0, -100.0]);

    let class = definition_for_rule_mode_and_keymode(GaugeType::Class, RuleMode::Dx, KeyMode::K9);
    assert_eq!(class.values, [0.15, 0.15, 0.06, -1.5, -3.0, -3.0]);
    assert_eq!(class.guts, DX_HARD_GUTS);

    let exclass =
        definition_for_rule_mode_and_keymode(GaugeType::ExClass, RuleMode::Dx, KeyMode::K9);
    assert_eq!(exclass.values, [0.15, 0.15, 0.03, -3.0, -6.0, -6.0]);
    let exhard_class =
        definition_for_rule_mode_and_keymode(GaugeType::ExHardClass, RuleMode::Dx, KeyMode::K9);
    assert_eq!(exhard_class.values, [0.15, 0.15, 0.0, -5.0, -10.0, -10.0]);

    let k7_with_pms_override =
        gauge_definitions_for_rule_mode_and_keymode(GaugeProperty::Pms, RuleMode::Dx, KeyMode::K7)
            .into_iter()
            .find(|definition| definition.gauge_type == GaugeType::Normal)
            .unwrap();
    assert_eq!(k7_with_pms_override.modifier, GaugeModifier::Iidx);
    assert_eq!(k7_with_pms_override.init, 22.0);
}

#[test]
fn dx_9key_damage_preserves_clear_border_and_floor() {
    for (gauge_type, bad, poor, border) in [
        (GaugeType::AssistEasy, 0.937, 2.929, 65.0),
        (GaugeType::Easy, 0.937, 2.929, 85.0),
        (GaugeType::Normal, 1.992, 5.976, 85.0),
        (GaugeType::Hard, 3.984, 11.952, 85.0),
        (GaugeType::ExHard, 7.968, 23.906, 85.0),
    ] {
        // Damage is independent of chart TOTAL and the dense-chart GOOD boost.
        for (total, total_notes) in [(160.0, 1), (999.0, 1536), (999.0, 1537)] {
            let mut gauge = GaugeState::new_with_property_and_rule_mode_and_keymode(
                gauge_type,
                total,
                total_notes,
                GaugeProperty::Pms,
                RuleMode::Dx,
                KeyMode::K9,
            );
            for (judge, damage) in
                [(Judge::Bad, bad), (Judge::Poor, poor), (Judge::EmptyPoor, poor)]
            {
                gauge.set_initial_value(120.0);
                gauge.apply_judge(judge, 1.0);
                assert!((gauge.current().value - (120.0 - damage)).abs() < 0.000_1);

                for margin in [-0.01, 0.0, 0.01] {
                    gauge.set_initial_value(border + damage + margin);
                    gauge.apply_judge(judge, 1.0);
                    assert!((gauge.current().value - (border + margin)).abs() < 0.000_1);
                    assert_eq!(gauge.current().is_qualified(), margin >= 0.0);
                }

                gauge.set_initial_value(2.0 + damage + 0.01);
                gauge.apply_judge(judge, 1.0);
                assert!((gauge.current().value - 2.01).abs() < 0.000_1);
                assert!(!gauge.current_closes_play_on_zero());
                gauge.apply_judge(judge, 1.0);
                assert_eq!(gauge.current().value, 2.0);
                assert_eq!(
                    gauge.current_closes_play_on_zero(),
                    matches!(gauge_type, GaugeType::Hard | GaugeType::ExHard)
                );
                assert!(!gauge.current().is_qualified());
            }

            gauge.set_initial_value(30.0);
            gauge.apply_hcn_drain();
            assert!((gauge.current().value - (30.0 - bad * 0.5)).abs() < 0.000_1);
        }
    }
}

#[test]
fn dx_9key_pop_recovery_uses_note_count_formula_and_dense_good_boost() {
    assert_eq!(pop_total_value(0), 0.0);
    assert_eq!(pop_total_value(3072), 300.0);
    assert_eq!(pop_total_value(3073), 298.0);

    let normal = definition_for_rule_mode_and_keymode(GaugeType::Normal, RuleMode::Dx, KeyMode::K9);
    let below = compile_gauge_definition(&normal, 999.0, 1536);
    let dense = compile_gauge_definition(&normal, 999.0, 1537);
    assert!((below.values[GaugeJudgeIndex::Pg as usize] - 0.234_375).abs() < 0.000_001);
    assert!((below.values[GaugeJudgeIndex::Gd as usize] - 0.117_187_5).abs() < 0.000_001);
    assert!((dense.values[GaugeJudgeIndex::Pg as usize] - 0.117_187_5).abs() < 0.000_001);
    assert!((dense.values[GaugeJudgeIndex::Gd as usize] - 0.117_187_5).abs() < 0.000_001);

    let empty = compile_gauge_definition(&normal, 999.0, 0);
    assert_eq!(empty.values[GaugeJudgeIndex::Pg as usize], 0.0);
    assert_eq!(empty.values[GaugeJudgeIndex::Bd as usize], -1.992);
}

#[test]
fn hcn_gauge_updates_use_beatoraja_great_and_bad_half_rate() {
    let mut gauge = GaugeState::new(GaugeType::Normal, 160.0, 1000);
    let start = gauge.current().value;

    // 1 tick = GREAT × 0.5 / BAD × 0.5 (beatoraja gauge.update(1|3, 0.5f))
    gauge.apply_hcn_hold();
    assert!((gauge.current().value - (start + 0.08)).abs() < f32::EPSILON);

    gauge.apply_hcn_drain();
    assert!((gauge.current().value - (start - 1.42)).abs() < 0.000_1);
}

#[test]
fn set_initial_value_carries_over_class_gauge() {
    let mut gauge = GaugeState::new(GaugeType::Class, 160.0, 1000);
    gauge.set_initial_value(45.0);
    assert_eq!(gauge.current().value, 45.0);
    assert!(gauge.current().is_qualified());
}

#[test]
fn set_initial_values_carries_auto_shift_gauges_independently() {
    let mut gauge = GaugeState::new_with_auto_shift(
        GaugeType::ExHardClass,
        GaugeAutoShiftMode::BestClear,
        160.0,
        1000,
    );

    gauge.set_initial_values(&[
        GaugeCarryValue { gauge_type: GaugeType::ExHardClass, value: 0.0 },
        GaugeCarryValue { gauge_type: GaugeType::ExClass, value: 72.0 },
        GaugeCarryValue { gauge_type: GaugeType::Class, value: 91.0 },
    ]);

    assert_eq!(gauge.gauge(GaugeType::ExHardClass).map(|gauge| gauge.value), Some(0.0));
    assert_eq!(gauge.gauge(GaugeType::ExClass).map(|gauge| gauge.value), Some(72.0));
    assert_eq!(gauge.gauge(GaugeType::Class).map(|gauge| gauge.value), Some(91.0));
    assert_eq!(gauge.selected, GaugeType::ExClass);
}

#[test]
fn select_to_under_result_does_not_exceed_original_gauge() {
    let mut gauge = GaugeState::new_with_auto_shift(
        GaugeType::Hard,
        GaugeAutoShiftMode::SelectToUnder,
        160.0,
        1000,
    );
    gauge
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::ExHard)
        .unwrap()
        .value = 100.0;
    gauge
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::Hard)
        .unwrap()
        .value = 90.0;

    let result_gauge = gauge.result_gauge();

    assert_eq!(result_gauge.definition.gauge_type, GaugeType::Hard);
}

#[test]
fn best_clear_uses_course_gauge_order_for_class_gauges() {
    let mut gauge = GaugeState::new_with_auto_shift(
        GaugeType::ExHardClass,
        GaugeAutoShiftMode::BestClear,
        160.0,
        1000,
    );
    assert_eq!(gauge.selected, GaugeType::ExHardClass);
    gauge
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::ExHardClass)
        .unwrap()
        .value = 0.0;
    gauge
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::ExClass)
        .unwrap()
        .value = 100.0;

    gauge.apply_judge(Judge::PGreat, 0.0);

    assert_eq!(gauge.selected, GaugeType::ExClass);
    assert_eq!(gauge.result_gauge().definition.gauge_type, GaugeType::ExClass);
}

#[test]
fn select_to_under_result_does_not_exceed_original_course_gauge() {
    let mut gauge = GaugeState::new_with_auto_shift(
        GaugeType::ExClass,
        GaugeAutoShiftMode::SelectToUnder,
        160.0,
        1000,
    );
    gauge
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::ExHardClass)
        .unwrap()
        .value = 100.0;
    gauge
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::ExClass)
        .unwrap()
        .value = 90.0;

    let result_gauge = gauge.result_gauge();

    assert_eq!(result_gauge.definition.gauge_type, GaugeType::ExClass);
}

#[test]
fn auto_shift_respects_bottom_shiftable_gauge() {
    let mut gauge = GaugeState::new_with_auto_shift(
        GaugeType::ExHard,
        GaugeAutoShiftMode::BestClear,
        160.0,
        1000,
    );
    gauge.set_bottom_shiftable_gauge(GaugeType::Normal);
    for gauge in &mut gauge.gauges {
        gauge.value = 0.0;
    }
    gauge
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::Easy)
        .unwrap()
        .value = 100.0;

    gauge.apply_judge(Judge::Poor, 1.0);

    assert_eq!(gauge.selected, GaugeType::Normal);
}

#[test]
fn auto_shift_result_respects_bottom_shiftable_gauge() {
    let mut gauge = GaugeState::new_with_auto_shift(
        GaugeType::ExHard,
        GaugeAutoShiftMode::BestClear,
        160.0,
        1000,
    );
    gauge.set_bottom_shiftable_gauge(GaugeType::Normal);
    for gauge in &mut gauge.gauges {
        gauge.value = 0.0;
    }
    gauge
        .gauges
        .iter_mut()
        .find(|gauge| gauge.definition.gauge_type == GaugeType::AssistEasy)
        .unwrap()
        .value = 100.0;
    gauge.selected = GaugeType::Normal;

    let result_gauge = gauge.result_gauge();

    assert_eq!(result_gauge.definition.gauge_type, GaugeType::Normal);
    assert!(!result_gauge.is_qualified());
}

#[test]
fn assist_easy_gauge_clears_with_light_assist_lamp_in_every_definition_table() {
    // beatoraja `ClearType.getClearTypeByGauge(ASSISTEASY)` は LightAssistEasy。
    // ゲージ選択だけで AssistEasy (実効Assist用のランプ) にはしない。
    let mut tables = Vec::new();
    for property in [
        GaugeProperty::FiveKeys,
        GaugeProperty::SevenKeys,
        GaugeProperty::Pms,
        GaugeProperty::Keyboard,
        GaugeProperty::Lr2,
    ] {
        tables.push((format!("{property:?}"), gauge_definitions_for(property)));
    }
    for rule_mode in [RuleMode::Beatoraja, RuleMode::Lr2Oraja, RuleMode::Dx] {
        for key_mode in [KeyMode::K7, KeyMode::K9] {
            tables.push((
                format!("{rule_mode:?}/{key_mode:?}"),
                gauge_definitions_for_rule_mode_and_keymode(
                    GaugeProperty::SevenKeys,
                    rule_mode,
                    key_mode,
                ),
            ));
        }
    }

    for (label, definitions) in tables {
        let clear_type_of = |gauge_type: GaugeType| {
            definitions
                .iter()
                .find(|definition| definition.gauge_type == gauge_type)
                .and_then(|definition| definition.clear_type)
        };
        assert_eq!(
            clear_type_of(GaugeType::AssistEasy),
            Some(ClearType::LightAssistEasy),
            "{label}"
        );
        assert_eq!(clear_type_of(GaugeType::Easy), Some(ClearType::Easy), "{label}");
        assert_eq!(clear_type_of(GaugeType::Normal), Some(ClearType::Normal), "{label}");
        assert_eq!(clear_type_of(GaugeType::Hard), Some(ClearType::Hard), "{label}");
        assert_eq!(clear_type_of(GaugeType::ExHard), Some(ClearType::ExHard), "{label}");
        assert!(
            definitions
                .iter()
                .all(|definition| definition.clear_type != Some(ClearType::AssistEasy)),
            "{label}"
        );
    }
}

/// OpenLR2 DEATH (`judge_damage[2]`): no recovery, -100 on BAD / POOR and no
/// damage on EMPTY POOR. LR2oraja's HAZARD keeps its own values.
#[test]
fn legacy_hazard_matches_openlr2_death_gauge() {
    let legacy = definition_for_rule_mode(GaugeType::Hazard, RuleMode::Legacy);
    assert_eq!(legacy.values, [0.0, 0.0, 0.0, -100.0, -100.0, 0.0]);
    let lr2oraja = definition_for_rule_mode(GaugeType::Hazard, RuleMode::Lr2Oraja);
    assert_eq!(lr2oraja.values, [0.15, 0.06, 0.0, -100.0, -100.0, -10.0]);

    let normal = definition_for_rule_mode(GaugeType::Normal, RuleMode::Legacy);
    assert_eq!(normal.values, [1.0, 1.0, 0.5, -4.0, -6.0, -2.0]);
}

fn legacy_gauge(gauge_type: GaugeType, total: f64, notes: u32) -> GaugeState {
    GaugeState::new_with_property_and_rule_mode(
        gauge_type,
        total,
        notes,
        GaugeProperty::Lr2,
        RuleMode::Legacy,
    )
}

fn legacy_hp(gauge: &GaugeState, gauge_type: GaugeType) -> f64 {
    gauge.gauge(gauge_type).and_then(|g| g.legacy_hp).expect("legacy gauge")
}

/// LR2_bmsload.cpp: `total / (float)notes` is a float division stored as a
/// double, and the HARD damage factor uses `int recover`.
#[test]
fn legacy_damage_table_keeps_lr2_c_types() {
    let groove = legacy_gauge_rule(GaugeType::Normal, 480.0, 619).unwrap();
    assert_eq!(groove.damage[5], f64::from(480.0_f32 / 619.0_f32));
    assert_eq!(groove.damage[3], f64::from(480.0_f32 / 1238.0_f32));
    assert_eq!(groove.damage[..3], [-2.0, -6.0, -4.0]);
    let easy = legacy_gauge_rule(GaugeType::Easy, 480.0, 619).unwrap();
    assert_eq!(easy.damage[5], f64::from(480.0_f32 / 619.0_f32) * 1.2);
    assert_eq!(easy.damage[1], -4.800000000000001);
    // 300 notes: notebase 3 - 50/250 = 2.8; TOTAL 200: recover = 7.
    let hard = legacy_gauge_rule(GaugeType::Hard, 200.0, 300).unwrap();
    let dmg = (2.8_f64 * 10.0).max(100.0 / 7.0) / 10.0;
    assert_eq!(hard.damage[..3], [dmg * -2.0, dmg * -10.0, dmg * -6.0]);
    assert_eq!(hard.damage[3..], [0.05, 0.1, 0.1]);
    let death = legacy_gauge_rule(GaugeType::Hazard, 200.0, 300).unwrap();
    assert_eq!(death.damage, [0.0, -100.0, -100.0, 0.0, 0.0, 0.0]);
}

#[test]
fn legacy_total_is_lr2_int_with_its_default() {
    assert_eq!(gauge_total_for_chart_and_rule_mode(Some(160.9), 500, RuleMode::Legacy), 160.0);
    // Below 1 is replaced: 1000 notes -> (400 / 5 + 360) * 0.8 = 352, truncated.
    assert_eq!(gauge_total_for_chart_and_rule_mode(Some(0.5), 1000, RuleMode::Legacy), 352.0);
    assert_eq!(openlr2_default_gauge_total(500), 256.0);
    assert_eq!(openlr2_default_gauge_total(100), 176.0);
}

/// The 0.6 scale uses the even-floored HP: 32.0000001 is 32 (full damage),
/// 31.9999 is 30 (scaled). f32 rounding used to put these on the wrong side.
#[test]
fn legacy_hard_scale_uses_the_even_floored_double_hp() {
    let mut gauge = legacy_gauge(GaugeType::Hard, 200.0, 2000);
    let dmg = legacy_gauge_rule(GaugeType::Hard, 200.0, 2000).unwrap().damage[1];
    let hard =
        gauge.gauges.iter_mut().find(|g| g.definition.gauge_type == GaugeType::Hard).unwrap();
    hard.legacy_hp = Some(32.000000000000625);
    hard.apply(GaugeJudgeIndex::Pr, 1.0);
    assert_eq!(hard.legacy_hp, Some(32.000000000000625 + dmg));
    hard.legacy_hp = Some(31.9999);
    hard.apply(GaugeJudgeIndex::Pr, 1.0);
    assert_eq!(hard.legacy_hp, Some(31.9999 + dmg * 0.6));
}

/// HP in (0, 2) is shown as 0 and closes play; the next judgement locks it to
/// 0 even if it would recover (`if (hp <= 0) HP = 0`).
#[test]
fn legacy_hard_below_two_closes_and_locks_on_the_next_judgement() {
    let mut gauge = legacy_gauge(GaugeType::Hard, 200.0, 2000);
    let hard =
        gauge.gauges.iter_mut().find(|g| g.definition.gauge_type == GaugeType::Hard).unwrap();
    // EMPTY POOR: 3 - 2 * 1.4 * 0.6 (dmg 1.4 for TOTAL 200, 2000 notes).
    hard.legacy_hp = Some(3.0);
    hard.apply(GaugeJudgeIndex::Epr, 1.0);
    let hp = hard.legacy_hp.unwrap();
    assert!(hp > 0.0 && hp < 2.0, "{hp}");
    assert_eq!(hard.value, 0.0);
    hard.apply(GaugeJudgeIndex::Pg, 1.0);
    assert_eq!(hard.legacy_hp, Some(0.0));
    assert!(gauge.current_closes_play_on_zero());
}

#[test]
fn legacy_groove_floor_and_mine_then_pgreat_order() {
    let mut gauge = legacy_gauge(GaugeType::Normal, 480.0, 619);
    let pg = legacy_gauge_rule(GaugeType::Normal, 480.0, 619).unwrap().damage[5];
    gauge.apply_mine(11.0);
    assert_eq!(legacy_hp(&gauge, GaugeType::Normal), 9.0);
    gauge.apply_mine(11.0);
    assert_eq!(legacy_hp(&gauge, GaugeType::Normal), 2.0);
    gauge.apply_judge(Judge::PGreat, 1.0);
    assert_eq!(legacy_hp(&gauge, GaugeType::Normal), 2.0 + pg);
    // GROOVE is not a closing gauge.
    assert!(!gauge.current_closes_play_on_zero());
}

#[test]
fn legacy_clear_border_uses_the_double_hp() {
    let mut gauge = legacy_gauge(GaugeType::Normal, 480.0, 619);
    let groove =
        gauge.gauges.iter_mut().find(|g| g.definition.gauge_type == GaugeType::Normal).unwrap();
    groove.legacy_hp = Some(79.999999999);
    groove.value = 79.999999999_f64 as f32;
    assert_eq!(groove.value, 80.0);
    assert!(!groove.is_qualified());
}

/// A POTION recovery reaches LEGACY's exact HP, so the next judgement keeps it.
#[test]
fn legacy_potion_recovery_survives_the_next_judgement() {
    let mut gauge = legacy_gauge(GaugeType::Normal, 400.0, 1_000);
    gauge.set_initial_value(20.0);
    gauge.apply_potion(10.0);
    assert_eq!(legacy_hp(&gauge, GaugeType::Normal), 30.0);
    gauge.apply_judge(Judge::PGreat, 1.0);
    let hp = legacy_hp(&gauge, GaugeType::Normal);
    assert!((hp - 30.4).abs() < 1e-6, "{hp}");
    assert_eq!(gauge.current().value, hp as f32);
}
