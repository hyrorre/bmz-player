use super::*;

/// The embedded skin settings as ECBE lays them out: seven customize buttons and
/// a list slider (type 7) moving down 154px from a 34px handle at y=242.
#[test]
fn lr2_skin_settings_slider_rows_and_area_follow_lr2() {
    let root = unique_test_dir("bmz-lr2-select-skin-settings");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("select.lr2skin");
    let mut csv = String::from(
        "#INFORMATION,5,test,test\n#RESOLUTION,1280,720\n#IMAGE,a.png\n\
        #SRC_SLIDER,0,0,0,0,46,34,1,1,0,0,2,154,7\n\
        #DST_SLIDER,0,0,720,242,48,34,0,255,255,255,255,0,0,0,0,0,0,0,0,0\n",
    );
    for row in 0..7 {
        csv.push_str(&format!(
            "#SRC_BUTTON,0,0,0,0,0,0,0,0,0,0,{},1,0\n\
            #DST_BUTTON,0,0,555,{},160,14,0,255,255,255,255,0,0,0,0,0,0,0,0,0\n",
            220 + row,
            254 + 24 * row,
        ));
    }
    fs::write(&path, csv).unwrap();
    let document = load_skin_document_uncached(
        &path,
        SkinKind::Select,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &LuaLoadRuntimeState::default(),
    )
    .unwrap()
    .document;

    // The skin shows seven rows, so ten items scroll by three.
    assert_eq!(document.lr2_skin_settings_rows(), Some(7));
    let settings = bmz_render::scene::Lr2SkinSettingsSnapshot {
        items: vec![(String::new(), String::new()); 10],
        visible_rows: 7,
        ..Default::default()
    };
    assert_eq!(settings.max_scroll(), 3);

    let snapshot = SelectSnapshot {
        time: TimeUs(1_500_000),
        lr2_skin_settings: Some(settings),
        ..Default::default()
    };
    let index = Default::default();
    let point = |x: f32, y: f32| (x / 1280.0, y / 720.0);
    let hit = |(x, y): (f32, f32)| {
        document
            .select_slider_hit(&snapshot, &index, x, y)
            .map(|hit| (hit.slider_type, (hit.value * 1000.0).round() / 1000.0))
    };
    // LR2: the handle is part of the track and the value follows its center.
    assert_eq!(hit(point(744.0, 244.0)), Some((7, 0.0)));
    assert_eq!(hit(point(744.0, 259.0 + 77.0)), Some((7, 0.5)));
    assert_eq!(hit(point(744.0, 428.0)), Some((7, 1.0)));
    assert_eq!(hit(point(744.0, 440.0)), None);
    // A held slider follows the cursor outside its track, clamped to the ends.
    let drag = |(x, y): (f32, f32)| {
        document.select_slider_drag(&snapshot, &index, 7, x, y).map(|hit| hit.value)
    };
    assert_eq!(drag(point(100.0, 700.0)), Some(1.0));
    assert_eq!(drag(point(100.0, 10.0)), Some(0.0));
    assert_eq!(document.select_slider_drag(&snapshot, &index, 1, 0.5, 0.5), None);

    // The settings area joins the rows and the slider's travel.
    let contains =
        |(x, y): (f32, f32)| document.select_lr2_skin_settings_contains(&snapshot, &index, x, y);
    assert!(contains(point(600.0, 300.0)));
    assert!(contains(point(744.0, 420.0)));
    assert!(!contains(point(900.0, 300.0)));
    let closed = SelectSnapshot { lr2_skin_settings: None, ..snapshot.clone() };
    assert!(!document.select_lr2_skin_settings_contains(&closed, &index, 600.0 / 1280.0, 0.4));
}
