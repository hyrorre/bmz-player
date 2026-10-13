use super::*;

#[test]
fn select_song_level_visibility_honors_row_flag() {
    let mut row = SelectRowSnapshot { kind: SelectRowKind::Song, ..SelectRowSnapshot::default() };
    assert!(select_row_shows_level(&row));

    row.show_level = false;
    assert!(!select_row_shows_level(&row));
}

#[test]
fn select_long_note_options_follow_the_selected_chart() {
    let document: SkinDocument = serde_json::from_str(r#"{ "type": 5 }"#).unwrap();
    let mut snapshot = SelectSnapshot {
        rows: vec![SelectRowSnapshot {
            index: 0,
            kind: SelectRowKind::Song,
            in_library: true,
            has_long_notes: true,
            ..SelectRowSnapshot::default()
        }],
        ..SelectSnapshot::default()
    };

    let (with_ln, _) = document.select_draw_state(&snapshot, None);
    assert_eq!(with_ln.chart_has_long_notes, Some(true));
    assert!(!test_skin_op(172, &[], &with_ln));
    assert!(test_skin_op(173, &[], &with_ln));

    snapshot.rows[0].has_long_notes = false;
    let (without_ln, _) = document.select_draw_state(&snapshot, None);
    assert_eq!(without_ln.chart_has_long_notes, Some(false));
    assert!(test_skin_op(172, &[], &without_ln));
    assert!(!test_skin_op(173, &[], &without_ln));

    snapshot.rows[0].kind = SelectRowKind::Folder;
    snapshot.rows[0].is_folder = true;
    let (folder, _) = document.select_draw_state(&snapshot, None);
    assert_eq!(folder.chart_has_long_notes, None);
    assert!(!test_skin_op(172, &[], &folder));
    assert!(!test_skin_op(173, &[], &folder));
}

#[test]
fn select_document_options_follow_selected_song_text_presence() {
    let no_document = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_has_document: false,
        ..SkinDrawState::default()
    };
    let with_document = SkinDrawState { select_has_document: true, ..no_document.clone() };
    let folder = SkinDrawState { select_row_kind: SelectRowKind::Folder, ..with_document.clone() };

    assert!(test_skin_op(174, &[], &no_document));
    assert!(!test_skin_op(175, &[], &no_document));
    assert!(!test_skin_op(174, &[], &with_document));
    assert!(test_skin_op(175, &[], &with_document));
    assert!(!test_skin_op(174, &[], &folder));
    assert!(!test_skin_op(175, &[], &folder));
}

#[test]
fn select_row_bar_slots_follow_beatoraja_bar_types() {
    let cases = [
        (SelectRowKind::Song, true, 0, 2),
        (SelectRowKind::Song, false, 4, 8),
        (SelectRowKind::Folder, true, 1, 4),
        (SelectRowKind::TableFolder, true, 2, 6),
        (SelectRowKind::CourseFolder, true, 2, 6),
        (SelectRowKind::NewCourse, true, 2, 6),
        (SelectRowKind::SearchFolder, true, 6, 10),
        (SelectRowKind::Course, true, 3, 7),
        (SelectRowKind::Course, false, 4, 8),
        (SelectRowKind::Executable, true, 2, 6),
        (SelectRowKind::RandomCourse, true, 2, 6),
        (SelectRowKind::RandomCourse, false, 4, 8),
        (SelectRowKind::Command, true, 5, 9),
        (SelectRowKind::NewSongFolder, true, 5, 9),
        (SelectRowKind::Container, true, 5, 9),
        (SelectRowKind::NoSong, false, 4, 8),
        (SelectRowKind::SettingsRoot, true, 8, 11),
        (SelectRowKind::SettingsFolder, true, 8, 11),
        (SelectRowKind::SettingsBack, true, 9, 12),
        (SelectRowKind::SettingsClose, true, 10, 13),
        (SelectRowKind::Config, true, 0, 2),
    ];

    for (kind, in_library, image_index, text_index) in cases {
        let row = SelectRowSnapshot {
            kind,
            in_library,
            is_folder: matches!(
                kind,
                SelectRowKind::Folder
                    | SelectRowKind::TableFolder
                    | SelectRowKind::CourseFolder
                    | SelectRowKind::NewSongFolder
                    | SelectRowKind::SearchFolder
                    | SelectRowKind::Command
                    | SelectRowKind::Container
                    | SelectRowKind::SettingsRoot
                    | SelectRowKind::SettingsFolder
                    | SelectRowKind::SettingsBack
                    | SelectRowKind::SettingsClose
            ),
            ..SelectRowSnapshot::default()
        };
        assert_eq!(select_row_bar_image_index(&row), image_index, "image index for {kind:?}");
        assert_eq!(select_row_bar_text_index(&row), text_index, "text index for {kind:?}");
    }
}

