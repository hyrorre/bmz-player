use super::*;
use crate::config::profile_config::BindingConfigEntry;

#[test]
fn operating_time_is_applied_to_select_snapshot() {
    let mut scene = AppSceneSnapshot::Select(SelectSnapshot::default());

    apply_operating_time_ms_to_scene(&mut scene, 90_061_234);

    let AppSceneSnapshot::Select(snapshot) = scene else {
        panic!("expected select snapshot");
    };
    assert_eq!(snapshot.operating_time_ms, 90_061_234);
}

#[test]
fn chart_snapshot_metadata_preserves_selected_chart_best_score() {
    let mut row = select_chart_row(7);
    row.best_score = Some(best_score_with_replay(456, "best.json"));
    let items = vec![SelectItem::Chart(row)];

    let (chart, best_ex_score) = chart_snapshot_metadata_for_chart(&items, 7, |_| {
        panic!("selected chart metadata should take priority")
    })
    .expect("selected chart metadata");

    assert_eq!(chart.title, "Title 7");
    assert_eq!(best_ex_score, Some(456));
}

#[test]
fn table_breadcrumb_uses_table_name_without_symbol_prefix() {
    let breadcrumb = table_breadcrumb_from_record(&DifficultyTableRecord {
        id: 1,
        source_url: "https://example.com/insane/".to_string(),
        name: "通常難易度表".to_string(),
        symbol: "★".to_string(),
        level_order: vec!["1".to_string()],
        fetched_at: 0,
    });

    assert_eq!(breadcrumb.name, "通常難易度表");
    assert_eq!(breadcrumb.symbol, "★");
}

#[test]
fn initial_folder_stack_starts_at_select_root_even_with_single_enabled_root() {
    let mut config = AppConfig::default();
    config.songs.roots =
        vec![PathEntry { path: "/music/bms".to_string(), enabled: true, recursive: true }];
    assert!(initial_folder_stack(&config).is_empty());
}

#[test]
fn skin_catalog_loads_mz_select_lua_header_when_available() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let skin_root = repo_root.join("data/skins");
    let path = skin_root.join("mz-select/music_select.luaskin");
    if !path.is_file() {
        return;
    }

    let (skin_type, candidate) =
        load_skin_candidate(&skin_root, &path, SkinCandidateOrigin::Bundled)
            .expect("load mz-select catalog candidate");

    assert_eq!(skin_type, 5);
    assert_eq!(candidate.path, "resource:skins/mz-select/music_select.luaskin");
    assert_eq!(candidate.origin, SkinCandidateOrigin::Bundled);
    assert!(candidate.name.contains("m-select"), "candidate name: {}", candidate.name);
}

#[test]
fn skin_catalog_loads_luxez_flat_select_lua_header_when_available() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let skin_root = repo_root.join("data/skins");
    let path = skin_root.join("Luxez-Flat/music_select.luaskin");
    if !path.is_file() {
        return;
    }

    let (skin_type, candidate) =
        load_skin_candidate(&skin_root, &path, SkinCandidateOrigin::Bundled)
            .expect("load Luxez-Flat catalog candidate");

    assert_eq!(skin_type, 5);
    assert_eq!(candidate.path, "resource:skins/Luxez-Flat/music_select.luaskin");
    assert_eq!(candidate.origin, SkinCandidateOrigin::Bundled);
    assert!(!candidate.name.trim().is_empty(), "candidate name should not be empty");
}

#[test]
fn select_action_maps_start_and_vertical_movement() {
    let keys = default_select_keys();
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::Enter), ElementState::Pressed, false, &keys),
        Some(SelectAction::EnterOrPlay)
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::ArrowUp), ElementState::Pressed, false, &keys),
        Some(SelectAction::Move(SelectMove::Previous))
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::ArrowDown), ElementState::Pressed, false, &keys),
        Some(SelectAction::Move(SelectMove::Next))
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::ShiftLeft), ElementState::Pressed, false, &keys),
        Some(SelectAction::Move(SelectMove::Previous))
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::ControlLeft), ElementState::Pressed, false, &keys),
        Some(SelectAction::Move(SelectMove::Next))
    );
    assert_eq!(
        select_action(
            PhysicalKey::Code(KeyCode::ControlRight),
            ElementState::Pressed,
            false,
            &keys
        ),
        Some(SelectAction::Move(SelectMove::Next))
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::ShiftRight), ElementState::Pressed, false, &keys),
        Some(SelectAction::Move(SelectMove::Previous))
    );
}

