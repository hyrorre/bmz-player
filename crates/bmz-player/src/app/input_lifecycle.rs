use super::*;

#[cfg(windows)]
#[path = "input_lifecycle/windows_modal_redraw.rs"]
mod windows_modal_redraw;

impl WinitApp {
    pub(super) fn sync_input_capture_target(&self) {
        let route = if self.raw_input_gameplay_blocked() {
            None
        } else {
            self.play_input_backend().map(|input| crate::input::capture::InputRoute {
                input,
                binding: self
                    .play
                    .play_option_input
                    .as_ref()
                    .map(|input| input.binding.clone())
                    .unwrap_or_else(|| bmz_gameplay::input::binding::LaneBinding {
                        entries: Vec::new(),
                    }),
                focused: self.ui.focused,
                keyboard_enabled: self.boot.app_config.input.keyboard_enabled,
            })
        };
        if let Some(capture) = &self.gamepad {
            capture.set_route(route);
        }
    }
    pub(super) fn refresh_player_stats_snapshot(&mut self) {
        self.select.player_stats = player_stats_snapshot(
            &self.boot.score_db,
            &self.boot.library_db,
            self.boot.profile_config.statistics.day_start_hour,
        );
    }

    pub(super) fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    pub(super) fn keyboard_input_backend(&self) -> Option<KeyboardInputBackend> {
        keyboard_input_backend_for_config(&self.boot.app_config)
    }

    pub(super) fn raw_input_keyboard_enabled(&self) -> bool {
        self.keyboard_input_backend() == Some(KeyboardInputBackend::RawInput)
            && !self
                .gamepad
                .as_ref()
                .is_some_and(crate::input::capture::InputCapture::native_keyboard_enabled)
    }

    pub(super) fn window_keyboard_gameplay_enabled(&self) -> bool {
        self.keyboard_input_backend() == Some(KeyboardInputBackend::Window)
            && !self
                .gamepad
                .as_ref()
                .is_some_and(crate::input::capture::InputCapture::native_keyboard_enabled)
    }

    pub(super) fn configure_device_events(&self, event_loop: &ActiveEventLoop) {
        let device_events = if !cfg!(windows) && self.raw_input_keyboard_enabled() {
            DeviceEvents::WhenFocused
        } else {
            DeviceEvents::Never
        };
        event_loop.listen_device_events(device_events);
    }

    pub(super) fn apply_egui_input_config(&mut self, window: &Window, before: &GlobalInputConfig) {
        let keyboard_changed = keyboard_runtime_config_changed(before, &self.boot.app_config.input);
        let gamepad_changed = gamepad_runtime_config_changed(before, &self.boot.app_config.input);
        if !keyboard_changed && !gamepad_changed {
            return;
        }

        // backend 境界をまたいで押下状態を持ち越さない。設定パネルはプレイ中に
        // 編集できないため、ここでは focus loss と同じリセットで安全に揃えられる。
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

        if keyboard_changed {
            self.ui.device_events_reconfigure_pending = true;
            tracing::info!(
                backend = ?self.boot.app_config.input.backend,
                enabled = self.boot.app_config.input.keyboard_enabled,
                "keyboard input backend configuration updated"
            );
        }
        if gamepad_changed {
            self.reinitialize_gamepad_backend(window);
        }
    }

    fn reinitialize_gamepad_backend(&mut self, window: &Window) {
        let input = &mut self.boot.app_config.input;
        input.gamepad_slot_runtime_device_ids = [None; 2];
        let enabled = input.gamepad_enabled;
        let requested = input.gamepad_backend;
        let configs = gamepad_scratch_configs(&self.boot.profile_config.input);

        // RawInputBackend の Drop で usage 登録を解除してから新 backend を作る。
        self.gamepad = None;
        if !enabled {
            self.gamepad = crate::input::capture::InputCapture::new(
                None,
                configs,
                self.raw_input_bridge.clone(),
            )
            .ok();
            if let Some(capture) = &mut self.gamepad {
                let _ = capture.attach_window(window);
            }
            return;
        }

        let mut gamepad =
            initialize_gamepad_backend(requested, configs, self.raw_input_bridge.clone());
        let attach_error = gamepad.as_mut().and_then(|backend| backend.attach_window(window).err());
        if let Some(error) = attach_error {
            tracing::warn!(%error, ?requested, "gamepad backend could not attach to the window; falling back to gilrs");
            gamepad = initialize_gilrs_backend(configs);
        }
        let slots = resolve_gamepad_runtime_slots(&self.boot.app_config.input, gamepad.as_ref());
        if let Some(backend) = &mut gamepad {
            backend.set_analog_config(
                configs,
                crate::input::gamepad::GamepadSlotMap::from_device_ids(slots),
            );
        }
        tracing::info!(
            ?requested,
            active = gamepad.as_ref().map(crate::input::capture::InputCapture::name),
            "gamepad input backend configuration applied immediately"
        );
        self.gamepad = gamepad;
    }

