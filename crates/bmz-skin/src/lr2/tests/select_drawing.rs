use super::*;

#[test]
fn lr2_select_trophies_keep_slots_and_normalize_negative_extents() {
    let root = unique_test_dir("lr2-select-trophies");
    let path = root.join("select.lr2skin");
    let mut csv = String::from("#INFORMATION,5,test,test\n#RESOLUTION,640,480\n");
    for index in [0, 2, 1, -1, 3] {
        csv.push_str(&format!(
            "#SRC_BAR_TROPHY,{index},111,0,0,10,10,1,1,0,0\n#DST_BAR_TROPHY,{index},0,80,30,-20,-10,0,255,255,255,255,1,0,0,0\n"
        ));
    }
    fs::write(&path, csv).unwrap();
    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let trophies = loaded.value["songlist"]["trophy"].as_array().unwrap();
    assert_eq!(trophies.len(), 3);
    let order = loaded.value["destination"]
        .as_array()
        .unwrap()
        .iter()
        .map(|destination| {
            assert_eq!(destination["lr2BarPart"]["part"], "trophy");
            destination["lr2BarPart"]["indices"][0].as_i64().unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(order, [0, 2, 1]);
    for trophy in trophies {
        let frame = &trophy["dst"][0];
        assert_eq!(frame["x"], 60);
        assert_eq!(frame["y"], -30);
        assert_eq!(frame["w"], 20);
        assert_eq!(frame["h"], 10);
    }
}

#[test]
fn lr2_select_labels_preserve_slots_and_declaration_order() {
    let root = unique_test_dir("lr2-select-labels");
    let path = root.join("select.lr2skin");
    let mut csv = String::from("#INFORMATION,5,test,test\n#RESOLUTION,640,480\n");
    for index in [0, 3, 4, 1, 2, -1, 5] {
        csv.push_str(&format!(
            "#SRC_BAR_LABEL,{index},111,0,0,10,10,1,1,0,0\n#DST_BAR_LABEL,{index},0,20,30,10,10,0,255,255,255,255,1,0,0,0\n"
        ));
    }
    fs::write(&path, csv).unwrap();
    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let labels = loaded.value["songlist"]["label"].as_array().unwrap();
    assert_eq!(labels.len(), 5);
    let order = loaded.value["destination"]
        .as_array()
        .unwrap()
        .iter()
        .map(|destination| {
            assert_eq!(destination["lr2BarPart"]["part"], "label");
            destination["lr2BarPart"]["indices"][0].as_i64().unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(order, [0, 3, 4, 1, 2]);
    for label in labels {
        assert_eq!(label["dst"][0]["x"], 20);
        assert_eq!(label["dst"][0]["y"], -40);
    }
}

#[test]
fn lr2_select_notechart_preserves_source_size_and_interleaved_destinations() {
    let root = unique_test_dir("lr2-select-notechart");
    let path = root.join("select.lr2skin");
    fs::write(
        &path,
        concat!(
            "#INFORMATION,5,test,test\n#RESOLUTION,640,480\n",
            "#SRC_NOTECHART,0,111,0,0,0,0,0,0,0,0,300,120,0,0,300,1,1,1,1\n",
            "#SRC_IMAGE,0,111,0,0,10,10,1,1,0,0\n",
            "#DST_NOTECHART,0,0,50,200,0,0,0,255,255,255,255,1,0,0,0,-1,0,0,0,0,40\n",
            "#DST_NOTECHART,0,150,60,210,0,0,0,255,255,255,255,1,0,0,0\n",
        ),
    )
    .unwrap();
    let loaded = load_lr2_csv_skin_value(&path, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let graph = &loaded.value["judgegraph"][0];
    assert_eq!(graph["delay"], 300);
    assert_eq!(graph["backTexOff"], 1);
    assert_eq!(graph["orderReverse"], 1);
    assert_eq!(graph["noGapX"], 1);
    let destination = &loaded.value["destination"][0];
    assert_eq!(destination["id"], graph["id"]);
    assert_eq!(destination["offset"], 40);
    assert_eq!(destination["dst"].as_array().unwrap().len(), 2);
    for (frame, x, y) in [(0, 50, 280), (1, 60, 270)] {
        assert_eq!(destination["dst"][frame]["x"], x);
        assert_eq!(destination["dst"][frame]["y"], y);
        assert_eq!(destination["dst"][frame]["w"], 300);
        assert_eq!(destination["dst"][frame]["h"], 120);
    }
}

#[test]
fn litone5_select_installed_drawing_definitions_load_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/LITONE5/Select/select.lr2skin");
    if !path.is_file() {
        eprintln!("SKIP missing {}", path.display());
        return;
    }
    let loaded = crate::load_lr2_csv_skin(
        &path,
        crate::SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(loaded.document.judgegraph.len(), 2);
    assert_eq!(loaded.document.songlist.as_ref().unwrap().label.len(), 5);
    assert_eq!(loaded.document.songlist.as_ref().unwrap().trophy.len(), 3);
    assert!(!loaded.warnings.iter().any(|w| w.message.contains("BAR_TROPHY")));
    assert!(!loaded.warnings.iter().any(|w| w.message.contains("BAR_LABEL")));
    assert!(!loaded.warnings.iter().any(|w| w.message.contains("NOTECHART")));
    eprintln!("LITONE5 Select: both note density graphs loaded");
}
