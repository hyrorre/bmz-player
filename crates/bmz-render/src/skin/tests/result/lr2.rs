use super::*;

#[test]
fn lr2_songlist_conditional_variants_keep_fixed_slots() {
    let entries: Vec<DestinationListEntry> = serde_json::from_value(serde_json::json!([
        {"if": [], "values": [{"id":"closed","op":[-21]}, {"id":"open","op":[21]}]},
        {"id":"next"}
    ]))
    .unwrap();
    let state = SkinDrawState { select_option_panel: 1, ..Default::default() };
    assert_eq!(songlist_destination_at(&entries, 0, &[], &state, true).unwrap().id, "open");
    assert_eq!(songlist_destination_at(&entries, 1, &[], &state, true).unwrap().id, "next");
}

#[test]
fn lr2_bar_conditions_distinguish_courses_actions_and_folders() {
    for (kind, expected) in [
        (SelectRowKind::NewCourse, [false, false, false, true, false]),
        (SelectRowKind::RandomCourse, [false, false, false, true, true]),
        (SelectRowKind::Course, [false, false, true, false, true]),
        (SelectRowKind::CourseFolder, [true, false, false, false, false]),
        (SelectRowKind::NewSongFolder, [true, false, false, false, false]),
        (SelectRowKind::Command, [true, false, false, false, false]),
        (SelectRowKind::Container, [true, false, false, false, false]),
        (SelectRowKind::SearchFolder, [true, false, false, false, false]),
        (SelectRowKind::Song, [false, true, false, false, true]),
        // BMZ's direct random selection is a playable extension.
        (SelectRowKind::Executable, [false, false, false, false, true]),
    ] {
        let state =
            SkinDrawState { select_screen: true, select_row_kind: kind, ..Default::default() };
        for (index, enabled) in expected.into_iter().enumerate() {
            assert_eq!(
                test_skin_ops(&[LR2_OPTION_BASE + index as i32 + 1], &[], &state),
                enabled,
                "{kind:?} op {}",
                index + 1
            );
        }
    }
}

#[test]
fn lr2_songlist_uses_new_titles_and_per_row_rival_decorations() {
    let names =
        ["body", "lamp", "player", "rival", "old", "new", "win", "lose", "draw", "unplayed"];
    let images = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            serde_json::json!({
                "id": name, "src": "src", "x": 0, "y": i * 10, "w": 10, "h": 10,
            })
        })
        .collect::<Vec<_>>();
    let part = |id: &str| serde_json::json!({"id": id, "dst": [{"x":0,"y":0,"w":10,"h":10}]});
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "type":5, "lr2":true, "w":100, "h":100,
        "image":images, "imageset":[{"id":"bars","images":["body"]}],
        "songlist":{
            "id":"list", "center":0, "liston":[part("bars")], "listoff":[part("bars")],
            "text":[part("old"),part("new")],
            "lamp":[part("lamp")], "playerlamp":[part("player")], "rivallamp":[part("rival")],
            "lr2rival":[part("win"),part("lose"),part("draw"),part("unplayed")],
        },
        "destination":[{"id":"list"}],
    }))
    .unwrap();
    let sources = mock_source("src", 100.0, 100.0);
    for (new_title, player, rival, expected) in [
        (false, None, None, vec![0, 1, 4]),
        (true, None, Some(0), vec![0, 1, 5]),
        (false, Some(200), Some(100), vec![0, 2, 3, 6, 4]),
        (false, Some(50), Some(100), vec![0, 2, 3, 7, 4]),
        (true, Some(100), Some(100), vec![0, 2, 3, 8, 5]),
        (false, None, Some(100), vec![0, 2, 3, 9, 4]),
    ] {
        let snapshot = SelectSnapshot {
            rival_selected: true,
            rows: vec![SelectRowSnapshot {
                lr2_new_title: new_title,
                ex_score: player,
                rival_ex_score: rival,
                ..Default::default()
            }],
            ..Default::default()
        };
        let rows = document
            .select_render_items(&sources, &snapshot)
            .iter()
            .filter_map(|item| match item {
                SkinRenderItem::Image { uv, .. } => Some((uv.y * 10.0).round() as i32),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(rows, expected);
    }
}

#[test]
fn lr2_songlist_renders_unowned_songs_and_courses_with_native_bodies() {
    let images: Vec<_> = (0..10)
        .map(|index| {
            serde_json::json!({
                "id": format!("body-{index}"), "src": "src",
                "x": 0, "y": index * 10, "w": 40, "h": 10,
            })
        })
        .collect();
    let body_ids: Vec<_> = (0..10).map(|index| format!("body-{index}")).collect();
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "type": 5, "lr2": true, "w": 100, "h": 100,
        "image": images,
        "imageset": [{"id": "bars", "images": body_ids}],
        "songlist": {
            "id": "songlist", "center": 0,
            "listoff": [{"id": "bars", "dst": [{"x": 0, "y": 0, "w": 40, "h": 10}]}],
            "liston": [{"id": "bars", "dst": [{"x": 0, "y": 0, "w": 40, "h": 10}]}],
        },
        "destination": [{"id": "songlist"}],
    }))
    .unwrap();
    let sources = mock_source("src", 100.0, 100.0);
    for (kind, in_library, expected_body) in [
        (SelectRowKind::Song, true, 0),
        (SelectRowKind::Song, false, 5),
        (SelectRowKind::NoSong, false, 5),
        (SelectRowKind::Command, true, 2),
        (SelectRowKind::Container, true, 2),
        (SelectRowKind::SearchFolder, true, 2),
        (SelectRowKind::TableFolder, true, 2),
        (SelectRowKind::Folder, true, 1),
        (SelectRowKind::CourseFolder, true, 6),
        (SelectRowKind::NewCourse, true, 7),
        (SelectRowKind::NewSongFolder, true, 3),
        (SelectRowKind::Executable, true, 2),
        (SelectRowKind::Course, true, 8),
        (SelectRowKind::Course, false, 5),
        (SelectRowKind::RandomCourse, true, 9),
        (SelectRowKind::RandomCourse, false, 5),
        // Settings categories are normal folders; entry, back and close are rival folders.
        (SelectRowKind::SettingsRoot, true, 4),
        (SelectRowKind::SettingsFolder, true, 1),
        (SelectRowKind::SettingsBack, true, 4),
        (SelectRowKind::SettingsClose, true, 4),
    ] {
        let snapshot = SelectSnapshot {
            rows: vec![SelectRowSnapshot { kind, in_library, ..Default::default() }],
            ..Default::default()
        };
        let items = document.select_render_items(&sources, &snapshot);
        assert_eq!(items.len(), 1, "{kind:?}, owned={in_library}");
        assert!(
            matches!(&items[0], SkinRenderItem::Image { uv, .. }
            if approx_eq(uv.y, expected_body as f32 / 10.0)),
            "{kind:?}, owned={in_library}: {items:?}"
        );
    }
}