    pub(super) fn raw_input_gameplay_blocked(&self) -> bool {
        if self.play.play_ending.is_some() {
            return true;
        }
        let practice_overlay = self
            .play
            .practice_session
            .as_ref()
            .is_some_and(|practice| practice.phase == PracticePhase::Config);
        egui_blocks_raw_play_keyboard(
            practice_overlay && self.ui.egui.is_some(),
            self.play.play_e1_held,
            self.play.play_e2_held,
        )
    }

    pub(super) fn play_input_backend(&self) -> Option<SharedInputBackend> {
        play_input_backend_for_context(
            self.play.active_play.as_ref().map(|active_play| &active_play.input),
            self.play.pending_play_start.is_some(),
            self.play.preloaded_play_session.as_ref().map(|preloaded| &preloaded.input),
            self.play.pending_play_preload.as_ref().map(|pending| &pending.input),
        )
    }

    pub(super) fn filter_app_input_bounce(
        &mut self,
        event: DeviceInputEvent,
    ) -> Option<DeviceInputEvent> {
        let config = input_bounce_config_from_profile(&self.boot.profile_config.input);
        self.input.accept_app_event(config, event)
    }

    pub(super) fn route_play_device_input(&mut self, event: DeviceInputEvent) {
        let Some(input) = self.play_input_backend() else {
            return;
        };
        input.push_shared_event(event.clone());
        if self.play.active_play.is_some() {
            return;
        }
        let visual_now = self.play_elapsed_time();
        if let Some(pending) = &mut self.play.pending_play_start {
            pending.visual_input.apply_event(&event, visual_now);
        }
        self.refresh_pending_play_visual_snapshot(visual_now);
    }

    pub(super) fn refresh_pending_play_visual_snapshot(&mut self, visual_now: TimeUs) {
        if self.play.active_play.is_some() {
            return;
        }
        let Some(pending) = &mut self.play.pending_play_start else {
            return;
        };
        pending.visual_input.advance(visual_now);
        let Some(snapshot) = &mut self.play.last_play_snapshot else {
            return;
        };
        crate::screens::play_snapshot::refresh_pending_play_input_visuals(
            snapshot,
            pending.visual_input.key_mode,
            pending.visual_input.lane_keyon_started_at,
            pending.visual_input.lane_keyoff_started_at,
            pending.visual_input.lane_scratch_angle_delta_ms,
            visual_now,
        );
    }

    pub(super) fn route_raw_keyboard_gameplay_input(
        &mut self,
        physical_key: PhysicalKey,
        state: ElementState,
    ) {
        if !self.raw_input_keyboard_enabled() {
            return;
        }
        if self.play_input_backend().is_none() {
            self.input.discard_raw_keyboard_transition(physical_key, state);
            return;
        }
        if self.active_play_uses_playback_rate_keys()
            && is_unassigned_autoplay_replay_playback_rate_key(
                physical_key,
                self.play.play_option_input.as_ref(),
            )
        {
            self.input.discard_raw_keyboard_transition(physical_key, state);
            return;
        }
        let config = input_bounce_config_from_profile(&self.boot.profile_config.input);
        let gameplay_blocked = self.raw_input_gameplay_blocked();
        if let Some(event) =
            self.input.raw_keyboard_transition(config, physical_key, state, gameplay_blocked)
        {
            self.route_play_device_input(event);
        }
    }

