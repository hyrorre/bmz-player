use super::frame_stalls::wait_for_select_list;
use super::*;
use crate::bootstrap::profile_tests::ProfileTestDir;
use winit::platform::windows::EventLoopBuilderExtWindows;

fn select_app(mut boot: BootstrappedApp, event_loop: &EventLoop<AppUserEvent>) -> Box<WinitApp> {
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
    app.viewer_waiting = false;
    app
}

fn finish_unchanged_scan(app: &mut WinitApp, roots: &[PathEntry]) {
    let (tx, finished) = mpsc::channel();
    app.jobs.song_scan.pending = Some(PendingSongScan {
        finished,
        progress: Arc::new(AtomicU64::new(0)),
        library_roots: Some(roots.to_vec()),
        scope: SongScanScope::Startup,
        download_ids: Vec::new(),
        download_dirs: Vec::new(),
    });
    tx.send(Ok(SongScanResult { report: Default::default(), downloaded: Vec::new() })).unwrap();
    app.poll_pending_song_scan();
}

#[test]
#[ignore = "creates an isolated Winit event loop; run alone"]
fn startup_scan_refreshes_removed_root_even_when_no_charts_changed() {
    let event_loop =
        EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
    let data = ProfileTestDir::new();
    let (mut boot, chart, _) = super::boot_chart::registered_charts(&data);
    // The fixture adds native paths directly. Normal startup loads their saved spelling.
    crate::config::app_config::normalize_song_root_paths(&mut boot.app_config.songs.roots);
    let mut app = select_app(boot, &event_loop);
    app.save_app_config_now().unwrap();
    let roots = crate::songs_cmd::configured_library_roots(&app.boot.app_config, &data.paths);
    let song_root = app.boot.app_config.songs.roots[0].path.clone();
    let contains_root = |app: &WinitApp| {
        app.select
            .select_items
            .iter()
            .any(|item| matches!(item, SelectItem::Folder { path, .. } if path == &song_root))
    };
    app.reload_select_items();
    wait_for_select_list(&mut app);
    assert!(contains_root(&app));
    finish_unchanged_scan(&mut app, &roots);
    assert!(!app.select.list_loader.loading());

    let mut saved = app.boot.app_config.clone();
    saved.songs.roots.clear();
    crate::config::save::save_app_config(&data.paths.config_toml, &saved).unwrap();
    finish_unchanged_scan(&mut app, &roots);
    assert!(app.select.list_loader.loading());
    wait_for_select_list(&mut app);
    assert!(contains_root(&app));

    app.boot.app_config.songs.roots.clear();
    app.save_app_config_now().unwrap();
    app.publish_song_scope().unwrap();
    finish_unchanged_scan(&mut app, &roots);
    wait_for_select_list(&mut app);
    assert!(!contains_root(&app));
    assert!(app.boot.library_db.chart_id_by_chart_file_path(&chart).unwrap().is_some());
}

#[test]
#[ignore = "creates an isolated Winit event loop; run alone"]
fn startup_course_repair_updates_the_open_course_list_once() {
    let event_loop =
        EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
    let data = ProfileTestDir::new();
    let (mut boot, path, _) = super::boot_chart::registered_charts(&data);
    let id = boot.library_db.chart_id_by_chart_file_path(&path).unwrap().unwrap();
    let sha = boot.library_db.chart_sha256_by_chart_id(id).unwrap().unwrap();
    let course = super::course_sources::course_definition(id, sha);
    let course_id = boot.library_db.upsert_course("test", &course, 0, 1).unwrap();
    boot.library_db
        .conn()
        .execute("UPDATE course_entries SET chart_id = NULL WHERE course_id = ?1", [course_id])
        .unwrap();
    let mut app = select_app(boot, &event_loop);
    app.change_select_list(SelectListAction::Enter(COURSE_ROOT_PATH.into()));
    wait_for_select_list(&mut app);
    let resolved = |app: &WinitApp| {
        app.select
            .select_items
            .iter()
            .find_map(|item| match item {
                SelectItem::Course(row) if row.course_id == course_id => Some(row.resolved_count),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(resolved(&app), 0);

    for expected in [2, 0] {
        let repair =
            crate::storage::migration::repair_course_entry_chart_links_once(&data.paths.library_db)
                .unwrap();
        assert_eq!(repair.repaired_entries, expected);
        let (tx, rx) = mpsc::channel();
        app.jobs.pending_course_link_repair = Some(rx);
        tx.send(Ok(repair)).unwrap();
        app.poll_pending_course_link_repair();
        assert_eq!(app.select.list_loader.loading(), expected > 0);
        wait_for_select_list(&mut app);
        assert_eq!(resolved(&app), 2);
    }
}
