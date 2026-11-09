//! Isolated app probes; no user's profile, database, or audio device is used.
use super::*;

#[cfg(windows)]
#[test]
#[ignore = "isolated Winit event loop; run alone with --release --ignored --nocapture"]
fn profile_skin_entry_wait() -> anyhow::Result<()> {
    use crate::bootstrap::profile_tests::ProfileTestDir;
    use winit::platform::windows::EventLoopBuilderExtWindows;

    anyhow::ensure!(!cfg!(debug_assertions), "run with --release");
    let event_loop = EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build()?;
    let data = ProfileTestDir::new();
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
    )?;
    for delay_ms in [10, 50, 200] {
        let generation = app.skin.skin_pipeline.bump_generation(SkinKind::Play);
        app.skin.skin_pipeline.set_pending(SkinKind::Play, true);
        let tx = app.skin.skin_pipeline.upload_tx.clone();
        let worker = thread::spawn(move || {
            thread::sleep(Duration::from_millis(delay_ms));
            let now = Instant::now();
            tx.send(PendingUploadResult {
                generation,
                path: "injected-skin.json".into(),
                kind: SkinKind::Play,
                queued_at: now,
                decode_started_at: now,
                decode_finished_at: now,
                upload_started_at: now,
                upload_finished_at: now,
                uploaded: Err(anyhow::anyhow!("injected completion")),
            })
            .unwrap();
        });
        let start = Instant::now();
        app.request_scene_skin(SkinKind::Play);
        eprintln!(
            "skin_wait injected_ms={delay_ms} caller_ms={:.3}",
            start.elapsed().as_secs_f64() * 1000.0
        );
        worker.join().unwrap();
        app.drain_pending_skins();
        app.poll_scene_skin_entries();
    }

    // A cold Decide holds its timer until installation, including a newer generation.
    app.play.pending_decide = Some(Box::new(DecideTransition {
        chart_id: 1,
        options: Default::default(),
        launch: DecideLaunch::Play,
        started_at: Instant::now() - Duration::from_secs(10),
        fadeout_started_at: None,
        cancel: false,
        exit: DecideExit::Fadeout,
        snapshot: Default::default(),
        title_override: None,
    }));
    let old = app.skin.skin_pipeline.bump_generation(SkinKind::Decide);
    app.skin.skin_pipeline.set_pending(SkinKind::Decide, true);
    app.request_scene_skin(SkinKind::Decide);
    app.advance_decide_transition();
    assert!(app.play.pending_decide.as_ref().unwrap().fadeout_started_at.is_none());
    app.skin.skin_pipeline.bump_generation(SkinKind::Decide);
    assert!(!app.skin.skin_pipeline.finish_upload(SkinKind::Decide, old));
    app.poll_scene_skin_entries();
    assert!(app.scene_skin_entry_loading());
    app.skin.skin_pipeline.set_pending(SkinKind::Decide, false);
    app.poll_scene_skin_entries();
    assert!(!app.scene_skin_entry_loading());
    assert!(
        app.play.pending_decide.as_ref().unwrap().started_at.elapsed() < Duration::from_secs(1)
    );

    // Cancellation does not wait for a stuck decoder.
    app.skin.skin_pipeline.set_pending(SkinKind::Decide, true);
    app.request_scene_skin(SkinKind::Decide);
    app.play.pending_decide.as_mut().unwrap().cancel = true;
    app.advance_decide_transition();
    assert!(app.play.pending_decide.is_none());

    app.result.finished_play = Some(debug_boot_finished_play_session());
    app.result.result_scene_started_at = Instant::now() - Duration::from_secs(60);
    app.skin.skin_pipeline.set_pending(SkinKind::Result, true);
    app.request_scene_skin(SkinKind::Result);
    assert_eq!(app.current_scene_kind(), AppSceneKind::Result);
    assert!(!app.result_input_ready());
    app.advance_result_exit();
    assert!(app.result.result_exit.is_none());
    app.skin.skin_pipeline.set_pending(SkinKind::Result, false);
    app.poll_scene_skin_entries();
    assert!(app.result.result_scene_started_at.elapsed() < Duration::from_secs(1));
    assert!(!app.scene_skin_entry_loading());
    Ok(())
}
