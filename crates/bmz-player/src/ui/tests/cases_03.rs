use super::*;

#[test]
fn skin_rows_only_build_widgets_inside_the_scroll_clip_rect() {
    let mut built_rows = 0;
    egui::__run_test_ui(|ui| {
        let visible =
            egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 64.0));
        ui.set_clip_rect(ui.clip_rect().intersect(visible));
        for row in 0..1_000 {
            show_culled_skin_row(ui, row, 20.0, |_ui| built_rows += 1);
        }
    });

    assert!(built_rows > 0);
    assert!(built_rows < 10, "画面外の行まで構築された: {built_rows}");
}

#[test]
fn filepath_default_uses_random_sentinel_for_random_def() {
    // def="Random" は具体ファイルへ固定せず、ランダム番兵を既定にする。
    let filepath = SkinFilepathDef {
        category: String::new(),
        name: "BG".to_string(),
        path: "bg/*.mp4".to_string(),
        def: "Random".to_string(),
    };
    let candidates = vec!["bg/one.mp4".to_string(), "bg/two.mp4".to_string()];
    assert_eq!(filepath_default(&filepath, &candidates).as_deref(), Some(RANDOM_FILE_SELECTION));
}

#[test]
fn filepath_default_prefers_default_stem_when_def_missing() {
    let filepath = SkinFilepathDef {
        category: String::new(),
        name: "Note".to_string(),
        path: "notes/*.png".to_string(),
        def: String::new(),
    };
    let candidates = vec!["pastel.png".to_string(), "default.png".to_string()];

    assert_eq!(filepath_default(&filepath, &candidates).as_deref(), Some("default.png"));
}

#[test]
fn fill_missing_skin_defaults_keeps_saved_values_and_fills_new_items() {
    let root = unique_test_dir("bmz-ui-defaults");
    fs::create_dir_all(root.join("notes")).unwrap();
    fs::write(root.join("notes/aaa.png"), []).unwrap();
    fs::write(root.join("notes/default.png"), []).unwrap();
    let defs = SceneSkinDefs {
        property: vec![
            SkinPropertyDef {
                category: String::new(),
                name: "Lane".to_string(),
                item: vec![
                    bmz_render::skin::SkinPropertyItemDef { name: "Off".to_string(), op: 0 },
                    bmz_render::skin::SkinPropertyItemDef { name: "On".to_string(), op: 1 },
                ],
                def: "On".to_string(),
            },
            SkinPropertyDef {
                category: String::new(),
                name: "Saved".to_string(),
                item: vec![
                    bmz_render::skin::SkinPropertyItemDef { name: "A".to_string(), op: 0 },
                    bmz_render::skin::SkinPropertyItemDef { name: "B".to_string(), op: 1 },
                ],
                def: "A".to_string(),
            },
        ],
        filepath: vec![SkinFilepathDef {
            category: String::new(),
            name: "Notes".to_string(),
            path: "notes/*.png".to_string(),
            def: "default".to_string(),
        }],
        offset: Vec::new(),
        ..Default::default()
    };
    let mut options = BTreeMap::from([("Saved".to_string(), "B".to_string())]);
    let mut files = BTreeMap::new();

    assert!(fill_missing_skin_defaults(&defs, Some(&root), &mut options, &mut files));

    assert_eq!(options.get("Lane").map(String::as_str), Some("On"));
    assert_eq!(options.get("Saved").map(String::as_str), Some("B"));
    assert_eq!(files.get("Notes").map(String::as_str), Some("notes/default.png"));
}

#[test]
fn fill_missing_skin_defaults_replaces_stale_option_selection() {
    let defs = SceneSkinDefs {
        property: vec![SkinPropertyDef {
            category: String::new(),
            name: "Graph".to_string(),
            item: vec![
                bmz_render::skin::SkinPropertyItemDef { name: "AC".to_string(), op: 922 },
                bmz_render::skin::SkinPropertyItemDef { name: "TYPE-M".to_string(), op: 923 },
            ],
            def: "AC".to_string(),
        }],
        filepath: Vec::new(),
        offset: Vec::new(),
        ..Default::default()
    };
    let mut options = BTreeMap::from([("Graph".to_string(), "999".to_string())]);
    let mut files = BTreeMap::new();

    assert!(fill_missing_skin_defaults(&defs, None, &mut options, &mut files));

    assert_eq!(options.get("Graph").map(String::as_str), Some("AC"));
}