#[test]
fn virtual_query_folders_expose_their_aggregated_song_count() {
    for kind in [SelectRowKind::Command, SelectRowKind::NewSongFolder] {
        let row = SelectRowSnapshot {
            kind,
            is_folder: true,
            folder_lamp_counts: [1; 11],
            ..Default::default()
        };
        assert_eq!(select_row_folder_song_count(&row), Some(11));
        let state =
            SkinDrawState { select_screen: true, select_row_kind: kind, ..Default::default() };
        assert!(select_folder_lamp_counts_available(&state));
    }
}

#[test]
fn select_settings_rows_use_dedicated_slots_with_legacy_fallbacks() {
    let search = SelectRowSnapshot {
        kind: SelectRowKind::SearchFolder,
        is_folder: true,
        ..SelectRowSnapshot::default()
    };
    let settings_root = SelectRowSnapshot {
        kind: SelectRowKind::SettingsRoot,
        is_folder: true,
        ..SelectRowSnapshot::default()
    };
    let settings_folder = SelectRowSnapshot {
        kind: SelectRowKind::SettingsFolder,
        is_folder: true,
        ..SelectRowSnapshot::default()
    };
    let settings_back = SelectRowSnapshot {
        kind: SelectRowKind::SettingsBack,
        is_folder: true,
        ..SelectRowSnapshot::default()
    };
    let settings_close = SelectRowSnapshot {
        kind: SelectRowKind::SettingsClose,
        is_folder: true,
        ..SelectRowSnapshot::default()
    };

    assert_eq!(select_row_bar_image_index(&search), 6);
    assert_eq!(select_row_bar_text_index(&search), 10);
    assert_eq!(select_row_bar_image_fallback_indices(&search), &[1]);
    assert_eq!(select_row_bar_text_fallback_indices(&search), &[4]);
    let cases: [(&SelectRowSnapshot, usize, usize, &[usize], &[usize]); 4] = [
        (&settings_root, 8, 11, &[6, 1], &[10, 4]),
        (&settings_folder, 8, 11, &[1], &[4]),
        (&settings_back, 9, 12, &[6, 1], &[10, 4]),
        (&settings_close, 10, 13, &[6, 1], &[10, 4]),
    ];
    for (row, image, text, fallback_images, fallback_texts) in cases {
        assert_eq!(select_row_bar_image_index(row), image);
        assert_eq!(select_row_bar_text_index(row), text);
        assert_eq!(select_row_bar_image_fallback_indices(row), fallback_images);
        assert_eq!(select_row_bar_text_fallback_indices(row), fallback_texts);
    }
}

#[test]
fn select_settings_image_slots_follow_legacy_search_and_folder_fallbacks() {
    let dedicated_slots: Vec<_> = (0..11).collect();
    for primary in [8, 9, 10] {
        assert_eq!(
            select_row_slot_with_fallbacks(&dedicated_slots, primary, &[6, 1]).copied(),
            Some(primary)
        );
    }

    // beatoraja skin may define index 7 as a legacy no-song bar. BMZ settings
    // slots start after all eight existing entries, so ECFN-style arrays
    // continue to use the search bar at index 6.
    let beatoraja_slots: Vec<_> = (0..8).collect();
    for primary in [8, 9, 10] {
        assert_eq!(
            select_row_slot_with_fallbacks(&beatoraja_slots, primary, &[6, 1]).copied(),
            Some(6)
        );
    }

    let folder_slots: Vec<_> = (0..2).collect();
    assert_eq!(select_row_slot_with_fallbacks(&folder_slots, 8, &[6, 1]).copied(), Some(1));

    assert_eq!(select_row_slot_with_fallbacks(&[0], 8, &[6, 1]).copied(), Some(0));
    assert_eq!(select_row_slot_with_fallbacks::<usize>(&[], 8, &[6, 1]), None);
}

