use super::*;

#[test]
fn result_long_note_options_and_index_use_effective_chart_state() {
    let no_ln = SkinDrawState {
        chart_has_long_notes: Some(false),
        result_ln_mode_index: Some(0),
        ..SkinDrawState::default()
    };
    assert!(test_skin_op(172, &[], &no_ln));
    assert!(!test_skin_op(173, &[], &no_ln));

    for (index, expected) in [(0, 0), (1, 1), (2, 2)] {
        let with_ln = SkinDrawState {
            chart_has_long_notes: Some(true),
            result_ln_mode_index: Some(index),
            ..SkinDrawState::default()
        };
        assert!(!test_skin_op(172, &[], &with_ln));
        assert!(test_skin_op(173, &[], &with_ln));
        assert_eq!(skin_image_index_number(308, &with_ln), Some(expected));
        assert_eq!(skin_state_event_index(308, &with_ln), expected as i32);
    }
}

#[test]
fn gradebar_constraint_ops_match_course_constraint_flags() {
    let course = SkinDrawState {
        select_row_kind: SelectRowKind::Course,
        select_course_constraints: CourseConstraintFlags {
            mirror: true,
            no_speed: true,
            no_great: true,
            gauge_7k: true,
            hcn: true,
            ..CourseConstraintFlags::default()
        },
        ..SkinDrawState::default()
    };
    let song = SkinDrawState {
        select_row_kind: SelectRowKind::Song,
        select_course_constraints: course.select_course_constraints,
        ..SkinDrawState::default()
    };

    assert!(test_skin_op(1003, &[], &course));
    assert!(test_skin_op(1005, &[], &course));
    assert!(test_skin_op(1007, &[], &course));
    assert!(test_skin_op(1012, &[], &course));
    assert!(test_skin_op(1017, &[], &course));
    assert!(!test_skin_op(1002, &[], &course));
    assert!(!test_skin_op(1016, &[], &course));
    assert!(!test_skin_op(1003, &[], &song));
    assert!(test_skin_op(-1003, &[], &song));
}

#[test]
fn play_mode_option_ops_reflect_autoplay_and_course_stage() {
    let normal_play = SkinDrawState::default();
    let autoplay = SkinDrawState { autoplay: true, ..SkinDrawState::default() };
    let course_stage1 =
        SkinDrawState { course_stage: Some(CourseStageMarker::Stage1), ..SkinDrawState::default() };
    let course_final =
        SkinDrawState { course_stage: Some(CourseStageMarker::Final), ..SkinDrawState::default() };

    // Starseeker freestage: op = {32, -290}
    assert!(test_skin_op(32, &[], &normal_play));
    assert!(!test_skin_op(290, &[], &normal_play));
    assert!(test_skin_ops(&[32, -290], &[], &normal_play));

    // Starseeker auto_play: op = {33}
    assert!(!test_skin_op(33, &[], &normal_play));
    assert!(test_skin_op(33, &[], &autoplay));

    // Course stage labels
    assert!(test_skin_ops(&[32, 290, 280], &[], &course_stage1));
    assert!(!test_skin_ops(&[32, 290, 280], &[], &course_final));
    assert!(test_skin_ops(&[32, 290, 289], &[], &course_final));

    // beatoraja currently leaves these defined constants without BooleanProperty handlers.
    for op in 291..=293 {
        assert!(
            !test_skin_op(op, &[op], &course_stage1),
            "{op} must not fall back to property defaults"
        );
        assert!(test_skin_op(-op, &[op], &course_stage1), "negative {op} should invert false");
    }
}

#[test]
fn wmii_result_draw_predicates_use_runtime_score_and_nearest_rank() {
    let near_aa = SkinDrawState { ex_score: 155, total_notes: 100, ..Default::default() };
    assert!(eval_skin_draw_condition("score_rate_band(6,7)", &near_aa));
    assert!(!eval_skin_draw_condition("score_rate_band(7,8)", &near_aa));
    assert!(eval_skin_draw_condition("nearest_rank(AA,minus)", &near_aa));
    assert!(eval_skin_draw_condition("nearest_rank_sign(minus)", &near_aa));
    assert!(!eval_skin_draw_condition("nearest_rank(A,plus)", &near_aa));

    let max = SkinDrawState { ex_score: 200, total_notes: 100, ..Default::default() };
    assert!(eval_skin_draw_condition("score_rate_band(9,10)", &max));
    assert!(eval_skin_draw_condition("nearest_rank(MAX,plus)", &max));
}

