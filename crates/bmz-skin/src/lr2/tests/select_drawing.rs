use super::*;

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
fn litone5_select_installed_notecharts_load_when_available() {
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
    assert!(!loaded.warnings.iter().any(|w| w.message.contains("NOTECHART")));
    eprintln!("LITONE5 Select: both note density graphs loaded");
}