#[test]
fn lr2_result_flip_uses_player_values_without_fabricating_an_opponent() {
    let mut state = SkinDrawState {
        ex_score: 1234,
        target_ex_score: Some(2345),
        result_failed: Some(false),
        skin_attempt: SkinAttemptState { session_mode_index: Some(4), ..Default::default() },
        ..Default::default()
    };
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 1, &state), Some(1234));
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 21, &state), Some(2345));
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 30, &state), None);
    state.lr2_result_flip = true;
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 1, &state), Some(2345));
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 21, &state), Some(1234));
    state.total_notes = 1000;
    assert!(test_skin_ops(&[LR2_RESULT_RANK_BASE + 13], &[], &state));
    assert!(!test_skin_ops(&[LR2_RESULT_RANK_BASE + 3], &[], &state));
    state.lr2_result_flip = false;
    assert!(test_skin_ops(&[LR2_RESULT_RANK_BASE + 3], &[], &state));
    assert!(!test_skin_ops(&[LR2_RESULT_RANK_BASE + 13], &[], &state));
}

#[test]
fn lr2_charts_animate_with_cache_and_filter_the_selected_gauge() {
    let mut doc: SkinDocument = serde_json::from_value(serde_json::json!({
        "w":100,"h":100,"lr2":true,
        "image":[{"id":"gauge","src":"src","w":2,"h":2}],
        "lr2Charts":[{"id":"gauge","score":false,"player":0,"index":0,"width":10,"height":80,"start":500,"end":1500}],
        "destination":[{"id":"gauge","dst":[{"x":5,"y":10,"w":2,"h":2,"a":0,"lr2Style":{"blend":0,"filter":1,"center":0}}]}]
    })).unwrap();
    doc.result_gauge_graph_points = vec![
        crate::snapshot::ResultGaugeGraphPoint {
            time_ms: 0,
            value: 20.0,
            max: 100.0,
            border: 80.0,
            gauge_type: 2,
            course_section_start: false,
        },
        crate::snapshot::ResultGaugeGraphPoint {
            time_ms: 0,
            value: 100.0,
            max: 100.0,
            border: 0.0,
            gauge_type: 3,
            course_section_start: false,
        },
    ];
    let sources = mock_source("src", 2.0, 2.0);
    let mut cache = ResultRenderCache::default();
    for (elapsed, count) in [(499, 0), (1000, 3), (1500, 5), (2000, 5)] {
        let state = SkinDrawState {
            elapsed_ms: elapsed,
            gauge_type: 2,
            result_failed: Some(false),
            ..Default::default()
        };
        let items = doc.static_render_items_with_graphs_cached(
            &sources,
            &state,
            &SkinTextState::default(),
            SkinRuntimeGraphs::from_document(&doc),
            Some(&mut cache),
        );
        assert_eq!(items.len(), count, "at {elapsed}");
        for item in items {
            let SkinRenderItem::Image { rect, tint, .. } = item else { panic!("image") };
            assert_eq!(tint.a, 1.0, "LR2 blend 0 ignores DST alpha");
            assert!((rect.y - 0.72).abs() < 0.001, "wrong gauge series: {rect:?}");
        }
    }
    let state = SkinDrawState {
        elapsed_ms: 700,
        gauge_type: 2,
        result_failed: Some(false),
        result_graph_end_ms: Some(0),
        ..Default::default()
    };
    assert_eq!(doc.static_image_render_items(&sources, &state).len(), 5);
    doc.lr2_result = Some(SkinLr2ResultDef { flip: true, ..Default::default() });
    assert!(doc.static_image_render_items(&sources, &state).is_empty());
    doc.lr2_charts[0].player = 1;
    assert_eq!(doc.static_image_render_items(&sources, &state).len(), 5);
    let mut late = doc.result_gauge_graph_points[0];
    late.time_ms = 900;
    late.value = 40.0;
    doc.result_gauge_graph_points.push(late);
    let state = SkinDrawState { result_duration_ms: 1000, ..state };
    let items = doc.static_image_render_items(&sources, &state);
    for item in items.iter().take(4) {
        let SkinRenderItem::Image { rect, .. } = item else { panic!("image") };
        assert!((rect.y - 0.72).abs() < 0.001, "time distribution: {rect:?}");
    }
    let SkinRenderItem::Image { rect, .. } = items.last().unwrap() else { panic!("image") };
    assert!((rect.y - 0.56).abs() < 0.001, "last sample: {rect:?}");
}

