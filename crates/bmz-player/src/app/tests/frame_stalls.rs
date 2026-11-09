//! Isolated app probes; no user's profile, database, or audio device is used.
use super::*;

#[test]
#[ignore = "release selected-chart DB probe; run alone with --ignored --nocapture"]
fn profile_selected_chart_queries() -> anyhow::Result<()> {
    anyhow::ensure!(!cfg!(debug_assertions), "run with --release");
    let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
    let (boot, path, _) = super::boot_chart::registered_charts(&data);
    let folder = path.parent().unwrap().to_string_lossy();
    let chart = boot.library_db.list_charts_in_folder(&folder)?.remove(0);
    let play = &boot.profile_config.play;
    let key = ScoreKey::new(
        chart.sha256,
        crate::ln_policy::score_ln_policy(play.ln_mode_policy, chart.ln_profile),
    )
    .with_rule_mode(play.rule_mode);
    for name in ["same_folder", "clear_history", "replay_slots"] {
        let mut samples = Vec::new();
        for iteration in 0..1010 {
            let start = Instant::now();
            match name {
                "same_folder" => {
                    std::hint::black_box(crate::screens::select_model::same_folder_difficulties(
                        &boot.library_db,
                        &boot.score_db,
                        &folder,
                        play.ln_mode_policy,
                        play.rule_mode,
                    )?);
                }
                "clear_history" => {
                    std::hint::black_box(crate::screens::select_model::lr2_clear_option_history(
                        &boot.score_db,
                        &chart,
                        play.ln_mode_policy,
                        play.rule_mode,
                    )?);
                }
                _ => {
                    std::hint::black_box(boot.score_db.replay_slots_for_chart(key)?);
                }
            }
            if iteration >= 10 {
                samples.push(start.elapsed().as_secs_f64() * 1e6);
            }
        }
        samples.sort_by(f64::total_cmp);
        eprintln!(
            "select query={name} median_us={:.3} p95_us={:.3} max_us={:.3}",
            samples[500], samples[950], samples[999]
        );
    }
    Ok(())
}

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