#[test]
fn wmii_next_rank_draw_predicates_use_forward_rank_boundaries() {
    let a_minus = SkinDrawState { ex_score: 120, total_notes: 100, ..Default::default() };
    assert!(eval_skin_draw_condition("wmii_next_rank_stage(3)", &a_minus));
    assert!(!eval_skin_draw_condition("wmii_next_rank_stage(2)", &a_minus));
    assert!(eval_skin_draw_condition("wmii_next_rank_diff_nonzero()", &a_minus));
    assert!(!eval_skin_draw_condition("wmii_next_rank_diff_zero()", &a_minus));

    let aa_minus = SkinDrawState { ex_score: 155, total_notes: 100, ..Default::default() };
    assert!(eval_skin_draw_condition("wmii_next_rank_stage(2)", &aa_minus));

    let max_minus = SkinDrawState { ex_score: 188, total_notes: 100, ..Default::default() };
    assert!(eval_skin_draw_condition("wmii_next_rank_stage(8)", &max_minus));
    assert!(eval_skin_draw_condition("wmii_next_rank_stage_no_max_minus(0)", &max_minus));

    let max = SkinDrawState { ex_score: 200, total_notes: 100, ..Default::default() };
    assert!(eval_skin_draw_condition("wmii_next_rank_stage(0)", &max));
    assert!(eval_skin_draw_condition("wmii_next_rank_diff_zero()", &max));
    assert!(!eval_skin_draw_condition("wmii_next_rank_diff_nonzero()", &max));
}

#[test]
fn result_key_mode_ops_use_result_key_mode() {
    let result_5k = SkinDrawState {
        result_failed: Some(false),
        key_mode: KeyMode::K5,
        ..SkinDrawState::default()
    };
    assert!(test_skin_op(161, &[], &result_5k));
    assert!(!test_skin_op(160, &[], &result_5k));

    let result_14k = SkinDrawState { key_mode: KeyMode::K14, ..result_5k };
    assert!(test_skin_op(162, &[], &result_14k));
    assert!(test_skin_op(SKIN_OPTION_BMZ_KEY_MODE_LAST, &[], &result_14k));
    assert!(test_skin_op(SKIN_OPTION_BMZ_DOUBLE_PLAY, &[], &result_14k));
    assert_eq!(skin_state_event_index(SKIN_REF_BMZ_KEY_MODE, &result_14k), 14);
    assert_eq!(skin_state_number(SKIN_REF_BMZ_ACTIVE_LANE_COUNT, &result_14k), Some(16));
}