#[test]
fn fill_missing_skin_defaults_keeps_stale_file_selection_like_beatoraja() {
    let root = unique_test_dir("bmz-ui-defaults-stale");
    fs::create_dir_all(root.join("notes")).unwrap();
    fs::write(root.join("notes/aaa.png"), []).unwrap();
    fs::write(root.join("notes/default.png"), []).unwrap();
    let defs = SceneSkinDefs {
        property: Vec::new(),
        filepath: vec![SkinFilepathDef {
            category: String::new(),
            name: "Notes".to_string(),
            path: "notes/*.png".to_string(),
            def: "default".to_string(),
        }],
        offset: Vec::new(),
        ..Default::default()
    };
    let mut options = BTreeMap::new();
    let mut files = BTreeMap::from([("Notes".to_string(), "../old/default.png".to_string())]);

    assert!(!fill_missing_skin_defaults(&defs, Some(&root), &mut options, &mut files));

    assert_eq!(files.get("Notes").map(String::as_str), Some("../old/default.png"));
}

#[test]
fn play_skin_defs_include_beatoraja_common_offsets() {
    let defs = SceneSkinDefs::from_play_document(None);

    let offsets: Vec<_> =
        defs.offset.iter().map(|offset| (offset.id, offset.name.as_str())).collect();
    assert!(offsets.contains(&(10, "All offset(%)")));
    assert!(offsets.contains(&(30, "Notes offset")));
    assert!(offsets.contains(&(32, "Judge offset")));
    assert!(offsets.contains(&(33, "Judge Detail offset")));
    assert!(offsets.contains(&(SKIN_OFFSET_BAR_LINE, "Bar Line offset")));

    let notes = defs.offset.iter().find(|offset| offset.id == 30).expect("notes offset def");
    assert!(notes.x && notes.y && notes.w && notes.h && notes.a);
}

#[test]
fn play_skin_defs_append_beatoraja_common_offsets_after_same_id_custom_defs() {
    let mut defs = SceneSkinDefs::default();
    defs.offset.push(SkinOffsetDef {
        category: "custom".to_string(),
        name: "Custom all".to_string(),
        id: 10,
        x: true,
        y: true,
        w: false,
        h: false,
        r: false,
        a: false,
    });

    defs.append_play_common_offsets();

    assert_eq!(defs.offset.iter().filter(|offset| offset.id == 10).count(), 2);
    assert_eq!(defs.offset.len(), 6);
    assert_eq!(
        defs.offset.iter().rfind(|offset| offset.id == 10).map(|offset| offset.name.as_str()),
        Some("All offset(%)")
    );
}

#[test]
fn play_skin_defs_enable_bar_line_alpha_when_skin_def_disables_it() {
    let mut defs = SceneSkinDefs::default();
    defs.offset.push(SkinOffsetDef {
        category: "custom".to_string(),
        name: "Custom bar".to_string(),
        id: SKIN_OFFSET_BAR_LINE,
        x: false,
        y: false,
        w: false,
        h: true,
        r: false,
        a: false,
    });

    defs.append_play_common_offsets();

    let bar_line = defs
        .offset
        .iter()
        .find(|offset| offset.id == SKIN_OFFSET_BAR_LINE)
        .expect("bar line offset def");
    assert!(bar_line.x && bar_line.y && bar_line.w && bar_line.h && bar_line.a);
}

#[test]
fn skin_offset_sync_prefers_name_and_updates_changed_definition_id() {
    let defs = vec![test_offset_def("Antique lane", 80)];
    let mut offsets = vec![
        SkinOffsetConfig {
            name: Some("Antique lane".to_string()),
            id: 70,
            x: 12,
            ..Default::default()
        },
        SkinOffsetConfig { id: 80, x: 99, ..Default::default() },
    ];

    assert!(sync_skin_offsets_with_defs(&defs, &mut offsets));
    assert_eq!(
        offsets,
        vec![SkinOffsetConfig {
            name: Some("Antique lane".to_string()),
            id: 80,
            x: 12,
            ..Default::default()
        }]
    );
}

