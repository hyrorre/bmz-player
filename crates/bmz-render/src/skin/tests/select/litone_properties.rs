use super::*;

#[test]
fn selected_clear_lamps_follow_each_selected_score_and_are_select_only() {
    let options = [100, 101, 1100, 1101, 102, 103, 104, 1102, 105, 1103, 1104];
    let mut state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        ..Default::default()
    };
    for (clear_index, &expected) in options.iter().enumerate() {
        state.select_clear_index = clear_index as i64;
        for &option in &options {
            assert_eq!(test_skin_ops(&[option], &[], &state), option == expected);
            assert_eq!(test_skin_ops(&[-option], &[], &state), option != expected);
        }
        // LITONE full-combo bars use these same conditions to replace rank bars.
        assert_eq!(test_skin_ops(&[-105, -1103, -1104], &[], &state), clear_index < 8);
        state.select_screen = false;
        assert!(!test_skin_ops(&[expected], &[], &state));
        state.select_screen = true;
    }
    state.select_clear_index = 0;
    state.select_row_kind = SelectRowKind::Folder;
    assert!(!test_skin_ops(&[100], &[], &state));
    state.select_row_kind = SelectRowKind::Course;
    assert!(test_skin_ops(&[100], &[], &state));
    state.in_settings = true;
    assert!(!test_skin_ops(&[100], &[], &state));
}

#[test]
fn ir_lamp_counts_and_fractional_rates_use_the_complete_population() {
    let mut state = SkinDrawState {
        ir_ranking: crate::scene::ResultIrSnapshot {
            state: crate::scene::ResultIrState::Loaded,
            total_player: Some(66),
            clear_counts: Some([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
            ..Default::default()
        },
        ..Default::default()
    };
    let count_refs = [202, 210, 204, 206, 212, 214, 216, 208, 218, 222, 224];
    let fractional_refs = [230, 234, 231, 232, 235, 236, 237, 233, 238, 239, 240];
    for (index, (&count_ref, &fractional_ref)) in
        count_refs.iter().zip(&fractional_refs).enumerate()
    {
        let count = index as i64 + 1;
        assert_eq!(skin_state_number(count_ref, &state), Some(count));
        assert_eq!(skin_state_number(count_ref + 1, &state), Some(count * 100 / 66));
        assert_eq!(skin_state_number(fractional_ref, &state), Some(count * 1_000 / 66 % 10));
    }
    for (reference, expected) in [(226, 63), (227, 95), (241, 4), (228, 30), (229, 45), (242, 4)] {
        assert_eq!(skin_state_number(reference, &state), Some(expected));
    }
    state.ir_ranking.clear_counts = Some([0; 11]);
    assert_eq!(skin_state_number(228, &state), Some(0));
    assert_eq!(skin_state_number(229, &state), None);
    assert_eq!(skin_state_number(242, &state), None);
    state.ir_ranking.state = crate::scene::ResultIrState::Loading;
    assert_eq!(skin_state_number(228, &state), None);
    state.ir_ranking.state = crate::scene::ResultIrState::Loaded;
    state.ir_ranking.clear_counts = None;
    state.ir_ranking.clear_rate = Some(98);
    assert_eq!(skin_state_number(227, &state), Some(98));
    assert_eq!(skin_state_number(226, &state), None);
    assert_eq!(skin_state_number(241, &state), None);
}

#[test]
fn scratch_only_charts_do_not_fall_back_to_total_for_normal_keys() {
    let state = SkinDrawState {
        select_screen: true,
        select_total_notes: 38,
        select_chart_scratch_notes: 38,
        ..Default::default()
    };
    assert_eq!(skin_state_number(350, &state), Some(0));
    assert_eq!(skin_state_number(352, &state), Some(38));
}
