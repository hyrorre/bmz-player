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
        .chain((0..9).flat_map(|slot| (0..3).map(move |field| 19400 + slot * 10 + field)))
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
