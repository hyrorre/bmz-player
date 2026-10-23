use super::*;

fn split_judgement(
    lane: Lane,
    judge: Judge,
    side: Option<TimingSide>,
    delta_us: i64,
    time_us: i64,
    timing_ms_suppressed: bool,
) -> crate::snapshot::DisplayJudgement {
    crate::snapshot::DisplayJudgement {
        lane,
        judge,
        side,
        text: String::new(),
        combo: 0,
        delta_us,
        time: TimeUs(time_us),
        is_miss: false,
        timing_ms_suppressed,
    }
}

#[test]
fn bmz_split_judge_runtime_keeps_scratch_and_keys_independent_past_800ms() {
    let document: SkinDocument = serde_json::from_str("{}").unwrap();
    let mut runtime = DynamicTimerRuntime::default();
    let mut state = SkinDrawState::default();

    runtime.ingest_judge_lane_state(
        &[
            split_judgement(Lane::Scratch, Judge::PGreat, None, -2_000, 100_000, false),
            split_judgement(
                Lane::Key1,
                Judge::Great,
                Some(TimingSide::Slow),
                8_000,
                400_000,
                false,
            ),
            split_judgement(
                Lane::Key8,
                Judge::Great,
                Some(TimingSide::Fast),
                -7_000,
                500_000,
                false,
            ),
            split_judgement(
                Lane::Scratch2,
                Judge::Good,
                Some(TimingSide::Slow),
                9_000,
                700_000,
                false,
            ),
        ],
        2,
        700_000,
    );
    runtime.advance(&document, &mut state, 700);

    assert_eq!(skin_timer_elapsed_ms(Some(19_010), &state), Some(600));
    assert_eq!(skin_timer_elapsed_ms(Some(19_011), &state), Some(300));
    assert_eq!(skin_timer_elapsed_ms(Some(19_012), &state), Some(0));
    assert_eq!(skin_timer_elapsed_ms(Some(19_013), &state), Some(200));
    assert!(test_skin_op(19_020, &[], &state));
    assert!(!test_skin_op(19_030, &[], &state));
    assert!(test_skin_op(19_041, &[], &state));
    assert!(test_skin_op(19_042, &[], &state));
    assert!(test_skin_op(19_033, &[], &state));
    assert_eq!(skin_state_number(19_050, &state), Some(2));
    assert_eq!(skin_state_number(19_051, &state), Some(-8));
    assert_eq!(skin_state_number(19_052, &state), Some(-9));
    assert_eq!(skin_state_number(19_053, &state), Some(7));

    // 標準の800msリストが空になっても、標準・拡張の両チャンネルを
    // renderer runtime に残し、各 destination の終端で表示を止める。
    runtime.ingest_judge_lane_state(&[], 2, 1_800_000);
    runtime.advance(&document, &mut state, 1_800);
    assert_eq!(skin_timer_elapsed_ms(Some(19_010), &state), Some(1_700));
    assert_eq!(skin_timer_elapsed_ms(Some(19_011), &state), Some(1_400));
    assert_eq!(skin_timer_elapsed_ms(Some(19_012), &state), Some(1_100));
    assert_eq!(skin_timer_elapsed_ms(Some(19_013), &state), Some(1_300));
    assert!(test_skin_op(19_020, &[], &state));
    assert_eq!(state.judge_ms, [Some(1_400), Some(1_100), None]);
    assert_eq!(state.judge_index, [Some(1), Some(2), None]);
}