#[test]
fn select_bar_type_ops_match_song_folder_and_course_rows() {
    let song = SkinDrawState {
        select_row_kind: SelectRowKind::Song,
        select_is_folder: false,
        ..SkinDrawState::default()
    };
    let folder = SkinDrawState {
        select_row_kind: SelectRowKind::Folder,
        select_is_folder: true,
        ..SkinDrawState::default()
    };
    let table_folder = SkinDrawState {
        select_row_kind: SelectRowKind::TableFolder,
        select_is_folder: true,
        ..SkinDrawState::default()
    };
    let search_folder = SkinDrawState {
        select_row_kind: SelectRowKind::SearchFolder,
        select_is_folder: true,
        ..SkinDrawState::default()
    };
    let settings_folder = SkinDrawState {
        select_row_kind: SelectRowKind::SettingsFolder,
        select_is_folder: true,
        ..SkinDrawState::default()
    };
    let settings_root = SkinDrawState {
        select_row_kind: SelectRowKind::SettingsRoot,
        select_is_folder: true,
        ..SkinDrawState::default()
    };
    let settings_back = SkinDrawState {
        select_row_kind: SelectRowKind::SettingsBack,
        select_is_folder: true,
        ..SkinDrawState::default()
    };
    let settings_close = SkinDrawState {
        select_row_kind: SelectRowKind::SettingsClose,
        select_is_folder: true,
        ..SkinDrawState::default()
    };
    let command = SkinDrawState {
        select_row_kind: SelectRowKind::Command,
        select_is_folder: true,
        ..SkinDrawState::default()
    };
    let container = SkinDrawState {
        select_row_kind: SelectRowKind::Container,
        select_is_folder: true,
        ..SkinDrawState::default()
    };
    let executable = SkinDrawState {
        select_row_kind: SelectRowKind::Executable,
        select_is_folder: false,
        ..SkinDrawState::default()
    };
    let random_course = SkinDrawState {
        select_row_kind: SelectRowKind::RandomCourse,
        select_is_folder: false,
        ..SkinDrawState::default()
    };
    let course = SkinDrawState {
        select_row_kind: SelectRowKind::Course,
        select_is_folder: false,
        ..SkinDrawState::default()
    };
    let unowned_song = SkinDrawState {
        select_row_kind: SelectRowKind::Song,
        select_is_folder: false,
        select_in_library: false,
        ..SkinDrawState::default()
    };

    assert!(test_skin_op(2, &[], &song));
    assert!(test_skin_op(2, &[], &unowned_song));
    assert!(!test_skin_op(1, &[], &song));
    assert!(!test_skin_op(3, &[], &song));
    assert!(test_skin_op(1, &[], &folder));
    assert!(test_skin_op(1, &[], &table_folder));
    assert!(test_skin_op(1, &[], &search_folder));
    assert!(test_skin_op(1, &[], &settings_root));
    assert!(test_skin_op(1, &[], &settings_folder));
    assert!(test_skin_op(1, &[], &settings_back));
    assert!(test_skin_op(1, &[], &settings_close));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SETTINGS_FOLDER, &[], &settings_root));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SETTINGS_FOLDER, &[], &settings_folder));
    assert!(!test_skin_op(SKIN_OPTION_BMZ_SETTINGS_FOLDER, &[], &settings_back));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SETTINGS_BACK, &[], &settings_back));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SETTINGS_CLOSE, &[], &settings_close));
    assert!(test_skin_op(1, &[], &command));
    assert!(test_skin_op(1, &[], &container));
    assert!(!test_skin_op(2, &[], &folder));
    assert!(test_skin_op(3, &[], &course));
    assert!(!test_skin_op(2, &[], &course));
    assert!(test_skin_op(1030, &[], &executable));
    assert!(!test_skin_op(1030, &[], &random_course));
    assert!(test_skin_op(1031, &[], &random_course));
    assert!(!test_skin_op(1031, &[], &course));
}

