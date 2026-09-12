use super::*;

pub(super) fn clear_window_cursor_state(
    position: &mut Option<PhysicalPosition<f64>>,
    dragging_slider_type: &mut Option<i32>,
) {
    *position = None;
    *dragging_slider_type = None;
}

impl ApplicationHandler<AppUserEvent> for WinitApp {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        match cause {
            StartCause::Init => {
                tracing::info!("winit app init");
                self.ensure_window(event_loop);
            }
            StartCause::ResumeTimeReached { start, requested_resume } => {
                let actual_wake_at = Instant::now();
                let effective_frame_limit = self.current_frame_limit();
                self.frame.record_wait_wake(
                    start,
                    requested_resume,
                    actual_wake_at,
                    effective_frame_limit,
                );
                // `WaitUntil` の deadline 到達時だけ描画を要求する。待機中に届いた
                // keyboard/device/user event は redraw を発生させず、その場で処理できる。
                self.request_redraw();
            }
            StartCause::WaitCancelled { .. } | StartCause::Poll => {}
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        tracing::info!("winit app resumed");
        self.ensure_window(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.window.as_ref().map(|window| window.id()) != Some(window_id) {
            return;
        }

        if let WindowEvent::KeyboardInput { event, .. } = &event
            && self.capture_egui_key_config_keyboard(event)
        {
            return;
        }

        // すべてのウィンドウイベントを egui へ供給する。RedrawRequested など
        // egui が関知しないイベントは egui_winit 側で無視される。
        let practice_overlay = self
            .play
            .practice_session
            .as_ref()
            .is_some_and(|practice| practice.phase == PracticePhase::Config);
        let select_course_builder = self.select.course_builder.is_some();
        let has_play_context = self.play.active_play.is_some()
            || self.play.pending_play_start.is_some()
            || self.viewer_waiting;
        let play_owns_keyboard_input = match &event {
            WindowEvent::KeyboardInput { event, .. } => {
                let control = physical_key_to_control(event.physical_key);
                let app_key_held = event.state == ElementState::Released
                    && control.as_ref().is_some_and(|control| {
                        self.input
                            .pressed_play_inputs
                            .contains(&(W_KEYBOARD_DEVICE_ID, control.clone()))
                    });
                keyboard_input_bypasses_egui(
                    has_play_context,
                    self.play.play_e1_held,
                    self.play.play_e2_held,
                    app_key_held,
                    control.as_ref(),
                    self.play.play_option_input.as_ref(),
                )
            }
            WindowEvent::Ime(_) => has_play_context && self.play_lane_value_changing(),
            _ => false,
        };
        // Press を egui へ渡すと、E1/E2 を押しながら行うプレイ操作が UI も
        // 同時に動かしてしまう。Release は egui の押下状態を残さないため供給する。
        let suppress_egui_event = match &event {
            WindowEvent::KeyboardInput { event, .. } => {
                play_owns_keyboard_input && event.state == ElementState::Pressed
            }
            WindowEvent::Ime(_) => play_owns_keyboard_input,
            _ => false,
        };
        let egui_consumed = if suppress_egui_event {
            false
        } else {
            match (self.window.clone(), self.ui.egui.as_mut()) {
                (Some(window), Some(egui)) => {
                    egui.on_window_event(&window, &event, practice_overlay, select_course_builder)
                }
                _ => false,
            }
        };

        match event {
            WindowEvent::CloseRequested => {
                self.save_configs_for_exit(self.active_hispeed(), "game exit");
                event_loop.exit();
            }
            WindowEvent::DroppedFile(path) => self.open_dropped_chart(path),
            WindowEvent::KeyboardInput { event, .. } => {
                // F1 で egui メニューを開閉する。
                if event.physical_key == PhysicalKey::Code(KeyCode::F1)
                    && event.state == ElementState::Pressed
                    && !event.repeat
                {
                    if let Some(egui) = self.ui.egui.as_mut() {
                        egui.toggle();
                    }
                    return;
                }
                // Practice 設定画面だけは keyboard を UI 専用にする。通常プレイ中の
                // F1メニュー等はeguiへ供給しつつ、プレイ側にも同じ入力を通す。
                if egui_blocks_window_keyboard_route(
                    has_play_context,
                    practice_overlay,
                    play_owns_keyboard_input,
                    egui_consumed,
                ) {
                    return;
                }
                if self.select.key_config_edit.is_none()
                    && event.state == ElementState::Pressed
                    && !event.repeat
                    && physical_key_name(event.physical_key)
                        .is_some_and(|control| self.select.select_keys.is_screenshot(&control))
                {
                    self.request_manual_screenshot();
                    return;
                }
                self.route_keyboard_input(&event);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.ui.last_cursor_action_at = Instant::now();
                if !self.ui.cursor_visible {
                    if let Some(window) = &self.window {
                        window.set_cursor_visible(true);
                    }
                    self.ui.cursor_visible = true;
                }
                if egui_consumed {
                    return;
                }
                self.route_mouse_wheel(delta);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.ui.last_cursor_position = Some(position);
                self.ui.last_cursor_action_at = Instant::now();
                if !self.ui.cursor_visible {
                    if let Some(window) = &self.window {
                        window.set_cursor_visible(true);
                    }
                    self.ui.cursor_visible = true;
                }
                if !egui_consumed {
                    self.route_select_slider_drag();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                clear_window_cursor_state(
                    &mut self.ui.last_cursor_position,
                    &mut self.select.select_slider_dragging_type,
                );
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.ui.last_cursor_action_at = Instant::now();
                if !self.ui.cursor_visible {
                    if let Some(window) = &self.window {
                        window.set_cursor_visible(true);
                    }
                    self.ui.cursor_visible = true;
                }
                if egui_consumed {
                    return;
                }
                self.route_mouse_input(state, button);
            }
            WindowEvent::Ime(ime) => {
                if play_owns_keyboard_input || practice_overlay || egui_consumed {
                    return;
                }
                self.route_ime_event(&ime);
            }
            WindowEvent::Resized(size) => {
                if let Err(error) = self
                    .renderer
                    .resize_surface(SurfaceSize { width: size.width, height: size.height })
                {
                    tracing::error!(
                        width = size.width,
                        height = size.height,
                        error = %format_error_chain(&error),
                        "failed to reconfigure renderer surface after resize"
                    );
                }
                // 検索モード中はリサイズに合わせて IME 候補ウィンドウ位置を再計算する。
                self.update_search_ime_cursor_area();
            }
            WindowEvent::Focused(focused) => {
                let event_focused = focused;
                let native_focused = self.window.as_ref().is_some_and(|window| window.has_focus());
                let previous_effective_focused = self.ui.focused;
                let focus_update = resolve_window_focus_update(
                    previous_effective_focused,
                    event_focused,
                    native_focused,
                    cfg!(target_os = "macos"),
                );
                if event_focused != native_focused {
                    tracing::warn!(
                        event_focused,
                        native_focused,
                        previous_effective_focused,
                        effective_focused = focus_update.effective_focused,
                        "window focus state mismatch"
                    );
                }
                if previous_effective_focused != focus_update.effective_focused {
                    let previous_effective_frame_limit = self.current_frame_limit();
                    self.ui.focused = focus_update.effective_focused;
                    let effective_frame_limit = self.current_frame_limit();
                    tracing::info!(
                        previous_effective_focused,
                        effective_focused = focus_update.effective_focused,
                        previous_effective_frame_limit,
                        effective_frame_limit,
                        "effective window focus and frame limit changed"
                    );
                }
                if focus_update.focus_lost {
                    if let Some(egui) = self.ui.egui.as_mut() {
                        egui.cancel_key_config_listening();
                    }
                    let releases = self.input.handle_focus_lost();
                    for event in releases.raw_keyboard {
                        self.route_play_device_input(event);
                    }
                    for event in releases.window_keyboard {
                        self.route_play_device_input(event);
                    }
                    self.sync_select_holds_from_pressed_controls();
                    self.clear_select_hold();
                    self.reset_select_analog_scroll();
                    self.reset_play_analog_scroll();
                    self.clear_result_ir_scroll_input();
                    self.clear_play_control_holds();
                }
            }
            WindowEvent::RedrawRequested => {
                let limit_start = Instant::now();
                if !self.begin_scheduled_frame(event_loop) {
                    return;
                }
                // 通常起動は最初の Select 描画後に direct boot するが、Viewer は
                // 初回 surface frame から Play を描く。system sound を待たず、
                // window/surface 準備直後に audio と preload を開始する。
                if !self.first_frame_startup_completed && self.viewer_mode {
                    self.ensure_audio_output();
                    self.start_deferred_boot();
                }
                let pacing_timings = self.frame.current_pacing_timings();
                let limit_us = instant_elapsed_us_u64(limit_start);
                let redraw_started_at = Instant::now();
                let scene_before = self.current_scene_kind();
                let pending_skin_before = self.has_pending_skin_reload();
                let render_probe_before = self.skin.pending_skin_render_probe.is_some();
                self.start_deferred_skin_uploads_if_ready();
                let cursor_start = Instant::now();
                if self.ui.cursor_visible
                    && self.ui.last_cursor_action_at.elapsed() >= Duration::from_secs(2)
                {
                    if let Some(window) = &self.window {
                        window.set_cursor_visible(false);
                    }
                    self.ui.cursor_visible = false;
                }
                let cursor_us = instant_elapsed_us_u64(cursor_start);
                // Worker completion should be applied before intentional frame pacing sleep;
                // otherwise reload latency includes the frame limiter wait.
                let drain_start = Instant::now();
                let skin_drain_stats = self.drain_pending_skins();
                let drain_us = instant_elapsed_us_u64(drain_start);
                let input_start = Instant::now();
                self.sync_input_capture_target();
                self.consume_captured_gamepad_events();
                if !self.viewer_waiting {
                    self.advance_select_hold_move();
                    self.advance_select_ir_battle_hold();
                    self.advance_select_analog_scroll();
                }
                self.advance_result_ir_scroll_hold();
                self.advance_result_ir_analog_scroll();
                let input_us = instant_elapsed_us_u64(input_start);
                let background_start = Instant::now();
                // 通常はworker完了eventで反映するが、worker panicなどでeventが届かない
                // 場合もdirect bootを待たせ続けないよう、frame側でもchannelをpollする。
                self.poll_system_sound_load();
                self.poll_chart_bga_texture_load();
                self.poll_play_preload();
                self.refresh_play_target_from_source();
                self.poll_select_maintenance();
                self.poll_select_ir_battle_replay();
                let background_us = instant_elapsed_us_u64(background_start);
                let transition_start = Instant::now();
                self.advance_decide_transition();
                self.advance_play_ending();
                self.advance_result_exit();
                let transition_us = instant_elapsed_us_u64(transition_start);
                let egui_start = Instant::now();
                self.run_egui_frame();
                let egui_us = instant_elapsed_us_u64(egui_start);
                if !self.first_frame_startup_completed {
                    self.ensure_audio_output();
                }
                let consume_active_play_start = Instant::now();
                self.consume_active_play();
                let consume_active_play_us = instant_elapsed_us_u64(consume_active_play_start);
                let scene_start = Instant::now();
                let scene_profile = self.render_current_scene();
                let scene_us = instant_elapsed_us_u64(scene_start);
                let post_scene_start = Instant::now();
                if !self.first_frame_startup_completed {
                    self.first_frame_startup_completed = true;
                    if !self.system_sound_load_blocks_deferred_boot() {
                        self.start_deferred_boot();
                    }
                    self.sync_select_maintenance_gate();
                    if self.current_scene_kind() == AppSceneKind::Result {
                        self.ensure_result_skin_ready(self.current_result_skin_slot());
                    }
                    // render_current_scene() が既に last_scene_kind を更新済み。
                    // None に戻すと次フレームの start_scene_timers_before_snapshot が
                    // result_scene_started_at を再初期化し、動画 decode 時計が巻き戻って
                    // clocked decode thread が古い loop_base で待ち続けることがある。
                }
                self.advance_draining_audio();
                if let Some(runtime) = &self.audio.audio_runtime {
                    // chart sample bank を保持する source の破棄は、CPAL callback
                    // ではなく app thread 側で回収する。
                    runtime.reap_retired_sources();
                }
                self.log_audio_diagnostics();
                let post_scene_us = instant_elapsed_us_u64(post_scene_start);
                let total_us = instant_elapsed_us_u64(redraw_started_at);
                if let Some(sample) = scene_profile {
                    self.frame.record_profile(
                        sample,
                        AppLoopFrameTimings {
                            total_redraw_us: total_us,
                            input_us,
                            background_us,
                            transition_us,
                            egui_us,
                            consume_active_play_us,
                            post_scene_us,
                            pacing: pacing_timings,
                        },
                    );
                }
                let pending_skin_after = self.has_pending_skin_reload();
                if skin_drain_stats.received_count > 0
                    || render_probe_before
                    || (pending_skin_before
                        && total_us >= duration_us_u64(SKIN_RELOAD_REDRAW_PROFILE_THRESHOLD))
                {
                    tracing::debug!(
                        scene_before = ?scene_before,
                        scene_after = ?self.current_scene_kind(),
                        pending_before = pending_skin_before,
                        pending_after = pending_skin_after,
                        render_probe_before,
                        received_uploads = skin_drain_stats.received_count,
                        applied_uploads = skin_drain_stats.applied_count,
                        max_upload_wait_us = skin_drain_stats.max_upload_wait_us,
                        total_us,
                        cursor_us,
                        drain_us,
                        limit_us,
                        input_us,
                        background_us,
                        transition_us,
                        egui_us,
                        scene_us,
                        post_scene_us,
                        "skin reload redraw timings"
                    );
                }
                if self.should_exit_via_select_hold() {
                    tracing::info!("escape held for 2s on select screen; exiting app");
                    self.save_configs_for_exit(self.active_hispeed(), "select exit hold");
                    event_loop.exit();
                    return;
                }
                self.handle_smoke_exit_after_redraw(event_loop);
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::Key(raw) = event {
            self.route_raw_keyboard_gameplay_input(raw.physical_key, raw.state);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppUserEvent) {
        match event {
            AppUserEvent::SkinUpload { sent_at } => {
                let event_received_at = Instant::now();
                let pending_before = self.has_pending_skin_reload();
                let drain_start = Instant::now();
                let skin_drain_stats = self.drain_pending_skins();
                let drain_us = instant_elapsed_us_u64(drain_start);
                self.request_redraw();
                tracing::debug!(
                    event_delay_us = instant_duration_us_u64(sent_at, event_received_at),
                    pending_before,
                    pending_after = self.has_pending_skin_reload(),
                    received_uploads = skin_drain_stats.received_count,
                    applied_uploads = skin_drain_stats.applied_count,
                    max_upload_wait_us = skin_drain_stats.max_upload_wait_us,
                    drain_us,
                    "skin upload ready event timings"
                );
            }
            AppUserEvent::SystemSoundReady { generation } => {
                self.poll_system_sound_load();
                self.request_redraw();
                tracing::debug!(generation, "handled system sound ready event");
            }
            AppUserEvent::CourseLinkRepair => {
                self.poll_pending_course_link_repair();
                self.request_redraw();
            }
            AppUserEvent::TableFetch => {
                self.poll_select_maintenance();
                self.request_redraw();
            }
            AppUserEvent::RivalSync => {
                self.poll_select_maintenance();
                self.request_redraw();
            }
            AppUserEvent::ViewerCommand(command) => match command {
                crate::viewer_ipc::ViewerCommand::Stop => {
                    tracing::info!("external viewer playback stopped; waiting for next command");
                    self.stop_viewer_playback();
                    self.request_redraw();
                }
                crate::viewer_ipc::ViewerCommand::Play { path, measure, battle } => {
                    if let Err(error) = self.play_viewer_chart(&path, measure, battle) {
                        tracing::error!(path = %path.display(), measure, battle, %error, "external viewer play request failed");
                    }
                    self.request_redraw();
                }
                crate::viewer_ipc::ViewerCommand::Quit => {
                    tracing::info!("external viewer exit requested");
                    event_loop.exit();
                }
            },
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if std::mem::take(&mut self.ui.device_events_reconfigure_pending) {
            self.configure_device_events(event_loop);
        }
        let pending_before = self.has_pending_skin_reload();
        if pending_before {
            let drain_start = Instant::now();
            let skin_drain_stats = self.drain_pending_skins();
            let drain_us = instant_elapsed_us_u64(drain_start);
            if skin_drain_stats.received_count > 0 {
                self.request_redraw();
            }
            if skin_drain_stats.received_count > 0
                || drain_us >= duration_us_u64(SKIN_RELOAD_REDRAW_PROFILE_THRESHOLD)
            {
                tracing::debug!(
                    pending_before,
                    pending_after = self.has_pending_skin_reload(),
                    received_uploads = skin_drain_stats.received_count,
                    applied_uploads = skin_drain_stats.applied_count,
                    max_upload_wait_us = skin_drain_stats.max_upload_wait_us,
                    drain_us,
                    "skin reload about_to_wait timings"
                );
            }
        }
        if self.shutdown_requested.load(Ordering::SeqCst) {
            tracing::info!("shutdown requested; exiting cleanly");
            event_loop.exit();
            return;
        }
        self.schedule_next_frame(event_loop);
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(handle) = self.integrations.discord_presence.take() {
            handle.shutdown();
        }
        self.flush_pending_screenshots("app exit");
        self.save_configs_for_exit(self.active_hispeed(), "game exit");
        self.wait_for_pending_play_result_on_exit();
        self.release_audio_for_process_exit();
        // Linux の winit/wgpu backend では Window より後に Surface を drop すると
        // native 側で落ちることがあるため、Window を保持したまま GPU 資源を解放する。
        self.ui.egui = None;
        if let Ok(mut cache) = self.skin.skin_pipeline.gpu_texture_cache.lock() {
            cache.clear();
        }
        self.renderer.detach_surface();
    }
}

impl WinitApp {
    fn open_dropped_chart(&mut self, path: PathBuf) {
        if self.viewer_mode {
            let battle = self.select.session_mode == SessionMode::AutoplayBattle;
            if let Err(error) = self.play_viewer_chart(&path, 0, battle) {
                tracing::warn!(path = %path.display(), %error, "failed to open dropped viewer chart");
                self.show_left_overlay_toast(format!(
                    "Could not open {}: {error:#}",
                    path.display()
                ));
            }
            return;
        }
        if !matches!(self.view_state(), AppViewState::Select) {
            self.show_left_overlay_toast("BMS files can only be dropped on the select screen");
            return;
        }
        let canonical = match path.canonicalize() {
            Ok(path) => path,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "dropped chart path is unavailable");
                self.show_left_overlay_toast(format!("Could not open {}", path.display()));
                return;
            }
        };
        if !crate::storage::scan::is_chart_file(&canonical) {
            self.show_left_overlay_toast(format!(
                "Unsupported chart file: {}",
                canonical.display()
            ));
            return;
        }
        match crate::storage::import::import_chart_file(
            &mut self.boot.library_db,
            &canonical,
            None,
            None,
            now_unix_seconds(),
        ) {
            Ok(imported) => {
                tracing::info!(
                    chart_id = imported.chart_id,
                    path = %canonical.display(),
                    "opening dropped chart"
                );
                self.reload_select_items();
                self.start_chart(imported.chart_id);
            }
            Err(error) => {
                tracing::error!(path = %canonical.display(), %error, "failed to import dropped chart");
                self.show_left_overlay_toast(format!(
                    "Could not open {}: {error:#}",
                    canonical.display()
                ));
            }
        }
    }

    fn wait_for_pending_play_result_on_exit(&mut self) {
        let pending = self
            .play
            .active_play
            .as_mut()
            .and_then(|active| active.running.pending_finished.take());
        let Some(pending) = pending else {
            return;
        };
        let elapsed_ms = pending.elapsed().as_millis();
        match pending.wait_for_completion() {
            Ok(()) => tracing::info!(elapsed_ms, "play result save completed during app exit"),
            Err(error) => {
                tracing::error!(%error, elapsed_ms, "play result save failed during app exit")
            }
        }
    }

    fn release_audio_for_process_exit(&mut self) {
        if self.audio.audio_runtime.as_ref().is_some_and(AudioRuntime::uses_pulseaudio_host) {
            // cpal 0.18 の PulseAudio backend は stream Drop 時に pulseaudio crate の
            // reactor 切断と stream delete が重なり、終了時に native 側で abort する
            // 環境がある。プロセス終了直前だけ handle を残し、通常の drop cascade
            // に戻らずプロセスを終了する。
            if let Some(audio) = self.audio.draining_audio.take() {
                std::mem::forget(audio);
            }
            if let Some(active_play) = self.play.active_play.take() {
                std::mem::forget(active_play);
            }
            if let Some(system_audio) = self.audio.system_audio.take() {
                std::mem::forget(system_audio);
            }
            if let Some(runtime) = self.audio.audio_runtime.take() {
                std::mem::forget(runtime);
            }
            tracing::debug!("released audio with PulseAudio shutdown workaround");
            return;
        }

        // プロセス終了前に音声出力を確実に Drop し、ASIO の停止・後処理を走らせる。
        self.audio.draining_audio = None;
        self.play.active_play = None;
        self.audio.system_audio = None;
        self.audio.audio_runtime = None;
    }
}
