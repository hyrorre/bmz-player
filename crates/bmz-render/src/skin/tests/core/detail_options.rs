use super::*;
use crate::scene::detail_options::{DetailOptionRow, DetailOptionsSnapshot, DetailValueKind};

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
    for id in (19300..=19312).chain(19400..=19469) {
        assert_eq!(
            skin_state_number(id, &state),
            crate::scene::detail_options::number(id, Some(&panel))
        );
    }
    let texts = lua_main_state_text_values(&state, &SkinTextState::default());
    for id in (19300..=19309)
        .chain((0..7).flat_map(|slot| (0..3).map(move |field| 19400 + slot * 10 + field)))
    {
        assert_eq!(
            texts.get(&id).map(String::as_str),
            crate::scene::detail_options::text(id, Some(&panel))
        );
    }
    for id in (19300..=19305).chain(19400..=19469) {
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
}
