use super::*;

impl WinitApp {
    pub(super) fn should_exit_via_select_hold(&mut self) -> bool {
        if !matches!(self.view_state(), AppViewState::Select) {
            self.select.select_exit_hold_started_at = None;
            return false;
        }
        let Some(started) = self.select.select_exit_hold_started_at else {
            return false;
        };
        started.elapsed() >= SELECT_EXIT_HOLD_DURATION
    }

    pub(super) fn select_exit_hold_progress(&self) -> f32 {
        let Some(started) = self.select.select_exit_hold_started_at else {
            return 0.0;
        };
        let elapsed = started.elapsed().as_secs_f32();
        let total = SELECT_EXIT_HOLD_DURATION.as_secs_f32();
        (elapsed / total).clamp(0.0, 1.0)
    }

    pub(super) fn select_time(&self) -> TimeUs {
        if !self.select.select_scene_timer_armed {
            return TimeUs::default();
        }
        let micros =
            self.select.select_scene_started_at.elapsed().as_micros().min(i64::MAX as u128) as i64;
        TimeUs(micros)
    }

    pub(super) fn select_bar_time(&self) -> TimeUs {
        if !self.select.select_scene_timer_armed {
            return TimeUs::default();
        }
        let micros =
            self.select.select_bar_started_at.elapsed().as_micros().min(i64::MAX as u128) as i64;
        TimeUs(micros)
    }

    pub(super) fn restart_select_bar_timer_without_scroll(&mut self, now: Instant) {
        self.select.select_bar_started_at = now;
        self.select.select_bar_scroll_direction = 0;
        self.select.select_bar_scroll_duration = Duration::ZERO;
    }

    pub(super) fn select_bar_scroll_progress(&self) -> f32 {
        if !self.select.select_scene_timer_armed
            || self.select.select_bar_scroll_direction == 0
            || self.select.select_bar_scroll_duration.is_zero()
        {
            return 0.0;
        }
        let elapsed = self.select.select_bar_started_at.elapsed();
        if elapsed >= self.select.select_bar_scroll_duration {
            return 0.0;
        }
        1.0 - elapsed.as_secs_f32() / self.select.select_bar_scroll_duration.as_secs_f32()
    }

    pub(super) fn select_scroll_duration_low(&self) -> Duration {
        Duration::from_millis(u64::from(select_scroll_duration_low_ms(&self.boot.app_config)))
    }

    pub(super) fn select_scroll_duration_high(&self) -> Duration {
        Duration::from_millis(u64::from(select_scroll_duration_high_ms(&self.boot.app_config)))
    }

    pub(super) fn play_elapsed_time(&self) -> TimeUs {
        if self.viewer_mode
            && self.viewer_paused
            && let Some(elapsed) = self.viewer_paused_play_elapsed
        {
            return elapsed;
        }
        let micros =
            self.play.play_scene_started_at.elapsed().as_micros().min(i64::MAX as u128) as i64;
        TimeUs(micros)
    }

    pub(super) fn decide_snapshot(&self, decide: &DecideTransition) -> RenderSnapshot {
        let mut snapshot = decide.snapshot_for_render();
        let elapsed = match decide.fadeout_started_at {
            Some(fadeout_started_at) => {
                let fadeout_duration = self.decide_fadeout_duration();
                let fadeout_elapsed = fadeout_started_at.elapsed().min(fadeout_duration);
                let scene_elapsed = decide_fadeout_scene_elapsed(
                    fadeout_started_at.duration_since(decide.started_at),
                    fadeout_elapsed,
                    self.decide_scene_duration(),
                    fadeout_duration,
                    self.decide_fadeout_scene_timing(),
                );
                TimeUs(scene_elapsed.as_micros().min(i64::MAX as u128) as i64)
            }
            None => elapsed_since(decide.started_at),
        };
        snapshot.play_elapsed_time = elapsed;
        snapshot.fadeout_elapsed_ms = decide.fadeout_started_at.map(|started_at| {
            let elapsed_ms = elapsed_since_ms(started_at);
            let fadeout_ms =
                self.decide_fadeout_duration().as_millis().min(i32::MAX as u128) as i32;
            elapsed_ms.min(fadeout_ms)
        });
        snapshot
    }

    pub(super) fn option_panel_time(&self) -> TimeUs {
        if !self.select.select_scene_timer_armed {
            return TimeUs::default();
        }
        let micros =
            self.select.option_panel_started_at.elapsed().as_micros().min(i64::MAX as u128) as i64;
        TimeUs(micros)
    }

    pub(super) fn set_start_held(&mut self, held: bool) {
        if self.input.start_held != held {
            self.input.start_held = held;
            self.update_select_option_panel();
        }
    }

    pub(super) fn set_select_held(&mut self, held: bool) {
        if self.input.select_held != held {
            self.input.select_held = held;
            self.update_select_option_panel();
        }
    }