#[test]
fn select_option_gamepad_lane_distinguishes_same_buttons_by_device() {
    let profile = ProfileConfig::new_default("default", "Default", 0);
    let control = "Button1";

    assert_eq!(
        select_option_lane_for_gamepad(
            &profile.input,
            crate::input::gamepad::GamepadSlotMap::from_slot_ids([Some(0), Some(1)]),
            DeviceId(16),
            control,
        ),
        Some(Lane::Key1)
    );
    assert_eq!(
        select_option_lane_for_gamepad(
            &profile.input,
            crate::input::gamepad::GamepadSlotMap::from_slot_ids([Some(0), Some(1)]),
            DeviceId(17),
            control,
        ),
        Some(Lane::Key8)
    );
    assert_eq!(
        select_option_lane_for_gamepad(
            &profile.input,
            crate::input::gamepad::GamepadSlotMap::from_slot_ids([Some(1), Some(0)]),
            DeviceId(16),
            control,
        ),
        Some(Lane::Key8)
    );
}

#[test]
fn select_row_click_enters_only_when_row_is_already_selected() {
    assert_eq!(
        select_row_click_action(2, MouseButton::Left, 0, 4, false),
        Some(SelectRowClickAction::Select(2))
    );
    assert_eq!(
        select_row_click_action(2, MouseButton::Left, 2, 4, false),
        Some(SelectRowClickAction::EnterOrPlay)
    );
    assert_eq!(select_row_click_action(4, MouseButton::Left, 2, 4, false), None);
    assert_eq!(
        select_row_click_action(2, MouseButton::Right, 2, 4, false),
        Some(SelectRowClickAction::ExitFolder)
    );
    assert_eq!(
        select_row_click_action(2, MouseButton::Right, 2, 4, true),
        Some(SelectRowClickAction::CancelSettingsEdit)
    );
    assert_eq!(select_row_click_action(2, MouseButton::Middle, 2, 4, false), None);
}

#[test]
fn select_key_bindings_identify_e_action_controls() {
    let keys = default_select_keys();

    assert_eq!(keys.e_action_for_control("Q"), Some(InputActionConfig::E1));
    assert_eq!(keys.e_action_for_control("W"), Some(InputActionConfig::E2));
    assert_eq!(keys.e_action_for_control("E"), Some(InputActionConfig::E3));
    assert_eq!(keys.e_action_for_control("R"), Some(InputActionConfig::E4));
    assert_eq!(keys.e_action_for_control("Slash"), None);
}

#[test]
fn select_scroll_slider_value_maps_to_nearest_row() {
    assert_eq!(select_scroll_slider_index(0.0, 0), None);
    assert_eq!(select_scroll_slider_index(0.5, 1), Some(0));
    assert_eq!(select_scroll_slider_index(-1.0, 10), Some(0));
    assert_eq!(select_scroll_slider_index(0.0, 10), Some(0));
    assert_eq!(select_scroll_slider_index(0.49, 10), Some(4));
    assert_eq!(select_scroll_slider_index(0.50, 10), Some(5));
    assert_eq!(select_scroll_slider_index(1.0, 10), Some(9));
    assert_eq!(select_scroll_slider_index(2.0, 10), Some(9));
}

#[test]
fn skin_video_source_fast_path_updates_selected_options() {
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "property": [
                    {
                        "name": "動画を使用する",
                        "def": "ON",
                        "item": [
                            { "name": "ON", "op": 920 },
                            { "name": "OFF", "op": 921 }
                        ]
                    }
                ],
                "source": [{ "id": "mv", "path": "mv/default.mp4" }],
                "image": [{ "id": "mv", "src": "mv", "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [{ "id": "mv", "op": [920], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] }]
            }
            "#,
        )
        .unwrap();
    let gating = skin_video_source_gating(&document, "mv");
    let mut sources = vec![ActiveSkinVideoSource {
        texture: SkinTextureId(0),
        path: PathBuf::new(),
        decoder: None,
        last_pts: None,
        loop_start_us: 0,
        active: gating.active,
        gating_op_sets: gating.op_sets,
        enabled_options: document.enabled_options(),
        result_ranktime_ms: document.ranktime,
        failed: false,
    }];

    apply_skin_video_source_enabled_options(
        &mut sources,
        &[921],
        &skin_document_property_ops(&document),
    );

    assert_eq!(sources[0].enabled_options, vec![921]);
    assert!(!sources[0].active);
}

