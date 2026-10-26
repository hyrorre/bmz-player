use super::*;
use crate::bootstrap::profile_tests::ProfileTestDir;

#[test]
fn initial_select_is_queued_before_surface_and_not_installed_synchronously() {
    let data = ProfileTestDir::new();
    std::fs::create_dir_all(&data.paths.data_dir).unwrap();
    let path = data.paths.data_dir.join("initial-select.json");
    std::fs::write(&path, r#"{"type":5,"name":"initial asynchronous select"}"#).unwrap();
    let mut renderer = Renderer::default();
    let mut pipeline = SkinPipelineRuntime::new();
    let skin = SkinConfig { select: path.to_string_lossy().into_owned(), ..Default::default() };
    let (_, _, pending_select, _, _) = load_initial_skin_textures(
        &mut renderer,
        &data.paths,
        &pipeline,
        0,
        "test",
        None,
        &skin,
        true,
        bmz_skin::LuaSkinRuntimeMode::default(),
        false,
    );
    assert!(pending_select);
    assert!(renderer.select_skin_document().is_none());
    assert_eq!(pipeline.screen_request(SkinKind::Select).unwrap().path, path);
    loop {
        let result =
            pipeline.decode_rx.as_ref().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
        if result.kind == SkinKind::Select {
            assert_eq!(result.result.unwrap().document.name, "initial asynchronous select");
            break;
        }
    }
    pipeline.shutdown_workers();
}

#[test]
fn frontend_reload_reuses_installed_fonts_after_cache_eviction_and_reloads_changes() {
    let data = ProfileTestDir::new();
    std::fs::create_dir_all(&data.paths.data_dir).unwrap();
    let path = data.paths.data_dir.join("decide.json");
    let font = data.paths.data_dir.join("font.ttf");
    std::fs::write(&font, b"font payload").unwrap();
    std::fs::write(&path, r#"{"type":6,"font":[{"id":"shared","path":"font.ttf"}]}"#).unwrap();
    let skin = SkinConfig { decide: path.to_string_lossy().into_owned(), ..Default::default() };
    let mut pipeline = SkinPipelineRuntime::new();
    for attempt in 0..3 {
        if attempt == 1 {
            // The renderer still owns this font even after its CPU payload is evicted.
            *pipeline.font_cache.lock().unwrap() = Default::default();
        } else if attempt == 2 {
            std::fs::write(&font, b"changed font payload").unwrap();
        }
        let (_, pending_decide, _) = reload_skin_textures(
            &data.paths,
            &mut pipeline,
            SkinReloadRequest { decide: true, ..Default::default() },
            "test",
            None,
            &skin,
            bmz_skin::LuaSkinRuntimeMode::default(),
            false,
            None,
            None,
        );
        assert!(pending_decide);
        let decoded = pipeline
            .decode_rx
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .result
            .unwrap();
        assert_eq!(decoded.stats.font_payload_skipped, usize::from(attempt == 1));
        let font = &decoded.fonts[0];
        assert_eq!(font.data.is_none(), attempt == 1);
        pipeline
            .installed_font_cache
            .insert(font.stored_id.clone(), font.cache_key.clone().unwrap());
    }
    pipeline.shutdown_workers();
}
