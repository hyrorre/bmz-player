use super::*;

/// 設定パネルの出力デバイス選択 ComboBox 用キャッシュ。
#[derive(Default)]
pub(super) struct AudioDevicePickerState {
    /// 列挙済み出力デバイス名(ASIO ならドライバ名)。
    pub(super) names: Vec<String>,
    /// `names` を列挙したときのバックエンド。変化したら再列挙する。
    pub(super) backend: Option<AudioBackend>,
}

impl EguiLayer {
    /// `show_fps` は右上 FPS オーバーレイの初期表示状態。
    pub fn new(window: &Window, show_fps: bool, font_search_paths: Vec<PathBuf>) -> Self {
        let ctx = egui::Context::default();
        let font_coverage = bmz_render::FontCoverage::Japanese;
        install_cjk_fonts(&ctx, font_coverage, &font_search_paths);
        let state = egui_winit::State::new(
            ctx.clone(),
            ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );
        Self {
            ctx,
            state,
            font_coverage,
            font_search_paths,
            visible: false,
            show_debug: false,
            show_random_trainer: false,
            debug_log_filter: DebugLogFilter::default(),
            debug_log_autoscroll: true,
            show_fps,
            show_settings: false,
            key_config: EguiKeyConfigUiState::default(),
            show_course_editor: false,
            course_editor: CourseEditorUiState::default(),
            skin_ui_path_cache: SkinUiPathCache::default(),
            license_notice_text: None,
            update_dialog_active: false,
            settings_new_root_path: String::new(),
            settings_add_root_error: String::new(),
            settings_new_table_url: String::new(),
            settings_add_table_error: String::new(),
            score_import_path: String::new(),
            score_import_kind: ScoreImportKind::default(),
            score_import_device_type: InputDeviceKind::Keyboard,
            score_import_status: String::new(),
            score_import_error: String::new(),
            replay_import_path: String::new(),
            replay_import_device_type: InputDeviceKind::Keyboard,
            replay_import_overwrite: false,
            replay_import_status: String::new(),
            replay_import_error: String::new(),
            replay_import_progress: None,
            audio_device_picker: AudioDevicePickerState::default(),
            obs_scene_picker: ObsScenePickerState::default(),
            ir_login: IrLoginUiState::default(),
            ir_device_key: IrDeviceKeyUiState::default(),
            profile_manager: ProfileManagerUiState::default(),
            directory_open_status: None,
        }
    }