#[test]
fn select_settings_row_ref_distinguishes_folder_back_and_close() {
    let cases = [
        (SelectRowKind::Song, 0),
        (SelectRowKind::SettingsRoot, 1),
        (SelectRowKind::SettingsFolder, 1),
        (SelectRowKind::SettingsBack, 2),
        (SelectRowKind::SettingsClose, 3),
    ];

    for (kind, expected) in cases {
        let state = SkinDrawState {
            select_screen: true,
            select_row_kind: kind,
            ..SkinDrawState::default()
        };
        assert_eq!(skin_state_event_index(SKIN_REF_BMZ_SELECT_SETTINGS_ROW_KIND, &state), expected);
        assert_eq!(
            skin_state_number(SKIN_REF_BMZ_SELECT_SETTINGS_ROW_KIND, &state),
            Some(i64::from(expected))
        );
    }
}

#[test]
fn table_song_op_matches_table_context() {
    let table_song = SkinDrawState { table_song: true, ..SkinDrawState::default() };
    let non_table_song = SkinDrawState::default();

    assert!(test_skin_op(1008, &[], &table_song));
    assert!(test_skin_op(-1008, &[], &non_table_song));
    assert!(!test_skin_op(1008, &[], &non_table_song));
}

#[test]
fn select_row_trophy_index_prefers_achieved_course_trophy_names() {
    let row = SelectRowSnapshot {
        kind: SelectRowKind::Course,
        achieved_trophy_names: vec!["bronzemedal".to_string(), "goldmedal".to_string()],
        ex_score: Some(0),
        total_notes: 100,
        ..SelectRowSnapshot::default()
    };
    assert_eq!(select_row_trophy_index(&row), Some(2));

    let silver = SelectRowSnapshot {
        kind: SelectRowKind::Course,
        achieved_trophy_names: vec!["silvermedal".to_string()],
        ..SelectRowSnapshot::default()
    };
    assert_eq!(select_row_trophy_index(&silver), Some(1));

    let high_score_without_trophy = SelectRowSnapshot {
        kind: SelectRowKind::Course,
        total_notes: 100,
        ex_score: Some(200),
        ..SelectRowSnapshot::default()
    };
    assert_eq!(select_row_trophy_index(&high_score_without_trophy), None);
}

#[test]
fn playable_bar_op_matches_library_presence() {
    let owned_song = SkinDrawState {
        select_row_kind: SelectRowKind::Song,
        select_is_folder: false,
        select_in_library: true,
        ..SkinDrawState::default()
    };
    let unowned_song = SkinDrawState {
        select_row_kind: SelectRowKind::Song,
        select_is_folder: false,
        select_in_library: false,
        ..SkinDrawState::default()
    };
    let owned_course = SkinDrawState {
        select_row_kind: SelectRowKind::Course,
        select_is_folder: false,
        select_in_library: true,
        ..SkinDrawState::default()
    };
    let unowned_course = SkinDrawState {
        select_row_kind: SelectRowKind::Course,
        select_is_folder: false,
        select_in_library: false,
        ..SkinDrawState::default()
    };
    let owned_random_course = SkinDrawState {
        select_row_kind: SelectRowKind::RandomCourse,
        select_is_folder: false,
        select_in_library: true,
        ..SkinDrawState::default()
    };
    let executable = SkinDrawState {
        select_row_kind: SelectRowKind::Executable,
        select_is_folder: false,
        select_in_library: false,
        ..SkinDrawState::default()
    };
    let folder = SkinDrawState {
        select_row_kind: SelectRowKind::Folder,
        select_is_folder: true,
        select_in_library: true,
        ..SkinDrawState::default()
    };

    assert!(test_skin_op(5, &[], &owned_song));
    assert!(!test_skin_op(5, &[], &unowned_song));
    assert!(test_skin_op(5, &[], &owned_course));
    assert!(!test_skin_op(5, &[], &unowned_course));
    assert!(test_skin_op(5, &[], &owned_random_course));
    assert!(test_skin_op(5, &[], &executable));
    assert!(!test_skin_op(5, &[], &folder));
    assert!(!test_skin_op(-5, &[], &owned_song));
    assert!(test_skin_op(-5, &[], &unowned_song));
    assert!(test_skin_op(-5, &[], &folder));
}

