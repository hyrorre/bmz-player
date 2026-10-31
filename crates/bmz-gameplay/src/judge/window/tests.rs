use super::*;

fn base_window() -> JudgeWindow {
    JudgeWindow::symmetric(16_000, 40_000, 80_000, 120_000, 500_000, 200_000, 16_000)
}

#[test]
fn playback_rate_scales_chart_windows_to_keep_wall_time_fixed() {
    let windows = JudgeWindows {
        note: base_window(),
        scratch: base_window(),
        long_note_end: base_window(),
        long_scratch_end: base_window(),
        long_note_release_margin_us: 200_000,
        long_scratch_release_margin_us: 150_000,
    };

    let slow = scale_judge_windows_for_playback_rate(windows, 50);
    let fast = scale_judge_windows_for_playback_rate(windows, 200);

    assert_eq!(slow.note.pgreat_us, 8_000);
    assert_eq!(slow.note.empty_poor_fast_us, 250_000);
    assert_eq!(slow.long_note_release_margin_us, 100_000);
    assert_eq!(fast.note.pgreat_us, 32_000);
    assert_eq!(fast.note.empty_poor_fast_us, 1_000_000);
    assert_eq!(fast.long_scratch_release_margin_us, 300_000);
}

fn rank_spec(value: i32, kind: JudgeRankKind) -> JudgeRankSpec {
    JudgeRankSpec { value, kind }
}

#[test]
fn maps_bms_rank_levels_to_percent() {
    assert_eq!(judge_rank_to_percent(0), 25);
    assert_eq!(judge_rank_to_percent(1), 50);
    assert_eq!(judge_rank_to_percent(2), 75);
    assert_eq!(judge_rank_to_percent(3), 100);
    assert_eq!(judge_rank_to_percent(4), 125);
    assert_eq!(judge_rank_to_percent(120), 120);
    assert_eq!(judge_rank_to_percent(-1), 75);
    assert_eq!(judge_rank_to_percent(9), 75);
}

#[test]
fn none_rank_uses_easy_default() {
    assert_eq!(judge_rank_to_percent_optional(None), 100);
}

#[test]
fn beatoraja_pms_rank_levels_follow_pms_table() {
    assert_eq!(beatoraja_judge_rank_to_percent_for_keymode(0, KeyMode::K9), 33);
    assert_eq!(beatoraja_judge_rank_to_percent_for_keymode(1, KeyMode::K9), 50);
    assert_eq!(beatoraja_judge_rank_to_percent_for_keymode(2, KeyMode::K9), 70);
    assert_eq!(beatoraja_judge_rank_to_percent_for_keymode(3, KeyMode::K9), 100);
    assert_eq!(beatoraja_judge_rank_to_percent_for_keymode(4, KeyMode::K9), 133);
    assert_eq!(beatoraja_judge_rank_to_percent_for_keymode(120, KeyMode::K9), 120);
    assert_eq!(beatoraja_judge_rank_to_percent_for_keymode(-1, KeyMode::K9), 70);
}

#[test]
fn beatoraja_rank_specs_follow_validate_rules() {
    assert_eq!(
        judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
            Some(rank_spec(100, JudgeRankKind::DefExRank)),
            KeyMode::K7,
            RuleMode::Beatoraja,
        ),
        75
    );
    assert_eq!(
        judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
            Some(rank_spec(100, JudgeRankKind::DefExRank)),
            KeyMode::K9,
            RuleMode::Beatoraja,
        ),
        70
    );
    assert_eq!(
        judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
            Some(rank_spec(125, JudgeRankKind::DefExRank)),
            KeyMode::K9,
            RuleMode::Beatoraja,
        ),
        87
    );
    assert_eq!(
        judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
            Some(rank_spec(120, JudgeRankKind::BmsonJudgeRank)),
            KeyMode::K9,
            RuleMode::Beatoraja,
        ),
        120
    );
    assert_eq!(
        judge_rank_spec_to_percent_optional_for_keymode_and_rule_mode(
            Some(rank_spec(0, JudgeRankKind::BmsonJudgeRank)),
            KeyMode::K9,
            RuleMode::Beatoraja,
        ),
        100
    );
}

#[test]
fn lr2oraja_rank_levels_follow_lr2_fallbacks() {
    assert_eq!(judge_rank_to_percent_optional_for_rule_mode(None, RuleMode::Lr2Oraja), 75);
    assert_eq!(judge_rank_to_percent_for_rule_mode(0, RuleMode::Lr2Oraja), 25);
    assert_eq!(judge_rank_to_percent_for_rule_mode(1, RuleMode::Lr2Oraja), 50);
    assert_eq!(judge_rank_to_percent_for_rule_mode(2, RuleMode::Lr2Oraja), 75);
    assert_eq!(judge_rank_to_percent_for_rule_mode(3, RuleMode::Lr2Oraja), 100);
    assert_eq!(judge_rank_to_percent_for_rule_mode(4, RuleMode::Lr2Oraja), 75);
}

