use super::*;
use crate::bootstrap::profile_tests::ProfileTestDir;

#[test]
fn result_favorites_share_select_bindings_and_common_action_priority() {
    let mut profile = ProfileConfig::new_default("default", "Default", 1);
    let keyboard = |name: &str| PhysicalControl::KeyboardKey(name.into());
    let keys = SelectKeyBindings::from_profile(&profile.input);
    assert_eq!(result_favorite_kind(&keyboard("F8"), &keys), Some(ResultFavoriteKind::Song));
    assert_eq!(result_favorite_kind(&keyboard("F9"), &keys), Some(ResultFavoriteKind::Chart));
    assert_eq!(result_favorite_kind(&keyboard("R"), &keys), None);
    for entry in &mut profile.input.ui.bindings {
        if entry.action == Some(InputActionConfig::SelectFavoriteSong) {
            entry.control = "F6".into();
        }
        if entry.action == Some(InputActionConfig::SelectFavoriteChart) {
            entry.device = "gamepad1".into();
            entry.control = "Button20".into();
        }
    }
    let keys = SelectKeyBindings::from_profile(&profile.input);
    assert_eq!(result_favorite_kind(&keyboard("F8"), &keys), None);
    assert_eq!(result_favorite_kind(&keyboard("F6"), &keys), Some(ResultFavoriteKind::Song));
    assert_eq!(
        result_favorite_kind(&PhysicalControl::GamepadButton("Button20".into()), &keys),
        Some(ResultFavoriteKind::Chart)
    );
    for entry in &mut profile.input.ui.bindings {
        if entry.action == Some(InputActionConfig::E1) {
            entry.control = "F6".into();
        }
    }
    let keys = SelectKeyBindings::from_profile(&profile.input);
    assert_eq!(result_favorite_kind(&keyboard("F6"), &keys), None);
}

#[test]
fn result_favorite_target_keeps_played_copy_and_rejects_missing_or_changed_chart_id() {
    let data = ProfileTestDir::new();
    let (boot, path, copy) = super::boot_chart::registered_charts(&data);
    let id = boot.library_db.chart_id_by_chart_file_path(&path).unwrap().unwrap();
    let copy_id = boot.library_db.chart_id_by_chart_file_path(&copy).unwrap().unwrap();
    let mut finished = debug_boot_finished_play_session();
    finished.result.chart_sha256 = boot.library_db.chart_sha256_by_chart_id(id).unwrap().unwrap();
    for (chart_id, file) in [(id, path), (copy_id, copy)] {
        let hints = result_favorite_hints(&boot.library_db, &finished, Some(chart_id)).unwrap();
        assert_eq!(Path::new(&hints.folder), file.parent().unwrap());
        assert_eq!(hints.title, finished.summary.title);
    }
    for chart_id in [None, Some(i64::MAX)] {
        assert!(
            result_favorite_hints(&boot.library_db, &finished, chart_id).unwrap().folder.is_empty()
        );
    }
    finished.result.chart_sha256 = [99; 32];
    assert!(
        result_favorite_hints(&boot.library_db, &finished, Some(id)).unwrap().folder.is_empty()
    );
}

