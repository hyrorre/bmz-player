use super::*;

#[test]
fn lr2_result_uses_separate_player_number_and_rank_references() {
    let root = unique_test_dir("lr2-result-refs");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("result.lr2skin");
    fs::write(&path, "#INFORMATION,7,test,test\n#IMAGE,a.png\n#SRC_NUMBER,0,0,0,0,100,10,10,1,0,0,121,0,4\n#DST_NUMBER,0,0,0,0,20,10,0,255,255,255,255,0,0,0,0,0,0,314\n").unwrap();
    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert_eq!(loaded.value["value"][0]["ref"], bmz_skin_document::LR2_RESULT_NUMBER_BASE + 21);
    let destinations = loaded.value["destination"].as_array().unwrap();
    assert_eq!(destinations[0]["op"][0], bmz_skin_document::LR2_RESULT_RANK_BASE + 14);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lr2_scene_resolves_sibling_theme_assets_and_custom_files() {
    let root = unique_test_dir("lr2-scene-paths");
    fs::create_dir_all(root.join("Select")).unwrap();
    fs::create_dir_all(root.join("Decide")).unwrap();
    fs::write(root.join("Decide/parts.png"), []).unwrap();
    let path = root.join("Select/select.lr2skin");
    assert!(root.join("Decide/parts.png").is_file());
    assert!(bmz_skin_assets::is_file(&root.join("Decide/parts.png")));
    assert_eq!(relative_to_skin_file_parent(&path, "Decide/parts.png"), "../Decide/parts.png");
    fs::write(&path, "#INFORMATION,5,test,test\n#CUSTOMFILE,Shared,LR2files/Theme/test/Decide/*.png,parts\n#IMAGE,LR2files/Theme/test/Decide/parts.png\n#IMAGE,LR2files/Theme/test/Decide/*.png\n").unwrap();
    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert_eq!(loaded.value["source"][0]["path"], "../Decide/parts.png");
    assert_eq!(loaded.value["source"][1]["path"], "../Decide/parts.png");
    fs::write(root.join("Decide/custom.png"), []).unwrap();
    let selected = BTreeMap::from([("Shared".into(), "../Decide/custom.png".into())]);
    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &selected).unwrap();
    assert_eq!(loaded.value["source"][1]["path"], "../Decide/custom.png");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lr2_scene_assets_decode_when_available() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins");
    for (relative, kind) in [
        ("LR2/Select/select.lr2skin", crate::SkinKind::Select),
        ("WMIX_HD/select/select.lr2skin", crate::SkinKind::Select),
        ("Seraphic/Select/[+]select.lr2skin", crate::SkinKind::Select),
        ("ECBE/Select/select.lr2skin", crate::SkinKind::Select),
        ("LR2/Result/result.lr2skin", crate::SkinKind::Result),
        ("WMIX_HD/result/WMIX_RESULT.lr2skin", crate::SkinKind::Result),
        ("WMIX_HD/courseresult/courseresult.lr2skin", crate::SkinKind::Result),
        ("Seraphic/Result/[+]result.lr2skin", crate::SkinKind::Result),
    ] {
        let path = root.join(relative);
        if !path.is_file() {
            eprintln!("SKIP missing {relative}");
            continue;
        }
        let loaded =
            crate::load_lr2_csv_skin(&path, kind, &BTreeMap::new(), &BTreeMap::new()).unwrap();
        let warnings = loaded.warnings.iter().map(|w| &w.message).collect::<BTreeSet<_>>();
        eprintln!(
            "{relative}: {} images, {} destinations, {:?}",
            loaded.document.image.len(),
            loaded.document.destination.len(),
            warnings
        );
        assert!(loaded.document.lr2);
        assert!(loaded.document.image.len() > 10, "{relative}");
        assert!(
            !loaded.warnings.iter().any(|w| w.message.contains("include not found")),
            "{relative}: {:?}",
            loaded.warnings
        );
        if kind == crate::SkinKind::Select {
            assert!(
                loaded
                    .document
                    .songlist
                    .as_ref()
                    .is_some_and(|l| l.lr2_bottom_origin && !l.listoff.is_empty()),
                "{relative}"
            );
        } else {
            assert!(loaded.document.lr2_result.is_some(), "{relative}");
            if loaded.document.skin_type != 15 {
                assert!(!loaded.document.lr2_charts.is_empty(), "{relative}");
            }
        }
    }
}