#[test]
fn lr2_button_indices_and_click_panel_restrictions_are_separate_from_bmz() {
    let state = SkinDrawState {
        select_screen: true,
        select_gauge_index: 2,
        select_extended_arrange_index: 4,
        select_hs_fix_index: 4,
        ..Default::default()
    };
    for (button, expected) in [(40, 0), (42, 3), (55, 2)] {
        assert_eq!(skin_state_event_index(LR2_BUTTON_BASE + button, &state), expected);
    }
    assert_eq!(skin_state_event_index(40, &state), 2);
    assert_eq!(
        skin_state_event_index(
            LR2_BUTTON_BASE + 55,
            &SkinDrawState { select_hs_fix_index: 1, ..state.clone() }
        ),
        -1
    );
    let doc: SkinDocument = serde_json::from_value(serde_json::json!({
        "w":100,"h":100,"lr2":true,
        "image":[{"id":"button","src":"src","w":10,"h":10,"act":90040,"clickable":true,"lr2Panel":1}],
        "destination":[{"id":"button","dst":[{"x":10,"y":10,"w":10,"h":10}]}]
    })).unwrap();
    assert!(doc.result_click_hit(&state, 0.15, 0.85).is_none());
    let state = SkinDrawState { select_option_panel: 1, ..state };
    assert!(doc.result_click_hit(&state, 0.15, 0.85).is_some());
}

#[test]
fn lr2_select_option_buttons_show_cycled_profile_values() {
    let select = SkinDrawState { select_screen: true, ..Default::default() };
    let index = |button: i32, state: &SkinDrawState| {
        skin_state_event_index(LR2_BUTTON_BASE + button, state)
    };
    // RANDOM: H-RANDOM / ALL-SCR are LR2 values 4 / 5; R-RANDOM has no LR2 frame.
    for (arrange, expected) in [(6, 4), (7, 5), (3, -1)] {
        let state = SkinDrawState { select_extended_arrange_index: arrange, ..select.clone() };
        assert_eq!(index(42, &state), expected);
    }
    // EFFECT: OFF / HIDDEN / SUDDEN / HID+SUD.
    for (sudden, hidden, expected) in
        [(false, false, 0), (false, true, 1), (true, false, 2), (true, true, 3)]
    {
        let state =
            SkinDrawState { lanecover_enabled: sudden, hidden_enabled: hidden, ..select.clone() };
        assert_eq!(index(50, &state), expected);
        // LANE COVER is ON with SUDDEN and HID+SUD.
        assert_eq!(index(46, &state), i32::from(sudden));
    }
    let state = SkinDrawState { judge_timing_auto_adjust: true, ..select.clone() };
    assert_eq!((index(75, &select), index(75, &state)), (0, 1));
    let state = SkinDrawState { lr2_select_target_index: 6, ..select.clone() };
    assert_eq!(index(77, &state), 6);
    // Result screens keep these select-only buttons hidden.
    let result = SkinDrawState { select_screen: false, ..state };
    assert_eq!((index(50, &result), index(75, &result), index(77, &result)), (-1, -1, -1));
    assert_eq!(index(46, &result), -1);
}