#[test]
fn lr2oraja_defexrank_scales_against_normal_rank() {
    assert_eq!(
        judge_rank_spec_to_percent_optional_for_rule_mode(
            Some(rank_spec(100, JudgeRankKind::DefExRank)),
            RuleMode::Lr2Oraja,
        ),
        75
    );
    assert_eq!(
        judge_rank_spec_to_percent_optional_for_rule_mode(
            Some(rank_spec(125, JudgeRankKind::DefExRank)),
            RuleMode::Lr2Oraja,
        ),
        93
    );
    assert_eq!(
        judge_rank_spec_to_percent_optional_for_rule_mode(
            Some(rank_spec(0, JudgeRankKind::DefExRank)),
            RuleMode::Lr2Oraja,
        ),
        75
    );
}

#[test]
fn lr2oraja_bmson_rank_uses_raw_percent() {
    assert_eq!(
        judge_rank_spec_to_percent_optional_for_rule_mode(
            Some(rank_spec(100, JudgeRankKind::BmsonJudgeRank)),
            RuleMode::Lr2Oraja,
        ),
        100
    );
    assert_eq!(
        judge_rank_spec_to_percent_optional_for_rule_mode(
            Some(rank_spec(0, JudgeRankKind::BmsonJudgeRank)),
            RuleMode::Lr2Oraja,
        ),
        100
    );
}

#[test]
fn scales_timing_judges_only() {
    let scaled = judge_window_for_rank(base_window(), 50);
    assert_eq!(scaled.pgreat_us, 8_000);
    assert_eq!(scaled.great_us, 20_000);
    assert_eq!(scaled.good_us, 40_000);
    assert_eq!(scaled.bad_fast_us, 60_000);
    assert_eq!(scaled.bad_slow_us, 60_000);
    assert_eq!(scaled.empty_poor_fast_us, 500_000);
    assert_eq!(scaled.empty_poor_slow_us, 200_000);
    assert_eq!(scaled.mine_hit_us, 16_000);
}

#[test]
fn very_hard_rank_halves_pgreat_window() {
    let window = judge_window_from_chart_rank(Some(0), base_window());
    assert_eq!(window.pgreat_us, 4_000);
}

#[test]
fn beatoraja_pms_scaling_keeps_fixed_pgreat_and_bad() {
    let base = beatoraja_note_judge_window_for_keymode(KeyMode::K9);
    let window = judge_window_for_rule_mode_and_keymode(base, 70, RuleMode::Beatoraja, KeyMode::K9);

    assert_eq!(window.pgreat_us, 20_000);
    assert_eq!(window.great_us, 35_000);
    assert_eq!(window.good_us, 81_900);
    assert_eq!(window.bad_fast_us, 183_000);
    assert_eq!(window.bad_slow_us, 183_000);
    assert_eq!(window.empty_poor_fast_us, 500_000);
    assert_eq!(window.empty_poor_slow_us, 175_000);
}

#[test]
fn beatoraja_pms_very_hard_clamps_great_to_fixed_pgreat() {
    let base = beatoraja_note_judge_window_for_keymode(KeyMode::K9);
    let window = judge_window_for_rule_mode_and_keymode(base, 33, RuleMode::Beatoraja, KeyMode::K9);

    assert_eq!(window.pgreat_us, 20_000);
    assert_eq!(window.great_us, 20_000);
    assert_eq!(window.good_us, 38_610);
    assert_eq!(window.bad_fast_us, 183_000);
}

#[test]
fn beatoraja_normal_rule_keeps_judge_windows_monotonic() {
    let base = beatoraja_long_scratch_end_judge_window_for_keymode(KeyMode::K5);
    let window =
        judge_window_for_rule_mode_and_keymode(base, 100, RuleMode::Beatoraja, KeyMode::K5);

    assert_eq!(window.pgreat_us, 130_000);
    assert_eq!(window.great_us, 160_000);
    assert_eq!(window.good_us, 160_000);
    assert_eq!(window.bad_fast_us, 260_000);
}

#[test]
fn beatoraja_7k_note_window_uses_asymmetric_bad_and_empty_poor() {
    let window = beatoraja_note_judge_window_for_keymode(KeyMode::K7);
    assert_eq!(window.pgreat_us, 20_000);
    assert_eq!(window.great_us, 60_000);
    assert_eq!(window.good_us, 150_000);
    assert_eq!(window.bad_fast_us, 220_000);
    assert_eq!(window.bad_slow_us, 280_000);
    assert_eq!(window.empty_poor_fast_us, 500_000);
    assert_eq!(window.empty_poor_slow_us, 150_000);
}