    pub(super) fn sync_select_holds_from_pressed_controls(&mut self) {
        let (start_held, select_held, e_action_holds) = select_hold_state_from_pressed_controls(
            &self.input.pressed_controls,
            &self.select.select_keys,
        );
        self.input.select_e_action_holds = e_action_holds;
        self.set_start_held(start_held);
        self.set_select_held(select_held);
    }

    pub(super) fn update_select_e_action_hold(&mut self, control: &str, held: bool) {
        let Some(action) = self.select.select_keys.e_action_for_control(control) else {
            return;
        };
        if held {
            self.input.select_e_action_holds.insert(action);
        } else {
            self.input.select_e_action_holds.remove(&action);
        }
    }

    pub(super) fn select_e_action_held(&self) -> bool {
        self.input.select_e_action_held()
    }

    pub(super) fn update_select_option_panel(&mut self) {
        let panel = if in_settings_stack(&self.select.folder_stack) {
            0
        } else {
            select_option_panel_for_holds(self.input.start_held, self.input.select_held)
        };
        let previous_panel = self.select.select_option_panel;
        let now = Instant::now();
        if transition_select_option_panel(
            &mut self.select.select_option_panel,
            &mut self.select.option_panel_started_at,
            &mut self.select.option_panel_off_started_at,
            panel,
            now,
        ) {
            self.reset_select_analog_scroll();
            if let Some(sound_type) = select_option_panel_sound_for_scene_transition(
                self.current_scene_kind(),
                previous_panel,
                panel,
            ) {
                self.play_system_sound(sound_type);
            }
        }
    }

    pub(super) fn begin_settings_edit(&mut self, entry_id: SettingsEntryId) {
        self.select.settings_edit = Some(SelectSettingsEditSession::Profile(
            SettingsEditSession::capture(&self.boot.profile_config, entry_id),
        ));
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
        tracing::info!(?entry_id, "settings edit mode started");
    }

    pub(super) fn begin_app_settings_edit(&mut self, entry_id: AppSettingsEntryId) {
        let choices = self.app_settings_choices(entry_id);
        self.select.settings_edit = Some(SelectSettingsEditSession::App(
            AppSettingsEditSession::capture(&self.boot.app_config, entry_id, choices),
        ));
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
        tracing::info!(?entry_id, "app settings edit mode started");
    }

    fn app_settings_choices(&self, entry_id: AppSettingsEntryId) -> AppSettingsChoices {
        match entry_id {
            AppSettingsEntryId::AudioBackend => {
                AppSettingsChoices::AudioBackends(crate::audio::available_audio_backends())
            }
            AppSettingsEntryId::AudioOutputDevice | AppSettingsEntryId::AudioAsioDriver => {
                let mut names = vec![String::new()];
                names
                    .extend(crate::audio::list_output_devices(&self.boot.app_config.audio.backend));
                let configured = if entry_id == AppSettingsEntryId::AudioAsioDriver {
                    &self.boot.app_config.audio.asio_driver
                } else {
                    &self.boot.app_config.audio.output_device
                };
                if !configured.is_empty() && !names.contains(configured) {
                    names.push(configured.clone());
                }
                names.dedup();
                AppSettingsChoices::Text(names)
            }
            AppSettingsEntryId::VideoMonitor => {
                let mut names = vec![String::new()];
                if let Some(window) = self.window.as_ref() {
                    names.extend(
                        window.available_monitors().map(|monitor| monitor_config_name(&monitor)),
                    );
                }
                if !self.boot.app_config.video.monitor_name.is_empty()
                    && !names.contains(&self.boot.app_config.video.monitor_name)
                {
                    names.push(self.boot.app_config.video.monitor_name.clone());
                }
                names.dedup();
                AppSettingsChoices::Text(names)
            }
            AppSettingsEntryId::VideoRenderer => {
                let values = bmz_render::available_wgpu_backends()
                    .into_iter()
                    .map(|backend| match backend {
                        bmz_render::WgpuBackend::Auto => {
                            crate::config::app_config::RendererBackend::Auto
                        }
                        bmz_render::WgpuBackend::Vulkan => {
                            crate::config::app_config::RendererBackend::Vulkan
                        }
                        bmz_render::WgpuBackend::Metal => {
                            crate::config::app_config::RendererBackend::Metal
                        }
                        bmz_render::WgpuBackend::Dx12 => {
                            crate::config::app_config::RendererBackend::Dx12
                        }
                        bmz_render::WgpuBackend::Gl => {
                            crate::config::app_config::RendererBackend::Gl
                        }
                    })
                    .collect();
                AppSettingsChoices::Renderers(values)
            }
            _ => AppSettingsChoices::None,
        }
    }