#[test]
fn lr2_numbers_follow_lr2_definitions() {
    let lr2 = |id: i32, state: &SkinDrawState| skin_state_number(LR2_NUMBER_BASE + id, state);
    let mut state = SkinDrawState {
        play_screen: true,
        key_mode: KeyMode::K7,
        total_notes: 100,
        past_notes: 50,
        ex_score: 120,
        gauge: 77.9,
        target_ex_score: Some(160),
        judge_counts: crate::snapshot::DisplayJudgeCounts {
            pgreat: 50,
            great: 20,
            good: 5,
            bad: 3,
            poor: 4,
            empty_poor: 6,
        },
        ..Default::default()
    };
    // POOR includes empty POOR, unlike beatoraja's NUMBER_POOR (114).
    assert_eq!(lr2(114, &state), Some(10));
    assert_eq!(skin_state_number(114, &state), Some(4));
    assert_eq!(lr2(100, &state), Some((50 * 4 + 20 * 2 + 5) * 50_000 / 100));
    assert_eq!(lr2(107, &state), Some(76));
    // Single play compares with the target ghost at the current progress.
    assert_eq!(lr2(108, &state), Some(120 - 80));
    // 120 / 200 is rank 5 (B); A starts at 200 * 6 / 9 = 133.
    assert_eq!(lr2(109, &state), Some(120 - 133));
    // Result 1P POOR goes through the flip namespace to the same definition.
    state.play_screen = false;
    state.result_failed = Some(false);
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 14, &state), Some(10));
    state.lr2_result_flip = true;
    assert_eq!(skin_state_number(LR2_RESULT_NUMBER_BASE + 34, &state), Some(10));
}

#[test]
fn lr2_select_ir_numbers_are_not_the_main_bpm() {
    let mut state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_chart_main_bpm: 150.0,
        select_ex_score: Some(900),
        rival_ex_score: Some(850),
        ..Default::default()
    };
    state.ir_ranking.rank = Some(12);
    state.ir_ranking.total_player = Some(345);
    let mut counts = [0; 11];
    counts[1] = 10; // FAILED
    counts[4] = 20; // EASY
    counts[6] = 10; // HARD
    state.ir_ranking.clear_counts = Some(counts);
    let lr2 = |id: i32, state: &SkinDrawState| skin_state_number(LR2_NUMBER_BASE + id, state);
    assert_eq!(lr2(92, &state), Some(12));
    assert_eq!(skin_state_number(92, &state), Some(150));
    assert_eq!(lr2(93, &state), Some(345));
    assert_eq!(lr2(94, &state), Some(75));
    assert_eq!(lr2(95, &state), Some(50));
    // Folders have no per-song IR values.
    state.select_is_folder = true;
    assert_eq!((lr2(92, &state), lr2(95, &state)), (None, None));
}

#[test]
fn lr2_options_follow_lr2_definitions() {
    let op = |id: i32, state: &SkinDrawState| test_skin_ops(&[LR2_OPTION_BASE + id], &[], state);
    let mut select = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_total_notes: 500,
        select_ex_score: Some(780), // 780 * 9 / 1000 = 7 (AA)
        select_clear_index: 7,      // EX-HARD is LR2's HARD lamp
        ..Default::default()
    };
    assert!(op(104, &select) && !op(103, &select) && !op(105, &select));
    assert!(op(111, &select) && !op(110, &select) && !op(112, &select));
    assert!(test_skin_ops(&[-(LR2_OPTION_BASE + 110)], &[], &select));
    // Ranks are hidden until the song has been played.
    select.select_clear_index = 0;
    assert!(op(100, &select) && !op(111, &select));
    select.select_is_folder = true;
    assert!(!op(100, &select));
    // Fixed values for settings BMZ does not have.
    for (id, expected) in [(52, true), (53, false), (54, true), (620, true), (621, false)] {
        assert_eq!(op(id, &select), expected, "option {id}");
    }
    // Result new rank 340..347 from the score just played.
    let result = SkinDrawState {
        result_failed: Some(false),
        total_notes: 500,
        ex_score: 900,
        ..Default::default()
    };
    assert!(op(340, &result) && !op(341, &result));
    let result = SkinDrawState { ex_score: 250, ..result };
    assert!(op(346, &result) && !op(347, &result));
}