#[test]
fn lr2_bar_sources_are_indexed_and_children_use_local_coordinates() {
    let root = unique_test_dir("lr2-bars");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("select.lr2skin");
    fs::write(&path, "#INFORMATION,5,test,test\n#IMAGE,a.png\n#FONT,20,1,0\n#SRC_BAR_BODY,0,0,0,0,100,30,1,1,0,0\n#SRC_BAR_BODY,1,0,0,30,100,30,1,1,0,0\n#DST_BAR_BODY_OFF,0,0,100,200,100,30,0,255,255,255,255,0,0,0,0,0,0\n#DST_BAR_BODY_ON,0,0,100,200,100,30,0,255,255,255,255,0,0,0,0,0,0\n#SRC_BAR_TITLE,0,0,0,0\n#DST_BAR_TITLE,0,0,10,5,90,20,0,255,255,255,255,0,0,0,0,0,0\n#BAR_CENTER,0\n#BAR_AVAILABLE,0,0\n").unwrap();
    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert_eq!(loaded.value["songlist"]["listoff"][0]["dst"][0]["y"], 250);
    assert_eq!(loaded.value["songlist"]["text"][2]["dst"][0]["y"], -25);
    assert_eq!(loaded.value["songlist"]["clickable"], json!([0]));
    assert_eq!(loaded.value["imageset"][0]["images"][0], loaded.value["image"][0]["id"]);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lr2_bar_title_ignores_reserved_options_but_preserves_if_conditions() {
    let root = unique_test_dir("lr2-bar-title-conditions");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("select.lr2skin");
    fs::write(
        &path,
        "#INFORMATION,5,test,test\n#FONT,20,1,0\n\
         #IF,32\n#SRC_BAR_TITLE,0,0,0,0\n\
         #DST_BAR_TITLE,0,0,10,5,90,20,0,255,255,255,255,0,0,0,0,0,0,1,2,3\n\
         #DST_BAR_TITLE,0,100,20,5,90,20,0,255,255,255,255,0,0,0,0,0,0\n#ENDIF\n",
    )
    .unwrap();
    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    let title = &loaded.value["songlist"]["text"][2];
    assert_eq!(title["op"], json!([32]));
    assert_eq!(title["dst"].as_array().unwrap().len(), 2);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lr2_scene_resolution_button_and_hover_metadata_are_preserved() {
    let root = unique_test_dir("lr2-scene-controls");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("select.lr2skin");
    fs::write(&path, "#INFORMATION,5,test,test\n#IMAGE,a.png\n#SRC_BUTTON,0,0,0,0,60,10,6,1,0,0,40,1,1,2\n#DST_BUTTON,0,0,1,2,60,10\n#SRC_ONMOUSE,0,0,0,0,20,30,1,1,0,0,0,1,2,15,25\n#DST_ONMOUSE,0,0,4.5,6.5,20,30\n").unwrap();
    let options = BTreeMap::from([("LR2 Resolution (BMZ)".into(), "1280x720".into())]);
    let loaded =
        crate::load_lr2_csv_skin(&path, crate::SkinKind::Select, &options, &BTreeMap::new())
            .unwrap();
    assert_eq!((loaded.document.w, loaded.document.h), (1280, 720));
    assert_eq!(loaded.document.image[0].act, Some(bmz_skin_document::LR2_BUTTON_BASE + 40));
    assert_eq!(loaded.document.image[0].click, 1);
    assert_eq!(loaded.document.image[0].lr2_panel, Some(1));
    let value = load_lr2_csv_skin_value(&path, &options, &BTreeMap::new()).unwrap().value;
    assert_eq!(value["destination"][1]["dst"][0]["x"], 4);
    assert_eq!(value["destination"][1]["mouseRect"], json!({"x":1,"y":3,"w":15,"h":25}));
    assert_eq!(loaded.dependencies.option_values.get(&99_002), Some(&true));
    fs::remove_dir_all(root).unwrap();
}