    pub(super) fn cancel_settings_edit(&mut self) {
        let Some(session) = self.select.settings_edit.take() else {
            return;
        };
        match session {
            SelectSettingsEditSession::Profile(session) => {
                let entry_id = session.entry_id;
                let score_context_before =
                    SelectScoreContext::from_profile(&self.boot.profile_config);
                session.restore(&mut self.boot.profile_config);
                self.sync_select_settings_from_profile_if_needed(entry_id);
                self.sync_changed_select_score_context(score_context_before);
                tracing::info!(?entry_id, "settings edit cancelled");
            }
            SelectSettingsEditSession::App(session) => {
                let entry_id = session.entry_id;
                session.restore(&mut self.boot.app_config);
                tracing::info!(?entry_id, "app settings edit cancelled");
            }
        }
        self.play_system_sound(crate::system_sound::SoundType::FolderClose);
    }

    pub(super) fn commit_settings_edit(&mut self) {
        let Some(session) = self.select.settings_edit.take() else {
            return;
        };
        match session {
            SelectSettingsEditSession::Profile(session) => {
                self.commit_profile_settings_edit(session);
            }
            SelectSettingsEditSession::App(session) => {
                self.commit_app_settings_edit(session);
            }
        }
    }

    fn commit_profile_settings_edit(&mut self, session: SettingsEditSession) {
        let entry_id = session.entry_id;
        self.boot.profile_config.updated_at = now_unix_seconds();
        match save_profile_config(&self.boot.profile_paths.profile_toml, &self.boot.profile_config)
        {
            Ok(()) => {
                self.sync_select_settings_from_profile_if_needed(entry_id);
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                tracing::info!(?entry_id, "settings edit saved");
            }
            Err(error) => {
                tracing::error!(%error, ?entry_id, "failed to save settings");
                let score_context_before =
                    SelectScoreContext::from_profile(&self.boot.profile_config);
                session.restore(&mut self.boot.profile_config);
                self.sync_select_settings_from_profile_if_needed(entry_id);
                self.sync_changed_select_score_context(score_context_before);
            }
        }
    }

    fn commit_app_settings_edit(&mut self, session: AppSettingsEditSession) {
        let entry_id = session.entry_id;
        match save_app_config(&self.boot.app_paths.config_toml, &self.boot.app_config) {
            Ok(()) => {
                if !entry_id.is_audio() {
                    let window = self.window.clone();
                    if let Some(window) = window {
                        self.apply_egui_video_config(&window);
                    }
                }
                self.reload_select_items();
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                tracing::info!(?entry_id, "app settings edit saved");
            }
            Err(error) => {
                tracing::error!(%error, ?entry_id, "failed to save app settings");
                session.restore(&mut self.boot.app_config);
            }
        }
    }

    pub(super) fn apply_select_audio_settings(&mut self) {
        if let Err(error) = save_app_config(&self.boot.app_paths.config_toml, &self.boot.app_config)
        {
            tracing::error!(%error, "failed to save audio settings before apply");
            let text = Localizer::new(self.boot.profile_config.ui.locale());
            self.show_left_overlay_toast(text.text("settings-audio-apply-failed"));
            return;
        }
        let applied = self.reopen_audio_output();
        self.reload_select_items();
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        if applied {
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            self.show_left_overlay_toast(text.text("settings-audio-apply-success"));
        } else {
            self.show_left_overlay_toast(text.text("settings-audio-apply-failed"));
        }
    }

    pub(super) fn begin_key_config_edit(
        &mut self,
        key_mode: bmz_core::lane::KeyMode,
        target: KeyBindingTarget,
    ) {
        self.select.key_config_edit =
            Some(KeyConfigEditSession::begin(key_mode, target, &self.boot.profile_config));
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
        tracing::info!(?key_mode, ?target, "key config listen started");
    }

    pub(super) fn cancel_key_config_edit(&mut self) {
        let Some(session) = self.select.key_config_edit.take() else {
            return;
        };
        let target = session.target;
        session.cancel(&mut self.boot.profile_config);
        self.suppress_select_analog_until_idle();
        self.play_system_sound(crate::system_sound::SoundType::FolderClose);
        tracing::info!(?target, "key config cancelled");
    }

    pub(super) fn commit_key_config_edit(&mut self) {
        let Some(session) = self.select.key_config_edit.take() else {
            return;
        };
        let target = session.target;
        if self.persist_key_config_edit_session(&session) {
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            tracing::info!(?target, "key config saved");
        } else {
            self.suppress_select_analog_until_idle();
        }
    }

    pub(super) fn apply_key_config_control(&mut self, control: &str) {
        let Some(session) = self.select.key_config_edit.as_ref() else {
            return;
        };
        if !session.listening {
            return;
        }
        if !matches!(
            session.target.slot(),
            KeyBindingSlot::KeyboardPrimary | KeyBindingSlot::KeyboardSecondary
        ) {
            return;
        }
        let target = session.target;
        let key_mode = session.key_mode;
        if let Err(error) =
            apply_play_binding(&mut self.boot.profile_config.input, key_mode, target, control)
        {
            tracing::warn!(%error, ?key_mode, ?target, control, "failed to apply key binding");
            return;
        }
        self.commit_key_config_edit();
    }