    /// メニュー表示状態を反転する (F1)。
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.cancel_key_config_listening();
        }
        tracing::info!(visible = self.visible, "egui menu toggled");
    }

    /// 選曲画面の「詳細設定」から egui メニューと本体設定パネルを開く。
    pub fn open_advanced_settings(&mut self) {
        self.visible = true;
        self.show_settings = true;
        tracing::info!("egui advanced settings opened from select");
    }

    /// 選曲スキンの beatoraja event 14 からスキン設定を直接開く。
    pub fn open_skin_settings(&mut self) {
        self.visible = true;
        self.show_settings = true;
        SettingsNavigation::select(&self.ctx, SettingsPage::Skin);
        tracing::info!("egui skin settings opened from select");
    }

    /// profile側からFPS表示設定が変更されたとき、eguiのcheckbox状態も揃える。
    pub fn set_show_fps(&mut self, show_fps: bool) {
        self.show_fps = show_fps;
    }

    pub fn set_score_import_status(&mut self, status: String, error: bool) {
        if error {
            self.score_import_error = status;
            self.score_import_status.clear();
        } else {
            self.score_import_status = status;
            self.score_import_error.clear();
        }
    }

    pub fn set_replay_import_status(&mut self, status: String, error: bool) {
        if error {
            self.replay_import_error = status;
            self.replay_import_status.clear();
        } else {
            self.replay_import_status = status;
            self.replay_import_error.clear();
        }
    }

    pub fn set_replay_import_progress(&mut self, progress: Option<ReplayImportProgress>) {
        self.replay_import_progress = progress;
    }

    pub fn key_config_listening(&self) -> bool {
        self.key_config.listening.is_some()
    }

    pub fn key_config_controller_listening(&self) -> bool {
        self.key_config.listening.is_some_and(|active| active.target.slot().is_controller())
    }

    pub fn cancel_key_config_listening(&mut self) {
        self.key_config.listening = None;
    }

    pub fn capture_key_config_keyboard(&mut self, control: &str) -> EguiKeyConfigInput {
        self.key_config.capture_keyboard(control)
    }

    pub fn capture_key_config_gamepad(&mut self, control: &str) -> EguiKeyConfigInput {
        self.key_config.capture_gamepad(control)
    }

    pub fn set_key_config_status(&mut self, message: String, error: bool) {
        self.key_config.status = Some(EguiKeyConfigStatus { message, error });
    }

    pub fn course_editor_visible(&self) -> bool {
        self.visible && self.show_course_editor
    }

    pub fn course_editor_search_query(&self) -> &str {
        &self.course_editor.search_query
    }

    pub fn set_course_editor_status(&mut self, status: String, error: bool) {
        self.course_editor.status = status;
        self.course_editor.error = error;
    }

    /// winit イベントを egui へ供給する。
    ///
    /// 戻り値が true のとき、その入力は egui が消費したのでゲーム側へ伝播させない。
    /// メニュー非表示中は egui に状態は渡すが消費とは扱わず、ゲーム操作を妨げない。
    pub fn on_window_event(
        &mut self,
        window: &Window,
        event: &WindowEvent,
        practice_overlay: bool,
        select_course_builder: bool,
    ) -> bool {
        let response = self.state.on_window_event(window, event);
        (self.blocks_game_input(practice_overlay) || select_course_builder) && response.consumed
    }

    pub fn blocks_game_input(&self, practice_overlay: bool) -> bool {
        self.visible || practice_overlay || self.update_dialog_active
    }

    /// 設定 metadata や profile 差分検出を含む完全な egui frame が必要かを返す。
    ///
    /// F1 menu 等が閉じている場合は、winit/egui の入力状態と texture delta だけを
    /// 進める idle frame へ切り替えられる。
    pub fn needs_full_frame(
        &self,
        scene: &str,
        practice_overlay: bool,
        select_course_builder: bool,
        has_update_dialog: bool,
    ) -> bool {
        SettingsFeedback::has_pending(&self.ctx)
            || egui_frame_needs_full_state(
                self.visible,
                practice_overlay,
                select_course_builder,
                has_update_dialog,
                scene,
                self.show_settings,
            )
    }

    /// UI が非表示のフレームを最小構成で進める。
    ///
    /// `take_egui_input` と `textures_delta` の消費は継続し、F1 で再表示したときに
    /// 入力状態や managed texture が不整合にならないようにする。
    pub fn run_idle_frame(
        &mut self,
        window: &Window,
        font_coverage: bmz_render::FontCoverage,
    ) -> EguiFrame {
        if font_coverage != self.font_coverage {
            install_cjk_fonts(&self.ctx, font_coverage, &self.font_search_paths);
            self.font_coverage = font_coverage;
        }
        self.update_dialog_active = false;
        let raw_input = self.state.take_egui_input(window);
        let full_output = self.ctx.run_ui(raw_input, |_| {});
        self.state.handle_platform_output(window, full_output.platform_output);
        let primitives = self.ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        EguiFrame {
            primitives,
            textures_delta: full_output.textures_delta,
            pixels_per_point: full_output.pixels_per_point,
        }
    }

    /// 1 フレーム分の UI を構築し、描画データと要求されたアクションを返す。
    pub fn run(&mut self, window: &Window, context: EguiRunContext<'_, '_>) -> EguiOutput {
        let EguiRunContext {
            info,
            log_buffer,
            app_config,
            profile_config,
            random_trainer,
            skin_meta,
            skin_catalog,
            course_result,
            course_preview,
            course_editor,
            mut select_course_builder,
            mut practice,
            mut result_ir,
            profile_root,
            app_paths,
            difficulty_tables,
            update_dialog,
            obs_connection_status,
            connected_gamepads,
        } = context;
        let font_coverage = profile_config.ui.locale().font_coverage();
        if font_coverage != self.font_coverage {
            install_cjk_fonts(&self.ctx, font_coverage, &self.font_search_paths);
            self.font_coverage = font_coverage;
        }
        let text = Localizer::new(profile_config.ui.locale());
        let raw_input = self.state.take_egui_input(window);
        let ctx = self.ctx.clone();
        SettingsFeedback::profile_changed(&ctx, &profile_config.id);
        let show_debug = &mut self.show_debug;
        let show_random_trainer = &mut self.show_random_trainer;
        let show_settings = &mut self.show_settings;
        let show_course_editor = &mut self.show_course_editor;
        let show_fps = &mut self.show_fps;
        let license_notice_text = &mut self.license_notice_text;
        let mut obs_enabled_changed = false;
        let mut save_app_config = false;
        let mut save_profile_config = self.ir_login.poll(profile_config, text);
        self.ir_device_key.poll(text);
        let mut key_config_action = None;
        let mut reset_skin_config = false;
        let mut skin_reload_request = SkinReloadRequest::default();
        let mut trigger_song_rescan = false;
        let mut song_scan_requests = Vec::new();
        let mut table_fetch_urls = Vec::new();
        let mut score_import_request = None;
        let mut replay_import_request = None;
        let mut cancel_replay_import = false;
        let mut apply_audio_output = false;
        let mut check_for_update = false;
        let mut update_dialog_action = None;
        let mut practice_start = false;
        let mut practice_leave = false;
        let mut course_editor_action = None;
        let mut select_course_builder_action = None;
        let settings_editable = !scene_restricts_settings(info.scene);
        let visible_flag = &mut self.visible;
        let ir_login = &mut self.ir_login;
        let directory_open_status = &mut self.directory_open_status;
        let update_dialog_allowed =
            update_dialog.is_some() && (info.scene == "Select" || *show_settings);
        self.update_dialog_active = update_dialog_allowed;
        let full_output = ctx.run_ui(raw_input, |ui| {
            if update_dialog_allowed && let Some(dialog) = update_dialog {
                update_dialog_action = build_update_dialog(ui.ctx(), dialog, text);
            }
            if let Some(practice_ctx) = practice.as_mut() {
                let panel = build_practice_panel(ui.ctx(), practice_ctx, text);
                practice_start |= panel.start_play;
                practice_leave |= panel.leave;
            }
            if let Some(builder) = select_course_builder.as_mut() {
                select_course_builder_action =
                    build_select_course_builder_panel(ui.ctx(), builder, text);
            }
            if *visible_flag {
                let ctx = ui.ctx();
                let result_ir_visible = result_ir.is_some();
                // IR ランキングも egui 補助ウィンドウなので、他の egui
                // ウィンドウと同じ F1 メニュー表示中だけ出す。
                if let Some(state) = result_ir.as_mut() {
                    build_result_ir_panel(ctx, state, text);
                }
                // Course info panels are developer/debug egui overlays, so keep
                // them behind the same F1 menu visibility gate as the other
                // egui windows.
                if let Some(summary) = course_result {
                    build_course_result_panel(ctx, summary, result_ir_visible, text);
                }
                if let Some(preview) = course_preview {
                    build_course_preview_panel(ctx, preview, text);
                }
                build_menu(
                    ctx,
                    visible_flag,
                    MenuPanelVisibility {
                        debug: show_debug,
                        random_trainer: show_random_trainer,
                        settings: show_settings,
                        course_editor: show_course_editor,
                    },
                    app_paths,
                    directory_open_status,
                    text,
                );
                build_debug_panel(
                    ctx,
                    show_debug,
                    info,
                    log_buffer,
                    &mut self.debug_log_filter,
                    &mut self.debug_log_autoscroll,
                    text,
                );
                build_random_trainer_panel(ctx, show_random_trainer, random_trainer, text);
                let profile_name = profile_config.display_name.clone();
                build_settings_window(ctx, show_settings, &profile_name, text, |ui| {
                    let before_app = serde_json::to_value(&*app_config).ok();
                    let before_obs = serde_json::to_value(&app_config.obs).ok();
                    let before_profile = serde_json::to_value(&*profile_config).ok();
                    let profile_settings_actions = build_profile_settings_panel(
                        ui,
                        ProfileSettingsPanelContext {
                            profile: profile_config,
                            app_config,
                            show_fps,
                            ir_login,
                            ir_device_key: &mut self.ir_device_key,
                            profile_manager: &mut self.profile_manager,
                            key_config: &mut self.key_config,
                            profile_root,
                            unrestricted: settings_editable,
                            text,
                        },
                    );
                    save_profile_config |= profile_settings_actions.save;
                    save_app_config |= profile_settings_actions.save_app_config;
                    key_config_action = profile_settings_actions.key_config_action;
                    let settings_actions = build_settings_panel(
                        ui,
                        window,
                        app_config,
                        profile_config,
                        show_fps,
                        settings_editable,
                        difficulty_tables,
                        text,
                        SettingsPanelState {
                            new_root_path: &mut self.settings_new_root_path,
                            add_root_error: &mut self.settings_add_root_error,
                            new_table_url: &mut self.settings_new_table_url,
                            add_table_error: &mut self.settings_add_table_error,
                            score_import_path: &mut self.score_import_path,
                            score_import_kind: &mut self.score_import_kind,
                            score_import_device_type: &mut self.score_import_device_type,
                            score_import_status: &self.score_import_status,
                            score_import_error: &self.score_import_error,
                            replay_import_path: &mut self.replay_import_path,
                            replay_import_device_type: &mut self.replay_import_device_type,
                            replay_import_overwrite: &mut self.replay_import_overwrite,
                            replay_import_status: &self.replay_import_status,
                            replay_import_error: &self.replay_import_error,
                            replay_import_progress: self.replay_import_progress,
                            audio_device_picker: &mut self.audio_device_picker,
                            obs_scene_picker: &mut self.obs_scene_picker,
                            obs_connection_status,
                            connected_gamepads,
                        },
                    );
                    obs_enabled_changed |= settings_actions.obs_enabled_changed;
                    save_app_config |= settings_actions.save;
                    save_profile_config |= settings_actions.save_profile;
                    check_for_update |= settings_actions.check_update;
                    trigger_song_rescan |= settings_actions.rescan;
                    song_scan_requests.extend(settings_actions.song_scan_requests);
                    table_fetch_urls.extend(settings_actions.table_fetch_urls);
                    apply_audio_output |= settings_actions.apply_audio;
                    score_import_request = settings_actions.score_import_request;
                    replay_import_request = settings_actions.replay_import_request;
                    cancel_replay_import = settings_actions.cancel_replay_import;
                    build_third_party_notice_page(ui, app_paths, license_notice_text);
                    let skin_actions = build_skin_panel(
                        ui,
                        &mut profile_config.skin,
                        skin_meta,
                        skin_catalog,
                        app_paths,
                        &mut self.skin_ui_path_cache,
                        text,
                    );
                    save_profile_config |= skin_actions.save;
                    reset_skin_config |= skin_actions.reset;
                    skin_reload_request.union(skin_actions.reload);
                    SettingsFeedback::changed(
                        ctx,
                        before_app != serde_json::to_value(&*app_config).ok(),
                        before_profile != serde_json::to_value(&*profile_config).ok(),
                    );
                    obs_enabled_changed |= before_obs != serde_json::to_value(&app_config.obs).ok();
                });
                if !*show_settings || !SettingsNavigation::load(ctx).accepts_key_capture() {
                    self.key_config.listening = None;
                }
                course_editor_action = build_course_editor_panel(
                    ctx,
                    show_course_editor,
                    &mut self.course_editor,
                    course_editor,
                    profile_root,
                    info.scene == "Select",
                    text,
                );
            }
        });
        SettingsFeedback::changed(&self.ctx, save_app_config, save_profile_config);
        let pending = SettingsFeedback::autosave(&self.ctx, !self.show_settings || !self.visible);
        save_app_config = pending.0;
        save_profile_config = pending.1;
        self.state.handle_platform_output(window, full_output.platform_output);
        let primitives = self.ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
        EguiOutput {
            frame: EguiFrame {
                primitives,
                textures_delta: full_output.textures_delta,
                pixels_per_point: full_output.pixels_per_point,
            },
            obs_enabled_changed,
            save_app_config,
            save_profile_config,
            key_config_action,
            reset_skin_config,
            skin_reload_request,
            trigger_song_rescan,
            song_scan_requests,
            table_fetch_urls,
            score_import_request,
            replay_import_request,
            cancel_replay_import,
            apply_audio_output,
            check_for_update,
            update_dialog_action,
            practice_start,
            practice_leave,
            course_editor_action,
            select_course_builder_action,
        }
    }
}

