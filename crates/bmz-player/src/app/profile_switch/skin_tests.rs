use super::*;
use crate::bootstrap::profile_tests::ProfileTestDir;
use bmz_render::skin::default_skin_manifest;

fn select_upload(generation: u64, name: &str) -> PendingUploadResult {
    let now = Instant::now();
    let document = serde_json::from_value(serde_json::json!({
        "type": 5, "name": name, "w": 100, "h": 100,
        "text": [{ "id": "name", "constantText": name, "size": 12 }],
        "destination": [{ "id": "name", "dst": [{ "x": 0, "y": 0, "w": 100, "h": 12 }] }]
    }))
    .unwrap();
    PendingUploadResult {
        generation,
        path: "select.json".into(),
        kind: SkinKind::Select,
        queued_at: now,
        decode_started_at: now,
        decode_finished_at: now,
        upload_started_at: now,
        upload_finished_at: now,
        uploaded: Ok(UploadedSkin {
            kind: SkinKind::Select,
            document,
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
fn profile_select_skin_rejects_failure_stale_wrong_scene_and_missing_manifest() {
    let manifest = default_skin_manifest();
    let mut skin = select_upload(3, "new");
    assert!(validate_profile_select_skin(&skin, 3, Some(&manifest)).is_ok());
    assert!(validate_profile_select_skin(&skin, 4, Some(&manifest)).is_err());
    assert!(validate_profile_select_skin(&skin, 3, None).is_err());
    skin.uploaded.as_mut().unwrap().document.skin_type = 0;
    assert!(validate_profile_select_skin(&skin, 3, Some(&manifest)).is_err());
    skin.uploaded = Err(anyhow::anyhow!("decode failed"));
    let error = validate_profile_select_skin(&skin, 3, Some(&manifest)).unwrap_err();
    assert!(error.to_string().contains("decode failed"));
    assert!(error.to_string().contains("select.json"));
}

#[test]
fn profile_select_skin_preparation_uses_target_settings_and_fresh_lua() {
    let data = ProfileTestDir::new();
    let root = data.paths.resource_dir.join("skins/test");
    std::fs::create_dir_all(root.join("parts")).unwrap();
    for name in ["one", "two"] {
        std::fs::write(root.join(format!("parts/{name}.txt")), name).unwrap();
    }
    std::fs::write(root.join("select.luaskin"), r#"
local state = require("main_state")
local skin = {
    type = 5,
    property = {{ name = "Theme", item = {{ name = "One", op = 900 }, { name = "Two", op = 901 }} }},
    filepath = {{ name = "Parts", path = "parts/*.txt" }},
    offset = {{ name = "Panel", id = 42, x = true }},
}
if skin_config == nil then return skin end
skin.name = state.text(2) .. ":" .. skin_config.option.Theme .. ":" .. skin_config.offset.Panel.x
skin.text = {{ id = "file", constantText = skin_config.get_path("parts/*.txt") }}
local count = 0
skin.text[2] = { id = "count", value = function() count = count + 1; return tostring(count) end }
skin.destination = {{ id = "count", dst = {{ x = 0, y = 0, w = 100, h = 12 }} }}
return skin
"#).unwrap();
    let mut pipeline = SkinPipelineRuntime::new();
    let mut renderer = Renderer::default();
    for (index, player, theme, file) in [(0, "First", "One", "one"), (1, "Second", "Two", "two")] {
        let mut profile = ProfileConfig::new_default("test", player, 0);
        profile.skin.select = "resource:skins/test/select.luaskin".into();
        profile.skin.select_options.insert("Theme".into(), theme.into());
        profile.skin.select_files.insert("Parts".into(), format!("parts/{file}.txt"));
        profile.skin.select_offsets.push(SkinOffsetConfig {
            id: 42,
            name: Some("Panel".into()),
            x: index * 10,
            ..Default::default()
        });
        let generation = queue_profile_select_skin(
            &data.paths,
            &profile,
            &mut pipeline,
            bmz_skin::LuaSkinRuntimeMode::Compat,
        )
        .unwrap();
        let result =
            pipeline.decode_rx.as_ref().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(result.generation, generation);
        let decoded = result.result.unwrap();
        assert_eq!(decoded.document.name, format!("{player}:{}:{}", 900 + index, index * 10));
        assert!(decoded.document.text[0].constant_text.ends_with(&format!("{file}.txt")));
        assert!(decoded.lua_runtime.is_some());
        install_decoded_skin(&mut renderer, decoded, default_skin_manifest()).unwrap();
        // 同じLuaファイルを再使用してもclosureは前profileのカウンタを引き継がない。
        for count in [1, 2] {
            renderer.prepare_scene(AppSceneSnapshot::Select(SelectSnapshot::default()));
            assert!(renderer.last_plan().unwrap().commands.iter().any(|command| matches!(
                command, bmz_render::plan::DrawCommand::Text { text, .. } if text == &count.to_string()
            )));
        }
    }
}

#[test]
fn profile_select_skin_preparation_handles_default_missing_and_unsupported_paths() {
    let data = ProfileTestDir::new();
    let default_path = default_skin_document_path_from_paths(&data.paths, SkinKind::Select);
    std::fs::create_dir_all(default_path.parent().unwrap()).unwrap();
    std::fs::write(&default_path, r#"{"type":5,"name":"default"}"#).unwrap();
    let mut pipeline = SkinPipelineRuntime::new();
    let mut profile = ProfileConfig::new_default("test", "Test", 0);
    profile.skin.select.clear();
    queue_profile_select_skin(&data.paths, &profile, &mut pipeline, Default::default()).unwrap();
    let result =
        pipeline.decode_rx.as_ref().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(result.path, default_path);
    assert_eq!(result.result.unwrap().document.name, "default");
    profile.skin.select = "resource:skins/missing.json".into();
    queue_profile_select_skin(&data.paths, &profile, &mut pipeline, Default::default()).unwrap();
    assert!(
        pipeline
            .decode_rx
            .as_ref()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .result
            .is_err()
    );
    profile.skin.select = "resource:skins/unsupported.txt".into();
    assert!(
        queue_profile_select_skin(&data.paths, &profile, &mut pipeline, Default::default())
            .is_err()
    );
}

// WinitAppの実際のpoll/install経路を、window/GPU/audioなしで検証する。
// Windowsはテストスレッドからevent proxyを生成できる。他OSのmain-thread制約は持ち込まない。
#[cfg(target_os = "windows")]
#[test]
fn profile_switch_keeps_rendering_old_skin_until_commit_and_on_failure() {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let event_loop =
        EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
    let data = ProfileTestDir::new();
    let boot = data.boot();
    let (maintenance_tx, _) = tokio::sync::watch::channel(false);
    let mut app = WinitApp::new(
        boot,
        AppOptions { viewer_play: true, ..Default::default() },
        Instant::now(),
        None,
        None,
        Arc::new(AtomicBool::new(false)),
        event_loop.create_proxy(),
        LogBuffer::default(),
        maintenance_tx,
        None,
    )
    .unwrap();
    app.skin.default_skin_manifest = Some(default_skin_manifest());
    assert!(app.apply_uploaded_skin(select_upload(0, "old")));

    let assert_frame = |app: &mut WinitApp, profile: &str, skin: &str| {
        assert_eq!(app.boot.profile_config.id, profile);
        assert_eq!(app.renderer.select_skin_document().unwrap().name, skin);
        app.renderer.prepare_scene(AppSceneSnapshot::Select(SelectSnapshot::default()));
        let commands = &app.renderer.last_plan().unwrap().commands;
        assert!(commands.iter().any(|command| matches!(command,
            bmz_render::plan::DrawCommand::Text { text, .. } if text == skin)));
        assert!(!commands.iter().any(|command| matches!(command,
            bmz_render::plan::DrawCommand::Text { text, .. } if text == "SELECT")));
    };
    let begin = |app: &mut WinitApp, action: ProfileManagerAction| {
        let prepared = prepare_profile_action(&data.paths, &action).unwrap().unwrap();
        let generation = app.skin.skin_pipeline.bump_generation(SkinKind::Select);
        app.skin.skin_pipeline.set_pending(SkinKind::Select, true);
        app.jobs.profile_change = Some(PendingProfileChange {
            action,
            stage: ProfileChangeStage::WaitingForSkin {
                prepared: Box::new(prepared),
                generation,
                uploaded: None,
            },
        });
        generation
    };
    let generation = begin(
        &mut app,
        ProfileManagerAction::Create { id: "next".into(), display_name: None, activate: true },
    );
    for _ in 0..3 {
        app.poll_profile_change();
        assert!(app.jobs.profile_change.is_some());
        assert_frame(&mut app, "default", "old");
    }
    // 古い結果は保留中の切替も画面も上書きしない。
    assert!(!app.apply_uploaded_skin(select_upload(generation - 1, "stale")));
    app.poll_profile_change();
    assert!(app.jobs.profile_change.is_some());
    assert!(!app.apply_uploaded_skin(select_upload(generation, "new")));
    assert_frame(&mut app, "default", "old");
    app.poll_profile_change();
    assert!(app.jobs.profile_change.is_none());
    assert_frame(&mut app, "next", "new");
    assert!(!app.select.select_scene_timer_armed);
    assert_eq!(
        crate::config::load::load_app_config(&data.paths.config_toml).unwrap().active_profile,
        "next"
    );

    let generation = begin(&mut app, ProfileManagerAction::Switch("default".into()));
    let mut failure = select_upload(generation, "failed");
    failure.uploaded = Err(anyhow::anyhow!("decode failed"));
    assert!(!app.apply_uploaded_skin(failure));
    app.poll_profile_change();
    assert!(app.jobs.profile_change.is_none());
    assert!(!app.skin.skin_pipeline.is_pending(SkinKind::Select));
    assert_frame(&mut app, "next", "new");

    let generation = begin(&mut app, ProfileManagerAction::Switch("default".into()));
    assert!(!app.apply_uploaded_skin(select_upload(generation, "unsaved")));
    app.boot.app_paths.config_toml = data.paths.data_dir.join("config-is-directory");
    std::fs::create_dir(&app.boot.app_paths.config_toml).unwrap();
    app.poll_profile_change();
    assert!(app.jobs.profile_change.is_none());
    assert_frame(&mut app, "next", "new");
    assert_eq!(
        crate::config::load::load_app_config(&data.paths.config_toml).unwrap().active_profile,
        "next"
    );
}