    pub(super) fn ensure_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let video = &self.boot.app_config.video;
        let requested_window_mode = video.mode.clone();
        let monitor = select_monitor(
            &video.monitor_name,
            event_loop.available_monitors(),
            event_loop.primary_monitor(),
        );
        let fullscreen = fullscreen_from_config(video, monitor.clone());
        let mut effective_mode = effective_window_mode(&fullscreen);
        let attributes = window_attributes_from_config(video).with_fullscreen(fullscreen);
        match event_loop.create_window(attributes) {
            Ok(window) => {
                let window = Arc::new(window);
                #[cfg(windows)]
                if let Err(error) = windows_modal_redraw::install(&window) {
                    tracing::warn!(%error, "failed to enable redraw during window move/resize");
                }
                window.set_visible(true);
                let mut size = surface_size_for_window(&window);
                // サーフェス生成前に present mode とバックエンド設定を反映させておく。
                if let Err(error) =
                    self.renderer.set_present_mode(config_present_mode(&self.boot.app_config.video))
                {
                    tracing::warn!(%error, "failed to prepare renderer present mode");
                }
                if let Err(error) = self
                    .renderer
                    .set_frame_latency_mode(config_frame_latency_mode(&self.boot.app_config.video))
                {
                    tracing::warn!(%error, "failed to prepare renderer frame latency mode");
                }
                let backend = config_renderer_backend(self.boot.app_config.video.renderer.clone());
                self.renderer.set_backend(backend);
                let mut fallback_attempted = false;
                loop {
                    match self.renderer.attach_surface(Arc::clone(&window), size) {
                        Ok(()) => {
                            if fallback_attempted {
                                tracing::info!(
                                    requested_window_mode = ?requested_window_mode,
                                    effective_window_mode = ?effective_mode,
                                    requested_renderer_backend = ?backend,
                                    surface_width = size.width,
                                    surface_height = size.height,
                                    "borderless fullscreen renderer surface fallback succeeded"
                                );
                            }
                            break;
                        }
                        Err(error) => {
                            if let Some(fallback_mode) = surface_attach_fallback_mode(
                                &requested_window_mode,
                                &effective_mode,
                                fallback_attempted,
                                cfg!(target_os = "windows"),
                            ) {
                                tracing::warn!(
                                    requested_window_mode = ?requested_window_mode,
                                    effective_window_mode = ?effective_mode,
                                    fallback_window_mode = ?fallback_mode,
                                    requested_renderer_backend = ?backend,
                                    configure_error = %format_error_chain(&error),
                                    "exclusive fullscreen renderer initialization failed; falling back to borderless fullscreen"
                                );
                                // `attach_surface` commits a WgpuRenderer only on success, so all
                                // failed candidate surfaces/devices are gone before this mode switch.
                                self.renderer.detach_surface();
                                window
                                    .set_fullscreen(Some(Fullscreen::Borderless(monitor.clone())));
                                effective_mode = fallback_mode;
                                fallback_attempted = true;
                                size = surface_size_for_window(&window);
                                tracing::info!(
                                    requested_window_mode = ?requested_window_mode,
                                    effective_window_mode = ?effective_mode,
                                    requested_renderer_backend = ?backend,
                                    surface_width = size.width,
                                    surface_height = size.height,
                                    "retrying renderer surface initialization after fullscreen fallback"
                                );
                                continue;
                            }

                            if fallback_attempted {
                                tracing::error!(
                                    requested_window_mode = ?requested_window_mode,
                                    effective_window_mode = ?effective_mode,
                                    requested_renderer_backend = ?backend,
                                    surface_width = size.width,
                                    surface_height = size.height,
                                    configure_error = %format_error_chain(&error),
                                    "borderless fullscreen renderer surface fallback failed"
                                );
                            } else {
                                tracing::error!(
                                    requested_window_mode = ?requested_window_mode,
                                    effective_window_mode = ?effective_mode,
                                    requested_renderer_backend = ?backend,
                                    surface_width = size.width,
                                    surface_height = size.height,
                                    configure_error = %format_error_chain(&error),
                                    "failed to initialize renderer surface"
                                );
                            }
                            event_loop.exit();
                            return;
                        }
                    }
                }
                self.ui.applied_window_mode = effective_mode.clone();
                self.ui.exclusive_fullscreen_fallback_active = requested_window_mode
                    == WindowMode::ExclusiveFullscreen
                    && effective_mode == WindowMode::BorderlessFullscreen;
                tracing::info!(
                    requested_window_mode = ?requested_window_mode,
                    effective_window_mode = ?effective_mode,
                    requested_renderer_backend = ?backend,
                    width = size.width,
                    height = size.height,
                    "window and renderer surface ready"
                );
                // Decide / Result の decode 結果は、Select の開始演出が終わるまで
                // skin_decode_rx に保持する。遷移側が先に必要とした場合は
                // ensure_skin_ready() が upload worker を即時起動する。
                self.configure_device_events(event_loop);
                let raw_input_attach_error =
                    self.gamepad.as_mut().and_then(|backend| backend.attach_window(&window).err());
                if let Some(error) = raw_input_attach_error {
                    tracing::warn!(%error, "gamepad backend could not attach to the window; falling back to gilrs");
                    let configs = gamepad_scratch_configs(&self.boot.profile_config.input);
                    self.gamepad = initialize_gilrs_backend(configs);
                    self.apply_gamepad_analog_config();
                }
                window.request_redraw();
                self.ui.egui = Some(EguiLayer::new(
                    &window,
                    self.boot.profile_config.ui.show_fps,
                    vec![self.boot.app_paths.bundled_noto_cjk_font_root()],
                ));
                self.window = Some(window);
            }
            Err(error) => {
                tracing::error!(%error, "failed to create window");
                event_loop.exit();
            }
        }
    }

    pub(super) fn start_deferred_boot(&mut self) {
        let Some(boot) = self.deferred_boot.take() else {
            return;
        };
        match boot {
            DeferredBoot::Chart {
                chart_id,
                replay_slot,
                skip_decide,
                score_save_disabled,
                start_time_us,
                bms_random_seed,
            } => {
                tracing::info!(chart_id, "booting directly into chart");
                if let Some(slot) = replay_slot {
                    if !self.try_start_replay_for_chart(chart_id, slot, false) {
                        tracing::warn!(slot, "boot replay slot empty; falling back to normal play");
                        self.start_boot_chart(
                            chart_id,
                            skip_decide,
                            score_save_disabled,
                            start_time_us,
                            bms_random_seed,
                        );
                    }
                } else {
                    self.start_boot_chart(
                        chart_id,
                        skip_decide,
                        score_save_disabled,
                        start_time_us,
                        bms_random_seed,
                    );
                }
            }
            DeferredBoot::Practice { chart_id, start_time_ms, end_time_ms } => {
                tracing::info!(chart_id, "booting into practice mode");
                self.enter_practice(chart_id, PracticeCliOverrides { start_time_ms, end_time_ms });
            }
            DeferredBoot::ReplayFile { path } => {
                tracing::info!(%path, "booting replay from file");
                if !self.try_start_replay_from_file(std::path::Path::new(&path)) {
                    tracing::warn!(%path, "replay file boot failed; staying on select");
                }
            }
            DeferredBoot::CourseReplay { course_id } => {
                let Some((stored, identity)) = self.course_identity_with_stored(course_id) else {
                    tracing::warn!(
                        course_id,
                        "course identity unavailable; --boot-course-replay has nothing to replay"
                    );
                    return;
                };
                let ln_policy =
                    match crate::screens::select_model::normalized_course_ln_policy_for_definition(
                        &self.boot.library_db,
                        &stored.definition,
                        self.boot.profile_config.play.ln_mode_policy,
                    ) {
                        Ok(policy) => policy,
                        Err(error) => {
                            tracing::warn!(%error, course_id, "course LN policy unavailable for replay");
                            return;
                        }
                    };
                let rule_mode = self.boot.profile_config.play.rule_mode;
                match self.boot.score_db.latest_course_score_id(
                    &identity.course_hash,
                    ln_policy,
                    rule_mode,
                ) {
                    Ok(Some(course_score_id)) => {
                        tracing::info!(course_id, course_score_id, "booting into course replay");
                        self.start_course_replay_with_auto_advance(
                            course_id,
                            course_score_id,
                            true,
                        );
                    }
                    Ok(None) => {
                        tracing::warn!(
                            course_id,
                            "no saved course attempt; --boot-course-replay has nothing to replay"
                        );
                    }
                    Err(error) => {
                        tracing::error!(
                            %error,
                            course_id,
                            "failed to look up latest course score for replay boot"
                        );
                    }
                }
            }
            DeferredBoot::Course { course_id } => {
                tracing::info!(course_id, "booting into fresh course");
                self.start_course_with_arrange(course_id, Vec::new(), true);
            }
        }
    }

    pub(super) fn start_boot_chart(
        &mut self,
        chart_id: i64,
        skip_decide: bool,
        score_save_disabled: bool,
        start_time_us: Option<i64>,
        bms_random_seed: Option<u64>,
    ) -> bool {
        self.start_boot_chart_with_presentation(
            chart_id,
            skip_decide,
            score_save_disabled,
            start_time_us,
            bms_random_seed,
            PlayEntryPresentation::Normal,
        )
    }

    pub(super) fn start_boot_chart_with_presentation(
        &mut self,
        chart_id: i64,
        skip_decide: bool,
        score_save_disabled: bool,
        start_time_us: Option<i64>,
        bms_random_seed: Option<u64>,
        presentation: PlayEntryPresentation,
    ) -> bool {
        self.select.autoplay_folder = None;
        let mut options = self.play_start_options();
        options.score_save_disabled = score_save_disabled;
        if bms_random_seed.is_some() {
            options.bms_random_seed = bms_random_seed;
        }
        if !self.prepare_session_mode_or_show_error(chart_id, &mut options) {
            return false;
        }
        self.play.practice_chart_zero_time = start_time_us.map(TimeUs);
        if skip_decide {
            self.start_play_preload(chart_id, options.clone());
            self.begin_preloaded_play_scene_with_presentation(chart_id, options, presentation);
        } else {
            self.begin_decide_for_chart(chart_id, options);
        }
        true
    }
}

pub(super) fn keyboard_runtime_config_changed(
    before: &GlobalInputConfig,
    after: &GlobalInputConfig,
) -> bool {
    before.backend != after.backend || before.keyboard_enabled != after.keyboard_enabled
}

pub(super) fn gamepad_runtime_config_changed(
    before: &GlobalInputConfig,
    after: &GlobalInputConfig,
) -> bool {
    before.gamepad_backend != after.gamepad_backend
        || before.gamepad_enabled != after.gamepad_enabled
}
