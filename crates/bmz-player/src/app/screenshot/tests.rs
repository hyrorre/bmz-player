use super::*;

const STAMP: &str = "20261105_203045_123";

fn filename(scene: &AppSceneSnapshot) -> String {
    screenshot_filename(STAMP, &screenshot_name(scene, None), 0)
}

#[test]
fn screenshot_select_uses_open_folder_and_omits_root() {
    let mut scene = bmz_render::sample::sample_select_scene();
    let AppSceneSnapshot::Select(snapshot) = &mut scene else { unreachable!() };
    snapshot.current_folder.clear();
    assert_eq!(filename(&scene), format!("{STAMP}_Music_Select.png"));
    let AppSceneSnapshot::Select(snapshot) = &mut scene else { unreachable!() };
    snapshot.current_folder = "Stella > st0".into();
    // The cursor's chart/folder title must not supply the open folder's name.
    assert_eq!(filename(&scene), format!("{STAMP}_Music_Select_Stella ＞ st0.png"));
}

#[test]
fn screenshot_decide_and_play_include_title_and_optional_subtitle() {
    let snapshot =
        RenderSnapshot {
            title: "曲名".into(), subtitle: "[ANOTHER]".into(), ..Default::default()
        };
    assert_eq!(
        filename(&AppSceneSnapshot::Play(snapshot.clone())),
        format!("{STAMP}_Play_曲名_[ANOTHER].png")
    );
    assert_eq!(
        filename(&AppSceneSnapshot::Decide(snapshot)),
        format!("{STAMP}_Decide_曲名_[ANOTHER].png")
    );
    assert_eq!(screenshot_title("曲名", " "), "曲名");
    assert_eq!(screenshot_title("", "[ANOTHER]"), "[ANOTHER]");
    assert_eq!(screenshot_title(".\n\0.", " "), "Untitled");
}

#[test]
fn screenshot_course_decide_keeps_stage_song_before_course_title_override() {
    let decide = DecideTransition {
        chart_id: 1,
        options: Default::default(),
        launch: DecideLaunch::Play,
        started_at: Instant::now(),
        fadeout_started_at: None,
        cancel: false,
        exit: Default::default(),
        snapshot: RenderSnapshot {
            title: "Stage Song".into(),
            subtitle: "[HYPER]".into(),
            ..Default::default()
        },
        title_override: Some(DecideTitleOverride {
            title: "Course Title".into(),
            subtitle: String::new(),
        }),
    };
    let scene = AppSceneSnapshot::Decide(decide.snapshot_for_render());
    let name = screenshot_name(&scene, Some(&decide.snapshot));
    assert_eq!(
        screenshot_filename(STAMP, &name, 0),
        format!("{STAMP}_Decide_Stage Song_[HYPER].png")
    );
}

#[test]
fn screenshot_result_uses_all_current_clear_labels_including_unsaved_modes() {
    let mut scene = bmz_render::sample::sample_result_scene();
    for (clear, label) in [
        (ClearType::NoPlay, "NoPlay"),
        (ClearType::Failed, "Failed"),
        (ClearType::AssistEasy, "AssistEasy"),
        (ClearType::LightAssistEasy, "LightAssistEasy"),
        (ClearType::Easy, "Easy"),
        (ClearType::Normal, "Normal"),
        (ClearType::Hard, "Hard"),
        (ClearType::ExHard, "ExHard"),
        (ClearType::FullCombo, "FullCombo"),
        (ClearType::Perfect, "Perfect"),
        (ClearType::Max, "Max"),
    ] {
        let AppSceneSnapshot::Result(snapshot) = &mut scene else { unreachable!() };
        snapshot.title = "Played Song".into();
        snapshot.subtitle = "[A]".into();
        snapshot.clear_type = clear;
        snapshot.best_clear_type = Some(ClearType::Max);
        snapshot.score_save_enabled = false;
        snapshot.autoplay = true;
        assert_eq!(filename(&scene), format!("{STAMP}_Played Song_[A]_{label}.png"));
    }
    let AppSceneSnapshot::Result(snapshot) = &mut scene else { unreachable!() };
    snapshot.title = "Course Title".into();
    snapshot.subtitle.clear();
    snapshot.clear_type = ClearType::Hard;
    assert_eq!(filename(&scene), format!("{STAMP}_Course Title_Hard.png"));
}