#[cfg(windows)]
#[test]
#[ignore = "release Winit/database probe; run alone with --ignored --nocapture"]
fn profile_select_list_dispatch_and_install() -> anyhow::Result<()> {
    use crate::bootstrap::profile_tests::ProfileTestDir;
    use winit::platform::windows::EventLoopBuilderExtWindows;
    anyhow::ensure!(!cfg!(debug_assertions), "run with --release");
    let event_loop = EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build()?;
    for count in [1_000, 10_000, 50_000] {
        let data = ProfileTestDir::new();
        let mut boot = data.boot();
        boot.app_config.input.gamepad_enabled = false;
        let table = crate::table_cmd::profile::synthetic_table(count);
        crate::table_cmd::store_fetched_table(&mut boot.library_db, &table)?;
        boot.app_config.tables.sources = vec![crate::config::app_config::DifficultyTableSource {
            url: table.source_url.clone(),
            enabled: true,
        }];
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
        app.viewer_mode = false;
        app.viewer_waiting = false;
        let mut requests = Vec::new();
        let mut installs = Vec::new();
        for iteration in 0..6 {
            let action = if iteration == 0 {
                SelectListAction::Enter(format!("bmz-table:{}\n1", table.source_url))
            } else {
                SelectListAction::Refresh
            };
            let start = Instant::now();
            app.change_select_list(action);
            let request_ms = start.elapsed().as_secs_f64() * 1000.0;
            let deadline = Instant::now() + Duration::from_secs(30);
            let install_ms = loop {
                let start = Instant::now();
                app.poll_select_list();
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                if !app.select.list_loader.loading() {
                    break elapsed;
                }
                anyhow::ensure!(Instant::now() < deadline, "list load timed out");
                thread::sleep(Duration::from_millis(1));
            };
            assert_eq!(
                app.select
                    .select_items
                    .iter()
                    .filter(|item| matches!(item, SelectItem::Chart(_)))
                    .count(),
                count
            );
            if iteration > 0 {
                requests.push(request_ms);
                installs.push(install_ms);
            }
        }
        requests.sort_by(f64::total_cmp);
        installs.sort_by(f64::total_cmp);
        eprintln!(
            "select_list entries={count} dispatch_median_ms={:.3} dispatch_max_ms={:.3} install_median_ms={:.3} install_max_ms={:.3}",
            requests[2], requests[4], installs[2], installs[4]
        );

        // A refresh must preserve cursor movement made while its query runs.
        app.change_select_list(SelectListAction::Refresh);
        app.select.selected_index = 7;
        wait_for_select_list(&mut app);
        assert_eq!(app.select.selected_index, 7);
        let previous_stack = app.select.folder_stack.clone();
        app.change_select_list(SelectListAction::Enter("bmz-table:missing\n1".into()));
        wait_for_select_list(&mut app);
        assert_eq!(app.select.folder_stack, previous_stack);
        assert_eq!(app.select.selected_index, 7);
        if count == 1_000 {
            account_store_finishes_before_identity_cleanup(&mut app)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn wait_for_select_list(app: &mut WinitApp) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while app.select.list_loader.loading() {
        app.poll_select_list();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(windows)]
fn account_store_finishes_before_identity_cleanup(app: &mut WinitApp) -> anyhow::Result<()> {
    use crate::config::profile_config::{IrConfig, IrProviderConfig, IrProviderRoleConfig};
    let config = |account: &str| IrConfig {
        primary_provider: "rian-ir".into(),
        providers: vec![IrProviderConfig {
            provider: "rian-ir".into(),
            provider_key: "rian-ir".into(),
            base_url: "https://example.invalid/ir/".into(),
            enabled: true,
            account_display_name: account.into(),
            account_id: account.into(),
            send_policy: Default::default(),
            role: IrProviderRoleConfig::Primary,
            last_login_at: Some(1),
            last_success_at: None,
        }],
        ..Default::default()
    };
    let old = RianTableIdentity::from_ir_config(&config("old")).unwrap();
    let next = RianTableIdentity::from_ir_config(&config("next")).unwrap();
    let mut table = crate::table_cmd::profile::synthetic_table(100);
    table.source_url = format!("{}popular", old.source_prefix());
    app.jobs.table_fetch.rian_identity = Some(old.clone());
    app.jobs.table_fetch.pending_store = Some(PendingTableStore::account(
        app.boot.app_paths.library_db.clone(),
        app.jobs.table_fetch.rian_generation,
        old.clone(),
        vec![table],
    ));
    app.boot.profile_config.ir = config("next");
    app.reconcile_rian_table_identity();
    assert_eq!(app.jobs.table_fetch.rian_identity.as_ref(), Some(&old));
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.jobs.table_fetch.pending_store.is_some() {
        app.poll_pending_table_store();
        anyhow::ensure!(Instant::now() < deadline, "account store timed out");
        thread::sleep(Duration::from_millis(1));
    }
    // Reconciliation is allowed only in Select after the first frame. Keep the
    // queued fetch off: this test never contacts an IR service.
    app.first_frame_startup_completed = true;
    app.jobs.table_fetch.rian_identity = Some(old.clone());
    app.jobs.table_fetch.rian_last_started_at = Some(Instant::now());
    // A missing identity performs cleanup without requesting new account data.
    app.boot.profile_config.ir = Default::default();
    app.reconcile_rian_table_identity();
    assert_eq!(app.jobs.table_fetch.rian_identity, None);
    while app.jobs.table_fetch.pending_store.is_some() {
        app.poll_pending_table_store();
        anyhow::ensure!(Instant::now() < deadline, "account cleanup timed out");
        thread::sleep(Duration::from_millis(1));
    }
    assert!(
        !app.boot
            .library_db
            .list_difficulty_tables()?
            .iter()
            .any(|table| old.owns_source(&table.source_url))
    );
    assert_ne!(old, next);
    wait_for_select_list(app);
    Ok(())
}