#[test]
fn grade_diff_destinations_use_the_fixed_next_rank_in_select_and_result() {
    fn destination(id: &str, op: i32) -> SkinDestinationDef {
        SkinDestinationDef {
            id: id.to_string(),
            blend: 0,
            filter: 0,
            ambient: false,
            ambient_mode: SkinAmbientMode::Full,
            ambient_spread: 20.0,
            ambient_blur: 50.0,
            timer: None,
            timer_expr: String::new(),
            loop_time: None,
            lr2_timing: false,
            center: 0,
            offset: 0,
            offsets: Vec::new(),
            bmz_detail_scroll: [0, 0],
            stretch: default_stretch(),
            op: vec![op],
            op_expr: Box::default(),
            draw: String::new(),
            act: None,
            click: 0,
            clickable: None,
            dst: Vec::new(),
            mouse_rect: None,
        }
    }
    fn grade_diff_value() -> SkinValueDef {
        SkinValueDef {
            id: "RANK_Diff_Exscore".to_string(),
            src: "num".to_string(),
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            divx: default_grid_division(),
            divy: default_grid_division(),
            timer: None,
            cycle: 0,
            align: 0,
            judge_align: None,
            digit: 0,
            padding: 0,
            zeropadding: 0,
            space: 0,
            ref_id: 154,
            value: None,
            expr: String::new(),
            value_expr: String::new(),
            offset: Vec::new(),
        }
    }

    let state = SkinDrawState {
        ex_score: 1900,
        total_notes: 1000,
        result_failed: Some(false),
        ..SkinDrawState::default()
    };
    assert!(destination_ops_match(&destination("RANK_s_MAX", 300), &[], &state));
    assert!(destination_ops_match(&destination("any_other_id", 300), &[], &state));
    assert!(!destination_ops_match(&destination("RANK_s_AAA", 301), &[], &state));
    assert_eq!(skin_value_number_for_destination(&grade_diff_value(), &state), Some(100));

    let select = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_ex_score: Some(1900),
        select_total_notes: 1000,
        select_play_count: 1,
        ..SkinDrawState::default()
    };
    assert!(destination_ops_match(&destination("RANK_s_MAX", 300), &[], &select));
    assert!(!destination_ops_match(&destination("RANK_s_AAA", 301), &[], &select));
    assert!(destination_ops_match(&destination("any_other_id", 301), &[], &select));
}

#[test]
fn next_result_diff_number_uses_the_positive_mimage_row() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 7,
                "w": 100,
                "h": 100,
                "value": [
                    {
                        "id": "RANK_Diff_Exscore",
                        "src": "num",
                        "x": 0,
                        "y": 0,
                        "w": 120,
                        "h": 40,
                        "divx": 12,
                        "divy": 2,
                        "digit": 5,
                        "ref": 154,
                        "zeropadding": 2
                    }
                ],
                "destination": [
                    {
                        "id": "RANK_s_E",
                        "op": [307],
                        "dst": [{"x": 0, "y": 20, "w": 10, "h": 10}]
                    },
                    {
                        "id": "RANK_Diff_Exscore",
                        "dst": [{"x": 10, "y": 20, "w": 10, "h": 10}]
                    }
                ]
            }
            "#,
    )
    .unwrap();
    let sources = HashMap::from([(
        "num".to_string(),
        SkinDocumentTexture {
            source_id: "num".to_string(),
            texture: SkinTextureId(42),
            source_size: SkinImageSize { width: 120.0, height: 40.0 },
        },
    )]);
    let state = SkinDrawState {
        ex_score: 100,
        total_notes: 1000,
        result_failed: Some(false),
        ..SkinDrawState::default()
    };

    let items = document.static_render_items(&sources, &state, &SkinTextState::default());
    let first_digit_uv = items.iter().find_map(|item| match item {
        SkinRenderItem::Image { texture: SkinTextureId(42), uv, .. } => Some(*uv),
        _ => None,
    });

    assert_eq!(first_digit_uv.map(|uv| uv.y), Some(0.0));
}

#[test]
fn result_replay_ops_reflect_result_replay_slots() {
    let no_replay = SkinDrawState { result_failed: Some(false), ..SkinDrawState::default() };
    let existing = SkinDrawState {
        result_failed: Some(false),
        result_replay_slots: [true, false, false, false],
        ..SkinDrawState::default()
    };
    let saved = SkinDrawState {
        result_failed: Some(false),
        result_replay_slots: [true, true, false, false],
        result_saved_replay_slots: [true, false, false, false],
        ..SkinDrawState::default()
    };

    assert!(test_skin_op(196, &[], &no_replay));
    assert!(!test_skin_op(197, &[], &no_replay));
    assert!(!test_skin_op(198, &[], &no_replay));
    assert!(test_skin_op(197, &[], &existing));
    assert!(!test_skin_op(196, &[], &existing));
    assert!(!test_skin_op(198, &[], &existing));
    assert!(test_skin_op(198, &[], &saved));
    assert!(!test_skin_op(197, &[], &saved));
    assert!(test_skin_op(1197, &[], &saved));
    assert!(!test_skin_op(1198, &[], &saved));
}
