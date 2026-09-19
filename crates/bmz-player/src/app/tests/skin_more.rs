use super::*;

#[test]
fn play_lua_skin_load_preserves_resolved_target_names_for_rival_and_target_refs() {
    use crate::select_options::{ResolvedTarget, TargetOption};

    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir()
        .join(format!("bmz-player-play-lua-target-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("play.luaskin");
    std::fs::write(
        &path,
        r#"
            local main_state = require("main_state")
            return {
                type = 0,
                text = {
                    { id = "rival", constantText = main_state.text(1) },
                    { id = "target", constantText = main_state.text(3) }
                }
            }
        "#,
    )
    .unwrap();

    let cases = [
        (TargetOption::RivalIndex(1), Some("Rival One"), "Rival One"),
        (TargetOption::RivalIndex(1), Some("ライバル_1"), "ライバル_1"),
        (TargetOption::RivalIndex(1), Some("AAA"), "AAA"),
        (TargetOption::RivalIndex(1), Some("NONE"), "NONE"),
        (TargetOption::RivalIndex(1), Some("IR_TOP"), "IR_TOP"),
        (TargetOption::RankAaa, Some(""), ""),
        (TargetOption::RankAaa, None, "RANK AAA"),
        (TargetOption::IrTop, None, ""),
        (TargetOption::None, None, ""),
    ];
    for runtime_mode in [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat] {
        for (target, name, expected) in cases {
            let options = PlayStartOptions {
                target,
                resolved_target: name
                    .map(|name| ResolvedTarget { name: name.to_string(), ex_score: 100 }),
                ..PlayStartOptions::default()
            };
            let mut runtime_state = lua_runtime_state_for_play(
                &options,
                false,
                KeyMode::K7,
                None,
                "Player",
                Default::default(),
            );
            runtime_state.runtime_mode = runtime_mode;
            let loaded = bmz_skin::load_lua_skin_with_runtime_state(
                &path,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &runtime_state,
            )
            .unwrap();
            for (index, ref_id) in [(0, 1), (1, 3)] {
                let setting = bmz_render::skin::target_setting_name(&target.as_string());
                let expected = if ref_id == 3 { setting.as_str() } else { expected };
                assert_eq!(
                    loaded.document.text[index].constant_text, expected,
                    "mode={runtime_mode:?}, target={target:?}, name={name:?}, ref={ref_id}"
                );
                assert_eq!(
                    loaded.dependencies.text_values.get(&ref_id).map(String::as_str),
                    Some(expected),
                    "load dependency: mode={runtime_mode:?}, ref={ref_id}"
                );
            }
        }
    }

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn play_lua_state_keeps_selected_rival_separate_from_target_setting() {
    use crate::select_options::{ResolvedTarget, TargetOption};

    for resolved_target in
        [None, Some(ResolvedTarget { name: "IR対象者".to_string(), ex_score: 100 })]
    {
        let state = lua_runtime_state_for_play(
            &PlayStartOptions {
                target: TargetOption::IrTop,
                rival_name: Some("選択_ライバル".to_string()),
                resolved_target,
                ..Default::default()
            },
            false,
            KeyMode::K7,
            None,
            "Player",
            Default::default(),
        );
        assert_eq!(state.text_values.get(&1).map(String::as_str), Some("選択_ライバル"));
        assert_eq!(state.text_values.get(&3).map(String::as_str), Some("IR TOP"));
    }
}

#[test]
fn play_lua_runtime_state_exposes_play_mode_and_score_save_options() {
    let normal = lua_runtime_state_for_play(
        &PlayStartOptions::default(),
        false,
        KeyMode::K7,
        None,
        "Player",
        Default::default(),
    );
    assert_eq!(normal.text_values.get(&2).map(String::as_str), Some("Player"));
    assert_eq!(normal.option_values.get(&61), Some(&true));
    assert_eq!(normal.option_values.get(&82), Some(&true));
    assert_eq!(normal.option_values.get(&84), Some(&false));
    assert_eq!(normal.number_values.get(&SKIN_REF_BMZ_KEY_MODE), Some(&7));
    assert_eq!(normal.number_values.get(&SKIN_REF_BMZ_ACTIVE_LANE_COUNT), Some(&8));
    assert_eq!(normal.option_values.get(&(SKIN_OPTION_BMZ_KEY_MODE_BASE + 3)), Some(&true));
    assert_eq!(normal.option_values.get(&SKIN_OPTION_BMZ_SINGLE_PLAY), Some(&true));
    assert_eq!(normal.number_values.get(&150), Some(&0));
    assert_eq!(normal.number_values.get(&170), Some(&0));
    assert_eq!(
        normal.option_values.get(&bmz_render::skin::SKIN_OPTION_BMZ_FIRST_PLAY),
        Some(&true)
    );

    let source_ln_bits = bmz_render::snapshot::SKIN_SOURCE_LN_UNDEFINED_BIT
        | bmz_render::snapshot::SKIN_SOURCE_LN_DEFINED_HCN_BIT;
    let attempt = bmz_render::snapshot::SkinAttemptState {
        source_key_mode: Some(KeyMode::K7),
        effective_key_mode: Some(KeyMode::K6),
        seven_to_six: true,
        seven_to_nine_pattern: 5,
        seven_to_nine_type: 2,
        source_ln_profile_bits: Some(source_ln_bits),
        session_mode_index: Some(3),
        ln_mode_index: Some(2),
        has_bga: Some(false),
        has_random_sequence: Some(true),
        ..Default::default()
    };
    let extended = lua_runtime_state_for_play(
        &PlayStartOptions::default(),
        false,
        KeyMode::K6,
        None,
        "Player",
        attempt,
    );
    assert_eq!(
        extended.number_values.get(&bmz_render::skin::SKIN_REF_BMZ_SOURCE_KEY_MODE),
        Some(&7)
    );
    assert_eq!(
        extended.number_values.get(&bmz_render::skin::SKIN_REF_BMZ_SOURCE_LN_PROFILE),
        Some(&9)
    );
    assert_eq!(
        extended.event_index_values.get(&bmz_render::skin::SKIN_REF_BMZ_SELECT_SESSION_MODE),
        Some(&3)
    );
    assert_eq!(
        extended.option_values.get(&bmz_render::skin::SKIN_OPTION_BMZ_SEVEN_TO_SIX),
        Some(&true)
    );
    assert_eq!(extended.event_index_values.get(&360), Some(&5));
    assert_eq!(extended.event_index_values.get(&361), Some(&2));
    assert_eq!(extended.option_values.get(&170), Some(&true));
    assert_eq!(extended.option_values.get(&179), Some(&true));

    let played_zero = lua_runtime_state_for_play(
        &PlayStartOptions::default(),
        false,
        KeyMode::K7,
        Some(0),
        "Player",
        Default::default(),
    );
    assert_eq!(played_zero.number_values.get(&150), Some(&0));
    assert_eq!(
        played_zero.option_values.get(&bmz_render::skin::SKIN_OPTION_BMZ_FIRST_PLAY),
        Some(&false)
    );

    let played = lua_runtime_state_for_play(
        &PlayStartOptions::default(),
        false,
        KeyMode::K7,
        Some(1234),
        "Player",
        Default::default(),
    );
    assert_eq!(played.number_values.get(&150), Some(&1234));
    assert_eq!(played.number_values.get(&170), Some(&1234));
    assert_eq!(
        played.option_values.get(&bmz_render::skin::SKIN_OPTION_BMZ_FIRST_PLAY),
        Some(&false)
    );

    let autoplay = lua_runtime_state_for_play(
        &PlayStartOptions { autoplay: true, ..PlayStartOptions::default() },
        false,
        KeyMode::K7,
        None,
        "Player",
        Default::default(),
    );
    assert_eq!(autoplay.option_values.get(&33), Some(&true));
    assert_eq!(autoplay.option_values.get(&60), Some(&true));
    assert_eq!(autoplay.option_values.get(&82), Some(&false));

    let replay = lua_runtime_state_for_play(
        &PlayStartOptions {
            replay_player: Some(bmz_gameplay::replay::ReplayPlayer::default()),
            ..PlayStartOptions::default()
        },
        false,
        KeyMode::K7,
        None,
        "Player",
        Default::default(),
    );
    assert_eq!(replay.option_values.get(&33), Some(&false));
    assert_eq!(replay.option_values.get(&84), Some(&true));

    let practice = lua_runtime_state_for_play(
        &PlayStartOptions { session_mode: SessionMode::Practice, ..PlayStartOptions::default() },
        true,
        KeyMode::K7,
        Some(1234),
        "Player",
        Default::default(),
    );
    assert_eq!(practice.option_values.get(&60), Some(&true));
    assert_eq!(practice.option_values.get(&32), Some(&true));
    assert_eq!(practice.option_values.get(&33), Some(&false));
    assert_eq!(practice.option_values.get(&82), Some(&true));
    assert_eq!(practice.option_values.get(&1080), Some(&true));
    assert_eq!(practice.number_values.get(&150), Some(&0));
    assert_eq!(
        practice.option_values.get(&bmz_render::skin::SKIN_OPTION_BMZ_FIRST_PLAY),
        Some(&false)
    );
}

#[test]
fn settings_headers_load_without_renderer_and_keep_play_offsets_scoped() {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root =
        std::env::temp_dir().join(format!("bmz-settings-header-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("select.json");
    std::fs::write(&path, r#"{"type":5,"property":[{"name":"Layout","def":"On","item":[{"name":"On","op":1},{"name":"Off","op":2}]}]}"#).unwrap();
    let paths =
        crate::paths::AppPaths::from_dirs(root.clone(), root.clone(), root.clone(), root.clone());
    // No renderer installation is required. Play and common slots can share a
    // path without sharing the play-only offset definitions.
    let common = skin_defs_from_path(&paths, &path.to_string_lossy(), false);
    let play = skin_defs_from_path(&paths, &path.to_string_lossy(), true);
    assert_eq!(common.property[0].name, "Layout");
    assert_eq!(common.property[0].item.len(), 2);
    assert!(common.offset.is_empty());
    assert!(play.offset.iter().any(|offset| offset.id == 10));
    assert_eq!(
        skin_defs_from_path(&paths, &path.to_string_lossy(), false).property[0].name,
        "Layout"
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&root).unwrap();
}

#[test]
fn play_skin_defs_load_from_configured_path_without_renderer_install() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = repo.join("data/skins/ECFN/play/play7.luaskin");
    if !path.is_file() {
        return;
    }

    let app_paths = crate::paths::AppPaths::from_dirs(
        repo.join("data"),
        repo.join("data"),
        repo.join("data/cache"),
        repo.join("data/logs"),
    );
    let defs = skin_defs_from_path(&app_paths, &path.to_string_lossy(), true);

    assert!(!defs.property.is_empty());
    assert!(!defs.filepath.is_empty());
    assert!(defs.offset.iter().any(|offset| offset.id == 10));
}

#[test]
fn skin_video_source_respects_static_property_ops() {
    let mut document: SkinDocument = serde_json::from_str(
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

    assert!(skin_video_source_gating(&document, "mv").active);

    document.user_selected_options = Some(vec![921]);
    assert!(!skin_video_source_gating(&document, "mv").active);
    assert!(skin_video_source_gating(&document, "unknown-source").active);
}

#[test]
fn json_skin_option_reload_detection_allows_op_only_skins() {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir()
        .join(format!("bmz-player-json-skin-reload-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let op_only = root.join("op-only.json");
    std::fs::write(
        &op_only,
        r#"
            {
                "type": 5,
                "property": [
                    {
                        "name": "Option",
                        "def": "ON",
                        "item": [
                            { "name": "ON", "op": 920 },
                            { "name": "OFF", "op": 921 }
                        ]
                    }
                ],
                "destination": [
                    { "id": "panel", "op": [920], "dst": [{ "x": 0, "y": 0, "w": 1, "h": 1 }] }
                ]
            }
            "#,
    )
    .unwrap();
    let load_time = root.join("load-time.json");
    std::fs::write(
        &load_time,
        r#"
            {
                "type": 5,
                "destination": [
                    { "if": 920, "values": [
                        { "id": "panel", "dst": [{ "x": 0, "y": 0, "w": 1, "h": 1 }] }
                    ] }
                ]
            }
            "#,
    )
    .unwrap();
    let include = root.join("include.json");
    std::fs::write(
            &include,
            r#"
            [
                { "if": 920, "value": { "id": "included", "src": "1", "x": 0, "y": 0, "w": 1, "h": 1 } }
            ]
            "#,
        )
        .unwrap();
    let includes_load_time = root.join("includes-load-time.json");
    std::fs::write(
        &includes_load_time,
        r#"
            {
                "type": 5,
                "image": [{ "include": "include.json" }]
            }
            "#,
    )
    .unwrap();
    let lua_skin = root.join("load-time.luaskin");
    std::fs::write(&lua_skin, "return { type = 5 }").unwrap();
    let lr2_skin = root.join("load-time.lr2skin");
    std::fs::write(&lr2_skin, "#LR2SKIN").unwrap();

    assert!(!skin_path_options_need_full_reload(&op_only).unwrap());
    assert!(skin_path_options_need_full_reload(&load_time).unwrap());
    assert!(skin_path_options_need_full_reload(&includes_load_time).unwrap());
    assert!(skin_path_options_need_full_reload(&lua_skin).unwrap());
    assert!(skin_path_options_need_full_reload(&lr2_skin).unwrap());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn skin_video_sources_need_runtime_state_only_for_active_gated_sources() {
    let make_source =
        |active: bool, failed: bool, gating_op_sets: Vec<Vec<i32>>| ActiveSkinVideoSource {
            texture: SkinTextureId(0),
            path: PathBuf::new(),
            decoder: None,
            last_pts: None,
            loop_start_us: 0,
            active,
            gating_op_sets,
            enabled_options: Vec::new(),
            result_ranktime_ms: 0,
            failed,
        };

    assert!(!skin_video_sources_need_runtime_state(&[
        make_source(true, false, Vec::new()),
        make_source(false, false, vec![vec![90]]),
        make_source(true, true, vec![vec![90]]),
    ]));
    let gated_source = make_source(true, false, vec![vec![90]]);
    assert!(skin_video_sources_need_runtime_state(&[gated_source]));
}

#[test]
fn play_skin_video_source_runtime_visibility_follows_bga_ops() {
    // ECFN の generic BGA 相当。beatoraja では BGA ON かつ曲BGAなしの時だけ
    // destination が有効になり、動画フレーム取得も走る。
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "property": [
                    {
                        "name": "Generic BGA",
                        "def": "P1",
                        "item": [
                            { "name": "P1", "op": 924 },
                            { "name": "P2", "op": 925 }
                        ]
                    }
                ],
                "source": [{ "id": "mv", "path": "generic.mp4" }],
                "image": [{ "id": "generic-BGA", "src": "mv", "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    { "id": "generic-BGA", "op": [41, 170, 924], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let gating = skin_video_source_gating(&document, "mv");
    assert!(gating.active);
    assert_eq!(gating.op_sets, vec![vec![41, 170, 924]]);
    let source = ActiveSkinVideoSource {
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
    };

    let visible_state = play_skin_video_draw_state(
        &RenderSnapshot {
            has_bga: false,
            bga_enabled: true,
            resources_loaded: true,
            ..RenderSnapshot::default()
        },
        None,
        None,
        0,
    );
    assert!(skin_video_source_runtime_visible(&source, &visible_state));

    let song_bga_state = play_skin_video_draw_state(
        &RenderSnapshot {
            has_bga: true,
            bga_enabled: true,
            resources_loaded: true,
            ..RenderSnapshot::default()
        },
        None,
        None,
        0,
    );
    assert!(!skin_video_source_runtime_visible(&source, &song_bga_state));

    let bga_off_state = play_skin_video_draw_state(
        &RenderSnapshot {
            has_bga: false,
            bga_enabled: false,
            resources_loaded: true,
            ..RenderSnapshot::default()
        },
        None,
        None,
        0,
    );
    assert!(!skin_video_source_runtime_visible(&source, &bga_off_state));

    let song_bga_off_state = play_skin_video_draw_state(
        &RenderSnapshot {
            has_bga: true,
            bga_enabled: false,
            resources_loaded: true,
            ..RenderSnapshot::default()
        },
        None,
        None,
        0,
    );
    assert!(!skin_video_source_runtime_visible(&source, &song_bga_off_state));
}

#[test]
fn play_skin_draw_state_maps_lane_cover_and_lift_offsets_to_skin_pixels() {
    let state = play_skin_video_draw_state(
        &RenderSnapshot {
            lane_cover: 0.5,
            lift: 0.25,
            hidden_cover: 0.1,
            ..RenderSnapshot::default()
        },
        Some(1080),
        Some(720),
        0,
    );

    assert_eq!(state.offset_lift_px, 180);
    assert_eq!(state.offset_lanecover_px, -360);
    assert_eq!(state.offset_hidden_cover_px, 54);
}

#[test]
fn play_skin_video_loaded_state_starts_with_ready_timer() {
    let preload_state = play_skin_video_draw_state(
        &RenderSnapshot {
            resources_loaded: true,
            ready_elapsed_time: None,
            ..RenderSnapshot::default()
        },
        None,
        None,
        0,
    );
    assert!(!preload_state.skin_loaded);

    let ready_state = play_skin_video_draw_state(
        &RenderSnapshot {
            resources_loaded: true,
            ready_elapsed_time: Some(TimeUs(0)),
            ..RenderSnapshot::default()
        },
        None,
        None,
        0,
    );
    assert!(ready_state.skin_loaded);

    let seamless_state = play_skin_video_draw_state(
        &RenderSnapshot {
            resources_loaded: false,
            play_elapsed_time: TimeUs(2_500_000),
            seamless_play_entry: true,
            ..RenderSnapshot::default()
        },
        None,
        None,
        1_000,
    );
    assert!(seamless_state.skin_loaded);
    assert_eq!(seamless_state.ready_timer_ms, None);
    assert_eq!(seamless_state.start_input_ms, Some(1_500));
}

#[test]
fn skin_video_source_gating_respects_conditional_destination_if_ops() {
    use bmz_render::skin::SkinDrawState;

    let mut document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 7,
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
                "source": [{ "id": "BG_AAA", "path": "BG/AAA/aaa.mp4" }],
                "image": [{ "id": "BG_AAA", "src": "BG_AAA", "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    {
                        "if": [920],
                        "values": [
                            { "id": "BG_AAA", "op": [90, 300], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] }
                        ]
                    }
                ]
            }
            "#,
        )
        .unwrap();

    let gating = skin_video_source_gating(&document, "BG_AAA");
    assert!(gating.active);
    assert_eq!(gating.op_sets, vec![vec![920, 90, 300]]);
    let aaa_state = SkinDrawState {
        result_failed: Some(false),
        ex_score: 18,
        total_notes: 9,
        ..SkinDrawState::default()
    };
    let source = ActiveSkinVideoSource {
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
    };
    assert!(skin_video_source_runtime_visible(&source, &aaa_state));

    document.user_selected_options = Some(vec![921]);
    let gating = skin_video_source_gating(&document, "BG_AAA");
    assert!(!gating.active);
    let disabled_source = ActiveSkinVideoSource {
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
    };
    assert!(!skin_video_source_runtime_visible(&disabled_source, &aaa_state));
}

#[test]
fn skin_logical_inputs_include_all_e_actions_and_ui_directions() {
    let keys = default_select_keys();
    let pressed = HashSet::from([
        "Q".to_string(),
        "W".to_string(),
        "E".to_string(),
        "R".to_string(),
        "ArrowLeft".to_string(),
        "DPadRight".to_string(),
        "ArrowUp".to_string(),
        "DPadDown".to_string(),
    ]);

    assert_eq!(
        skin_logical_input_snapshot_from_pressed_controls(&pressed, &keys).held,
        [true; bmz_render::skin::SKIN_BMZ_INPUT_COUNT]
    );
}

#[test]
fn loaded_skin_reset_preserves_non_skin_profile_settings() {
    let mut current = ProfileConfig::new_default("default", "Current", 1);
    current.play.random = RandomOptionConfig::SRandom;
    current.input.gamepad1.analog_scratch_sensitivity = 2.5;
    current.ui.show_fps = true;
    current.skin.select = "current/select.json".to_string();

    let mut loaded = ProfileConfig::new_default("default", "Disk", 2);
    loaded.play.random = RandomOptionConfig::Mirror;
    loaded.input.gamepad1.analog_scratch_sensitivity = 0.5;
    loaded.ui.show_fps = false;
    loaded.skin.select = "disk/select.json".to_string();

    replace_skin_config_from_loaded_profile(&mut current, loaded);

    assert_eq!(current.display_name, "Current");
    assert_eq!(current.updated_at, 1);
    assert_eq!(current.play.random, RandomOptionConfig::SRandom);
    assert_eq!(current.input.gamepad1.analog_scratch_sensitivity, 2.5);
    assert!(current.ui.show_fps);
    assert_eq!(current.skin.select, "disk/select.json");
}

#[test]
fn play_skin_key_mode_uses_battle_double_mode() {
    assert_eq!(
        play_skin_key_mode_for_options(
            KeyMode::K7,
            DoubleOption::Battle,
            SessionMode::Normal,
            KeyModeConversionConfig::Off,
            false,
        ),
        KeyMode::K14
    );
    assert_eq!(
        play_skin_key_mode_for_options(
            KeyMode::K7,
            DoubleOption::Battle,
            SessionMode::Normal,
            KeyModeConversionConfig::SevenToNine,
            false,
        ),
        KeyMode::K14
    );
    assert_eq!(
        play_skin_key_mode_for_options(
            KeyMode::K7,
            DoubleOption::BattleAutoScratch,
            SessionMode::Normal,
            KeyModeConversionConfig::Off,
            false,
        ),
        KeyMode::K14
    );
    assert_eq!(
        play_skin_key_mode_for_options(
            KeyMode::K5,
            DoubleOption::Battle,
            SessionMode::Normal,
            KeyModeConversionConfig::Off,
            false,
        ),
        KeyMode::K10
    );
    assert_eq!(
        play_skin_key_mode_for_options(
            KeyMode::K7,
            DoubleOption::Flip,
            SessionMode::Normal,
            KeyModeConversionConfig::Off,
            false,
        ),
        KeyMode::K7
    );
    assert_eq!(
        play_skin_key_mode_for_options(
            KeyMode::K14,
            DoubleOption::Battle,
            SessionMode::Normal,
            KeyModeConversionConfig::Off,
            false,
        ),
        KeyMode::K14
    );
    assert_eq!(
        play_skin_key_mode_for_options(
            KeyMode::K7,
            DoubleOption::Off,
            SessionMode::AutoplayBattle,
            KeyModeConversionConfig::Off,
            false,
        ),
        KeyMode::K7
    );
    assert_eq!(
        play_skin_key_mode_for_options(
            KeyMode::K7,
            DoubleOption::Battle,
            SessionMode::AutoplayBattle,
            KeyModeConversionConfig::SevenToSix,
            false,
        ),
        KeyMode::K7
    );
    assert_eq!(
        play_skin_key_mode_for_options(
            KeyMode::K7,
            DoubleOption::Battle,
            SessionMode::Normal,
            KeyModeConversionConfig::Off,
            true,
        ),
        KeyMode::K7
    );
    assert_eq!(
        play_skin_key_mode_for_options(
            KeyMode::K7,
            DoubleOption::Off,
            SessionMode::Normal,
            KeyModeConversionConfig::SevenToNine,
            false,
        ),
        KeyMode::K9
    );
    assert_eq!(
        play_skin_key_mode_for_options(
            KeyMode::K5,
            DoubleOption::Off,
            SessionMode::Normal,
            KeyModeConversionConfig::SpToDp,
            false,
        ),
        KeyMode::K10
    );
}