#[test]
fn beatoraja_other_keymodes_use_7k_empty_poor_window() {
    let seven = beatoraja_note_judge_window_for_keymode(KeyMode::K7);
    assert_eq!(beatoraja_note_judge_window_for_keymode(KeyMode::K4), seven);
    assert_eq!(beatoraja_note_judge_window_for_keymode(KeyMode::K6), seven);
    assert_eq!(beatoraja_note_judge_window_for_keymode(KeyMode::K8), seven);
}

#[test]
fn beatoraja_7k_scratch_window_uses_scratch_table() {
    let window = beatoraja_scratch_judge_window_for_keymode(KeyMode::K7);

    assert_eq!(window.pgreat_us, 30_000);
    assert_eq!(window.great_us, 70_000);
    assert_eq!(window.good_us, 160_000);
    assert_eq!(window.bad_fast_us, 230_000);
    assert_eq!(window.bad_slow_us, 290_000);
    assert_eq!(window.empty_poor_fast_us, 500_000);
    assert_eq!(window.empty_poor_slow_us, 160_000);
}

#[test]
fn beatoraja_long_note_end_windows_use_long_tables() {
    let five = beatoraja_long_note_end_judge_window_for_keymode(KeyMode::K5);
    assert_eq!(five.pgreat_us, 120_000);
    assert_eq!(five.great_us, 150_000);
    assert_eq!(five.good_us, 200_000);
    assert_eq!(five.bad_fast_us, 250_000);
    assert_eq!(five.bad_slow_us, 250_000);

    let seven_scratch = beatoraja_long_scratch_end_judge_window_for_keymode(KeyMode::K7);
    assert_eq!(seven_scratch.pgreat_us, 130_000);
    assert_eq!(seven_scratch.great_us, 170_000);
    assert_eq!(seven_scratch.good_us, 210_000);
    assert_eq!(seven_scratch.bad_fast_us, 230_000);
    assert_eq!(seven_scratch.bad_slow_us, 290_000);
}

#[test]
fn lr2oraja_rank_scaling_matches_reference_table() {
    let base = lr2oraja_note_judge_window();
    let window = judge_window_for_rule_mode(base, 50, RuleMode::Lr2Oraja);

    assert_eq!(window.pgreat_us, 15_000);
    assert_eq!(window.great_us, 30_000);
    assert_eq!(window.good_us, 60_000);
    assert_eq!(window.bad_fast_us, 200_000);
    assert_eq!(window.empty_poor_fast_us, 1_000_000);
    assert_eq!(window.empty_poor_slow_us, 0);
}

#[test]
fn lr2oraja_default_rank_scales_note_and_long_end_windows() {
    let base = lr2oraja_judge_windows();
    let window = judge_windows_for_rule_mode(base, 75, RuleMode::Lr2Oraja);

    assert_eq!(window.note.pgreat_us, 18_000);
    assert_eq!(window.note.great_us, 40_000);
    assert_eq!(window.note.good_us, 100_000);
    assert_eq!(window.note.bad_fast_us, 200_000);
    assert_eq!(window.note.empty_poor_fast_us, 1_000_000);

    assert_eq!(window.long_note_end.pgreat_us, 100_000);
    assert_eq!(window.long_note_end.great_us, 100_000);
    assert_eq!(window.long_note_end.good_us, 100_000);
    assert_eq!(window.long_note_end.bad_fast_us, 200_000);
    assert_eq!(window.long_note_end.empty_poor_fast_us, 0);
}

#[test]
fn dx_mode_uses_iidx_window_without_rank_scaling() {
    let base = dx_note_judge_window();
    let window = judge_window_for_rule_mode(base, 25, RuleMode::Dx);

    assert_eq!(window.pgreat_us, 16_666);
    assert_eq!(window.great_us, 33_333);
    assert_eq!(window.good_us, 116_666);
    assert_eq!(window.bad_fast_us, 200_000);
    assert_eq!(window.empty_poor_fast_us, 500_000);
    assert_eq!(window.empty_poor_slow_us, 200_000);
}

#[test]
fn dx_mode_uses_iidx_long_note_end_window() {
    let windows = judge_windows_for_keymode_and_rule_mode(KeyMode::K7, RuleMode::Dx);

    assert_eq!(windows.note.pgreat_us, 16_666);
    assert_eq!(windows.note.great_us, 33_333);
    assert_eq!(windows.note.good_us, 116_666);
    assert_eq!(windows.scratch, windows.note);
    assert_eq!(windows.long_note_end.pgreat_us, 116_666);
    assert_eq!(windows.long_note_end.great_us, 116_666);
    assert_eq!(windows.long_note_end.good_us, 116_666);
    assert_eq!(windows.long_note_end.bad_fast_us, 200_000);
    assert_eq!(windows.long_note_end.empty_poor_fast_us, 0);
    assert_eq!(windows.long_scratch_end, windows.long_note_end);
    assert_eq!(windows.long_note_release_margin_us, 0);
}