#[test]
fn bmz_split_judge_runtime_preserves_fast_slow_filter_and_resets_on_rewind() {
    let document: SkinDocument = serde_json::from_str("{}").unwrap();
    let mut runtime = DynamicTimerRuntime::default();
    let mut state = SkinDrawState::default();

    runtime.ingest_judge_lane_state(
        &[split_judgement(Lane::Key1, Judge::PGreat, None, -3_000, 1_000_000, true)],
        1,
        1_000_000,
    );
    runtime.advance(&document, &mut state, 1_000);
    assert!(test_skin_op(19_021, &[], &state));
    assert!(!test_skin_op(19_031, &[], &state));
    assert!(!test_skin_op(19_041, &[], &state));
    assert_eq!(skin_state_number(19_051, &state), None);

    runtime.ingest_judge_lane_state(&[], 1, 0);
    runtime.advance(&document, &mut state, 0);
    assert_eq!(skin_timer_elapsed_ms(Some(19_011), &state), None);
    assert!(!test_skin_op(19_021, &[], &state));
    assert_eq!(skin_state_number(19_051, &state), None);
}

#[test]
fn bmz_split_judge_ids_cover_all_six_channels() {
    let state = SkinDrawState {
        judge_lane_ms: std::array::from_fn(|slot| Some(slot as i32)),
        judge_lane_index: [Some(0); SKIN_BMZ_JUDGE_LANE_COUNT],
        judge_lane_timing_sign: std::array::from_fn(|slot| {
            Some(if slot % 2 == 0 { 1 } else { -1 })
        }),
        judge_lane_timing_ms: std::array::from_fn(|slot| Some(slot as i32 + 1)),
        ..Default::default()
    };

    for slot in 0..SKIN_BMZ_JUDGE_LANE_COUNT {
        assert_eq!(
            skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_JUDGE_LANE_BASE + slot as i32), &state),
            Some(slot as i32)
        );
        assert!(test_skin_op(SKIN_OPTION_BMZ_JUDGE_LANE_PGREAT_BASE + slot as i32, &[], &state));
        assert_eq!(
            test_skin_op(SKIN_OPTION_BMZ_JUDGE_LANE_FAST_BASE + slot as i32, &[], &state),
            slot % 2 == 0
        );
        assert_eq!(
            test_skin_op(SKIN_OPTION_BMZ_JUDGE_LANE_SLOW_BASE + slot as i32, &[], &state),
            slot % 2 == 1
        );
        assert_eq!(
            skin_state_number(SKIN_REF_BMZ_JUDGE_LANE_DURATION_BASE + slot as i32, &state),
            Some(-(slot as i64 + 1))
        );
    }
}

#[test]
fn bmz_split_judge_runtime_maps_third_region_scratch_and_keys() {
    let document: SkinDocument = serde_json::from_str("{}").unwrap();
    let mut runtime = DynamicTimerRuntime::default();
    let mut state = SkinDrawState::default();

    runtime.ingest_judge_lane_state(
        &[
            split_judgement(
                Lane::Scratch2,
                Judge::Great,
                Some(TimingSide::Fast),
                -4_000,
                100_000,
                false,
            ),
            split_judgement(
                Lane::Key14,
                Judge::Great,
                Some(TimingSide::Slow),
                6_000,
                200_000,
                false,
            ),
        ],
        3,
        200_000,
    );
    runtime.advance(&document, &mut state, 200);

    assert_eq!(skin_timer_elapsed_ms(Some(19_014), &state), Some(100));
    assert_eq!(skin_timer_elapsed_ms(Some(19_015), &state), Some(0));
    assert!(test_skin_op(19_034, &[], &state));
    assert!(test_skin_op(19_045, &[], &state));
    assert_eq!(skin_state_number(19_054, &state), Some(4));
    assert_eq!(skin_state_number(19_055, &state), Some(-6));
}

