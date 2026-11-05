use super::*;

fn simple_play_hit_error() -> Option<DecodedSkin> {
    let skin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/skins/simple-play-simple/system/play7.luaskin");
    if !skin_path.is_file() {
        return None;
    }
    let files = BTreeMap::from([(
        "ヒットエラービジュアライザ HitErrorVisualizer".into(),
        "default".into(),
    )]);
    let mut decoded = decode_beatoraja_skin_request(BeatorajaSkinDecodeRequest {
        pinned_sources: None,
        skin_path: &skin_path,
        kind: SkinKind::Play,
        options: &BTreeMap::new(),
        files: &files,
        runtime_state: &LuaLoadRuntimeState::default(),
        document_cache: None,
        source_cache: None,
        texture_cache: None,
        font_cache: None,
        installed_fonts: None,
        library_roots: &test_app_paths().skin_library_roots(),
    })
    .expect("decode unmodified Simple Play Simple with HitErrorVisualizer enabled");
    // Exercise the original visualizer through the real loader, independently
    // of unrelated images/fonts/notes in the external skin.
    decoded.document.destination.retain(|entry| {
        matches!(entry, DestinationListEntry::Single(destination) if destination.id == "hiterrorvisualizer")
    });
    assert_eq!(decoded.document.destination.len(), 1);
    assert_eq!(decoded.document.hiterror_visualizer.len(), 1);
    Some(decoded)
}

#[test]
fn simple_play_hit_error_fits_its_original_destination_when_available() {
    let Some(decoded) = simple_play_hit_error() else { return };
    let mut state = SkinDrawState::default();
    state.hit_error_ring[1] = 20;
    state.hit_error_ring_index = 1;
    DynamicTimerRuntime::default().advance(&decoded.document, &mut state, 0);
    let items = decoded.document.static_image_render_items(&HashMap::new(), &state);
    assert_eq!(items.len(), 3, "default hit, centre and EMA");
    for item in items {
        let SkinRenderItem::Rect { rect, .. } = item else { panic!("expected a line") };
        assert!(rect.x >= 126.0 / 1920.0 && rect.x + rect.width <= 598.0 / 1920.0 + 0.000001);
        assert!((rect.y - 888.0 / 1080.0).abs() < 0.000001);
        assert!((rect.height - 12.0 / 1080.0).abs() < 0.000001);
    }
}

#[test]
#[ignore = "requires installed Simple Play Simple assets and an offscreen GPU"]
fn simple_play_hit_error_gpu_changes_only_its_original_destination() {
    use bmz_render::renderer::SurfaceSize;

    let decoded = simple_play_hit_error().expect("Simple Play Simple assets are required");
    let mut renderer = Renderer::default();
    renderer.attach_offscreen(SurfaceSize { width: 1920, height: 1080 }).unwrap();
    set_decoded_skin_context(
        &mut renderer,
        SkinKind::Play,
        SkinManifest::default(),
        decoded.document,
        decoded.lua_runtime,
        Vec::new(),
        false,
    );
    let AppSceneSnapshot::Play(mut snapshot) = bmz_render::sample::sample_play_scene() else {
        panic!("expected Play sample")
    };
    snapshot.resources_loaded = true;
    snapshot.recent_judgements.clear();
    snapshot.hit_error_ring = Default::default();
    renderer.prepare_scene(AppSceneSnapshot::Play(snapshot.clone()));
    renderer.render_last_plan().unwrap();
    let before = renderer.read_offscreen_rgba().unwrap();
    snapshot.hit_error_ring.index = 1;
    snapshot.hit_error_ring.values[1] = 20;
    renderer.prepare_scene(AppSceneSnapshot::Play(snapshot));
    renderer.render_last_plan().unwrap();
    let after = renderer.read_offscreen_rgba().unwrap();
    let mut changed = 0;
    for (index, (before, after)) in
        before.as_chunks::<4>().0.iter().zip(after.as_chunks::<4>().0).enumerate()
    {
        if before != after {
            changed += 1;
            let (x, y) = (index % 1920, index / 1920);
            assert!((126..598).contains(&x) && (888..900).contains(&y), "changed {x},{y}");
        }
    }
    assert!(changed > 0, "a real hit and the updated EMA must change GPU pixels");
}
