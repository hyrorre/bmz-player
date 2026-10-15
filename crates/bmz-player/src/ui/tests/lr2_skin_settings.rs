use super::*;

fn candidate(path: &str) -> SkinCandidate {
    SkinCandidate {
        name: path.to_string(),
        path: path.to_string(),
        origin: SkinCandidateOrigin::User,
    }
}

#[test]
fn lr2_skin_targets_map_to_bmz_slots() {
    for button in [170, 171, 172, 173, 174, 175, 176, 177, 182, 183, 185] {
        let target = Lr2SkinTarget::from_button(button).expect("mapped");
        assert_eq!(target.button(), button);
    }
    // KEYCONFIG / SKINSELECT / SOUNDSET / THEME / BATTLE9 have no BMZ skin slot.
    for button in [178, 179, 180, 181, 184] {
        assert_eq!(Lr2SkinTarget::from_button(button), None, "button {button}");
    }
    assert!(Lr2SkinTarget::from_button(170).unwrap().is_play());
    assert!(!Lr2SkinTarget::from_button(175).unwrap().is_play());
}

#[test]
fn lr2_skin_change_wraps_through_default_and_candidates_keeping_history() {
    let target = Lr2SkinTarget::from_button(175).unwrap();
    let catalog = SkinCatalog {
        select: vec![candidate("data:skins/a.lr2skin"), candidate("data:skins/b.lr2skin")],
        ..Default::default()
    };
    let mut skin = SkinConfig { select: "data:skins/a.lr2skin".to_string(), ..Default::default() };
    skin.select_options.insert("Layout".to_string(), "HD".to_string());

    assert!(target.cycle_skin(&mut skin, &catalog, 1));
    assert_eq!(skin.select, "data:skins/b.lr2skin");
    assert!(skin.select_options.is_empty(), "b has no saved options");
    assert!(target.cycle_skin(&mut skin, &catalog, 1));
    assert_eq!(skin.select, "", "wraps to BMZ's default skin");
    assert!(target.cycle_skin(&mut skin, &catalog, -1));
    assert!(target.cycle_skin(&mut skin, &catalog, -1));
    assert_eq!(skin.select, "data:skins/a.lr2skin");
    assert_eq!(skin.select_options.get("Layout").map(String::as_str), Some("HD"));
}

#[test]
fn lr2_skin_items_list_options_then_files_and_cycle_with_wraparound() {
    let root = unique_test_dir("bmz-ui-lr2-skin-items");
    let skin_dir = root.join("data").join("skins").join("t");
    fs::create_dir_all(skin_dir.join("notes")).unwrap();
    for name in ["a.png", "b.png"] {
        fs::write(skin_dir.join("notes").join(name), b"").unwrap();
    }
    fs::write(skin_dir.join("select.lr2skin"), "").unwrap();
    let app_paths = AppPaths::from_dirs(
        root.join("resources"),
        root.join("data"),
        root.join("cache"),
        root.join("logs"),
    );
    let defs = SceneSkinDefs {
        property: vec![SkinPropertyDef {
            category: String::new(),
            name: "Lane".to_string(),
            item: vec![
                bmz_render::skin::SkinPropertyItemDef { name: "Off".to_string(), op: 900 },
                bmz_render::skin::SkinPropertyItemDef { name: "On".to_string(), op: 901 },
            ],
            def: "On".to_string(),
        }],
        filepath: vec![SkinFilepathDef {
            category: String::new(),
            name: "Notes".to_string(),
            path: "notes/*.png".to_string(),
            def: "b".to_string(),
        }],
        ..Default::default()
    };
    let target = Lr2SkinTarget::from_button(175).unwrap();
    let mut skin =
        SkinConfig { select: "data:skins/t/select.lr2skin".to_string(), ..Default::default() };
    let rows = |skin: &SkinConfig| {
        lr2_skin_items(target, &defs, skin, &app_paths)
            .into_iter()
            .map(|item| (item.name, item.value))
            .collect::<Vec<_>>()
    };
    // Unsaved values show the skin defaults.
    assert_eq!(rows(&skin), vec![("Lane".into(), "On".into()), ("Notes".into(), "b.png".into())]);
    // Options wrap around.
    assert!(lr2_cycle_skin_item(target, &defs, &mut skin, &app_paths, 0, 1));
    assert_eq!(skin.select_options.get("Lane").map(String::as_str), Some("Off"));
    // Files step through RANDOM and the matching files.
    assert!(lr2_cycle_skin_item(target, &defs, &mut skin, &app_paths, 1, 1));
    assert_eq!(rows(&skin)[1].1, "RANDOM");
    assert!(lr2_cycle_skin_item(target, &defs, &mut skin, &app_paths, 1, 1));
    assert_eq!(rows(&skin)[1].1, "a.png");
    assert!(!lr2_cycle_skin_item(target, &defs, &mut skin, &app_paths, 5, 1));

    // A single choice never changes, so nothing is saved and the skin is not reloaded.
    let single = SceneSkinDefs {
        property: vec![SkinPropertyDef {
            category: String::new(),
            name: "Only".to_string(),
            item: vec![bmz_render::skin::SkinPropertyItemDef { name: "On".to_string(), op: 900 }],
            def: "On".to_string(),
        }],
        ..Default::default()
    };
    assert!(!lr2_cycle_skin_item(target, &single, &mut skin, &app_paths, 0, 1));
    assert!(!skin.select_options.contains_key("Only"));
}
