use super::*;

fn write_pm_character(directory: &Path, file: &str, width: i32) {
    fs::create_dir_all(directory).unwrap();
    fs::write(
        directory.join(file),
        format!(
            "#CharBMP character.png\n#Anime 100\n#Size {width} 1\n#00 0 0 1 1\n#Pattern 01 00\n"
        ),
    )
    .unwrap();
    image::RgbaImage::from_pixel(1, 1, image::Rgba([30, 60, 90, 255]))
        .save(directory.join("character.png"))
        .unwrap();
}

fn decode_pm_fixture(
    entry: &Path,
    roots: &[PathBuf],
    pattern: &str,
    selected: Option<&str>,
) -> DecodedSkin {
    fs::create_dir_all(entry.parent().unwrap()).unwrap();
    fs::write(
        entry,
        serde_json::json!({
            "type": 0,
            "filepath": [{"name": "character", "path": pattern}],
            "source": [{"id": "character-source", "path": pattern}],
            "pmchara": [{"id": "pm", "src": "character-source", "type": 0, "color": 1}],
            "destination": [{"id": "pm", "dst": [{"x": 0, "y": 0, "w": 1, "h": 1}]}]
        })
        .to_string(),
    )
    .unwrap();
    let files =
        selected.into_iter().map(|value| ("character".to_owned(), value.to_owned())).collect();
    decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        skin_path: entry,
        kind: SkinKind::Play,
        options: &BTreeMap::new(),
        files: &files,
        runtime_state: &LuaLoadRuntimeState::default(),
        library_roots: roots,
        pinned_sources: None,
        document_cache: None,
        source_cache: None,
        texture_cache: None,
        font_cache: None,
        installed_fonts: None,
    })
    .unwrap()
}

fn assert_pm_expanded(decoded: &DecodedSkin, width: i32, reference: &str) {
    let runtime = decoded.document.pmchara[0]
        .runtime
        .as_ref()
        .unwrap_or_else(|| panic!("PMchara runtime missing for {reference}"));
    assert_eq!(runtime.canvas_width, width, "{reference}");
    let source_id = &runtime.motions[0].source_id;
    assert!(decoded.document.source.iter().any(|source| &source.id == source_id), "{reference}");
    assert!(decoded.sources.iter().any(|source| &source.source_id == source_id), "{reference}");
}

#[test]
fn json_pmchara_resolves_direct_case_variants_and_prefers_exact_names() {
    let temp = unique_test_dir("bmz-json-pmchara-case");
    let library = temp.join("Skins");
    let entry = library.join("Theme/System/play.json");
    write_pm_character(&library.join("Theme/Chara/Alice"), "Avatar.CHP", 21);
    write_pm_character(&library.join("Theme/Chara/日本語"), "Portrait.CHP", 22);
    write_pm_character(&library.join("Other/Chara/Alice"), "Avatar.CHP", 23);
    for (path, width) in [
        ("../chara/alice", 21),
        ("../chara/alice/avatar.chp", 21),
        ("../Chara/Alice/Avatar.CHP", 21),
        ("../chara/日本語/portrait.chp", 22),
        ("skin/other/chara/alice", 23),
        ("skin/old-theme-name/chara/alice", 21),
    ] {
        assert_pm_expanded(
            &decode_pm_fixture(&entry, std::slice::from_ref(&library), path, None),
            width,
            path,
        );
    }
    // A case-sensitive filesystem can contain both names in the same directory.
    let other_case = library.join("Theme/Chara/ALICE");
    if !other_case.exists() {
        write_pm_character(&other_case, "Avatar.CHP", 24);
        let path = "../Chara/ALICE/Avatar.CHP";
        assert_pm_expanded(&decode_pm_fixture(&entry, &[library], path, None), 24, path);
    }
}

#[test]
fn json_pmchara_resolves_wildcard_parents_nested_suffixes_and_saved_choices() {
    let temp = unique_test_dir("bmz-json-pmchara-wildcard-case");
    let library = temp.join("Skins");
    let entry = library.join("Theme/System/play.json");
    write_pm_character(&library.join("Theme/Chara/Alice/Parts"), "Avatar.CHP", 21);
    write_pm_character(&library.join("Theme/Chara/Bob/Parts"), "Avatar.CHP", 22);
    let pattern = "../chara/*/parts/avatar.chp";
    for (selected, width) in
        [(None, 21), (Some("../CHARA/bob/parts/avatar.chp"), 22), (Some("bOb"), 22)]
    {
        let decoded = decode_pm_fixture(&entry, std::slice::from_ref(&library), pattern, selected);
        assert_pm_expanded(&decoded, width, selected.unwrap_or(pattern));
    }
}

