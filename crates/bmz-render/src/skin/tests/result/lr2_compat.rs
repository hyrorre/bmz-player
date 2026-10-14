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
