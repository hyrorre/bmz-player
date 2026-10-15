use super::*;

#[test]
fn included_endif_and_unclosed_if_cannot_change_parent_side_branch() {
    let root = unique_test_dir("lr2-result-file-conditions");
    fs::create_dir_all(&root).unwrap();
    let entry = root.join("result.lr2skin");
    fs::write(&entry, "#INFORMATION,7,test,test\n#CUSTOMOPTION,side,900,Left,Right\n#IF,900\n#INCLUDE,left.csv\n#ELSEIF,901\n#INCLUDE,right.csv\n#ENDIF\n#IF,950\n#IMAGE,assigned.png\n#ENDIF\n").unwrap();
    fs::write(root.join("left.csv"), "#ENDIF\n#IMAGE,left.png\n#SETOPTION,950,1\n#IF,900\n")
        .unwrap();
    fs::write(root.join("right.csv"), "#IMAGE,right.png\n").unwrap();
    for (side, expected) in
        [("Left", vec!["left.png", "assigned.png"]), ("Right", vec!["right.png"])]
    {
        let loaded = load_lr2_csv_skin_value(
            &entry,
            &BTreeMap::from([("side".into(), side.into())]),
            &BTreeMap::new(),
        )
        .unwrap();
        let paths: Vec<_> = loaded.value["source"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, expected);
    }
}

#[test]
fn included_draw_conditions_survive_child_endifs_and_restore_the_caller() {
    let root = unique_test_dir("lr2-result-include-draw-gates");
    fs::create_dir_all(&root).unwrap();
    let entry = root.join("result.lr2skin");
    fs::write(&entry, "#INFORMATION,7,test,test\n#IMAGE,atlas.png\n#IF,105\n#INCLUDE,child.csv\n#ELSE\n#INCLUDE,child.csv\n#ENDIF\n").unwrap();
    fs::write(root.join("child.csv"), "#ENDIF\n#IF,624\n#SRC_IMAGE,0,0,0,0,8,8,1,1,0,0,0,0,0\n#DST_IMAGE,0,0,0,0,8,8,0,255,255,255,255,1,0,0,0,0,0,0,0,0\n#ENDIF\n").unwrap();
    let loaded = load_lr2_csv_skin_value(&entry, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert_eq!(
        loaded.value["destination"][0]["op"],
        json!([bmz_skin_document::LR2_OPTION_BASE + 105, 624])
    );
    assert_eq!(
        loaded.value["destination"][1]["op"],
        json!([-(bmz_skin_document::LR2_OPTION_BASE + 105), 624])
    );
}

#[test]
fn continued_assets_keep_slots_paths_color_keys_and_font_definitions() {
    let files = BTreeMap::new();
    let mut builder = CsvBuilder::new(Path::new("result.lr2skin"), Header::default(), &files);
    builder.inherited_assets = crate::Lr2ContinueAssets {
        sources: BTreeMap::from([(
            "0".into(),
            bmz_skin_document::SkinSourceDef {
                id: "0".into(),
                path: "C:/previous/frame.png".into(),
                lr2_color_key: Some([1, 2, 3]),
            },
        )]),
        fonts: BTreeMap::from([(
            "lr2font-0".into(),
            bmz_skin_document::SkinFontDef {
                id: "lr2font-0".into(),
                path: "C:/previous/font.lr2font".into(),
                font_type: 1,
            },
        )]),
        system_fonts: vec![bmz_skin_document::SkinLr2SystemFontDef {
            size: 24,
            thickness: 3,
            font_type: 1,
        }],
    };
    builder.add_source("CONTINUE");
    builder.add_source("CONTINUE");
    builder.add_lr2_font("CONTINUE");
    builder.add_system_font(&parse_csv_line("#FONT,CONTINUE").unwrap());
    assert_eq!(builder.sources[0]["path"], "C:/previous/frame.png");
    assert_eq!(builder.sources[0]["lr2ColorKey"], json!([1, 2, 3]));
    assert_eq!(builder.sources[1]["path"], "bmz://lr2/empty");
    assert_eq!(builder.fonts[0]["path"], "C:/previous/font.lr2font");
    assert_eq!(builder.system_fonts[0], json!({"size":24,"thickness":3,"type":1}));
}

#[test]
fn result_lr2_only_refs_and_flat_buttons_are_converted() {
    let files = BTreeMap::new();
    let mut builder = CsvBuilder::new(
        Path::new("result.lr2skin"),
        Header { skin_type: 7, ..Default::default() },
        &files,
    );
    builder.add_source("atlas.png");
    builder
        .execute(&parse_csv_line("#SRC_BUTTON,0,0,0,0,328,16,4,1,0,0,40,0,0,0").unwrap())
        .unwrap();
    assert_eq!(builder.images[0]["lr2Button"], true);
    for id in [154, 163, 164] {
        assert_eq!(lr2_number_ref(7, id), Some(bmz_skin_document::LR2_NUMBER_BASE + id));
        assert_eq!(lr2_number_ref(0, id), None);
    }
    assert_eq!(lr2_option_ref(7, 105), Some(bmz_skin_document::LR2_OPTION_BASE + 105));
}

#[test]
fn wmix_play_selects_best_clear_source_before_result_continues_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMIX_HD/play/HDPLAY_AC.lr2skin");
    if !path.is_file() {
        return;
    }
    for (option, name) in [(100, "NOPLAY.png"), (104, "HARDCLEAR.png"), (105, "FULLCOMBO.png")] {
        let runtime = (100..=105).map(|id| (id, id == option)).collect();
        let loaded = load_lr2_csv_skin_value_with_runtime_options(
            &path,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &runtime,
        )
        .unwrap();
        assert!(loaded.value["source"][23]["path"].as_str().unwrap().ends_with(name));
        assert_eq!(loaded.dependencies.option_values.get(&option), Some(&true));
    }
}

#[test]
fn wmix_left_and_right_load_only_their_selected_file_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/WMIX_HD/result/WMIX_RESULT.lr2skin");
    if !path.is_file() {
        return;
    }
    for (side, suffix, absent) in
        [("Left", "result_L.csv", "result_R.csv"), ("Right", "result_R.csv", "result_L.csv")]
    {
        let loaded = load_lr2_csv_skin_value(
            &path,
            &BTreeMap::from([("side".into(), side.into())]),
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(loaded.dependencies.loaded_files.keys().any(|p| p.ends_with(suffix)));
        assert!(!loaded.dependencies.loaded_files.keys().any(|p| p.ends_with(absent)));
    }
}