#[test]
#[cfg(unix)]
fn json_pmchara_case_lookup_obeys_roots_aliases_and_symlink_boundaries() {
    use std::os::unix::fs::symlink;
    let temp = unique_test_dir("bmz-json-pmchara-case-boundary");
    let library = temp.join("Skins");
    let alias = temp.join("SkinAlias");
    let outside = temp.join("Outside");
    let entry = alias.join("Theme/play.json");
    let inside = library.join("Theme/Chara/Alice");
    write_pm_character(&inside, "Avatar.CHP", 21);
    write_pm_character(&outside, "Avatar.CHP", 22);
    symlink(&library, &alias).unwrap();
    symlink(&inside, library.join("Theme/InsideLink")).unwrap();
    symlink(&outside, library.join("Theme/OutsideLink")).unwrap();
    symlink(&outside, library.join("Theme/OnlyOutside")).unwrap();
    fs::create_dir(library.join("Theme/MemberPool")).unwrap();
    symlink(&outside, library.join("Theme/MemberPool/Outward")).unwrap();
    symlink(&inside, outside.join("Return")).unwrap();
    symlink(outside.join("Avatar.CHP"), inside.join("Escape.CHP")).unwrap();

    let roots = std::slice::from_ref(&alias);
    let via_alias = alias.join("theme/insidelink/avatar.chp").to_string_lossy().into_owned();
    for path in ["insidelink", via_alias.as_str()] {
        assert_pm_expanded(&decode_pm_fixture(&entry, roots, path, None), 21, path);
    }
    let absolute_outside = outside.join("Avatar.CHP").to_string_lossy().into_owned();
    for path in [
        absolute_outside.as_str(),
        "../../outside/avatar.chp",
        "outsidelink/avatar.chp",
        "onlyoutside/*.chp",
        "memberpool/*/avatar.chp",
        "OutsideLink/Return/Avatar.CHP",
        "chara/alice/escape.chp",
    ] {
        let decoded = decode_pm_fixture(&entry, roots, path, None);
        assert!(decoded.document.pmchara[0].runtime.is_none(), "{path}");
        assert!(decoded.sources.is_empty(), "{path}");
    }
    let selected = decode_pm_fixture(&entry, roots, "chara/*", Some(&absolute_outside));
    assert_pm_expanded(&selected, 21, "outward selection falls back to a permitted character");
    let granted_path = outside.join("avatar.chp").to_string_lossy().into_owned();
    let granted = decode_pm_fixture(&entry, &[alias, outside], &granted_path, None);
    assert_pm_expanded(&granted, 22, &granted_path);
}

#[test]
fn lua_skin_decode_expands_selected_pm_chara_directory() {
    let library_root_dir = unique_test_dir("bmz-player-pmchara");
    let library_root = library_root_dir.join("skins");
    let entry_dir = library_root.join("theme/system");
    let character_dir = library_root.join("theme/customize/pm/default");
    fs::create_dir_all(&entry_dir).unwrap();
    fs::create_dir_all(&character_dir).unwrap();
    let entry = entry_dir.join("play7.luaskin");
    fs::write(
        &entry,
        r#"
            return {
                type = 0,
                source = {
                    { id = "pm-source", path = "../customize/pm/*" }
                },
                pmchara = {
                    { id = "pm", src = "pm-source", color = 1, type = 0, side = 1 }
                },
                destination = {
                    { id = "pm", dst = {{ x = 0, y = 0, w = 20, h = 30 }} }
                }
            }
        "#,
    )
    .unwrap();
    fs::write(
        character_dir.join("sample.chp"),
        b"#CharBMP character.png\n#Anime 100\n#Size 20 30\n#00 0 0 20 30\n#Pattern 01 00\n",
    )
    .unwrap();
    let source_png =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/default/note.png");
    fs::copy(source_png, character_dir.join("character.png")).unwrap();

    let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        pinned_sources: None,
        skin_path: &entry,
        kind: SkinKind::Play,
        options: &BTreeMap::new(),
        files: &BTreeMap::new(),
        runtime_state: &LuaLoadRuntimeState::default(),
        document_cache: None,
        source_cache: None,
        texture_cache: None,
        font_cache: None,
        installed_fonts: None,
        library_roots: &[library_root],
    })
    .unwrap();

    let runtime = decoded.document.pmchara[0].runtime.as_ref().expect("expanded PMchara");
    assert_eq!(runtime.motions.len(), 1);
    let source_id = &runtime.motions[0].source_id;
    assert!(decoded.sources.iter().any(|source| &source.source_id == source_id));
}

