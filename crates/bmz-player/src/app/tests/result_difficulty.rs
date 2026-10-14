use super::*;
use bmz_render::skin::{DestinationListEntry, SkinDocument};
use std::sync::Mutex;

fn difficulty_runtime(summary: &ResultSummary) -> bmz_skin::LuaLoadRuntimeState {
    let mut runtime = lua_runtime_state_for_result(
        false,
        None,
        true,
        false,
        summary.key_mode,
        result_lua_runtime_number_values_for_summary(summary),
        "test",
    );
    apply_skin_attempt_lua_load_state(&mut runtime, summary.skin_attempt);
    apply_result_summary_lua_load_state(&mut runtime, summary, "", "", "");
    runtime
}

#[test]
fn result_load_difficulty_conditions_replace_previous_summary() {
    let mut summary = debug_boot_result_summary();
    let mut runtime = difficulty_runtime(&summary);
    for (label, expected) in [
        ("4", 154),
        ("5", 155),
        ("", 150),
        ("unknown", 150),
        ("0", 150),
        ("6", 150),
        ("1", 151),
        ("2", 152),
        ("3", 153),
        ("BEGINNER", 151),
        ("normal", 152),
        ("Hyper", 153),
        (" ANOTHER ", 154),
        ("insane", 155),
        ("", 150),
    ] {
        summary.difficulty_name = label.to_string();
        apply_result_summary_lua_load_state(&mut runtime, &summary, "", "", "");
        for option in 150..=155 {
            assert_eq!(
                runtime.option_values.get(&option),
                Some(&(option == expected)),
                "{label:?}"
            );
        }
    }
}

#[test]
fn result_difficulty_asset_cache_follows_app_summary_for_normal_and_course() {
    let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
    std::fs::create_dir_all(&data.paths.data_dir).unwrap();
    image::RgbaImage::new(6, 1).save(data.paths.data_dir.join("difficulty.png")).unwrap();
    let mut summary = debug_boot_result_summary();
    for skin_type in [7, 15] {
        let kind = SkinKind::Result;
        let path = data.paths.data_dir.join(format!("difficulty-{skin_type}.lr2skin"));
        let mut csv = format!("#INFORMATION,{skin_type},difficulty,test\n#IMAGE,difficulty.png\n");
        for difficulty in 0..=5 {
            csv.push_str(&format!(
                "#IF,{}\n#SRC_IMAGE,0,0,{difficulty},0,1,1,1,1,0,0\n#ENDIF\n",
                150 + difficulty
            ));
        }
        csv.push_str("#DST_IMAGE,0,0,0,0,1,1\n");
        std::fs::write(&path, csv).unwrap();
        let cache = Arc::new(Mutex::new(crate::skin_loader::SkinDocumentCache::default()));
        for (label, expected, cache_hit) in [
            ("4", 4, false),
            ("ANOTHER", 4, true),
            ("5", 5, false),
            ("INSANE", 5, true),
            ("", 0, false),
        ] {
            summary.difficulty_name = label.to_string();
            let decoded =
                crate::skin_loader::decode_beatoraja_skin_with_options_and_runtime_state_and_caches(
                    &path,
                    kind,
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    &difficulty_runtime(&summary),
                    Some(cache.clone()),
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap();
            assert_eq!(decoded.document.image.len(), 1, "{kind:?} {label:?}");
            assert_eq!(decoded.document.image[0].x, expected, "{kind:?} {label:?}");
            assert_eq!(decoded.document.destination.len(), 1);
            assert_eq!(decoded.stats.document_cache_hits, usize::from(cache_hit));
        }
    }
}

fn wmix_result_path() -> PathBuf {
    std::env::var_os("BMZ_TEST_LR2_SKIN_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/skins"))
        .join("WMIX_HD/result/WMIX_RESULT.lr2skin")
}

fn wmix_options(side: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("side".to_string(), side.to_string()),
        ("clear type playskin link".to_string(), "LINK OFF".to_string()),
    ])
}

fn assert_wmix_difficulty(document: &SkinDocument, y: i32) {
    let images = document
        .image
        .iter()
        .filter(|image| image.src == "0" && image.x == 1377 && image.w == 242 && image.h == 41)
        .collect::<Vec<_>>();
    assert_eq!(images.len(), 1, "WMIX must load exactly one difficulty label");
    assert_eq!(images[0].y, y);
    assert!(document.destination.iter().any(|entry| matches!(entry,
        DestinationListEntry::Single(destination) if destination.id == images[0].id
    )));
}

#[test]
fn result_difficulty_wmix_selects_labels_from_app_state_when_available() {
    let path = wmix_result_path();
    if !path.is_file() {
        eprintln!("SKIP missing {}", path.display());
        return;
    }
    let mut summary = debug_boot_result_summary();
    for side in ["Left", "Right"] {
        for (label, y) in [("4", 496), ("ANOTHER", 496), ("5", 454), ("INSANE", 454)] {
            summary.difficulty_name = label.to_string();
            let loaded = bmz_skin::load_lr2_csv_skin_with_runtime_state(
                &path,
                bmz_skin::SkinKind::Result,
                &wmix_options(side),
                &BTreeMap::new(),
                &difficulty_runtime(&summary),
            )
            .unwrap();
            assert_wmix_difficulty(&loaded.document, y);
            for option in 150..=155 {
                assert_eq!(
                    loaded.dependencies.option_values.get(&option),
                    Some(&(option == if y == 496 { 154 } else { 155 }))
                );
            }
            eprintln!("WMIX {side} {label}: difficulty atlas y={y}");
        }
    }
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires WMIX assets, a GPU and a Winit event loop; run alone with --ignored"]
fn result_difficulty_wmix_gpu_previews_use_full_app_state() {
    use winit::platform::windows::EventLoopBuilderExtWindows;

    let path = wmix_result_path();
    assert!(path.is_file(), "WMIX assets are required: {}", path.display());
    let event_loop =
        EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
    let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
    let mut boot = data.boot();
    boot.app_config.input.gamepad_enabled = false;
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
    app.viewer_mode = false;
    app.result.finished_play = Some(debug_boot_finished_play_session());
    app.result.result_scene_started_at = Instant::now() - Duration::from_secs(5);
    app.renderer
        .attach_offscreen(bmz_render::renderer::SurfaceSize { width: 1280, height: 720 })
        .unwrap();
    let output = std::env::var_os("BMZ_TEST_PREVIEW_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("bmz-wmix-result-difficulty"));
    std::fs::create_dir_all(&output).unwrap();
    for side in ["Left", "Right"] {
        for (label, y) in [("ANOTHER", 496), ("INSANE", 454)] {
            app.result.finished_play.as_mut().unwrap().summary.difficulty_name = label.to_string();
            let runtime = app.result_lua_runtime_state(ResultSkinSlot::Normal, false, None);
            let decoded = crate::skin_loader::decode_beatoraja_skin_with_options_and_runtime_state(
                &path,
                SkinKind::Result,
                &wmix_options(side),
                &BTreeMap::new(),
                &runtime,
            )
            .unwrap();
            assert_wmix_difficulty(&decoded.document, y);
            crate::skin_loader::install_decoded_skin(
                &mut app.renderer,
                decoded,
                default_skin_manifest(),
            )
            .unwrap();
            let scene = app.scene_snapshot();
            app.renderer.render_scene(scene).unwrap();
            image::RgbaImage::from_raw(1280, 720, app.renderer.read_offscreen_rgba().unwrap())
                .unwrap()
                .save(output.join(format!("result-{side}-{label}.png")))
                .unwrap();
        }
    }
    eprintln!("WMIX Result previews: {}", output.display());
}
