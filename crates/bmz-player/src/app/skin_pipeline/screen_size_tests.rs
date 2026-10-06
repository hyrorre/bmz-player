use super::*;
use crate::skin_loader::UploadedSkin;
use std::time::Instant;

fn request(pipeline: &SkinPipelineRuntime, path: &str) -> SkinDecodeRequest {
    SkinDecodeRequest::new(
        pipeline.generation(SkinKind::Select),
        path.into(),
        SkinKind::Select,
        BTreeMap::new(),
        BTreeMap::new(),
        bmz_skin::LuaLoadRuntimeState { screen_size: pipeline.screen_size, ..Default::default() },
    )
}

fn uploaded(pipeline: &SkinPipelineRuntime, size: Option<[u32; 2]>) -> PendingUploadResult {
    let now = Instant::now();
    PendingUploadResult {
        generation: pipeline.generation(SkinKind::Select),
        path: "skin.lua".into(),
        kind: SkinKind::Select,
        queued_at: now,
        decode_started_at: now,
        decode_finished_at: now,
        upload_started_at: now,
        upload_finished_at: now,
        uploaded: Ok(UploadedSkin {
            load_dependencies: bmz_skin::SkinLoadDependencies {
                screen_size: size,
                ..Default::default()
            },
            kind: SkinKind::Select,
            document: serde_json::from_value(serde_json::json!({"type":5})).unwrap(),
            lua_runtime: None,
            fonts: Vec::new(),
            prepared: Vec::new(),
            audio_assets: Vec::new(),
            decode_stats: Default::default(),
            upload_stats: Default::default(),
        }),
    }
}

#[test]
fn screen_dimensions_recover_initial_failure_and_reject_stale_uploads() {
    let mut pipeline = SkinPipelineRuntime::new();
    pipeline.record_screen_request(request(&pipeline, "skin.lua"));
    let mut failed = uploaded(&pipeline, None);
    failed.uploaded = Err(anyhow::anyhow!("screen_height was zero before attachment"));
    pipeline.record_load_result(&failed.path, Some("failed before attachment".into()));
    assert!(!pipeline.screen_load_needs_refresh(SkinKind::Select));
    pipeline.screen_size = [1920, 1080];
    assert!(pipeline.screen_load_needs_refresh(SkinKind::Select));
    assert!(pipeline.uploaded_screen_size_is_stale(&failed));
    assert!(pipeline.uploaded_screen_size_is_stale(&uploaded(&pipeline, Some([0, 0]))));
    assert!(!pipeline.uploaded_screen_size_is_stale(&uploaded(&pipeline, None)));
    pipeline.record_screen_request(request(&pipeline, "skin.lua"));
    assert!(
        !pipeline.uploaded_screen_size_is_stale(&failed),
        "do not retry an ordinary failure forever"
    );

    let old = uploaded(&pipeline, Some([1920, 1080]));
    pipeline.screen_size = [800, 600];
    assert!(pipeline.uploaded_screen_size_is_stale(&old));
    pipeline.bump_generation(SkinKind::Select);
    pipeline.set_pending(SkinKind::Select, true);
    assert!(!pipeline.finish_upload(SkinKind::Select, old.generation));
    assert!(pipeline.is_pending(SkinKind::Select));
    let b = uploaded(&pipeline, Some([800, 600]));
    pipeline.screen_size = [1920, 1080];
    assert!(pipeline.uploaded_screen_size_is_stale(&b), "A -> B -> A must not install B");
}

#[test]
fn screen_dimensions_do_not_inherit_dependencies_from_an_old_profile_or_path() {
    let mut pipeline = SkinPipelineRuntime::new();
    pipeline.screen_size = [1920, 1080];
    pipeline.record_screen_request(request(&pipeline, "old-profile.lua"));
    pipeline.record_screen_dependencies(
        SkinKind::Select,
        bmz_skin::SkinLoadDependencies {
            screen_size: Some(pipeline.screen_size),
            random_file_paths: BTreeMap::from([("bg/*".into(), vec!["old-profile.png".into()])]),
            ..Default::default()
        },
    );
    pipeline.screen_size = [800, 600];
    assert!(pipeline.screen_load_needs_refresh(SkinKind::Select));
    assert!(pipeline.screen_dependencies(SkinKind::Select).is_some());
    pipeline.bump_generation(SkinKind::Select);
    let mut next = request(&pipeline, "new-profile.lua");
    next.runtime_state
        .pinned_random_file_paths
        .insert("bg/*".into(), vec!["new-profile.png".into()]);
    pipeline.record_screen_request(next);
    assert!(pipeline.screen_dependencies(SkinKind::Select).is_none());
    assert!(!pipeline.screen_load_needs_refresh(SkinKind::Select));
    assert_eq!(
        pipeline.screen_request(SkinKind::Select).unwrap().runtime_state.pinned_random_file_paths["bg/*"],
        ["new-profile.png"]
    );
}

#[test]
fn screen_dimensions_retry_real_lua_after_zero_height_startup_failure() {
    use crate::app::skin_loading::apply_json_skin_sync;
    use crate::app::spawn_skin_decode;
    use crate::bootstrap::profile_tests::ProfileTestDir;
    let data = ProfileTestDir::new();
    std::fs::create_dir_all(&data.paths.data_dir).unwrap();
    let path = data.paths.data_dir.join("size.lua");
    std::fs::write(
        &path,
        r#"
        local s = require('main_state')
        local aspect = s.screen_width() / s.screen_height()
        assert(aspect == aspect, 'screen_height was zero')
        return {type=5,name=tostring(aspect)}
    "#,
    )
    .unwrap();
    let mut renderer = bmz_render::renderer::Renderer::default();
    let mut pipeline = SkinPipelineRuntime::new();
    apply_json_skin_sync(
        &mut renderer,
        &pipeline,
        &data.paths,
        &path,
        SkinKind::Select,
        Some(&bmz_render::skin::default_skin_manifest()),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &Default::default(),
    );
    assert!(pipeline.load_error(&path).is_some());
    pipeline.screen_size = [800, 600];
    assert!(pipeline.screen_load_needs_refresh(SkinKind::Select));
    let mut retry = pipeline.screen_request(SkinKind::Select).unwrap();
    retry.generation = pipeline.bump_generation(SkinKind::Select);
    spawn_skin_decode(&pipeline, retry);
    let decoded = pipeline
        .decode_rx
        .as_ref()
        .unwrap()
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .result
        .unwrap();
    assert_eq!(decoded.load_dependencies.screen_size, Some([800, 600]));
    assert!(decoded.document.name.starts_with("1.333"));
}