#[test]
fn logical_input_press_edges_drive_options_timers_and_runtime_events() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
                "type": 0, "w": 1, "h": 1, "destination": [],
                "runtimeFlag": [{ "id": 1, "initial": false }],
                "runtimeEvent": [{
                    "id": -20001,
                    "toggleFlags": [1],
                    "triggerAction": "e1_press"
                }]
            }"#,
    )
    .unwrap();
    let mut runtime = DynamicTimerRuntime::default();
    let mut state = SkinDrawState::default();

    // A held input on scene entry is synchronized without inventing a press edge.
    state.logical_input_held[0] = true;
    runtime.advance(&document, &mut state, 100);
    assert_eq!(state.runtime_flags.get(&1), Some(&false));
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_INPUT_BASE), &state), None);
    assert!(test_skin_op(SKIN_OPTION_BMZ_INPUT_BASE, &[], &state));

    state.logical_input_held[0] = false;
    runtime.advance(&document, &mut state, 110);
    state.logical_input_held[0] = true;
    runtime.advance(&document, &mut state, 120);
    assert_eq!(state.runtime_flags.get(&1), Some(&true));
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_INPUT_BASE), &state), Some(0));

    runtime.advance(&document, &mut state, 150);
    assert_eq!(state.runtime_flags.get(&1), Some(&true));
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_INPUT_BASE), &state), Some(30));
}

#[test]
fn e1_e2_aggregate_timers_cover_hold_and_release_transitions() {
    let document: SkinDocument =
        serde_json::from_str(r#"{ "type": 0, "w": 1, "h": 1, "destination": [] }"#).unwrap();
    let mut runtime = DynamicTimerRuntime::default();
    let mut state = SkinDrawState::default();

    // A scene-entry hold must not invent either side of the aggregate transition.
    state.logical_input_held[0] = true;
    runtime.advance(&document, &mut state, 100);
    assert!(test_skin_op(SKIN_OPTION_BMZ_E1_E2_HELD, &[], &state));
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_E1_E2_PRESS), &state), None);
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_E1_E2_RELEASE), &state), None);

    state.logical_input_held[0] = false;
    runtime.advance(&document, &mut state, 110);
    assert!(!test_skin_op(SKIN_OPTION_BMZ_E1_E2_HELD, &[], &state));
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_E1_E2_RELEASE), &state), None);

    state.logical_input_held[0] = true;
    runtime.advance(&document, &mut state, 120);
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_E1_E2_PRESS), &state), Some(0));
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_E1_E2_RELEASE), &state), None);

    // Adding and removing E2 must not restart or release while E1/E2 remains held.
    state.logical_input_held[1] = true;
    runtime.advance(&document, &mut state, 150);
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_E1_E2_PRESS), &state), Some(30));
    state.logical_input_held[0] = false;
    runtime.advance(&document, &mut state, 170);
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_E1_E2_PRESS), &state), Some(50));
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_E1_E2_RELEASE), &state), None);

    state.logical_input_held[1] = false;
    runtime.advance(&document, &mut state, 180);
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_E1_E2_PRESS), &state), None);
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_E1_E2_RELEASE), &state), Some(0));
    runtime.advance(&document, &mut state, 210);
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_E1_E2_RELEASE), &state), Some(30));

    // A new aggregate press cancels the release animation and starts over.
    state.logical_input_held[1] = true;
    runtime.advance(&document, &mut state, 220);
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_E1_E2_PRESS), &state), Some(0));
    assert_eq!(skin_timer_elapsed_ms(Some(SKIN_TIMER_BMZ_E1_E2_RELEASE), &state), None);
}

#[test]
fn end_of_note_timers_use_elapsed_since_end_of_note() {
    let inactive =
        SkinDrawState { elapsed_ms: 5_000, end_of_note_ms: None, ..SkinDrawState::default() };
    assert_eq!(skin_timer_elapsed_ms(Some(143), &inactive), None);
    assert_eq!(skin_timer_elapsed_ms(Some(144), &inactive), None);

    let active = SkinDrawState {
        elapsed_ms: 5_000,
        end_of_note: true,
        end_of_note_ms: Some(250),
        end_of_note_2p_ms: Some(325),
        ..SkinDrawState::default()
    };
    assert_eq!(skin_timer_elapsed_ms(Some(143), &active), Some(250));
    assert_eq!(skin_timer_elapsed_ms(Some(144), &active), Some(325));
}