#[test]
fn lr2_select_numbers_follow_lr2_definitions() {
    let mut state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_total_notes: 500,
        select_ex_score: Some(780),
        select_play_level: 12,
        difficulty: 4,
        judge_timing_offset_ms: -15,
        select_sort_index: 2,
        select_double_option_index: 1,
        key_mode: KeyMode::K7,
        judge_counts: crate::snapshot::DisplayJudgeCounts {
            pgreat: 300,
            great: 180,
            good: 15,
            ..Default::default()
        },
        ..Default::default()
    };
    let lr2 = |id: i32, state: &SkinDrawState| skin_state_number(LR2_NUMBER_BASE + id, state);
    // Judge timing instead of the sort; beatoraja's 12 stays the sort on select.
    assert_eq!((lr2(12, &state), skin_state_number(12, &state)), (Some(-15), Some(2)));
    // Only the selected chart's difficulty slot (ANOTHER) has a level.
    assert_eq!(lr2(48, &state), Some(12));
    assert_eq!((lr2(45, &state), lr2(49, &state)), (None, None));
    // EQ4 is not BMZ's DP option index.
    assert_eq!((lr2(54, &state), skin_state_number(54, &state)), (None, Some(1)));
    assert_eq!(lr2(70, &state), Some((300 * 4 + 180 * 2 + 15) * 50_000 / 500));
    assert_eq!(lr2(73, &state), Some(78));
    // IR clear classes: FAILED / EASY (assist + easy) / CLEAR / HARD (+EX-HARD) / FULLCOMBO.
    let mut counts = [0; 11];
    (counts[1], counts[2], counts[4], counts[5], counts[7], counts[10]) = (5, 1, 4, 6, 2, 2);
    state.ir_ranking.state = crate::scene::ResultIrState::Loaded;
    state.ir_ranking.clear_counts = Some(counts);
    let classes = (210..=219).map(|id| lr2(id, &state)).collect::<Vec<_>>();
    assert_eq!(classes, [5, 25, 5, 25, 6, 30, 2, 10, 2, 10].map(Some).to_vec(),);
}

#[test]
fn lr2_select_panel_option_and_selected_score_rate_graphs() {
    let mut state = SkinDrawState {
        select_screen: true,
        select_total_notes: 400,
        judge_counts: crate::snapshot::DisplayJudgeCounts { pgreat: 100, ..Default::default() },
        ..Default::default()
    };
    assert!(test_skin_ops(&[LR2_OPTION_BASE + 20], &[], &state));
    state.select_option_panel = 1;
    assert!(!test_skin_ops(&[LR2_OPTION_BASE + 20], &[], &state));
    let graph: SkinGraphDef =
        serde_json::from_value(serde_json::json!({"id": "pg", "type": 140})).unwrap();
    assert!((graph_raw_value(&graph, &state) - 0.25).abs() < 1e-6);
}

#[test]
fn lr2_select_dp_conditions_follow_style_instead_of_chart() {
    for (mode, double) in
        [(0, false), (2, false), (3, false), (4, true), (5, true), (6, true), (7, false)]
    {
        for key_mode in [KeyMode::K7, KeyMode::K14] {
            let state = SkinDrawState {
                select_screen: true,
                lr2_select_mode_index: mode,
                key_mode,
                select_chart_key_mode: Some(key_mode),
                ..Default::default()
            };
            assert_eq!(
                test_skin_ops(&[LR2_OPTION_BASE + 10, -(LR2_OPTION_BASE + 11)], &[], &state),
                double
            );
            assert_eq!(test_skin_ops(&[-(LR2_OPTION_BASE + 12)], &[], &state), !double);
            assert!(test_skin_ops(&[LR2_OPTION_BASE + 572, -(LR2_OPTION_BASE + 571)], &[], &state));
        }
    }
    for panel in 0..=9 {
        let state = SkinDrawState { select_option_panel: panel, ..Default::default() };
        for required in 1..=9 {
            assert_eq!(
                test_skin_ops(&[LR2_OPTION_BASE + 20 + required], &[], &state),
                i32::from(panel) == required
            );
        }
    }
}

#[test]
fn lr2_ghost_rival_and_lamp_challenge_values() {
    let lr2 = |id: i32, state: &SkinDrawState| skin_state_number(LR2_NUMBER_BASE + id, state);
    let op = |id: i32, state: &SkinDrawState| test_skin_ops(&[LR2_OPTION_BASE + id], &[], state);
    // Play: best / target ghosts at half progress of a 100-note chart.
    let play = SkinDrawState {
        play_screen: true,
        total_notes: 100,
        past_notes: 50,
        ex_score: 90,
        best_ex_score: Some(150),
        target_ex_score: Some(180),
        ..Default::default()
    };
    assert_eq!((lr2(150, &play), lr2(151, &play)), (Some(75), Some(90)));
    assert_eq!((lr2(155, &play), lr2(156, &play)), (Some(37), Some(50)));
    assert_eq!((lr2(157, &play), lr2(158, &play)), (Some(45), Some(0)));
    // Single play 2P numbers follow the target ghost.
    let two_p = |id: i32| skin_state_number(SKIN_REF_BMZ_LR2_2P_BASE + id - 120, &play);
    assert_eq!((two_p(121), two_p(122), two_p(128)), (Some(90), Some(90), Some(0)));
    assert!(op(354, &play) && !op(352, &play) && !op(353, &play));

    // Select: rival lamp / rank / max EX and the lamp the current settings can earn.
    let mut select = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_total_notes: 500,
        rival_ex_score: Some(820),
        rival_clear_index: Some(6),
        select_gauge_index: 5,
        ..Default::default()
    };
    assert!(op(644, &select) && !op(643, &select));
    assert!(op(651, &select) && !op(650, &select));
    assert_eq!(
        (lr2(272, &select), lr2(273, &select), lr2(274, &select)),
        (Some(1000), Some(82), Some(500))
    );
    assert!(op(66, &select));
    select.select_extended_arrange_index = 6; // H-RANDOM
    assert!(op(63, &select));
    select.select_session_mode_index = 2; // AUTOPLAY
    assert!(op(62, &select));
}