#[test]
fn select_banner_ops_follow_selected_banner_presence() {
    let no_banner =
        SkinDrawState { select_screen: true, select_has_banner: false, ..SkinDrawState::default() };
    let with_banner =
        SkinDrawState { select_screen: true, select_has_banner: true, ..SkinDrawState::default() };
    let play_screen =
        SkinDrawState { select_screen: false, select_has_banner: true, ..SkinDrawState::default() };

    assert!(test_skin_op(192, &[], &no_banner));
    assert!(!test_skin_op(193, &[], &no_banner));
    assert!(!test_skin_op(192, &[], &with_banner));
    assert!(test_skin_op(193, &[], &with_banner));
    assert!(!test_skin_op(192, &[], &play_screen));
    assert!(!test_skin_op(193, &[], &play_screen));

    assert!(test_skin_ops(&[2, 192], &[], &no_banner));
    assert!(!test_skin_ops(&[2, 193], &[], &no_banner));
    assert!(!test_skin_ops(&[2, 192], &[], &with_banner));
    assert!(test_skin_ops(&[2, 193], &[], &with_banner));
}

#[test]
fn result_panel_draw_condition_uses_runtime_selection() {
    let ir = SkinDrawState { result_panel: Some(1), ..Default::default() };
    let graph = SkinDrawState { result_panel: Some(2), ..Default::default() };
    assert!(eval_skin_draw_condition("result_panel(1)", &ir));
    assert!(!eval_skin_draw_condition("result_panel(2)", &ir));
    assert!(eval_skin_draw_condition("result_panel(0) or result_panel(2)", &graph,));
}

#[test]
fn select_score_available_requires_an_actual_score_record() {
    let folder = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Folder,
        select_is_folder: true,
        select_in_library: true,
        select_ex_score: Some(1234),
        ..SkinDrawState::default()
    };
    let zero_score = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_ex_score: Some(0),
        ..SkinDrawState::default()
    };
    let out_of_library = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: false,
        select_ex_score: Some(1234),
        ..SkinDrawState::default()
    };

    assert!(!eval_skin_draw_condition("select_score_available()", &folder));
    assert!(eval_skin_draw_condition("select_score_available()", &zero_score));
    assert!(!eval_skin_draw_condition("select_score_available()", &out_of_library));
}

#[test]
fn select_rank_ops_reflect_selected_ex_score() {
    let aa_state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_ex_score: Some(1556),
        select_total_notes: 1000,
        ..SkinDrawState::default()
    };
    let max_state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_ex_score: Some(2000),
        select_total_notes: 1000,
        ..SkinDrawState::default()
    };
    let f_state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_ex_score: Some(300),
        select_total_notes: 1000,
        ..SkinDrawState::default()
    };

    assert!(test_skin_op(201, &[], &aa_state));
    assert!(test_skin_op(302, &[], &aa_state));
    assert!(!test_skin_op(200, &[], &aa_state));
    assert!(test_skin_op(-200, &[], &aa_state));
    assert!(test_skin_op(200, &[], &max_state));
    assert!(test_skin_op(300, &[], &max_state));
    assert!(test_skin_op(207, &[], &f_state));
    assert!(!test_skin_op(307, &[], &f_state));
    assert!(!test_skin_op(200, &[], &SkinDrawState::default()));
}

#[test]
fn select_rank_ops_are_false_for_folder_rows() {
    let state = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Folder,
        select_is_folder: true,
        select_in_library: true,
        select_ex_score: Some(1556),
        select_total_notes: 1000,
        ..SkinDrawState::default()
    };

    assert!(!test_skin_op(201, &[], &state));
    assert!(!test_skin_op(302, &[], &state));
}