#[test]
fn fixed_delay_timer_starts_after_source_delay() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
                "type": 0, "w": 1, "h": 1, "destination": [],
                "fixedDelayTimer": [{ "id": 11900, "sourceTimer": 143, "delayMs": 1000 }]
            }"#,
    )
    .unwrap();
    let mut runtime = DynamicTimerRuntime::default();
    let mut state = SkinDrawState { end_of_note_ms: Some(999), ..SkinDrawState::default() };

    runtime.advance(&document, &mut state, 5_000);
    assert_eq!(skin_timer_elapsed_ms(Some(11900), &state), None);

    state.end_of_note_ms = Some(1_250);
    runtime.advance(&document, &mut state, 5_251);
    assert_eq!(skin_timer_elapsed_ms(Some(11900), &state), Some(250));

    state.end_of_note_ms = None;
    runtime.advance(&document, &mut state, 5_252);
    assert_eq!(skin_timer_elapsed_ms(Some(11900), &state), None);
}

#[test]
fn zero_delay_timer_alias_follows_source_timer() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
                "type": 0, "w": 1, "h": 1, "destination": [],
                "fixedDelayTimer": [{ "id": 11901, "sourceTimer": 143, "delayMs": 0 }]
            }"#,
    )
    .unwrap();
    let mut runtime = DynamicTimerRuntime::default();
    let mut state = SkinDrawState { end_of_note_ms: Some(1_250), ..SkinDrawState::default() };

    runtime.advance(&document, &mut state, 5_000);
    assert_eq!(skin_timer_elapsed_ms(Some(11901), &state), Some(1_250));

    state.end_of_note_ms = None;
    runtime.advance(&document, &mut state, 5_001);
    assert_eq!(skin_timer_elapsed_ms(Some(11901), &state), None);
}

#[test]
fn timer_zero_uses_scene_elapsed_time() {
    let state = SkinDrawState { elapsed_ms: 1_800, ..SkinDrawState::default() };

    assert_eq!(skin_timer_elapsed_ms(Some(0), &state), Some(1_800));
}

#[test]
fn start_input_timer_activates_strictly_after_skin_input_delay() {
    let mut runtime = DynamicTimerRuntime::default();
    assert_eq!(runtime.start_input_elapsed_ms(499, 500), None);
    assert_eq!(runtime.start_input_elapsed_ms(500, 500), None);
    assert_eq!(runtime.start_input_elapsed_ms(725, 500), Some(0));
    assert_eq!(runtime.start_input_elapsed_ms(750, 500), Some(25));

    let state = SkinDrawState { start_input_ms: Some(25), ..SkinDrawState::default() };
    assert_eq!(skin_timer_elapsed_ms(Some(1), &state), Some(25));
}

#[test]
fn start_input_timer_restarts_after_scene_time_rewinds() {
    let mut runtime = DynamicTimerRuntime::default();
    assert_eq!(runtime.start_input_elapsed_ms(501, 500), Some(0));
    assert_eq!(runtime.start_input_elapsed_ms(600, 500), Some(99));

    assert_eq!(runtime.start_input_elapsed_ms(0, 500), None);
    assert_eq!(runtime.start_input_elapsed_ms(725, 500), Some(0));
}

#[test]
fn rhythm_timer_uses_bpm_normalized_snapshot_time() {
    let inactive = SkinDrawState::default();
    assert_eq!(skin_timer_elapsed_ms(Some(140), &inactive), None);

    let active = SkinDrawState { rhythm_timer_ms: Some(2_750), ..SkinDrawState::default() };
    assert_eq!(skin_timer_elapsed_ms(Some(140), &active), Some(2_750));
}