#[test]
fn lr2_level_bar_uses_same_folder_difficulties() {
    use crate::scene::SelectSameFolderDifficulty as Slot;
    let op = |id: i32, state: &SkinDrawState| test_skin_ops(&[LR2_OPTION_BASE + id], &[], state);
    let lr2 = |id: i32, state: &SkinDrawState| skin_state_number(LR2_NUMBER_BASE + id, state);
    let empty = Slot::default();
    let state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_chart_key_mode: Some(KeyMode::K7),
        select_same_folder: Some([
            empty,
            Slot { charts: 1, level: Some(5), clear_index: 4 },
            Slot { charts: 1, level: Some(9), clear_index: 0 },
            Slot { charts: 2, level: Some(12), clear_index: 7 },
            Slot { charts: 1, level: Some(13), clear_index: 10 },
        ]),
        ..Default::default()
    };
    // Levels per difficulty; a missing difficulty has no level.
    assert_eq!(lr2(45, &state), None);
    assert_eq!((lr2(46, &state), lr2(48, &state), lr2(49, &state)), (Some(5), Some(12), Some(13)));
    // Missing / present / one chart / several charts.
    assert!(op(500, &state) && !op(505, &state));
    assert!(op(506, &state) && op(511, &state) && op(518, &state) && !op(513, &state));
    // Lamps per difficulty: EASY is LR2 lamp 2, EX-HARD folds into HARD (4), MAX into FC (5).
    assert!(op(532, &state) && op(540, &state) && op(554, &state) && op(565, &state));
    assert!(op(520, &state));
    // 7 keys flash above level 12.
    assert!(op(73, &state) && !op(78, &state) && op(79, &state) && !op(74, &state));
    // Bar length is the level over the same limit.
    let graph = |kind: i32| {
        let def: SkinGraphDef =
            serde_json::from_value(serde_json::json!({"id": "g", "type": kind})).unwrap();
        graph_raw_value(&def, &state)
    };
    assert!((graph(106) - 5.0 / 12.0).abs() < 1e-6);
    assert_eq!((graph(105), graph(109)), (0.0, 1.0));
}

#[test]
fn lr2_clear_option_history_bits_drive_options_118_to_149() {
    let op = |id: i32, state: &SkinDrawState| test_skin_ops(&[LR2_OPTION_BASE + id], &[], state);
    let state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_lr2_option_history: Some(0x1 | 0x200 | 0x100_0000),
        ..Default::default()
    };
    assert!(op(118, &state) && op(127, &state) && op(142, &state));
    assert!(!op(119, &state) && !op(126, &state) && !op(134, &state));
    let folder = SkinDrawState { select_is_folder: true, ..state };
    assert!(!op(118, &folder));
}

#[test]
fn lr2_course_stage_levels() {
    let mut state = SkinDrawState { select_screen: true, ..Default::default() };
    state.select_course_levels[0] = Some(10);
    state.select_course_levels[1] = Some(12);
    let lr2 = |id: i32| skin_state_number(LR2_NUMBER_BASE + id, &state);
    assert_eq!((lr2(250), lr2(251), lr2(252)), (Some(10), Some(12), None));
}

#[test]
fn lr2_course_stage_count_options_follow_course_definition() {
    // Two stages were played before a FAILED; the course has four.
    let state = SkinDrawState {
        result_failed: Some(true),
        course_result: crate::scene::CourseResultSkinSnapshot {
            stage_count: 2,
            definition_stage_count: 4,
            ..Default::default()
        },
        ..Default::default()
    };
    let op = |id: i32| test_skin_ops(&[LR2_OPTION_BASE + id], &[], &state);
    assert!(op(580) && op(583) && !op(584) && !op(589));
}