#[test]
fn skin_offset_sync_expands_legacy_duplicate_id_into_independent_names() {
    let defs = vec![test_offset_def("Lane A", 42), test_offset_def("Lane B", 42)];
    let mut offsets = vec![SkinOffsetConfig { id: 42, y: -8, ..Default::default() }];

    assert!(sync_skin_offsets_with_defs(&defs, &mut offsets));
    assert_eq!(offsets.len(), 2);
    assert_eq!(offsets[0].name.as_deref(), Some("Lane A"));
    assert_eq!(offsets[1].name.as_deref(), Some("Lane B"));
    assert_eq!(offsets[0].y, -8);
    assert_eq!(offsets[1].y, -8);

    let mut edited = offsets[0].clone();
    edited.y = 24;
    assert!(update_skin_offset_value(&mut offsets, &defs[0], edited));
    assert_eq!(offsets[0].y, 24);
    assert_eq!(offsets[1].y, -8);
}

#[test]
fn skin_offset_sync_shares_first_named_value_across_duplicate_name_ids() {
    let defs = vec![test_offset_def("Shared", 51), test_offset_def("Shared", 52)];
    let mut offsets = vec![
        SkinOffsetConfig { name: Some("Shared".to_string()), id: 51, a: 120, ..Default::default() },
        SkinOffsetConfig { name: Some("Shared".to_string()), id: 52, a: 240, ..Default::default() },
    ];

    assert!(sync_skin_offsets_with_defs(&defs, &mut offsets));
    assert_eq!(offsets.iter().map(|offset| offset.id).collect::<Vec<_>>(), vec![51, 52]);
    assert!(offsets.iter().all(|offset| offset.a == 120));

    let mut edited = offsets[1].clone();
    edited.a = 64;
    assert!(update_skin_offset_value(&mut offsets, &defs[1], edited));
    assert!(offsets.iter().all(|offset| offset.a == 64));
}

#[test]
fn reset_scene_skin_to_defaults_clears_saved_values_and_restores_factory_defaults() {
    let root = unique_test_dir("bmz-ui-reset-scene");
    fs::create_dir_all(root.join("notes")).unwrap();
    fs::write(root.join("notes/aaa.png"), []).unwrap();
    fs::write(root.join("notes/default.png"), []).unwrap();
    let defs = SceneSkinDefs {
        property: vec![SkinPropertyDef {
            category: String::new(),
            name: "Lane".to_string(),
            item: vec![
                bmz_render::skin::SkinPropertyItemDef { name: "Off".to_string(), op: 0 },
                bmz_render::skin::SkinPropertyItemDef { name: "On".to_string(), op: 1 },
            ],
            def: "On".to_string(),
        }],
        filepath: vec![SkinFilepathDef {
            category: String::new(),
            name: "Notes".to_string(),
            path: "notes/*.png".to_string(),
            def: "default".to_string(),
        }],
        offset: vec![SkinOffsetDef {
            category: "test".to_string(),
            name: "Judge".to_string(),
            id: 32,
            x: true,
            y: true,
            w: false,
            h: false,
            r: false,
            a: false,
        }],
        ..Default::default()
    };
    let mut options = BTreeMap::from([("Lane".to_string(), "Off".to_string())]);
    let mut files = BTreeMap::from([("Notes".to_string(), "aaa.png".to_string())]);
    let mut offsets = vec![SkinOffsetConfig { id: 32, x: 99, ..Default::default() }];

    assert!(reset_scene_skin_to_defaults(
        &defs,
        Some(&root),
        &mut options,
        &mut files,
        &mut offsets
    ));

    assert_eq!(options.get("Lane").map(String::as_str), Some("On"));
    assert_eq!(files.get("Notes").map(String::as_str), Some("notes/default.png"));
    assert!(offsets.is_empty());
}