#[test]
fn keybeam_runtime_suppresses_ln_press_and_its_release_fade() {
    let document: SkinDocument =
        serde_json::from_str(r#"{ "type": 0, "w": 1, "h": 1, "destination": [] }"#).unwrap();
    let mut runtime = DynamicTimerRuntime::default();
    let mut state = SkinDrawState::default();
    let lane = Lane::Key1.index();

    state.keyon_ms[lane] = Some(0);
    runtime.advance(&document, &mut state, 100);
    assert!(state.keybeam_hold_active[lane]);

    state.keyon_ms[lane] = Some(10);
    state.hold_ms[lane] = Some(0);
    runtime.advance(&document, &mut state, 110);
    assert!(!state.keybeam_hold_active[lane]);

    state.keyon_ms[lane] = None;
    state.hold_ms[lane] = None;
    state.keyoff_ms[lane] = Some(0);
    runtime.advance(&document, &mut state, 120);
    assert!(!state.keybeam_fade_active[lane]);

    state.keyoff_ms[lane] = None;
    state.keyon_ms[lane] = Some(0);
    runtime.advance(&document, &mut state, 200);
    state.keyon_ms[lane] = None;
    state.keyoff_ms[lane] = Some(0);
    runtime.advance(&document, &mut state, 210);
    assert!(state.keybeam_fade_active[lane]);
    assert!(eval_skin_draw_condition("keybeam_fade(121) != 0", &state));
}

#[test]
fn keybeam_timer_lane_mapping_matches_skin_timer_mapping() {
    assert_eq!(keybeam_lane_for_keyon_timer(108), Some(Lane::Key8.index()));
    assert_eq!(keybeam_lane_for_keyon_timer(109), Some(Lane::Key9.index()));
    assert_eq!(keybeam_lane_for_keyon_timer(110), Some(Lane::Scratch2.index()));
    assert_eq!(keybeam_lane_for_keyoff_timer(128), Some(Lane::Key8.index()));
    assert_eq!(keybeam_lane_for_keyoff_timer(129), Some(Lane::Key9.index()));
    assert_eq!(keybeam_lane_for_keyoff_timer(130), Some(Lane::Scratch2.index()));
}

#[test]
fn bomb_timer_activates_only_for_active_lane() {
    // timer=51 maps to bomb Key1 (TIMER_BOMB_1P_KEY1 = 50 + Lane::Key1.index() = 51)
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "source": [{ "id": 1, "path": "bomb.png" }],
                "image": [{ "id": "bomb-img", "src": 1, "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "bomb-img", "timer": 51, "dst": [
                        { "time": 0, "x": 10, "y": 10, "w": 10, "h": 10 }
                    ]}
                ]
            }
            "#,
    )
    .unwrap();
    let sources = HashMap::from([(
        "1".to_string(),
        SkinDocumentTexture {
            source_id: "1".to_string(),
            texture: SkinTextureId(9),
            source_size: SkinImageSize { width: 10.0, height: 10.0 },
        },
    )]);

    // All lanes inactive → no items
    let inactive_state = SkinDrawState::default();
    let items_inactive = document.static_image_render_items(&sources, &inactive_state);
    assert_eq!(items_inactive.len(), 0, "should be empty when all bomb timers are None");

    // Key1 (index=1) active → items returned
    let active_state = SkinDrawState {
        bomb_ms: {
            let mut a = [None; LANE_COUNT];
            a[1] = Some(0);
            a
        },
        ..SkinDrawState::default()
    };
    let items_active = document.static_image_render_items(&sources, &active_state);
    assert_eq!(items_active.len(), 1, "should have one item when Key1 bomb timer is active");
}