#[test]
fn result_song_favorite_removes_other_chart_representatives_and_survives_reopening() {
    let data = ProfileTestDir::new();
    let (mut boot, path, _) = super::boot_chart::registered_charts(&data);
    let alt_path = path.with_file_name("another.bms");
    std::fs::write(&alt_path, "#TITLE Another\n#BPM 150\n#00012:01\n").unwrap();
    crate::storage::scan::scan_song_roots(
        &mut boot.library_db,
        &boot.app_config.songs.roots,
        &boot.app_config.scan,
        2,
        false,
    )
    .unwrap();
    let id = boot.library_db.chart_id_by_chart_file_path(&path).unwrap().unwrap();
    let alt_id = boot.library_db.chart_id_by_chart_file_path(&alt_path).unwrap().unwrap();
    let sha = boot.library_db.chart_sha256_by_chart_id(id).unwrap().unwrap();
    let alt_sha = boot.library_db.chart_sha256_by_chart_id(alt_id).unwrap().unwrap();
    assert_ne!(sha, alt_sha);
    let mut finished = debug_boot_finished_play_session();
    finished.result.chart_sha256 = sha;
    let hints = result_favorite_hints(&boot.library_db, &finished, Some(id)).unwrap();
    boot.collection_db.upsert_favorite_song(alt_sha, &hints, 1).unwrap();
    boot.collection_db.upsert_favorite_song(sha, &hints, 1).unwrap();
    assert!(
        !toggle_favorite_song_for_hints(&boot.library_db, &mut boot.collection_db, sha, &hints, 2)
            .unwrap()
    );
    assert!(boot.collection_db.favorite_song_records().unwrap().is_empty());
    assert!(
        toggle_favorite_song_for_hints(&boot.library_db, &mut boot.collection_db, sha, &hints, 3)
            .unwrap()
    );
    let reopened = data.boot();
    let records = reopened.collection_db.favorite_song_records().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].representative_sha256, sha);
    assert_eq!(records[0].origin_folder_hint, hints.folder);
    assert_eq!(
        favorite_song_representatives_for_folder(
            &reopened.library_db,
            &reopened.collection_db,
            &hints.folder
        )
        .unwrap(),
        vec![sha]
    );
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires an isolated Winit event loop and offscreen GPU; run alone with --ignored"]
fn result_favorites_route_keys_skin_events_and_course_boundaries_without_skin_buttons() {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let event_loop =
        EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
    let data = ProfileTestDir::new();
    let (mut boot, path, _) = super::boot_chart::registered_charts(&data);
    boot.app_config.input.gamepad_enabled = false;
    let chart_id = boot.library_db.chart_id_by_chart_file_path(&path).unwrap().unwrap();
    let sha = boot.library_db.chart_sha256_by_chart_id(chart_id).unwrap().unwrap();
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
    app.renderer
        .attach_offscreen(bmz_render::renderer::SurfaceSize { width: 32, height: 32 })
        .unwrap();
    app.viewer_mode = false;
    app.viewer_waiting = false;
    app.clear_pending_play_start();
    app.result.finished_play = Some(debug_boot_finished_play_session());
    app.result.finished_play.as_mut().unwrap().result.chart_sha256 = sha;
    app.play.last_started_chart_id = Some(chart_id);
    // Random Select leaves its executable row, rather than the played chart, under the cursor.
    app.select.select_items =
        vec![SelectItem::Executable(crate::screens::select_model::SelectExecutableRow {
            title: "Random Select".into(),
            kind: crate::screens::select_model::SelectExecutableKind::RandomSelect,
            chart_ids: vec![chart_id],
        })];
    app.select.selected_index = 0;
    assert!(app.selected_chart_row().is_none());
    crate::skin_loader::set_decoded_skin_context(
        &mut app.renderer,
        SkinKind::Result,
        default_skin_manifest(),
        serde_json::from_value(serde_json::json!({"type": 7, "input": 2000, "scene": 0})).unwrap(),
        None,
        vec![],
        false,
    );
    app.result.result_scene_started_at = Instant::now() - Duration::from_secs(60);
    app.refresh_result_favorites();
    assert!(!app.result.result_favorite_song);
    assert!(!app.result.result_favorite_chart);
    let song = PhysicalControl::KeyboardKey("F8".into());
    let chart = PhysicalControl::KeyboardKey("F9".into());
    assert!(app.handle_result_control(&song, true, false, false));
    assert!(app.handle_result_control(&chart, true, false, false));
    assert!(app.result.result_favorite_song);
    assert!(app.result.result_favorite_chart);
    assert!(app.result.result_exit.is_none());
    for (pressed, repeat) in [(false, false), (true, true)] {
        assert!(app.handle_result_control(&song, pressed, repeat, false));
        assert!(app.handle_result_control(&chart, pressed, repeat, false));
    }
    app.refresh_result_favorites();
    assert!(app.result.result_favorite_song && app.result.result_favorite_chart);
    let AppSceneSnapshot::Result(snapshot) = app.scene_snapshot() else {
        panic!("result snapshot");
    };
    assert!(snapshot.favorite_song && snapshot.favorite_chart);
    app.execute_result_skin_event(89);
    app.execute_result_skin_event(90);
    assert!(!app.result.result_favorite_song && !app.result.result_favorite_chart);
    assert!(app.handle_course_intermediate_control(&song, true, false, false));
    assert!(app.handle_course_intermediate_control(&chart, true, false, false));
    assert!(app.result.result_exit.is_none(), "favorites must not advance the course");
    assert!(app.result.result_favorite_song && app.result.result_favorite_chart);

    app.result.result_scene_started_at = Instant::now();
    assert!(!app.result_input_ready());
    app.handle_result_control(&song, true, false, false);
    app.execute_result_skin_event(90);
    assert!(app.result.result_favorite_song && app.result.result_favorite_chart);
    app.result.result_scene_started_at = Instant::now() - Duration::from_secs(60);
    app.result.result_exit = Some(ResultExit {
        started_at: Instant::now(),
        action: ResultExitAction::Leave,
        skip_requested: false,
        skip_final_frame_held: false,
    });
    app.handle_result_control(&song, true, false, false);
    app.execute_result_skin_event(90);
    assert!(app.result.result_favorite_song && app.result.result_favorite_chart);
    app.result.result_exit = None;

    app.result.finished_course = Some(CourseResultSummary {
        course_id: 1,
        course_score_id: None,
        course_played_at: None,
        ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
        rule_mode: RuleMode::Beatoraja,
        title: "Course".into(),
        kind: bmz_core::course::CourseKind::Dan,
        course_titles: Default::default(),
        stage_charts: vec![],
        entry_summaries: vec![],
        entry_arranges: vec![],
        total_ex_score: 0,
        max_ex_score: 0,
        total_notes: 0,
        course_ln_mode: None,
        bp: 0,
        final_clear_type: ClearType::NoPlay,
        final_gauge_type: GaugeType::Normal,
        final_gauge_value: 0.0,
        course_max_combo: 0,
        judge_counts: Default::default(),
        trophy_results: vec![],
        course_clear: false,
        course_failed: false,
        total_entries: 1,
        played_entries: 1,
        replay_slots: [false; 4],
        saved_replay_slots: [false; 4],
        best_score: None,
        previous_best_score: None,
    });
    app.refresh_result_favorites();
    assert!(!app.result.result_favorite_song && !app.result.result_favorite_chart);
    app.handle_course_result_control(&song, true, false, false);
    app.handle_course_result_control(&chart, true, false, false);
    app.execute_result_skin_event(89);
    app.execute_result_skin_event(90);
    assert!(app.boot.collection_db.is_favorite_chart(sha).unwrap());
    assert!(app.boot.collection_db.is_favorite_song_representative(sha).unwrap());
    app.leave_result();
    assert!(app.select.select_items.iter().any(|item| {
        matches!(item, SelectItem::Folder { path, .. } if path == crate::screens::select_model::FAVORITE_ROOT_PATH)
    }));
    assert!(!app.result.result_favorite_song && !app.result.result_favorite_chart);
}
