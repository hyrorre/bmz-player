use super::*;

#[test]
fn qwilight_lane_covers_are_copied_into_the_shared_folder_without_overwriting() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("picked");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("Cover.png"), b"first").unwrap();
    fs::write(source.join("cover.txt"), b"text").unwrap();
    let other = temp.path().join("other");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("Cover.png"), b"second").unwrap();
    let paths =
        AppPaths::from_dirs(temp.path().into(), temp.path().into(), "cache".into(), "logs".into());
    let dir = qwilight_lane_cover_dir(&paths);
    assert_eq!(dir, temp.path().join("skins").join(bmz_skin::QWILIGHT_LANE_COVER_DIR));

    assert_eq!(add_qwilight_lane_covers(&dir, &[source.join("Cover.png")]).unwrap(), ["Cover.png"]);
    // The same image is reused; a different one with that name gets a new name.
    let names =
        add_qwilight_lane_covers(&dir, &[source.join("Cover.png"), other.join("Cover.png")])
            .unwrap();
    assert_eq!(names, ["Cover.png", "Cover (2).png"]);
    assert_eq!(fs::read(dir.join("Cover.png")).unwrap(), b"first");
    assert_eq!(fs::read(dir.join("Cover (2).png")).unwrap(), b"second");
    assert!(add_qwilight_lane_covers(&dir, &[source.join("cover.txt")]).is_err());
    assert!(!dir.join("cover.txt").exists());

    // Only BMZ's own copy is removed; names that leave the folder are rejected.
    assert!(remove_qwilight_lane_cover(&dir, "../picked/Cover.png").is_err());
    assert!(source.join("Cover.png").exists());
    remove_qwilight_lane_cover(&dir, "Cover.png").unwrap();
    assert!(!dir.join("Cover.png").exists());
    assert!(source.join("Cover.png").exists());
    assert!(remove_qwilight_lane_cover(&dir, "Cover.png").is_err());
}

#[test]
fn qwilight_property_labels_follow_ui_locale_fallback() {
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "property": [{"name": "ギア", "def": "Left", "item": [
            {"name": "Left", "op": 1}, {"name": "Right", "op": 2}
        ]}],
        "propertyLabels": {"ギア": {
            "name": {"en": "Gear", "ko": "기어"},
            "items": {"Right": {"ko": "오른쪽", "en": ""}}
        }}
    }))
    .unwrap();
    let defs = SceneSkinDefs::from_qwilight_play_document(Some(&document));
    assert_eq!(defs.property_label("ギア", AppLocale::Ja), "ギア");
    assert_eq!(defs.property_label("ギア", AppLocale::ZhHk), "Gear");
    assert_eq!(defs.property_label("ギア", AppLocale::Ko), "기어");
    assert_eq!(defs.item_label("ギア", "Right", AppLocale::Ko), "오른쪽");
    // Blank text falls back to the saved name.
    assert_eq!(defs.item_label("ギア", "Right", AppLocale::En), "Right");
    assert_eq!(defs.item_label("ギア", "Left", AppLocale::Ko), "Left");
    assert!(defs.qwilight);
    assert!(defs.offset.iter().all(|offset| ![33, SKIN_OFFSET_BAR_LINE].contains(&offset.id)));
}