#[test]
fn dst_if_value_uses_default_when_option_disabled() {
    // No property → no enabled options → conditional frame skipped, only end frame {time:500}.
    // 最初のキーフレーム時刻 (500) より前は描画されず、500ms 以降に既定位置 (0,0) で描画される。
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "src", "path": "a.png" }],
                "image": [{ "id": "img", "src": "src", "w": 10, "h": 10 }],
                "destination": [
                    { "id": "img", "loop": 500, "dst": [
                        { "if": [920], "value": { "time": 0, "x": 100, "y": 200, "w": 50, "h": 50 } },
                        { "time": 500 }
                    ]}
                ]
            }
            "#,
        )
        .unwrap();

    let sources = mock_source("src", 10.0, 10.0);

    // elapsed=0: 最初のキーフレーム時刻 (500) より前なので描画しない。
    let before = document.static_image_render_items(
        &sources,
        &SkinDrawState { elapsed_ms: 0, ..SkinDrawState::default() },
    );
    assert!(before.is_empty(), "destination is not drawn before its first keyframe time");

    // elapsed=500: 条件フレームが skip され、{time:500} の既定位置 (0,0) で描画される。
    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState { elapsed_ms: 500, ..SkinDrawState::default() },
    );
    assert_eq!(items.len(), 1);
    let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
    assert!(approx_eq(rect.x, 0.0), "expected default x=0, got {}", rect.x);
    assert!(approx_eq(rect.y, 1.0), "expected default y=1, got {}", rect.y);
}

#[test]
fn offset_lift_shifts_destination_y() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "src", "path": "a.png" }],
                "image": [{ "id": "img", "src": "src", "w": 10, "h": 10 }],
                "destination": [
                    { "id": "img", "offset": 3, "dst": [
                        { "time": 0, "x": 100, "y": 200, "w": 50, "h": 50 }
                    ]}
                ]
            }
            "#,
    )
    .unwrap();

    let sources = mock_source("src", 10.0, 10.0);
    let state_no_lift = SkinDrawState { offset_lift_px: 0, ..SkinDrawState::default() };
    let state_lifted = SkinDrawState { offset_lift_px: 72, ..SkinDrawState::default() };

    let items_no_lift = document.static_image_render_items(&sources, &state_no_lift);
    let items_lifted = document.static_image_render_items(&sources, &state_lifted);

    assert_eq!(items_no_lift.len(), 1);
    assert_eq!(items_lifted.len(), 1);

    let SkinRenderItem::Image { rect: rect_no_lift, .. } = &items_no_lift[0] else { panic!() };
    let SkinRenderItem::Image { rect: rect_lifted, .. } = &items_lifted[0] else { panic!() };

    // With lift=72px on a 720h canvas, beatoraja y shifts upward in bottom-origin space.
    assert!(approx_eq(rect_no_lift.y, (720 - 200 - 50) as f32 / 720.0));
    assert!(
        approx_eq(rect_lifted.y, (720 - (200 + 72) - 50) as f32 / 720.0),
        "expected y shifted by lift, got {}",
        rect_lifted.y
    );
}

#[test]
fn offset_lanecover_shifts_destination_y() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "w": 1280, "h": 720,
                "source": [{ "id": "src", "path": "a.png" }],
                "image": [{ "id": "img", "src": "src", "w": 10, "h": 10 }],
                "destination": [
                    { "id": "img", "offset": 4, "dst": [
                        { "time": 0, "x": 0, "y": 720, "w": 50, "h": 50 }
                    ]}
                ]
            }
            "#,
    )
    .unwrap();

    let sources = mock_source("src", 10.0, 10.0);
    // lanecover=0.5, lift=0 → offset_lanecover_px = (0-1)*720*0.5 = -360
    let state = SkinDrawState { offset_lanecover_px: -360, ..SkinDrawState::default() };
    let items = document.static_image_render_items(&sources, &state);

    assert_eq!(items.len(), 1);
    let SkinRenderItem::Image { rect, .. } = &items[0] else { panic!() };
    // y=720 shifted by -360 in bottom-origin space: top = 720 - (720 - 360 + 50).
    assert!(
        approx_eq(rect.y, (720 - (720 - 360 + 50)) as f32 / 720.0),
        "expected shifted y, got {}",
        rect.y
    );
}

