use super::*;

#[test]
fn lua_skin_decode_expands_selected_pm_chara_directory() {
    let library_root = unique_test_dir("bmz-player-pmchara").join("skins");
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
    let library_root = unique_test_dir("bmz-player-json-pmchara").join("skins");
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