impl EguiKeyConfigUiState {
    fn capture_keyboard(&mut self, control: &str) -> EguiKeyConfigInput {
        let Some(active) = self.listening else {
            return EguiKeyConfigInput::NotHandled;
        };
        match control {
            "Escape" => {
                self.listening = None;
                EguiKeyConfigInput::Consumed
            }
            "Delete" | "Backspace" => {
                self.listening = None;
                EguiKeyConfigInput::Action(EguiKeyConfigAction::Clear {
                    key_mode: active.key_mode,
                    target: active.target,
                })
            }
            _ if active.target.slot().is_controller() => EguiKeyConfigInput::Consumed,
            _ => {
                self.listening = None;
                EguiKeyConfigInput::Action(EguiKeyConfigAction::Bind {
                    key_mode: active.key_mode,
                    target: active.target,
                    control: control.to_string(),
                })
            }
        }
    }

    fn capture_gamepad(&mut self, control: &str) -> EguiKeyConfigInput {
        let Some(active) = self.listening else {
            return EguiKeyConfigInput::NotHandled;
        };
        if !active.target.slot().is_controller() {
            return EguiKeyConfigInput::Consumed;
        }
        self.listening = None;
        EguiKeyConfigInput::Action(EguiKeyConfigAction::Bind {
            key_mode: active.key_mode,
            target: active.target,
            control: control.to_string(),
        })
    }
}