#[test]
fn custom_offset_adjusts_destination_geometry_and_alpha() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "w": 100, "h": 100,
                "source": [{ "id": "src", "path": "a.png" }],
                "image": [{ "id": "img", "src": "src", "w": 10, "h": 10 }],
                "destination": [
                    { "id": "img", "offset": 42, "dst": [
                        { "time": 0, "x": 10, "y": 20, "w": 30, "h": 40, "a": 200 }
                    ]}
                ]
            }
            "#,
    )
    .unwrap();

    let sources = mock_source("src", 10.0, 10.0);
    let mut offsets = SkinOffsetValues::default();
    offsets.set(42, crate::skin_offset::SkinOffsetValue { x: 6, y: 8, w: 10, h: 12, r: 0, a: -50 });
    let items = document.static_image_render_items(
        &sources,
        &SkinDrawState { skin_offsets: offsets, ..SkinDrawState::default() },
    );

    assert_eq!(items.len(), 1);
    let SkinRenderItem::Image { rect, tint, .. } = &items[0] else { panic!() };
    assert!(approx_eq(rect.x, (10 + 6 - 10 / 2) as f32 / 100.0));
    assert!(approx_eq(rect.y, (100 - (20 + 8 - 12 / 2) - (40 + 12)) as f32 / 100.0));
    assert!(approx_eq(rect.width, 40.0 / 100.0));
    assert!(approx_eq(rect.height, 52.0 / 100.0));
    assert!(approx_eq(tint.a, 150.0 / 255.0));
}

#[test]
fn all_offset_transforms_play_skin_render_item() {
    let mut offsets = SkinOffsetValues::default();
    offsets.set(
        OFFSET_ALL,
        crate::skin_offset::SkinOffsetValue { x: 10, y: 20, w: 50, h: -50, r: 0, a: 0 },
    );
    let item = SkinRenderItem::Image {
        texture: SkinTextureId(1),
        rect: Rect { x: 0.2, y: 0.4, width: 0.1, height: 0.2 },
        uv: TextureRegion::default(),
        tint: Color::rgb(1.0, 1.0, 1.0),
        blend: BlendMode::Normal,
        scale: SkinImageScale::Stretch,
        border: None,
        source_size: None,
        linear_filter: false,
    };

    let item = apply_all_offset_to_render_item(
        item,
        &SkinDrawState { skin_offsets: offsets, ..SkinDrawState::default() },
    );

    let SkinRenderItem::Image { rect, .. } = item else { panic!() };
    assert!(approx_eq(rect.x, 0.4));
    assert!(approx_eq(rect.y, 0.5));
    assert!(approx_eq(rect.width, 0.15));
    assert!(approx_eq(rect.height, 0.1));
}

#[test]
fn all_offset_scales_text_and_rotated_images_after_local_layout() {
    let mut offsets = SkinOffsetValues::default();
    offsets.set(
        OFFSET_ALL,
        crate::skin_offset::SkinOffsetValue { x: 10, y: 20, w: 50, h: -50, r: 0, a: 0 },
    );
    let state = SkinDrawState { skin_offsets: offsets, ..SkinDrawState::default() };
    let style = TextStyle {
        gradient: None,
        font_id: None,
        size: 0.1,
        bitmap_size: None,
        color: Color::rgb(1.0, 1.0, 1.0),
        layer: TextLayer::Skin,
        align: TextAlign::Left,
        max_width: 0.0,
        overflow: TextOverflow::Overflow,
        wrapping: false,
        outline: None,
        shadow: None,
    };

    let text = apply_all_offset_to_render_item(
        SkinRenderItem::Text {
            origin: Point { x: 0.2, y: 0.4 },
            text: "text".to_string(),
            style,
            caret: None,
            blend: BlendMode::Normal,
            post_scale: Point { x: 1.0, y: 1.0 },
        },
        &state,
    );
    let SkinRenderItem::Text { origin, post_scale, .. } = text else { panic!() };
    assert!(approx_eq(origin.x, 0.4));
    assert!(approx_eq(origin.y, 0.5));
    assert_eq!(post_scale, Point { x: 1.5, y: 0.5 });

    let rotated = apply_all_offset_to_render_item(
        SkinRenderItem::RotatedImage {
            texture: SkinTextureId(1),
            rect: Rect { x: 0.2, y: 0.4, width: 0.1, height: 0.2 },
            uv: TextureRegion::default(),
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
            source_size: None,
            linear_filter: false,
            angle_deg: 45.0,
            center: Point { x: 0.0, y: 1.0 },
            post_scale: Point { x: 1.0, y: 1.0 },
        },
        &state,
    );
    let SkinRenderItem::RotatedImage { rect, center, post_scale, .. } = rotated else { panic!() };
    assert_eq!(center, Point { x: 0.0, y: 1.0 });
    assert!(approx_eq(rect.x, 0.4));
    assert!(approx_eq(rect.y, 0.4));
    assert!(approx_eq(rect.width, 0.1));
    assert!(approx_eq(rect.height, 0.2));
    assert_eq!(post_scale, Point { x: 1.5, y: 0.5 });
}