#[test]
fn reset_scene_skin_to_defaults_removes_named_defs_without_same_id_name_collision() {
    let defs = SceneSkinDefs { offset: vec![test_offset_def("Current", 32)], ..Default::default() };
    let mut options = BTreeMap::new();
    let mut files = BTreeMap::new();
    let mut offsets = vec![
        SkinOffsetConfig { name: Some("Current".to_string()), id: 32, x: 10, ..Default::default() },
        SkinOffsetConfig { name: Some("Other".to_string()), id: 32, x: 20, ..Default::default() },
    ];

    assert!(reset_scene_skin_to_defaults(&defs, None, &mut options, &mut files, &mut offsets));
    assert_eq!(offsets.len(), 1);
    assert_eq!(offsets[0].name.as_deref(), Some("Other"));
    assert_eq!(offsets[0].x, 20);
}

#[test]
fn skin_slot_history_restores_options_files_and_offsets_by_path() {
    let mut skin = SkinConfig {
        play7: "data/skins/ECFN/play/play7.luaskin".to_string(),
        play7_offsets: vec![SkinOffsetConfig {
            name: Some("Judge offset".to_string()),
            id: 32,
            x: 12,
            ..Default::default()
        }],
        ..SkinConfig::default()
    };
    skin.play7_options.insert("Judge".to_string(), "On".to_string());
    skin.play7_files.insert("Notes".to_string(), "default.png".to_string());

    save_skin_slot_history(&mut skin, SkinSlot::Play7);
    skin.play7 = "data/skins/Starseeker/play/play7.luaskin".to_string();
    skin.play7_options.insert("Judge".to_string(), "Off".to_string());
    skin.play7_files.insert("Notes".to_string(), "other.png".to_string());
    skin.play7_offsets = vec![SkinOffsetConfig {
        name: Some("Judge offset".to_string()),
        id: 32,
        x: -4,
        ..Default::default()
    }];
    save_skin_slot_history(&mut skin, SkinSlot::Play7);

    skin.play7 = "data/skins/ECFN/play/play7.luaskin".to_string();
    restore_skin_slot_history(&mut skin, SkinSlot::Play7);

    assert_eq!(skin.play7_options.get("Judge").map(String::as_str), Some("On"));
    assert_eq!(skin.play7_files.get("Notes").map(String::as_str), Some("default.png"));
    assert_eq!(
        skin.play7_offsets,
        vec![SkinOffsetConfig {
            name: Some("Judge offset".to_string()),
            id: 32,
            x: 12,
            ..Default::default()
        }]
    );
}

#[test]
fn skin_slot_history_isolates_same_path_by_slot() {
    let shared_path = "data/skins/shared/play.luaskin".to_string();
    let mut skin = SkinConfig {
        play7: shared_path.clone(),
        play14: shared_path,
        play7_offsets: vec![SkinOffsetConfig { id: 30, h: 7, ..Default::default() }],
        play14_offsets: vec![SkinOffsetConfig { id: 30, h: 14, ..Default::default() }],
        ..SkinConfig::default()
    };

    save_skin_slot_history(&mut skin, SkinSlot::Play7);
    save_skin_slot_history(&mut skin, SkinSlot::Play14);
    skin.play7_offsets.clear();
    skin.play14_offsets.clear();
    restore_skin_slot_history(&mut skin, SkinSlot::Play7);
    restore_skin_slot_history(&mut skin, SkinSlot::Play14);

    assert_eq!(skin.play7_offsets[0].h, 7);
    assert_eq!(skin.play14_offsets[0].h, 14);
}

#[test]
fn skin_slot_history_restores_legacy_path_only_entry() {
    let path = "data/skins/legacy/play7.luaskin".to_string();
    let mut skin = SkinConfig { play7: path.clone(), ..SkinConfig::default() };
    skin.history.insert(
        path.clone(),
        SkinHistoryEntryConfig {
            offsets: vec![SkinOffsetConfig { id: 30, h: 12, ..Default::default() }],
            ..Default::default()
        },
    );

    restore_skin_slot_history(&mut skin, SkinSlot::Play7);

    assert_eq!(skin.play7_offsets[0].h, 12);
    assert!(skin.history.contains_key(&skin_slot_history_key(SkinSlot::Play7, &path)));
}
