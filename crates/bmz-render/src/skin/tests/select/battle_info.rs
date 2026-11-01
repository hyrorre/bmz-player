use super::*;

#[test]
fn battle_info_refs_use_only_the_open_lists_opponent_and_clear_on_close() {
    let document: SkinDocument = serde_json::from_str(r#"{"type":5}"#).unwrap();
    let mut snapshot = SelectSnapshot {
        arrange: "S-RANDOM".into(),
        assist: "G-BATTLE".into(),
        ..Default::default()
    };
    let (closed, _) = document.select_draw_state(&snapshot, None);
    assert!(!test_skin_op(SKIN_OPTION_BMZ_SELECT_BATTLE_ACTIVE, &[], &closed));
    let mut pattern: Vec<u8> = (0..16).collect();
    pattern[1..8].copy_from_slice(&[2, 5, 3, 7, 1, 4, 6]);
    snapshot.battle = Some(Arc::new(crate::scene::SelectBattleSnapshot {
        name: "PLAYER".into(),
        arrange: Some("RANDOM".into()),
        lane_shuffle_pattern: pattern,
        ..Default::default()
    }));
    let (open, _) = document.select_draw_state(&snapshot, None);
    assert!(test_skin_op(SKIN_OPTION_BMZ_SELECT_BATTLE_ACTIVE, &[], &open));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SELECT_BATTLE_OPTIONS, &[], &open));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SELECT_BATTLE_PATTERN, &[], &open));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SELECT_BATTLE_LANE_BASE, &open), Some(2));
    assert_eq!(skin_state_number(450, &open), Some(0));
    let values = lua_main_state_text_values(&open, &SkinTextState::default());
    assert_eq!(values[&SKIN_TEXT_BMZ_SELECT_BATTLE_ARRANGE], "RANDOM");
    assert_eq!(values[&SKIN_TEXT_BMZ_SELECT_BATTLE_NAME], "PLAYER");
    snapshot.battle = None;
    let (closed, _) = document.select_draw_state(&snapshot, None);
    assert_eq!(skin_state_number(SKIN_REF_BMZ_SELECT_BATTLE_LANE_BASE, &closed), Some(0));
    assert_eq!(
        lua_main_state_text_values(&closed, &SkinTextState::default())
            [&SKIN_TEXT_BMZ_SELECT_BATTLE_ARRANGE],
        ""
    );
}

#[test]
fn battle_info_dp_flip_tiles_keep_the_source_sides_key_numbers() {
    let mut pattern: Vec<u8> = (0..16).collect();
    pattern[1] = 14;
    pattern[8] = 7;
    let battle = Arc::new(crate::scene::SelectBattleSnapshot {
        key_mode: KeyMode::K14,
        double_option: Some("FLIP".into()),
        lane_shuffle_pattern: pattern,
        ..Default::default()
    });
    let refs = select_battle_lane_refs(&battle);
    assert_eq!(refs[0], 7);
    assert_eq!(refs[10], 7);
}
