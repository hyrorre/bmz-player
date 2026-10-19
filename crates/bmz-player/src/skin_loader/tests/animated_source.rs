use super::*;

fn copy_fixture(root: &Path, fixture: &str, name: &str) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/songs/bga-compat");
    fs::copy(source.join(fixture), root.join(name)).unwrap();
}

#[test]
fn json_skin_sources_turn_animations_into_beatoraja_sprite_sheets() {
    let root = unique_test_dir("animated-skin-source");
    fs::create_dir_all(&root).unwrap();
    copy_fixture(&root, "animated.gif", "anim.gif");
    copy_fixture(&root, "webp_anim.webp", "anim.webp");
    copy_fixture(&root, "apng_anim.png", "anim.png");
    copy_fixture(&root, "webp_still.webp", "still.webp");
    let path = root.join("play.json");
    fs::write(
        &path,
        r#"{
            "type": 0, "w": 320, "h": 180,
            "source": [
                { "id": "gif", "path": "anim.gif" },
                { "id": "webp", "path": "anim.webp" },
                { "id": "apng", "path": "anim.png" },
                { "id": "still", "path": "still.webp" }
            ],
            "image": [
                { "id": "gif-image", "src": "gif", "x": 0, "y": 0, "w": -1, "h": -1, "divy": 2, "cycle": 200 },
                { "id": "webp-image", "src": "webp", "x": 0, "y": 0, "w": -1, "h": -1, "divy": 3, "cycle": 300 },
                { "id": "apng-image", "src": "apng", "x": 0, "y": 0, "w": -1, "h": -1, "divy": 3, "cycle": 300 },
                { "id": "still-image", "src": "still", "x": 0, "y": 0, "w": -1, "h": -1 }
            ],
            "destination": [
                { "id": "gif-image", "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] },
                { "id": "webp-image", "dst": [{ "x": 20, "y": 0, "w": 10, "h": 10 }] },
                { "id": "apng-image", "dst": [{ "x": 40, "y": 0, "w": 10, "h": 10 }] },
                { "id": "still-image", "dst": [{ "x": 60, "y": 0, "w": 10, "h": 10 }] }
            ]
        }"#,
    )
    .unwrap();

    let decoded = decode_beatoraja_skin(&path, SkinKind::Play).unwrap();

    let size = |id: &str| {
        let source = decoded
            .sources
            .iter()
            .find(|source| source.source_id == id)
            .unwrap_or_else(|| panic!("source {id} was not decoded"));
        (source.size.width, source.size.height)
    };
    // Animated sources keep their frames; the sheet is laid out for the GPU at upload, and
    // the provisional size already matches it for small frames. Two 2x2 frames stack into
    // one column; three frames into a 1x3 grid.
    assert_eq!(size("gif"), (2.0, 4.0));
    assert_eq!(size("webp"), (2.0, 6.0));
    assert_eq!(size("apng"), (2.0, 6.0));
    assert_eq!(size("still"), (2.0, 2.0));
    for source in &decoded.sources {
        let animated = source.source_id != "still";
        assert_eq!(source.animation.is_some(), animated, "{}", source.source_id);
        assert_eq!(source.asset.is_none(), animated, "{}", source.source_id);
    }
    assert_eq!(
        decoded
            .sources
            .iter()
            .find(|source| source.source_id == "webp")
            .unwrap()
            .animation
            .as_ref()
            .unwrap()
            .frames()
            .len(),
        3
    );

    // Without a GPU the sheet waits for the device; the skin still installs every source.
    let mut renderer = Renderer::default();
    install_decoded_skin(&mut renderer, decoded, bmz_render::skin::default_skin_manifest())
        .unwrap();
}

#[test]
fn lr2_image_sources_accept_webp() {
    let root = unique_test_dir("animated-skin-source-lr2");
    fs::create_dir_all(&root).unwrap();
    copy_fixture(&root, "webp_anim.webp", "anim.webp");
    let path = root.join("play.lr2skin");
    fs::write(
        &path,
        "#INFORMATION,0,Animated,Author\n#RESOLUTION,100,100\n#IMAGE,anim.webp\n\
         #SRC_IMAGE,0,0,0,0,-1,-1,1,3,300,0\n\
         #DST_IMAGE,0,0,0,0,10,10,0,255,255,255,255,0,0,0,0,0,0,0,0,0,0\n",
    )
    .unwrap();

    let decoded = decode_beatoraja_skin(&path, SkinKind::Play).unwrap();

    let source = decoded.sources.iter().find(|source| source.source_id == "0").unwrap();
    assert_eq!((source.size.width, source.size.height), (2.0, 6.0));
}
