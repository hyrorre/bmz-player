use super::*;

#[test]
fn lr2_result_target_rank_duration_and_clear_conditions_are_consistent() {
    let mut state = SkinDrawState {
        ex_score: 2048,
        total_notes: 1038,
        past_notes: 1038,
        result_failed: Some(false),
        target_ex_score: Some(1846),
        target_max_combo: Some(1038),
        best_clear_index: Some(10),
        previous_best_clear_index: Some(8),
        result_duration_ms: 131000,
        // G-BATTLE keeps the target ghost on the other side.
        skin_attempt: SkinAttemptState { session_mode_index: Some(4), ..Default::default() },
        ..Default::default()
    };
    assert_eq!(skin_state_number(LR2_NUMBER_BASE + 154, &state), Some(-28));
    assert_eq!(skin_state_number(154, &state), Some(28));
    assert_eq!(skin_state_number(LR2_NUMBER_BASE + 163, &state), Some(2));
    assert_eq!(skin_state_number(LR2_NUMBER_BASE + 164, &state), Some(11));
    assert_eq!(skin_state_number(163, &state), Some(0));
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 21, &state), Some(1846));
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 25, &state), Some(1038));
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 26, &state), Some(1038));
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 8, &state), Some(202));
    assert!(test_skin_ops(&[LR2_OPTION_BASE + 105], &[], &state));
    assert!(!test_skin_ops(&[LR2_OPTION_BASE + 104], &[], &state));
    state.lr2_result_flip = true;
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 1, &state), Some(1846));
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 21, &state), Some(2048));
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 30, &state), Some(0));
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 10, &state), None);
    state.target_ex_score = None;
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 1, &state), None);
    state.ex_score = 2076;
    assert_eq!(skin_state_number(LR2_NUMBER_BASE + 154, &state), Some(0));
}

#[test]
fn lr2_result_other_side_is_an_empty_player_in_single_play() {
    let mut state = SkinDrawState {
        ex_score: 2048,
        total_notes: 1038,
        result_failed: Some(false),
        target_ex_score: Some(1846),
        target_max_combo: Some(1038),
        ..Default::default()
    };
    let other =
        |id: i32, state: &SkinDrawState| skin_state_number(LR2_RESULT_NUMBER_BASE + id, state);
    // EX, rate, max combo and judge counts are 0; the notes stay; the EX diff and the
    // next-rank diff follow LR2's formulas for an EX of 0.
    for id in [20, 21, 22, 23, 25, 27, 30, 31, 32, 33, 34, 35, 36] {
        assert_eq!(other(id, &state), Some(0), "{}", id + 100);
    }
    assert_eq!(other(26, &state), Some(1038));
    assert_eq!(other(28, &state), Some(-2048));
    assert_eq!(other(29, &state), Some(-461));
    state.lr2_result_flip = true;
    assert_eq!(other(1, &state), Some(0));
    assert_eq!(other(21, &state), Some(2048));
}

#[test]
fn lr2_buttons_select_flat_cells_while_json_buttons_keep_row_selection() {
    for (divx, divy, index, expected_x, expected_y) in
        [(4, 1, 2, 0.5, 0.0), (1, 6, 2, 0.0, 2.0 / 6.0), (2, 3, 2, 0.0, 1.0 / 3.0)]
    {
        let image:SkinImageDef=serde_json::from_value(serde_json::json!({"id":"button","src":"src","w":120,"h":60,"divx":divx,"divy":divy,"act":LR2_BUTTON_BASE+40,"lr2Button":true})).unwrap();
        let state = SkinDrawState { gauge_type: 5, ..Default::default() };
        assert_eq!(skin_state_event_index(LR2_BUTTON_BASE + 40, &state), index);
        let uv = skin_image_texture_region_for_state(
            &image,
            SkinImageSize { width: 120.0, height: 60.0 },
            &state,
            (0, 0, 120, 60),
        );
        assert!(approx_eq(uv.x, expected_x));
        assert!(approx_eq(uv.y, expected_y));
    }
    let image:SkinImageDef=serde_json::from_value(serde_json::json!({"id":"button","src":"src","w":120,"h":60,"divx":4,"divy":1,"act":LR2_BUTTON_BASE+40})).unwrap();
    let uv = skin_image_texture_region_for_state(
        &image,
        SkinImageSize { width: 120.0, height: 60.0 },
        &SkinDrawState { gauge_type: 5, ..Default::default() },
        (0, 0, 120, 60),
    );
    assert_eq!(uv.x, 0.0);
}