#[test]
fn json_skin_decode_expands_pm_chara_directory_default() {
    let library_root_dir = unique_test_dir("bmz-player-json-pmchara");
    let library_root = library_root_dir.join("skins");
    let entry_dir = library_root.join("theme/system");
    let character_dir = library_root.join("theme/customize/pm/default");
    fs::create_dir_all(&entry_dir).unwrap();
    fs::create_dir_all(&character_dir).unwrap();
    let entry = entry_dir.join("play7.json");
    fs::write(
        &entry,
        r#"{
            "type": 0,
            "filepath": [
                { "name": "pm", "path": "../customize/pm/*", "def": "default" }
            ],
            "source": [
                { "id": "pm-source", "path": "../customize/pm/*" }
            ],
            "pmchara": [
                { "id": "pm", "src": "pm-source", "color": 1, "type": 0, "side": 1 }
            ],
            "destination": [
                { "id": "pm", "dst": [{ "x": 0, "y": 0, "w": 20, "h": 30 }] }
            ]
        }"#,
    )
    .unwrap();
    fs::write(
        character_dir.join("sample.chp"),
        b"#CharBMP character.png\n#Anime 100\n#Size 20 30\n#00 0 0 20 30\n#Pattern 01 00\n",
    )
    .unwrap();
    let source_png =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/default/note.png");
    fs::copy(source_png, character_dir.join("character.png")).unwrap();

    let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        pinned_sources: None,
        skin_path: &entry,
        kind: SkinKind::Play,
        options: &BTreeMap::new(),
        files: &BTreeMap::new(),
        runtime_state: &LuaLoadRuntimeState::default(),
        document_cache: None,
        source_cache: None,
        texture_cache: None,
        font_cache: None,
        installed_fonts: None,
        library_roots: &[library_root],
    })
    .unwrap();

    assert!(decoded.document.pmchara[0].runtime.is_some());
}

#[test]
fn real_simple_play_loads_bga_gauge_mascots_and_pm_chara_when_available() {
    let data_skins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins");
    let entry = data_skins.join("simple-play-simple/system/play7.luaskin");
    if !entry.is_file() {
        return;
    }

    let files =
        BTreeMap::from([("ぽみゅキャラ Pmchara".to_string(), "PMchara_sample".to_string())]);
    let decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        pinned_sources: None,
        skin_path: &entry,
        kind: SkinKind::Play,
        options: &BTreeMap::new(),
        files: &files,
        runtime_state: &LuaLoadRuntimeState::default(),
        document_cache: None,
        source_cache: None,
        texture_cache: None,
        font_cache: None,
        installed_fonts: None,
        library_roots: std::slice::from_ref(&data_skins),
    })
    .unwrap();

    assert!(decoded.document.bga.is_some());
    assert!(decoded.document.gauge.as_ref().is_some_and(|gauge| !gauge.nodes.is_empty()));
    assert!(decoded.document.image.iter().any(|image| image.id == "mascot"));
    assert!(decoded.document.image.iter().any(|image| image.id == "movingmascot"));
    let pmchara = decoded
        .document
        .pmchara
        .iter()
        .find(|pmchara| pmchara.id == "pmchara")
        .expect("simple-play PMchara definition");
    let runtime = pmchara.runtime.as_ref().expect("expanded simple-play PMchara");
    assert!(runtime.motions.iter().any(|motion| motion.motion == 1));
    assert!(runtime.motions.iter().all(|motion| {
        decoded.sources.iter().any(|source| source.source_id == motion.source_id)
    }));
}