#[test]
fn dx_9key_uses_fixed_pop_windows_and_release_margin() {
    let windows = judge_windows_for_keymode_and_rule_mode(KeyMode::K9, RuleMode::Dx);

    assert_eq!(windows.note.pgreat_us, 25_000);
    assert_eq!(windows.note.great_us, 50_000);
    assert_eq!(windows.note.good_us, 87_500);
    assert_eq!(windows.note.bad_fast_us, 100_000);
    assert_eq!(windows.note.bad_slow_us, 100_000);
    assert_eq!(windows.note.empty_poor_fast_us, 500_000);
    assert_eq!(windows.note.empty_poor_slow_us, 112_500);
    assert_eq!(windows.long_note_end.pgreat_us, 120_000);
    assert_eq!(windows.long_note_end.great_us, 150_000);
    assert_eq!(windows.long_note_end.good_us, 217_000);
    assert_eq!(windows.long_note_end.bad_fast_us, 283_000);
    assert_eq!(windows.long_note_release_margin_us, 200_000);

    let scaled = judge_windows_for_rule_mode_and_keymode(windows, 25, RuleMode::Dx, KeyMode::K9);
    assert_eq!(scaled, windows);
}

#[test]
fn beatoraja_9key_uses_pms_release_margin() {
    let windows = judge_windows_for_keymode_and_rule_mode(KeyMode::K9, RuleMode::Beatoraja);
    assert_eq!(windows.long_note_release_margin_us, 200_000);
    assert_eq!(windows.long_scratch_release_margin_us, 0);
}

#[test]
fn dx_ignores_rank_and_exrank_events() {
    use bmz_chart::model::JudgeRankEvent;
    use bmz_core::time::TimeUs;

    let events = vec![
        JudgeRankEvent { tick: Default::default(), time: TimeUs(1_000), rank_percent: 50 },
        JudgeRankEvent { tick: Default::default(), time: TimeUs(2_000), rank_percent: 25 },
    ];
    let header = Some(rank_spec(0, JudgeRankKind::BmsRank));
    assert_eq!(judge_percent_at_time(header, &events, TimeUs(0), RuleMode::Dx), 100);
    assert_eq!(judge_percent_at_time(header, &events, TimeUs(1_500), RuleMode::Dx), 100);
    assert_eq!(judge_percent_at_time(header, &events, TimeUs(2_500), RuleMode::Dx), 100);
}

#[test]
fn beatoraja_ignores_exrank_events() {
    use bmz_chart::model::JudgeRankEvent;
    use bmz_core::time::TimeUs;

    let events =
        vec![JudgeRankEvent { tick: Default::default(), time: TimeUs(1_000), rank_percent: 25 }];
    let header = Some(rank_spec(3, JudgeRankKind::BmsRank));
    assert_eq!(judge_percent_at_time(header, &events, TimeUs(0), RuleMode::Beatoraja), 100);
    assert_eq!(judge_percent_at_time(header, &events, TimeUs(1_500), RuleMode::Beatoraja), 100);
}

#[test]
fn beatoraja_pms_percent_at_time_uses_pms_header_and_ignores_events() {
    use bmz_chart::model::JudgeRankEvent;
    use bmz_core::time::TimeUs;

    let events =
        vec![JudgeRankEvent { tick: Default::default(), time: TimeUs(1_000), rank_percent: 25 }];
    let header = Some(rank_spec(2, JudgeRankKind::BmsRank));
    assert_eq!(
        judge_percent_at_time_for_keymode(
            header,
            &events,
            TimeUs(0),
            KeyMode::K9,
            RuleMode::Beatoraja,
        ),
        70
    );
    assert_eq!(
        judge_percent_at_time_for_keymode(
            header,
            &events,
            TimeUs(1_500),
            KeyMode::K9,
            RuleMode::Beatoraja,
        ),
        70
    );
}

#[test]
fn lr2oraja_ignores_exrank_events() {
    use bmz_chart::model::JudgeRankEvent;
    use bmz_core::time::TimeUs;

    let events =
        vec![JudgeRankEvent { tick: Default::default(), time: TimeUs(1_000), rank_percent: 125 }];
    let header = Some(rank_spec(3, JudgeRankKind::BmsRank));
    assert_eq!(judge_percent_at_time(header, &events, TimeUs(0), RuleMode::Lr2Oraja), 100);
    assert_eq!(judge_percent_at_time(header, &events, TimeUs(1_500), RuleMode::Lr2Oraja), 100);
}