    pub(super) fn apply_key_config_gamepad(&mut self, control: &str) {
        let Some(session) = self.select.key_config_edit.as_ref() else {
            return;
        };
        if !session.listening || !session.target.slot().is_controller() {
            return;
        }
        let target = session.target;
        let key_mode = session.key_mode;
        if let Err(error) =
            apply_play_binding(&mut self.boot.profile_config.input, key_mode, target, control)
        {
            tracing::warn!(%error, ?key_mode, ?target, control, "failed to apply controller binding");
            return;
        }
        self.commit_key_config_edit();
    }

    pub(super) fn clear_key_config_binding(&mut self) {
        let Some(session) = self.select.key_config_edit.as_ref() else {
            return;
        };
        if !session.listening {
            return;
        }
        let target = session.target;
        let key_mode = session.key_mode;
        if let Err(error) =
            clear_play_binding(&mut self.boot.profile_config.input, key_mode, target)
        {
            tracing::warn!(%error, ?key_mode, ?target, "failed to clear key binding");
            return;
        }
        self.commit_key_config_edit();
    }

    pub(super) fn adjust_settings_edit(&mut self, direction: i32) {
        if direction == 0 {
            return;
        }
        let Some(session) = self.select.settings_edit.as_ref() else {
            return;
        };
        let profile_entry = match session {
            SelectSettingsEditSession::Profile(session) => Some(session.entry_id),
            SelectSettingsEditSession::App(_) => None,
        };
        let score_context_before =
            profile_entry.map(|_| SelectScoreContext::from_profile(&self.boot.profile_config));
        let changed = match session {
            SelectSettingsEditSession::Profile(session) => {
                let delta = direction
                    * crate::config::settings_registry::settings_adjust_step(session.entry_id);
                adjust_settings_draft(&mut self.boot.profile_config, session, delta)
            }
            SelectSettingsEditSession::App(session) => {
                session.adjust(&mut self.boot.app_config, direction)
            }
        };
        if !changed {
            return;
        }
        if let Some(entry_id) = profile_entry {
            self.sync_select_settings_from_profile_if_needed(entry_id);
            self.sync_changed_select_score_context(
                score_context_before.expect("profile edit captured score context"),
            );
        }
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    pub(super) fn sync_select_settings_from_profile_if_needed(
        &mut self,
        entry_id: SettingsEntryId,
    ) {
        self.sync_select_play_options_from_profile_if_needed(entry_id);
        if entry_id == SettingsEntryId::SelectInputMode {
            self.select.select_keys =
                SelectKeyBindings::from_profile(&self.boot.profile_config.input);
            self.sync_select_holds_from_pressed_controls();
        }
        if matches!(
            entry_id,
            SettingsEntryId::AnalogScratch1P
                | SettingsEntryId::AnalogScratchSensitivity1P
                | SettingsEntryId::AnalogScratchThreshold1P
                | SettingsEntryId::AnalogScratch2P
                | SettingsEntryId::AnalogScratchSensitivity2P
                | SettingsEntryId::AnalogScratchThreshold2P
        ) {
            self.apply_gamepad_analog_config();
        }
        if SettingsEntryId::VOLUME_ENTRIES.contains(&entry_id) {
            self.sync_realtime_profile_settings();
        }
        if entry_id == SettingsEntryId::Language {
            self.reload_select_items();
        }
        if entry_id == SettingsEntryId::HispeedMode {
            self.reload_select_items();
        }
        if entry_id == SettingsEntryId::ShowFps
            && let Some(egui) = self.ui.egui.as_mut()
        {
            egui.set_show_fps(self.boot.profile_config.ui.show_fps);
        }
    }

    pub(super) fn sync_changed_gamepad_analog_config_from_profile(
        &mut self,
        before: &ProfileInputConfig,
    ) {
        let after = &self.boot.profile_config.input;
        if before.gamepad1 == after.gamepad1 && before.gamepad2 == after.gamepad2 {
            return;
        }
        self.apply_gamepad_analog_config();
    }

    pub(super) fn apply_gamepad_analog_config(&mut self) {
        let configs = gamepad_scratch_configs(&self.boot.profile_config.input);
        let slots =
            resolve_gamepad_runtime_slots(&self.boot.app_config.input, self.gamepad.as_ref());
        if let Some(gamepad) = &mut self.gamepad {
            gamepad.set_analog_config(
                configs,
                crate::input::gamepad::GamepadSlotMap::from_device_ids(slots),
            );
        }
        self.reset_select_analog_scroll();
        self.reset_play_analog_scroll();
        self.clear_result_ir_scroll_input();
    }

    pub(super) fn sync_select_play_options_from_profile_if_needed(
        &mut self,
        entry_id: SettingsEntryId,
    ) {
        if !entry_id.is_play_setting() {
            return;
        }
        self.sync_select_play_options_from_profile();
        self.invalidate_play_preload();
        self.play.play_media_cache = None;
    }

    pub(super) fn sync_select_play_options_from_profile(&mut self) {
        let options = select_play_options_from_profile(&self.boot.profile_config.play);
        self.set_select_play_options(options);
    }

    pub(super) fn sync_changed_select_play_options_from_profile(
        &mut self,
        before: &PlayDefaultsConfig,
    ) {
        let current = self.current_select_play_options();
        let next = merge_changed_select_play_options_from_profile(
            current,
            before,
            &self.boot.profile_config.play,
        );
        if next != current {
            self.set_select_play_options(next);
            tracing::info!("applied profile play settings to select options");
        }
    }

    pub(super) fn sync_changed_select_score_context(&mut self, before: SelectScoreContext) {
        let after = SelectScoreContext::from_profile(&self.boot.profile_config);
        if before == after {
            return;
        }

        self.select.select_folder_summaries.sync_score_context(
            &mut self.select.select_items,
            self.boot.profile_config.play.ln_mode_policy,
            self.boot.profile_config.play.rule_mode,
        );
        self.reload_select_items();
        self.invalidate_play_preload();
        // Result画面からのリトライ用cacheも古いscore key / LN変換済みchartを持つ。
        self.play.play_media_cache = None;
        tracing::info!(
            rule_mode = after.rule_mode.as_str(),
            ln_mode = after.ln_mode_policy.display_label(),
            "applied profile score context to select"
        );
    }

    pub(super) fn current_select_play_options(&self) -> CurrentPlayOptions {
        CurrentPlayOptions {
            arrange: self.select.arrange_option,
            arrange_2p: self.select.arrange_option_2p,
            target: self.select.target_option,
            gauge: self.select.gauge_option,
            gauge_auto_shift: self.select.gauge_auto_shift_option,
            bottom_shiftable_gauge: self.select.bottom_shiftable_gauge_option,
            double_option: self.select.double_option,
            hs_fix: self.select.hs_fix_option,
            session_mode: self.select.session_mode,
        }
    }

    pub(super) fn set_select_play_options(&mut self, options: CurrentPlayOptions) {
        self.select.arrange_option = options.arrange;
        self.select.arrange_option_2p = options.arrange_2p;
        self.select.target_option = options.target;
        self.select.gauge_option = options.gauge;
        self.select.gauge_auto_shift_option = options.gauge_auto_shift;
        self.select.bottom_shiftable_gauge_option = options.bottom_shiftable_gauge;
        self.select.double_option = options.double_option;
        self.select.hs_fix_option = options.hs_fix;
        self.select.session_mode = options.session_mode;
    }

    pub(super) fn route_settings_control(&mut self, control: &str) -> bool {
        let bindings = SettingsBindings::from_profile(&self.boot.profile_config.input);

        if control.starts_with("Axis")
            && (self.select.select_keys.is_select_scratch_up(control)
                || self.select.select_keys.is_select_scratch_down(control))
        {
            return true;
        }

        if self.select.key_config_edit.is_some() {
            if bindings.is_back(control) {
                self.cancel_key_config_edit();
            }
            return true;
        }

        if self.select.settings_edit.is_some() {
            if bindings.is_confirm(control) {
                self.commit_settings_edit();
                return true;
            }
            if bindings.is_back(control) {
                self.cancel_settings_edit();
                return true;
            }
            if bindings.is_increase(control) {
                self.adjust_settings_edit(1);
                return true;
            }
            if bindings.is_decrease(control) {
                self.adjust_settings_edit(-1);
                return true;
            }
            return true;
        }

        if bindings.is_back(control) {
            self.exit_folder();
            return true;
        }
        if let Some(select_move) =
            settings_browse_move_control(control, &bindings, &self.select.select_keys)
        {
            self.move_selection(select_move);
            self.start_select_hold_move(select_move, control.to_string());
            return true;
        }
        if bindings.is_confirm(control) {
            return match self.select.select_items.get(self.select.selected_index) {
                Some(SelectItem::Config(row)) => {
                    self.begin_settings_edit(row.entry_id);
                    true
                }
                Some(SelectItem::AppConfig(row)) => {
                    self.begin_app_settings_edit(row.entry_id);
                    true
                }
                Some(SelectItem::KeyBinding(row)) => {
                    self.begin_key_config_edit(row.key_mode, row.target);
                    true
                }
                Some(SelectItem::Folder { .. }) => {
                    self.enter_or_play_selected();
                    true
                }
                Some(SelectItem::SettingsBack | SelectItem::SettingsClose) => {
                    self.exit_folder();
                    true
                }
                Some(SelectItem::AdvancedSettings) => {
                    self.open_advanced_settings_from_select();
                    true
                }
                Some(SelectItem::ApplyAudioSettings) => {
                    self.apply_select_audio_settings();
                    true
                }
                _ => false,
            };
        }
        false
    }

    pub(super) fn cycle_bga_option(&mut self) {
        self.boot.profile_config.play.bga = cycle_bga_option(self.boot.profile_config.play.bga);
        tracing::info!(
            bga = bga_mode_as_str(self.boot.profile_config.play.bga),
            "bga option changed"
        );
    }

    pub(super) fn toggle_gauge_auto_shift(&mut self) {
        self.select.gauge_auto_shift_option =
            cycle_gauge_auto_shift_option(self.select.gauge_auto_shift_option);
        tracing::info!(
            gauge_auto_shift = gauge_auto_shift_as_str(self.select.gauge_auto_shift_option),
            "gauge auto shift changed"
        );
    }

    pub(super) fn toggle_visual_offset_auto_adjust(&mut self) {
        self.boot.profile_config.judge.visual_offset_auto_adjust =
            !self.boot.profile_config.judge.visual_offset_auto_adjust;
        self.boot.profile_config.updated_at = now_unix_seconds();
        self.sync_realtime_profile_settings();
        tracing::info!(
            visual_offset_auto_adjust = self.boot.profile_config.judge.visual_offset_auto_adjust,
            "visual offset auto adjust changed"
        );
    }

    pub(super) fn apply_play_option_control(&mut self, control: &str) -> bool {
        if self.select.select_keys.is_key1(control) {
            self.select.arrange_option = self.select.arrange_option.cycle();
            tracing::info!(arrange = self.select.arrange_option.as_str(), "arrange option changed");
            true
        } else if self.select.select_keys.is_key2(control) {
            self.select.arrange_option = self.select.arrange_option.cycle_prev();
            tracing::info!(arrange = self.select.arrange_option.as_str(), "arrange option changed");
            true
        } else if self.select.select_keys.is_key8(control) {
            self.select.arrange_option_2p = self.select.arrange_option_2p.cycle();
            tracing::info!(
                arrange_2p = self.select.arrange_option_2p.as_str(),
                "2P arrange changed"
            );
            true
        } else if self.select.select_keys.is_key9(control) {
            self.select.arrange_option_2p = self.select.arrange_option_2p.cycle_prev();
            tracing::info!(
                arrange_2p = self.select.arrange_option_2p.as_str(),
                "2P arrange changed"
            );
            true
        } else if self.select.select_keys.is_ui_key3(control) {
            self.select.gauge_option = cycle_gauge_option(self.select.gauge_option);
            tracing::info!(gauge = ?self.select.gauge_option, "gauge option changed");
            true
        } else if self.select.select_keys.is_ui_key4(control) {
            self.select.gauge_option = cycle_gauge_option_prev(self.select.gauge_option);
            tracing::info!(gauge = ?self.select.gauge_option, "gauge option changed");
            true
        } else if self.select.select_keys.is_ui_key5(control) {
            if !self.begin_selected_play_mode_edit() {
                return false;
            }
            self.select.hs_fix_option = self.select.hs_fix_option.cycle();
            self.boot.profile_config.play.hs_fix =
                hs_fix_config_from_option(self.select.hs_fix_option);
            self.finish_selected_play_mode_edit();
            tracing::info!(hs_fix = self.select.hs_fix_option.as_str(), "HS-FIX option changed");
            true
        } else if self.select.select_keys.is_ui_key6(control) {
            self.select.double_option = self.select.double_option.cycle();
            if matches!(
                self.select.double_option,
                DoubleOption::Battle | DoubleOption::BattleAutoScratch
            ) && self.boot.profile_config.play.key_mode_conversion
                != KeyModeConversionConfig::Off
            {
                self.boot.profile_config.play.key_mode_conversion = KeyModeConversionConfig::Off;
                self.boot.profile_config.updated_at = now_unix_seconds();
                self.invalidate_play_preload();
                self.play.play_media_cache = None;
            }
            tracing::info!(
                double_option = self.select.double_option.as_str(),
                "double option changed"
            );
            true
        } else if self.select.select_keys.is_ui_key7(control) {
            self.set_session_mode(self.select.session_mode.cycle());
            tracing::info!(
                session_mode = self.select.session_mode.as_str(),
                "session mode changed"
            );
            true
        } else {
            false
        }
    }

    pub(super) fn apply_gamepad_play_option_control(
        &mut self,
        device: DeviceId,
        control: &str,
    ) -> bool {
        let app_config = self.play_session_app_config();
        let slots = crate::input::gamepad::GamepadSlotMap::from_runtime_or_legacy(
            app_config.input.gamepad_slot_runtime_device_ids,
            app_config.input.gamepad_slot_gilrs_ids,
        );
        match select_option_lane_for_gamepad(
            &self.boot.profile_config.input,
            slots,
            device,
            control,
        ) {
            Some(Lane::Key1) => {
                self.select.arrange_option = self.select.arrange_option.cycle();
                tracing::info!(
                    arrange = self.select.arrange_option.as_str(),
                    "arrange option changed"
                );
                true
            }
            Some(Lane::Key2) => {
                self.select.arrange_option = self.select.arrange_option.cycle_prev();
                tracing::info!(
                    arrange = self.select.arrange_option.as_str(),
                    "arrange option changed"
                );
                true
            }
            Some(Lane::Key8) => {
                self.select.arrange_option_2p = self.select.arrange_option_2p.cycle();
                tracing::info!(
                    arrange_2p = self.select.arrange_option_2p.as_str(),
                    "2P arrange changed"
                );
                true
            }
            Some(Lane::Key9) => {
                self.select.arrange_option_2p = self.select.arrange_option_2p.cycle_prev();
                tracing::info!(
                    arrange_2p = self.select.arrange_option_2p.as_str(),
                    "2P arrange changed"
                );
                true
            }
            _ => self.apply_play_option_control(control),
        }
    }

    pub(super) fn apply_assist_option_control(&mut self, control: &str) -> bool {
        let button_id = if self.select.select_keys.is_key1(control) {
            301
        } else if self.select.select_keys.is_key2(control) {
            302
        } else if self.select.select_keys.is_key3(control) {
            303
        } else if self.select.select_keys.is_key4(control) {
            304
        } else if self.select.select_keys.is_key5(control) {
            305
        } else if self.select.select_keys.is_key6(control) {
            306
        } else if self.select.select_keys.is_key7(control) {
            307
        } else {
            return false;
        };
        let changed = self.boot.profile_config.play.assist.toggle_beatoraja_button(button_id);
        if changed {
            self.boot.profile_config.updated_at = now_unix_seconds();
            self.invalidate_play_preload();
        }
        changed
    }

    pub(super) fn apply_gamepad_assist_option_control(
        &mut self,
        device: DeviceId,
        control: &str,
    ) -> bool {
        let app_config = self.play_session_app_config();
        let slots = crate::input::gamepad::GamepadSlotMap::from_runtime_or_legacy(
            app_config.input.gamepad_slot_runtime_device_ids,
            app_config.input.gamepad_slot_gilrs_ids,
        );
        let button_id = match select_option_lane_for_gamepad(
            &self.boot.profile_config.input,
            slots,
            device,
            control,
        ) {
            Some(Lane::Key1) => Some(301),
            Some(Lane::Key2) => Some(302),
            Some(Lane::Key3) => Some(303),
            Some(Lane::Key4) => Some(304),
            Some(Lane::Key5) => Some(305),
            Some(Lane::Key6) => Some(306),
            Some(Lane::Key7) => Some(307),
            _ => None,
        };
        if let Some(button_id) = button_id {
            let changed = self.boot.profile_config.play.assist.toggle_beatoraja_button(button_id);
            if changed {
                self.boot.profile_config.updated_at = now_unix_seconds();
                self.invalidate_play_preload();
            }
            changed
        } else {
            self.apply_assist_option_control(control)
        }
    }

    pub(super) fn set_session_mode(&mut self, session_mode: SessionMode) {
        self.select.session_mode = session_mode;
        self.boot.profile_config.play.session_mode = Some(session_mode);
        self.boot.profile_config.play.auto_play = session_mode.primary_autoplay();
    }

    pub(super) fn apply_target_option_cycle(&mut self, cycle: TargetCycle) {
        self.select.target_option = match cycle {
            TargetCycle::Previous => self.select.target_option.cycle_prev(),
            TargetCycle::Next => self.select.target_option.cycle(),
        };
        tracing::info!(target = self.select.target_option.as_str(), "target option changed");
    }

    pub(super) fn apply_detail_option_control(&mut self, control: &str) -> bool {
        if self.select.select_keys.cycle_bga() == Some(control)
            || self.select.select_keys.is_ui_key1(control)
        {
            self.cycle_bga_option();
            true
        } else if let Some(delta) = green_number_delta_control(control, &self.select.select_keys) {
            self.adjust_select_green_number(delta)
        } else if let Some(delta_ms) =
            visual_offset_delta_control(control, &self.select.select_keys)
        {
            self.adjust_visual_offset_ms(delta_ms)
        } else {
            false
        }
    }

    pub(super) fn adjust_select_green_number(&mut self, delta: i32) -> bool {
        if !self.begin_selected_play_mode_edit() {
            return false;
        }
        if self.boot.profile_config.lane.floating_policy == FloatingPolicyConfig::Disabled {
            return false;
        }
        let current = self.boot.profile_config.lane.target_green_number.max(1);
        let next = adjusted_green_number(current, delta);
        if current == next {
            return false;
        }
        self.boot.profile_config.lane.target_green_number = next;
        self.finish_selected_play_mode_edit();
        self.sync_realtime_profile_settings();
        tracing::info!(target_green_number = next, "select green number changed");
        true
    }

    pub(super) fn adjust_visual_offset_ms(&mut self, delta_ms: i32) -> bool {
        let runtime_mode = crate::app::select_flow_mode_config::play_config_key_mode_for_runtime(
            self.play
                .active_play
                .as_ref()
                .map(|active| active.running.session.play_config_key_mode),
            self.play.pending_play_start.as_ref().map(|pending| pending.play_config_key_mode),
        );
        if let Some(key_mode) = runtime_mode {
            self.boot.profile_config.activate_play_mode(key_mode);
        } else if !self.begin_selected_play_mode_edit() {
            return false;
        }
        let changed = crate::config::settings_registry::adjust_settings_value(
            &mut self.boot.profile_config,
            SettingsEntryId::VisualOffsetMs,
            delta_ms,
        );
        if changed {
            self.finish_selected_play_mode_edit();
            self.sync_realtime_profile_settings();
            tracing::info!(
                visual_offset_ms = self.boot.profile_config.judge.visual_offset_us / 1_000,
                "visual judge offset changed"
            );
        }
        changed
    }

    pub(super) fn apply_select_action(&mut self, action: SelectAction, hold_control: Option<&str>) {
        if self.select.ir_battle.active {
            match action {
                SelectAction::EnterOrPlay => self.start_selected_battle(),
                SelectAction::ExitFolder => {
                    self.close_select_ir_battle();
                }
                SelectAction::OpenFolder => self.handle_select_open_folder_action(),
                SelectAction::Reload => self.reload_from_select_context(),
                SelectAction::AutoplayFolder => self.start_autoplay_folder_selected(),
                SelectAction::OpenPrimaryIr => self.open_primary_ir_for_selected(),
                SelectAction::OpenKeyConfig => self.execute_select_skin_event(13, 0),
                SelectAction::CycleRival => self.cycle_active_rival(1),
                SelectAction::OpenDocuments => self.open_selected_chart_documents(),
                SelectAction::Move(select_move) => {
                    self.move_selection(select_move);
                    if matches!(
                        select_move,
                        SelectMove::Previous
                            | SelectMove::Next
                            | SelectMove::PagePrevious
                            | SelectMove::PageNext
                    ) && let Some(control) = hold_control
                    {
                        self.start_select_hold_move(select_move, control.to_string());
                    }
                }
                SelectAction::FavoriteSong
                | SelectAction::FavoriteChart
                | SelectAction::SameFolder
                | SelectAction::ModeFilter
                | SelectAction::Sort
                | SelectAction::LnMode
                | SelectAction::DifficultyFilter
                | SelectAction::ReplayCycle
                | SelectAction::ReplayPlay => {}
            }
            return;
        }
        match action {
            SelectAction::EnterOrPlay => self.enter_or_play_selected(),
            SelectAction::ExitFolder => self.exit_folder(),
            SelectAction::OpenFolder => self.handle_select_open_folder_action(),
            SelectAction::Reload => self.reload_from_select_context(),
            SelectAction::AutoplayFolder => self.start_autoplay_folder_selected(),
            SelectAction::OpenPrimaryIr => self.open_primary_ir_for_selected(),
            SelectAction::OpenKeyConfig => self.execute_select_skin_event(13, 0),
            SelectAction::CycleRival => self.cycle_active_rival(1),
            SelectAction::OpenDocuments => self.open_selected_chart_documents(),
            SelectAction::FavoriteSong => self.toggle_favorite_song_selected(),
            SelectAction::FavoriteChart => self.toggle_favorite_chart_selected(),
            SelectAction::SameFolder => self.open_same_folder_for_selected(),
            SelectAction::ModeFilter => self.cycle_select_mode_filter(1),
            SelectAction::Sort => self.cycle_select_sort(1),
            SelectAction::LnMode => self.cycle_select_ln_mode(1),
            SelectAction::DifficultyFilter => self.cycle_select_difficulty_filter(1),
            SelectAction::ReplayCycle => {
                self.cycle_selected_replay_slot(1);
            }
            SelectAction::ReplayPlay => {
                self.start_selected_replay_slot();
            }
            SelectAction::Move(select_move) => {
                self.move_selection(select_move);
                if matches!(
                    select_move,
                    SelectMove::Previous
                        | SelectMove::Next
                        | SelectMove::PagePrevious
                        | SelectMove::PageNext
                ) && let Some(control) = hold_control
                {
                    self.start_select_hold_move(select_move, control.to_string());
                }
            }
        }
    }

    pub(super) fn apply_result_action(&mut self, action: ResultAction, course_result: bool) {
        match (course_result, action) {
            (false, ResultAction::Retry) => {
                self.begin_result_exit(ResultExitAction::Retry(ResultRetryMode::SameArrange))
            }
            (true, ResultAction::Retry) => {
                self.begin_result_exit(ResultExitAction::RetryCourseSameArrange)
            }
            (_, ResultAction::Leave) => self.begin_result_exit(ResultExitAction::Leave),
        }
    }
}