#[test]
fn select_key_mode_op_160_requires_song_row_key_mode() {
    let config_row = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Config,
        in_settings: true,
        ..SkinDrawState::default()
    };
    assert!(!test_skin_op(160, &[], &config_row));

    let song_7k = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        select_chart_key_mode: Some(KeyMode::K7),
        ..SkinDrawState::default()
    };
    assert!(test_skin_op(160, &[], &song_7k));
    assert!(!test_skin_op(161, &[], &song_7k));
    assert!(test_skin_op(SKIN_OPTION_BMZ_KEY_MODE_BASE + 3, &[], &song_7k));
    assert!(test_skin_op(SKIN_OPTION_BMZ_SINGLE_PLAY, &[], &song_7k));
    assert!(!test_skin_op(SKIN_OPTION_BMZ_NO_SCRATCH, &[], &song_7k));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_KEY_MODE, &song_7k), Some(7));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_ACTIVE_LANE_COUNT, &song_7k), Some(8));

    let table_folder = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::TableFolder,
        select_is_folder: true,
        select_chart_key_mode: Some(KeyMode::K7),
        skin_attempt: SkinAttemptState {
            effective_key_mode: Some(KeyMode::K7),
            ..Default::default()
        },
        ..SkinDrawState::default()
    };
    assert!(test_skin_op(1, &[], &table_folder));
    assert!(!test_skin_op(160, &[], &table_folder));
    assert_eq!(skin_state_number(SKIN_REF_BMZ_KEY_MODE, &table_folder), None);
    assert_eq!(skin_state_number(SKIN_REF_BMZ_ACTIVE_LANE_COUNT, &table_folder), None);
    assert!(!test_skin_op(SKIN_OPTION_BMZ_KEY_MODE_BASE + 3, &[], &table_folder));

    let course = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Course,
        skin_attempt: SkinAttemptState {
            effective_key_mode: Some(KeyMode::K7),
            ..Default::default()
        },
        ..SkinDrawState::default()
    };
    assert!(test_skin_op(3, &[], &course));
    assert!(!test_skin_op(160, &[], &course));
}

#[test]
fn select_settings_screen_hides_bpm_numbers() {
    let state = SkinDrawState {
        select_screen: true,
        in_settings: true,
        select_max_bpm: 180.0,
        select_min_bpm: 120.0,
        ..SkinDrawState::default()
    };
    assert_eq!(skin_state_number(90, &state), None);
    assert_eq!(skin_state_number(91, &state), None);
}

#[test]
fn select_song_boolean_options_use_selected_chart_metadata() {
    let fixed_bga_chart = SkinDrawState {
        select_screen: true,
        select_row_kind: SelectRowKind::Song,
        select_in_library: true,
        chart_has_long_notes: Some(false),
        min_bpm: 150.0,
        max_bpm: 150.0,
        skin_attempt: SkinAttemptState {
            has_bga: Some(true),
            has_random_sequence: Some(false),
            ..Default::default()
        },
        ..SkinDrawState::default()
    };
    assert!(!test_skin_op(170, &[], &fixed_bga_chart));
    assert!(test_skin_op(171, &[], &fixed_bga_chart));
    assert!(test_skin_op(176, &[], &fixed_bga_chart));
    assert!(!test_skin_op(177, &[], &fixed_bga_chart));
    assert!(test_skin_op(178, &[], &fixed_bga_chart));
    assert!(!test_skin_op(179, &[], &fixed_bga_chart));

    let variable_random_chart = SkinDrawState {
        min_bpm: 120.0,
        max_bpm: 180.0,
        skin_attempt: SkinAttemptState {
            has_bga: Some(false),
            has_random_sequence: Some(true),
            ..Default::default()
        },
        ..fixed_bga_chart
    };
    assert!(test_skin_op(170, &[], &variable_random_chart));
    assert!(!test_skin_op(171, &[], &variable_random_chart));
    assert!(!test_skin_op(176, &[], &variable_random_chart));
    assert!(test_skin_op(177, &[], &variable_random_chart));
    assert!(!test_skin_op(178, &[], &variable_random_chart));
    assert!(test_skin_op(179, &[], &variable_random_chart));

    let no_chart = SkinDrawState { select_screen: true, ..Default::default() };
    for op in 170..=179 {
        if matches!(op, 172..=175) {
            continue;
        }
        assert!(!test_skin_op(op, &[], &no_chart), "option {op}");
    }
}