#[test]
fn notes_offset_resizes_note_from_beatoraja_bottom_anchor() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "w": 100, "h": 100,
                "note": {
                    "id": "notes",
                    "note": ["n1"],
                    "dst": [{ "time": 0, "x": 10, "y": 20, "w": 30, "h": 40 }]
                },
                "destination": [{ "id": "notes", "offset": 30 }]
            }
            "#,
    )
    .unwrap();
    let mut offsets = SkinOffsetValues::default();
    offsets.set(
        OFFSET_NOTES_1P,
        crate::skin_offset::SkinOffsetValue { x: 10, y: 20, w: 5, h: 20, r: 0, a: 0 },
    );

    let area = document.note_lane_area(Lane::Key1, KeyMode::K7, &[]).unwrap();
    let center_y = area.y + area.height * 0.5;
    let original = Rect { x: area.x, y: center_y - 0.05, width: area.width, height: 0.1 };
    let rect = document.apply_notes_offset_to_rect(
        original,
        &SkinDrawState { skin_offsets: offsets, ..SkinDrawState::default() },
    );

    assert!(approx_eq(rect.x, original.x + 0.1));
    assert!(approx_eq(rect.y, original.y - 0.4));
    assert!(approx_eq(rect.width, original.width + 0.05));
    assert!(approx_eq(rect.height, 0.3));
    assert!(approx_eq(rect.y + rect.height, original.y + original.height - 0.2));
}

#[test]
fn notes_offset_uses_only_marker_ids_and_deduplicates_them() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
            "w": 100, "h": 100,
            "destination": [{ "id": "notes", "offsets": [30, 30, 42, 0, 200] }]
        }"#,
    )
    .unwrap();
    let mut offsets = SkinOffsetValues::default();
    offsets.set(
        30,
        crate::skin_offset::SkinOffsetValue { x: 10, y: 20, w: 5, h: 6, ..Default::default() },
    );
    offsets.set(
        42,
        crate::skin_offset::SkinOffsetValue { x: -3, y: 4, w: 7, h: -2, ..Default::default() },
    );
    offsets.set(
        SKIN_OFFSET_BAR_LINE,
        crate::skin_offset::SkinOffsetValue {
            x: 100,
            y: 100,
            w: 100,
            h: 100,
            ..Default::default()
        },
    );
    let state = SkinDrawState { skin_offsets: offsets, ..SkinDrawState::default() };

    assert_eq!(
        document.notes_destination_offset(&state),
        crate::skin_offset::SkinOffsetValue { x: 7, y: 24, w: 12, h: 4, ..Default::default() }
    );

    let document_without_marker: SkinDocument =
        serde_json::from_str(r#"{ "w": 100, "h": 100 }"#).unwrap();
    let rect = Rect { x: 0.1, y: 0.2, width: 0.3, height: 0.4 };
    assert_eq!(document_without_marker.apply_notes_offset_to_rect(rect, &state), rect);
}