#[test]
fn select_action_maps_page_and_edge_movement() {
    let keys = default_select_keys();
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::PageUp), ElementState::Pressed, false, &keys),
        Some(SelectAction::Move(SelectMove::PagePrevious))
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::PageDown), ElementState::Pressed, false, &keys),
        Some(SelectAction::Move(SelectMove::PageNext))
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::Home), ElementState::Pressed, false, &keys),
        Some(SelectAction::Move(SelectMove::First))
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::End), ElementState::Pressed, false, &keys),
        Some(SelectAction::Move(SelectMove::Last))
    );
}

#[test]
fn select_action_maps_configured_lane_keys() {
    let keys = default_select_keys();
    // Key1(Z), Key3(X), Key5(C), Key7(V) → EnterOrPlay
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyZ), ElementState::Pressed, false, &keys),
        Some(SelectAction::EnterOrPlay)
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyV), ElementState::Pressed, false, &keys),
        Some(SelectAction::EnterOrPlay)
    );
    // Key2(S), Key4(D), Key6(F) → ExitFolder
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyS), ElementState::Pressed, false, &keys),
        Some(SelectAction::ExitFolder)
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyD), ElementState::Pressed, false, &keys),
        Some(SelectAction::ExitFolder)
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyF), ElementState::Pressed, false, &keys),
        Some(SelectAction::ExitFolder)
    );
    // E2(W) is also mapped to ExitFolder for direct lookup paths.
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyW), ElementState::Pressed, false, &keys),
        Some(SelectAction::ExitFolder)
    );
}

#[test]
fn select_action_maps_collection_keys() {
    let keys = default_select_keys();
    for (key, expected) in [
        (KeyCode::Digit1, SelectAction::ModeFilter),
        (KeyCode::Digit2, SelectAction::Sort),
        (KeyCode::Digit3, SelectAction::LnMode),
        (KeyCode::Digit4, SelectAction::ReplayCycle),
        (KeyCode::Numpad4, SelectAction::ReplayCycle),
        (KeyCode::Digit8, SelectAction::SameFolder),
        (KeyCode::Numpad8, SelectAction::SameFolder),
        (KeyCode::Digit9, SelectAction::OpenDocuments),
        (KeyCode::Numpad9, SelectAction::OpenDocuments),
    ] {
        assert_eq!(
            select_action(PhysicalKey::Code(key), ElementState::Pressed, false, &keys),
            Some(expected),
        );
    }
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::F8), ElementState::Pressed, false, &keys),
        Some(SelectAction::FavoriteSong)
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::F9), ElementState::Pressed, false, &keys),
        Some(SelectAction::FavoriteChart)
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::Numpad5), ElementState::Pressed, false, &keys),
        Some(SelectAction::ReplayPlay)
    );
}

#[test]
fn select_action_maps_configurable_shortcuts() {
    let keys = default_select_keys();
    for (key, expected) in [
        (KeyCode::F3, SelectAction::OpenFolder),
        (KeyCode::F5, SelectAction::Reload),
        (KeyCode::F10, SelectAction::AutoplayFolder),
        (KeyCode::F11, SelectAction::OpenPrimaryIr),
        (KeyCode::Digit6, SelectAction::OpenKeyConfig),
        (KeyCode::Digit7, SelectAction::CycleRival),
        (KeyCode::Numpad7, SelectAction::CycleRival),
        (KeyCode::Numpad9, SelectAction::OpenDocuments),
    ] {
        assert_eq!(
            select_action(PhysicalKey::Code(key), ElementState::Pressed, false, &keys),
            Some(expected),
        );
    }
    assert!(keys.is_screenshot("F12"));
}