#[test]
fn lr2_embedded_skin_settings_texts_buttons_and_scroll_slider() {
    let settings = crate::scene::Lr2SkinSettingsSnapshot {
        target_button: 175,
        skin_name: "EC:BE".to_string(),
        skin_author: "ric".to_string(),
        items: (0..13).map(|i| (format!("Option {i}"), format!("Value {i}"))).collect(),
        scroll: 2,
        visible_rows: 0,
    };
    let text = SkinTextState { lr2_skin_settings: Some(&settings), ..SkinTextState::default() };
    let state = SkinDrawState {
        select_screen: true,
        lr2_skin_target: 175,
        lr2_skin_scroll: 2.0 / 3.0,
        ..Default::default()
    };
    let lr2_text = |id: i32| skin_main_state_text(LR2_TEXT_BASE + id, Some(&state), &text);
    assert_eq!((lr2_text(50), lr2_text(51)), ("EC:BE".to_string(), "ric".to_string()));
    // Ten rows from the scroll position; rows past the list are blank.
    assert_eq!((lr2_text(100), lr2_text(110)), ("Option 2".to_string(), "Value 2".to_string()));
    assert_eq!((lr2_text(109), lr2_text(119)), ("Option 11".to_string(), "Value 11".to_string()));
    assert_eq!(settings.max_scroll(), 3);
    // The edited skin type is lit; skin change and row buttons are shown.
    let index = |button: i32| skin_state_event_index(LR2_BUTTON_BASE + button, &state);
    assert_eq!((index(175), index(170), index(190), index(225)), (1, 0, 0, 0));
    assert_eq!(skin_slider_progress_by_type(7, &state), Some(2.0 / 3.0));
}

#[test]
fn lr2_option_labels_show_bmz_values() {
    let text = SkinTextState {
        select_mode: select_mode_label("7K"),
        select_sort: "SCORE",
        select_gauge: "HAZARD",
        select_arrange: "H-RANDOM",
        select_bga: "AUTO",
        ..SkinTextState::default()
    };
    let state = SkinDrawState {
        select_screen: true,
        judge_timing_auto_adjust: true,
        select_difficulty_filter_index: 4,
        hidden_enabled: true,
        lanecover_enabled: true,
        ..Default::default()
    };
    let lr2_text = |id: i32| skin_main_state_text(LR2_TEXT_BASE + id, Some(&state), &text);
    assert_eq!(lr2_text(60), "7KEYS");
    assert_eq!(lr2_text(61), "SCORE");
    assert_eq!(lr2_text(62), "ANOTHER");
    assert_eq!(lr2_text(63), "H-RANDOM");
    assert_eq!(lr2_text(65), "HAZARD");
    assert_eq!(lr2_text(76), "AUTO");
    assert_eq!(lr2_text(80), "ON");
    assert_eq!(lr2_text(84), "HID+SUD");
    assert_eq!(lr2_text(85), "HID+SUD");
    // Settings BMZ cannot change keep LR2's labels blank.
    assert_eq!(lr2_text(71), "");
}

#[test]
fn lr2_assist_and_battle_follow_bmz_settings() {
    let battle = |session: &str, double: &str, conversion: &str| {
        lr2_select_battle_index(&crate::scene::SelectSnapshot {
            assist: session.to_string(),
            double_option: double.to_string(),
            key_mode_conversion: conversion.to_string(),
            ..Default::default()
        })
    };
    assert_eq!(battle("NORMAL", "FLIP", "OFF"), 0);
    assert_eq!(battle("NORMAL", "BATTLE", "OFF"), 2);
    assert_eq!(battle("PRACTICE", "BATTLE AS", "7K TO 9K"), 2);
    assert_eq!(battle("NORMAL", "OFF", "SP TO DP"), 3);
    assert_eq!(battle("G-BATTLE", "BATTLE", "SP TO DP"), 4);

    let op = |id: i32, state: &SkinDrawState| test_skin_ops(&[LR2_OPTION_BASE + id], &[], state);
    let index = |button: i32, state: &SkinDrawState| {
        skin_state_event_index(LR2_BUTTON_BASE + button, state)
    };
    let text = SkinTextState { select_double_option: "BATTLE AS", ..SkinTextState::default() };
    let select = SkinDrawState { select_screen: true, ..Default::default() };
    let assisted = SkinDrawState {
        auto_scratch_setting: true,
        lr2_select_battle_index: 2,
        select_double_option_index: 3,
        ..select.clone()
    };
    assert_eq!((index(44, &select), index(45, &assisted), index(56, &assisted)), (0, 1, 2));
    // FLIP is off while D-BATTLE uses DP OPTION.
    assert_eq!(index(54, &assisted), 0);
    assert!(op(54, &select) && op(56, &select) && !op(55, &select) && !op(57, &select));
    assert!(op(55, &assisted) && op(57, &assisted) && !op(54, &assisted));
    // D-BATTLE plays on both sides, as LR2's DOUBLE options 10 / 12.
    assert!(op(10, &assisted) && op(12, &assisted) && !op(10, &select));
    let lr2_text = |id: i32, state: &SkinDrawState| {
        skin_main_state_text(LR2_TEXT_BASE + id, Some(state), &text)
    };
    assert_eq!(
        (lr2_text(67, &select), lr2_text(68, &assisted)),
        ("OFF".into(), "AUTO SCRATCH".into())
    );
    assert_eq!((lr2_text(69, &select), lr2_text(69, &assisted)), ("OFF".into(), "D-BATTLE".into()));
    assert_eq!(lr2_text(70, &assisted), "OFF");
    for (battle, label) in [(3, "SP-TO-DP"), (4, "G-BATTLE")] {
        let state = SkinDrawState { lr2_select_battle_index: battle, ..select.clone() };
        assert_eq!(lr2_text(69, &state), label);
    }

    // Play reads the scratch lanes actually played automatically.
    let mut play =
        SkinDrawState { play_screen: true, auto_scratch_setting: true, ..Default::default() };
    play.auto_note_lanes[Lane::Scratch.index()] = true;
    assert!(op(55, &play) && op(56, &play) && !op(57, &play));
    // Result and other scenes show the setting, like LR2's `p1_assist`.
    let result = SkinDrawState { auto_scratch_setting: true, ..Default::default() };
    assert!(op(55, &result) && op(57, &result) && !op(54, &result));
    assert!(op(54, &SkinDrawState::default()));

    // Lamp challenge: AUTO SCRATCH limits to EASY except in D-BATTLE; SP-TO-DP saves nothing.
    let lamp = |state: &SkinDrawState| (62..=66).find(|&id| op(id, state)).map(|id| id - 62);
    let normal = SkinDrawState { select_gauge_index: 2, ..select.clone() };
    assert_eq!(lamp(&normal), Some(2));
    let auto = SkinDrawState { auto_scratch_setting: true, ..normal.clone() };
    assert_eq!(lamp(&auto), Some(1));
    assert_eq!(lamp(&SkinDrawState { lr2_select_battle_index: 2, ..auto.clone() }), Some(2));
    assert_eq!(lamp(&SkinDrawState { lr2_select_battle_index: 3, ..normal }), Some(0));
}

