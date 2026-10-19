use super::*;
use crate::scene::detail_options::{
    DetailOptionChoice, DetailOptionRow, DetailOptionsSnapshot, DetailValueKind,
};

#[test]
fn detail_options_resolvers_and_lua_text_share_the_snapshot() {
    let panel = std::sync::Arc::new(DetailOptionsSnapshot {
        cursor: 0,
        viewport_start: 0,
        items: vec![DetailOptionRow {
            item_id: 201,
            category_id: 2,
            scope: 0,
            value: 1,
            value_index: 1,
            choice_count: 3,
            choices: vec![
                DetailOptionChoice { value: 0, label: "ASSIST EASY".into() },
                DetailOptionChoice { value: 1, label: "EASY".into() },
                DetailOptionChoice { value: 2, label: "NORMAL".into() },
            ]
            .into(),
            kind: DetailValueKind::Enum,
            label: "GAS下限".into(),
            value_label: "EASY".into(),
            category: "GAUGE".into(),
            description: "説明".into(),
            reason: "非適用".into(),
            auxiliary: "GAS: OFF".into(),
            status: "[非適用]".into(),
            editable: true,
            effective: false,
        }]
        .into(),
        title: "DETAIL OPTIONS".into(),
        scope_label: "PROFILE".into(),
        guide: "操作".into(),
        position: "1 / 1".into(),
    });
    let state = SkinDrawState {
        detail_options: Some(panel.clone()),
        select_option_panel: 2,
        ..Default::default()
    };
    for id in (19300..=19312).chain(19400..=19489).chain(19500..=20075) {
        assert_eq!(
            skin_state_number(id, &state),
            crate::scene::detail_options::number(id, Some(&panel))
        );
    }
    let texts = lua_main_state_text_values(&state, &SkinTextState::default());
    for id in (19300..=19309)
        .chain((0..9).flat_map(|slot| (0..4).map(move |field| 19400 + slot * 10 + field)))
    {
        assert_eq!(
            texts.get(&id).map(String::as_str),
            crate::scene::detail_options::text(id, Some(&panel))
        );
    }
    for id in (19300..=19305).chain(19400..=19489).chain(19500..=20075) {
        let expected = crate::scene::detail_options::option(id, Some(&panel)).unwrap();
        assert_eq!(test_skin_op(id, &[], &state), expected);
        assert_eq!(test_skin_op(-id, &[], &state), !expected);
    }
    assert!(test_skin_op(22, &[], &state)); // legacy E2 meaning is retained
    assert!(!test_skin_op(21, &[], &state));
    assert!(!test_skin_op(23, &[], &state));
    let closed = SkinDrawState::default();
    assert_eq!(skin_state_number(19305, &closed), Some(-1));
    assert!(!test_skin_op(19300, &[], &closed));
    // A single item occupies the center slot (3), leaving both edges empty.
    assert_eq!(texts[&19433], "GAUGE");
    assert_eq!(texts[&19423], "");
    let choice_base = 19500 + 3 * 64;
    assert_eq!(texts[&choice_base], "ASSIST EASY");
    assert_eq!(texts[&(choice_base + 4)], "EASY");
    assert_eq!(texts[&(choice_base + 8)], "NORMAL");
    assert_eq!(texts[&(choice_base + 12)], "");
    assert!(test_skin_op(choice_base + 5, &[], &state));
    assert!(!test_skin_op(choice_base + 1, &[], &state));
    assert_eq!(skin_state_number(19500, &closed), Some(-1));
    assert!(!test_skin_op(19500, &[], &closed));

    let mut snapshot = crate::scene::SelectSnapshot {
        detail_options_closing: Some(crate::scene::detail_options::DetailOptionsClosingSnapshot {
            panel: panel.clone(),
            scroll: -0.5,
        }),
        option_panel_off_times: [None, Some(TimeUs(100_000)), None, None, None, None],
        ..Default::default()
    };
    let mut document: SkinDocument =
        serde_json::from_str(r#"{"type":5,"bmzDetailOptions":1}"#).unwrap();
    assert!(!document.bmz_detail_options_close);
    for skin_type in [0, 5] {
        for version in [0, 1, 2] {
            document.skin_type = skin_type;
            document.bmz_detail_options = version;
            assert_eq!(document.uses_detail_options(), skin_type == 5 && version == 1);
            let active = crate::scene::SelectSnapshot {
                option_panel: 2,
                detail_options: Some(panel.clone()),
                detail_options_scroll: 0.5,
                ..Default::default()
            };
            let (state, _) = document.select_draw_state(&active, None);
            assert_eq!(test_skin_op(19300, &[], &state), document.uses_detail_options());
            if !document.uses_detail_options() {
                assert!(state.detail_options.is_none());
                assert_eq!(state.detail_options_scroll, 0.0);
                assert_eq!(skin_state_number(19305, &state), Some(-1));
                assert_eq!(
                    lua_main_state_text_values(&state, &SkinTextState::default())
                        .get(&19300)
                        .map(String::as_str)
                        .unwrap_or_default(),
                    ""
                );
            }
        }
    }
    document.skin_type = 5;
    document.bmz_detail_options = 1;
    let (state, _) = document.select_draw_state(&snapshot, None);
    assert!(!test_skin_op(19300, &[], &state));
    document.bmz_detail_options_close = true;
    let (state, _) = document.select_draw_state(&snapshot, None);
    assert!(std::sync::Arc::ptr_eq(state.detail_options.as_ref().unwrap(), &panel));
    assert_eq!(state.detail_options_scroll, -0.5);
    assert!(test_skin_op(19300, &[], &state));
    assert!(!test_skin_op(22, &[], &state));
    assert_eq!(skin_state_number(19305, &state), Some(1));
    assert_eq!(lua_main_state_text_values(&state, &SkinTextState::default())[&19300], "GAS下限");
    for elapsed in [-1, 300_000, 500_000] {
        snapshot.option_panel_off_times[1] = Some(TimeUs(elapsed));
        assert!(snapshot.closing_detail_options().is_none());
    }
    snapshot.option_panel_off_times[1] = Some(TimeUs(100_000));
    for other_panel in [1, 2, 3] {
        snapshot.option_panel = other_panel;
        assert_eq!(snapshot.closing_detail_options().is_none(), other_panel == 2);
        let (state, _) = document.select_draw_state(&snapshot, None);
        assert_eq!(test_skin_op(19300, &[], &state), other_panel != 2);
        assert!(test_skin_op(20 + i32::from(other_panel), &[], &state));
    }
    snapshot.option_panel = 0;
    snapshot.in_settings = true;
    assert!(snapshot.closing_detail_options().is_none());
    snapshot.in_settings = false;
    snapshot.detail_options = Some(panel);
    assert!(snapshot.closing_detail_options().is_none());
}

#[test]
fn detail_raise_follows_the_column_through_the_scroll_interpolation() {
    let panel = std::sync::Arc::new(DetailOptionsSnapshot {
        cursor: 0,
        viewport_start: 0,
        items: Vec::new().into(),
        title: String::new(),
        scope_label: String::new(),
        guide: String::new(),
        position: String::new(),
    });
    let raised_y = |column: i32, scroll: f32| {
        let destination: SkinDestinationDef = serde_json::from_value(serde_json::json!({
            "id": "column", "bmzDetailRaise": [column, 24], "dst": [{"x": 0, "y": 100, "w": 10, "h": 10}]
        }))
        .unwrap();
        let state = SkinDrawState {
            detail_options: Some(panel.clone()),
            detail_options_scroll: scroll,
            ..Default::default()
        };
        let mut frame = ResolvedSkinFrame { y: 100, ..Default::default() };
        apply_skin_offset_to_frame_inner(&destination, &mut frame, &state, false, false);
        frame.y - 100
    };
    // The centre column stands fully raised; neighbours rise as they slide in.
    for (column, scroll, expected) in [
        (3, 0.0, 24),
        (2, 0.0, 0),
        (4, 0.0, 0),
        (3, 0.5, 12),
        (2, 0.5, 12),
        (4, -0.5, 12),
        (3, -1.0, 0),
        (2, 1.0, 24),
        (1, 0.5, 0),
    ] {
        assert_eq!(raised_y(column, scroll), expected, "column {column}, scroll {scroll}");
    }
    // Without the E2 panel nothing moves.
    let destination: SkinDestinationDef = serde_json::from_value(serde_json::json!({
        "id": "column", "bmzDetailRaise": [3, 24], "dst": [{"x": 0, "y": 100, "w": 10, "h": 10}]
    }))
    .unwrap();
    let mut frame = ResolvedSkinFrame { y: 100, ..Default::default() };
    apply_skin_offset_to_frame_inner(
        &destination,
        &mut frame,
        &SkinDrawState::default(),
        false,
        false,
    );
    assert_eq!(frame.y, 100);
}