#[test]
fn configurable_key_config_shortcut_replaces_digit_six() {
    let mut input = crate::config::play_input::default_profile_input();
    apply_play_binding(
        &mut input,
        KeyMode::K7,
        KeyBindingTarget::Action {
            action: InputActionConfig::SelectOpenKeyConfig,
            slot: KeyBindingSlot::KeyboardPrimary,
        },
        "A",
    )
    .unwrap();
    let keys = SelectKeyBindings::from_profile(&input);

    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyA), ElementState::Pressed, false, &keys),
        Some(SelectAction::OpenKeyConfig),
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::Digit6), ElementState::Pressed, false, &keys),
        None,
    );
}

#[test]
fn configurable_select_shortcut_replaces_its_default_key() {
    let mut input = crate::config::play_input::default_profile_input();
    apply_play_binding(
        &mut input,
        KeyMode::K7,
        KeyBindingTarget::Action {
            action: InputActionConfig::SelectOpenFolder,
            slot: KeyBindingSlot::KeyboardPrimary,
        },
        "A",
    )
    .unwrap();
    let keys = SelectKeyBindings::from_profile(&input);

    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyA), ElementState::Pressed, false, &keys),
        Some(SelectAction::OpenFolder),
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::F3), ElementState::Pressed, false, &keys),
        None,
    );
}

#[test]
fn configurable_mode_filter_shortcut_replaces_digit_one() {
    let mut input = crate::config::play_input::default_profile_input();
    apply_play_binding(
        &mut input,
        KeyMode::K7,
        KeyBindingTarget::Action {
            action: InputActionConfig::SelectModeFilter,
            slot: KeyBindingSlot::KeyboardPrimary,
        },
        "A",
    )
    .unwrap();
    let keys = SelectKeyBindings::from_profile(&input);

    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyA), ElementState::Pressed, false, &keys),
        Some(SelectAction::ModeFilter),
    );
    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::Digit1), ElementState::Pressed, false, &keys),
        None,
    );
}

#[test]
fn primary_ir_page_url_uses_provider_specific_course_hash() {
    let identity = PrimaryIrPageIdentity::Course {
        canonical_hash: Some("canonical".to_string()),
        rian_hash_v1: Some("rian".to_string()),
        bms_ir_course_key: Some("ab".repeat(32)),
    };
    let mut generic = crate::config::profile_config::IrProviderConfig::custom();
    generic.base_url = "https://ir.example.test/".to_string();
    assert_eq!(
        primary_ir_page_url(&generic, &identity).unwrap(),
        "https://ir.example.test/courses/canonical"
    );

    let mut rian = crate::config::profile_config::IrProviderConfig::rian_ir();
    rian.base_url = "https://rian.example.test".to_string();
    let url = primary_ir_page_url(&rian, &identity).unwrap();
    assert!(url.contains("rian"));
    assert!(!url.contains("canonical"));

    let mut bms_ir = crate::config::profile_config::IrProviderConfig::bms_ir();
    bms_ir.base_url = crate::ir::bms_ir::BMS_IR_DEFAULT_BASE_URL.to_string();
    let url = primary_ir_page_url(&bms_ir, &identity).unwrap();
    assert!(url.contains(&"ab".repeat(32)));
    assert!(!url.contains("canonical"));
}

#[test]
fn deprecated_select_enter_and_option_bga_bindings_are_ignored() {
    let mut input = crate::config::play_input::default_profile_input();
    for action in [InputActionConfig::SelectEnter, InputActionConfig::SelectOptionBga] {
        input.ui.bindings.push(BindingConfigEntry {
            device: "keyboard".to_string(),
            control: "A".to_string(),
            keyboard_slot: None,
            lane: None,
            action: Some(action),
            scratch: None,
        });
    }
    let keys = SelectKeyBindings::from_profile(&input);

    assert_eq!(
        select_action(PhysicalKey::Code(KeyCode::KeyA), ElementState::Pressed, false, &keys),
        None,
    );
    assert_ne!(keys.cycle_bga(), Some("A"));
}

#[test]
fn select_control_action_uses_key2_binding_for_controller_back() {
    let input = crate::config::play_input::default_profile_input();
    let keys = SelectKeyBindings::from_profile(&input);

    assert!(keys.is_back("Button2"));
    assert_eq!(select_control_action("Button2", &keys), Some(SelectAction::ExitFolder));
    assert_eq!(select_control_action("Button1", &keys), Some(SelectAction::EnterOrPlay));
}