#[test]
fn lr2_ir_status_options_are_exclusive() {
    use crate::scene::{ResultIrSnapshot, ResultIrState};
    let state = |ir_state: ResultIrState, total_player: Option<i64>| SkinDrawState {
        select_screen: true,
        ir_ranking: ResultIrSnapshot { state: ir_state, total_player, ..Default::default() },
        ..Default::default()
    };
    let lr2 = |state: &SkinDrawState| {
        (600..=608)
            .filter(|id| test_skin_ops(&[LR2_OPTION_BASE + id], &[], state))
            .collect::<Vec<_>>()
    };
    // IR not set and non-chart rows are Offline in BMZ.
    for (ir_state, total, expected) in [
        (ResultIrState::Offline, None, 600),
        (ResultIrState::Waiting, None, 606),
        (ResultIrState::Loading, None, 607),
        (ResultIrState::Loaded, Some(0), 603),
        (ResultIrState::Loaded, Some(1), 602),
        (ResultIrState::Loaded, Some(25), 602),
        (ResultIrState::Failed, None, 604),
        (ResultIrState::Restricted, None, 605),
    ] {
        assert_eq!(lr2(&state(ir_state, total)), vec![expected], "{ir_state:?} {total:?}");
    }
    // Players -> no players -> failure -> players never shows two statuses.
    for (ir_state, total, expected) in [
        (ResultIrState::Loaded, Some(3), 602),
        (ResultIrState::Loaded, Some(0), 603),
        (ResultIrState::Failed, None, 604),
        (ResultIrState::Loaded, Some(3), 602),
    ] {
        assert_eq!(lr2(&state(ir_state, total)), vec![expected]);
    }
    // beatoraja / JSON / Lua keep their overlapping meanings.
    let shared = |state: &SkinDrawState| {
        (600..=608).filter(|id| test_skin_ops(&[*id], &[], state)).collect::<Vec<_>>()
    };
    assert_eq!(shared(&state(ResultIrState::Loaded, Some(0))), vec![602, 603]);
    assert_eq!(shared(&state(ResultIrState::Failed, None)), vec![604, 608]);
    // beatoraja has no banned-song property, so a restricted chart keeps the failure display.
    assert_eq!(shared(&state(ResultIrState::Restricted, None)), vec![604, 608]);
}

#[test]
fn select_key_filter_labels_follow_lr2_wording() {
    for (mode, label) in [
        ("ALL", "ALL KEYS"),
        ("7K", "7KEYS"),
        ("5K", "5KEYS"),
        ("14K", "14KEYS"),
        ("10K", "10KEYS"),
        ("9K", "9KEYS"),
        ("4K", "4KEYS"),
        ("6K", "6KEYS"),
        ("8K", "8KEYS"),
    ] {
        assert_eq!(select_mode_label(mode), label);
    }
}