#[test]
fn lr2_played_gauge_button_folds_gauges_missing_from_lr2() {
    // LR2 cells: GROOVE 0 / HARD 1 / HAZARD 2 / EASY 3.
    for (gauge_type, cell) in
        [(0, 3), (1, 3), (2, 0), (3, 1), (4, 1), (5, 2), (6, 1), (7, 1), (8, 1)]
    {
        let state = SkinDrawState {
            gauge_type,
            opponent_gauge_type: Some(gauge_type),
            ..Default::default()
        };
        assert_eq!(skin_state_event_index(LR2_BUTTON_BASE + 40, &state), cell, "{gauge_type}");
        assert_eq!(skin_state_event_index(LR2_BUTTON_BASE + 41, &state), cell, "{gauge_type}");
    }
    // Select keeps settings LR2 cannot show (EX-HARD) as an empty cell.
    let select = SkinDrawState { select_screen: true, select_gauge_index: 4, ..Default::default() };
    assert_eq!(skin_state_event_index(LR2_BUTTON_BASE + 40, &select), -1);
}

#[test]
fn lr2_played_gauge_button_hides_course_stage_lamp_rounded_to_no_play() {
    use bmz_core::clear::ClearType;
    // A passed course stage keeps option 90 but its lamp is rounded to NoPlay.
    let stage = |clear: ClearType, failed: bool| SkinDrawState {
        gauge_type: 6,
        opponent_gauge_type: Some(6),
        select_clear_index: clear as i64,
        result_failed: Some(failed),
        ..Default::default()
    };
    let button = |state: &SkinDrawState| skin_state_event_index(LR2_BUTTON_BASE + 40, state);
    assert_eq!(button(&stage(ClearType::NoPlay, false)), -1);
    assert_eq!(skin_state_event_index(LR2_BUTTON_BASE + 41, &stage(ClearType::NoPlay, false)), 1);
    assert_eq!(button(&stage(ClearType::FullCombo, false)), 1);
    assert_eq!(button(&stage(ClearType::Hard, false)), 1);
    assert_eq!(button(&stage(ClearType::Failed, true)), 1);
    assert_eq!(button(&SkinDrawState { gauge_type: 6, ..Default::default() }), 1);
}

#[test]
fn first_play_update_options_compare_with_empty_score_on_result_only() {
    let mut state = SkinDrawState {
        ex_score: 10,
        max_combo: 5,
        total_notes: 10,
        result_bp: Some(3),
        result_failed: Some(false),
        ..Default::default()
    };
    for op in [330, 331, 332, 335] {
        assert!(test_skin_ops(&[op], &[], &state), "{op}");
    }
    for op in [1330, 1331, 1332] {
        assert!(!test_skin_ops(&[op], &[], &state), "{op}");
    }
    state.ex_score = 0;
    state.max_combo = 0;
    assert!(test_skin_ops(&[1330], &[], &state));
    assert!(test_skin_ops(&[1331], &[], &state));
    // Outside Result there is no score to compare with.
    state.result_failed = None;
    state.ex_score = 10;
    for op in [330, 331, 332, 335] {
        assert!(!test_skin_ops(&[op], &[], &state), "{op}");
    }
}

#[test]
fn lr2_next_rank_uses_integer_floor_boundary_and_max_zero() {
    for (score, expected) in [(4, 0), (8, 0), (9, -1), (10, 0)] {
        assert_eq!(
            skin_state_number(
                LR2_NUMBER_BASE + 154,
                &SkinDrawState { ex_score: score, total_notes: 5, ..Default::default() }
            ),
            Some(expected)
        );
    }
}