#[test]
fn screenshot_filename_sanitizes_control_and_path_characters() {
    assert_eq!(
        sanitize_component(" A\\B/C:D*E?F\"G<H>I|J\tK\nL\0M. "),
        "A￥B／C：D＊E？F”G＜H＞I｜J K L M"
    );
    let name = ScreenshotName {
        body: screenshot_title("../CON", ""),
        clear_type: Some(ClearType::Failed),
    };
    let file = screenshot_filename(STAMP, &name, 0);
    assert_eq!(file, format!("{STAMP}_／CON_Failed.png"));
    assert_eq!(Path::new(&file).components().count(), 1);
}

#[test]
fn screenshot_paths_reserve_inflight_names_and_keep_existing_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut runtime = ScreenshotRuntime::default();
    let name = ScreenshotName {
        body: "曲😀".repeat(100),
        clear_type: Some(ClearType::LightAssistEasy),
    };
    let first = runtime.next_path(dir.path(), STAMP, &name);
    let second = runtime.next_path(dir.path(), STAMP, &name);
    assert_ne!(first, second, "readbacks have not created either file yet");
    assert!(second.file_name().unwrap().to_str().unwrap().ends_with("_LightAssistEasy_1.png"));
    std::fs::write(&first, b"existing screenshot").unwrap();
    let third = runtime.next_path(dir.path(), STAMP, &name);
    assert_ne!(third, first);
    assert_ne!(third, second);
    for path in [&first, &second, &third] {
        let file = path.file_name().unwrap().to_str().unwrap();
        assert!(file.len() <= MAX_FILENAME_BYTES);
        assert!(file.starts_with(STAMP));
        assert!(file.contains("_LightAssistEasy"));
    }
    std::fs::write(&second, b"second screenshot").unwrap();
    assert_eq!(std::fs::read(&first).unwrap(), b"existing screenshot");
    assert_eq!(runtime.reserved_paths.len(), 2, "completed paths leave the reservation set");
}

#[test]
fn screenshot_timestamp_has_local_calendar_fields_and_milliseconds() {
    let stamp = screenshot_timestamp(1_709_251_199_123);
    assert_eq!(stamp.len(), 19);
    assert_eq!(&stamp[8..9], "_");
    assert_eq!(&stamp[15..], "_123");
    assert!(stamp.chars().all(|ch| ch.is_ascii_digit() || ch == '_'));
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires an isolated Winit event loop and offscreen GPU; run alone with --ignored"]
fn screenshot_capture_binds_name_to_rendered_frame_after_transition() {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let event_loop =
        EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
    let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
    let mut boot = data.boot();
    boot.app_config.input.gamepad_enabled = false;
    boot.app_config.screenshot.copy_to_clipboard = false;
    let dir = data.paths.data_dir.join("screenshots");
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
    app.renderer.attach_offscreen(SurfaceSize { width: 32, height: 32 }).unwrap();
    app.clear_pending_play_start();
    app.play.pending_decide = None;
    app.viewer_mode = false;
    app.viewer_waiting = false;
    assert_eq!(app.view_state(), AppViewState::Select);
    app.request_manual_screenshot();
    app.screenshots.pending.as_mut().unwrap().stamp = STAMP.into();
    let toast = app.smoke.left_overlay_toast.as_ref().unwrap().message.clone();
    assert!(!toast.is_empty());
    assert_ne!(app.left_overlay_text(), toast);
    assert!(!app.renderer.has_pending_screenshot());
    // Input was received in Select; the next rendered frame is already Result.
    let mut finished = debug_boot_finished_play_session();
    finished.summary.title = "Played Song".into();
    finished.summary.subtitle = "[ANOTHER]".into();
    finished.summary.clear_type = ClearType::Failed;
    finished.summary.best_clear_type = Some(ClearType::Max);
    app.result.finished_play = Some(finished);
    app.render_current_scene();
    let Some(AppSceneSnapshot::Result(rendered)) = app.renderer.last_scene() else {
        panic!("the captured frame must be Result");
    };
    assert_eq!(rendered.title, "Played Song");
    assert_eq!(rendered.clear_type, ClearType::Failed);
    assert_ne!(rendered.overlay.left_text, toast);
    app.flush_pending_screenshots("screenshot regression test");
    let path = dir.join(format!("{STAMP}_Played Song_[ANOTHER]_Failed.png"));
    let png = image::open(&path).unwrap();
    assert_eq!((png.width(), png.height()), (32, 32));
    assert!(app.screenshots.pending.is_none());
    // Explicit smoke paths still bypass manual naming.
    let explicit = dir.join("explicit.png");
    app.smoke.smoke_screenshot_path = Some(explicit.clone());
    app.smoke.smoke_exit_after_frames = Some(1);
    app.render_current_scene();
    app.flush_pending_screenshots("explicit smoke screenshot regression test");
    assert!(explicit.is_file());
    assert!(path.is_file());
}