pub(super) fn egui_frame_needs_full_state(
    visible: bool,
    practice_overlay: bool,
    select_course_builder: bool,
    has_update_dialog: bool,
    scene: &str,
    show_settings: bool,
) -> bool {
    visible
        || practice_overlay
        || select_course_builder
        || (has_update_dialog && (scene == "Select" || show_settings))
}

/// egui のデフォルトフォントは CJK グリフを含まないため、locale の地域別字形を
/// 優先した全 CJK face を各フォントファミリの末尾 fallback として登録する。
pub(super) fn install_cjk_fonts(
    ctx: &egui::Context,
    preferred: bmz_render::FontCoverage,
    font_search_paths: &[PathBuf],
) {
    let fallbacks = bmz_render::renderer::load_cjk_font_fallback_data(preferred, font_search_paths);
    ctx.set_fonts(cjk_font_definitions(fallbacks));
}

pub(super) fn cjk_font_definitions(
    fallbacks: Vec<(bmz_render::FontCoverage, bmz_render::renderer::SystemFontData)>,
) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    for (coverage, data) in fallbacks {
        let name = cjk_font_name(coverage).to_owned();
        let mut font_data = egui::FontData::from_owned(data.bytes).tweak(egui::FontTweak {
            scale: 1.0,
            y_offset_factor: 0.26,
            y_offset: 0.0,
            ..Default::default()
        });
        font_data.index = data.font_index;
        fonts.font_data.insert(name.clone(), std::sync::Arc::new(font_data));
        // Latin は egui 既定フォントの先頭順を維持し、欠落グリフだけ CJK face へ
        // preferred 順で fallback させる。
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            if let Some(chain) = fonts.families.get_mut(&family) {
                chain.push(name.clone());
            }
        }
    }
    fonts
}

pub(super) const fn cjk_font_name(coverage: bmz_render::FontCoverage) -> &'static str {
    match coverage {
        bmz_render::FontCoverage::Japanese => "bmz_cjk_japanese",
        bmz_render::FontCoverage::Korean => "bmz_cjk_korean",
        bmz_render::FontCoverage::SimplifiedChinese => "bmz_cjk_simplified_chinese",
        bmz_render::FontCoverage::TraditionalChinese => "bmz_cjk_traditional_chinese",
        bmz_render::FontCoverage::HongKong => "bmz_cjk_hong_kong",
    }
}

#[cfg(test)]
mod key_config_capture_tests {
    use super::*;
    use crate::config::profile_config::{InputActionConfig, LaneConfig};

    fn keyboard_target() -> EguiKeyConfigListenTarget {
        EguiKeyConfigListenTarget {
            key_mode: KeyMode::K7,
            target: KeyBindingTarget::Key {
                lane: LaneConfig::Key1,
                slot: KeyBindingSlot::KeyboardPrimary,
            },
        }
    }

    fn controller_target() -> EguiKeyConfigListenTarget {
        EguiKeyConfigListenTarget {
            key_mode: KeyMode::K7,
            target: KeyBindingTarget::Action {
                action: InputActionConfig::E1,
                slot: KeyBindingSlot::Controller,
            },
        }
    }

    #[test]
    fn keyboard_listener_ignores_gamepad_and_captures_keyboard() {
        let mut state =
            EguiKeyConfigUiState { listening: Some(keyboard_target()), ..Default::default() };
        assert_eq!(state.capture_gamepad("Button1"), EguiKeyConfigInput::Consumed);
        assert!(state.listening.is_some());
        assert!(matches!(
            state.capture_keyboard("Q"),
            EguiKeyConfigInput::Action(EguiKeyConfigAction::Bind { control, .. }) if control == "Q"
        ));
        assert!(state.listening.is_none());
    }

    #[test]
    fn controller_listener_supports_axis_clear_and_escape() {
        let mut state =
            EguiKeyConfigUiState { listening: Some(controller_target()), ..Default::default() };
        assert!(matches!(
            state.capture_gamepad("Axis1+"),
            EguiKeyConfigInput::Action(EguiKeyConfigAction::Bind { control, .. }) if control == "Axis1+"
        ));

        state.listening = Some(controller_target());
        assert!(matches!(
            state.capture_keyboard("Delete"),
            EguiKeyConfigInput::Action(EguiKeyConfigAction::Clear { .. })
        ));

        state.listening = Some(controller_target());
        assert_eq!(state.capture_keyboard("Escape"), EguiKeyConfigInput::Consumed);
        assert!(state.listening.is_none());
    }
}
