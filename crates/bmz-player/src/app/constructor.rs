use super::*;

impl WinitApp {
    pub(super) fn new(
        boot: BootstrappedApp,
        options: AppOptions,
        startup_started_at: Instant,
        audio_runtime: Option<AudioRuntime>,
        system_audio: Option<crate::audio::SystemAudio>,
        shutdown_requested: Arc<AtomicBool>,
        event_proxy: EventLoopProxy<AppUserEvent>,
        log_buffer: LogBuffer,
        maintenance_select_tx: tokio::sync::watch::Sender<bool>,
        raw_input_bridge: Option<crate::input::rawinput::RawInputBridge>,
    ) -> Result<Self> {
        let mut boot = boot;
        if let Some(cli_renderer) = options.renderer.clone() {
            tracing::info!(?cli_renderer, "overriding renderer backend via CLI option");
            boot.app_config.video.renderer = cli_renderer;
        }

        // ネットワークへ出る前に、DBから必要なURLだけ選定しておく。実際の取得は
        // 最初の描画後に開始するため、初回起動でもウィンドウ表示を待たせない。
        let startup_table_fetch_urls = startup_difficulty_table_fetch_urls_for_boot(&boot);
        let startup_rival_sync = RianRivalSyncRequest::from_profile(&boot.profile_config);

        let folder_stack = initial_folder_stack(&boot.app_config);
        let initial_mode_filter =
            SelectModeFilter::from_str_or_default(&boot.profile_config.select.mode_filter);
        let select_difficulty_filter = SelectDifficultyFilter::from_str_or_default(
            &boot.profile_config.select.difficulty_filter,
        );
        let select_sort = SelectSort::from_str_or_default(&boot.profile_config.select.sort);
        let (select_items, select_mode_filter) = load_items_for_stack(
            &boot,
            &folder_stack,
            &[],
            initial_mode_filter,
            select_difficulty_filter,
            select_sort,
        );
        boot.profile_config.select.mode_filter = select_mode_filter.as_str().to_string();
        if let Some(key_mode) = crate::app::select_flow_mode_config::select_item_play_mode(
            select_items.first(),
            select_mode_filter,
        ) {
            boot.profile_config.activate_play_mode(key_mode);
        }
        let boot_chart_id = resolve_boot_chart_id(&boot.library_db, &options);
        log_startup_options(&options);

        let session_mode = if options.autoplay_on_start {
            SessionMode::Autoplay
        } else {
            session_mode_from_profile(&boot.profile_config.play)
        };
        let selected_replay_slot =
            select_items.first().and_then(crate::app::play_flow_replay::first_replay_slot_for_item);
        let gauge_option = if boot.profile_config.play.gauge == GaugeTypeConfig::AutoShift {
            GaugeTypeConfig::ExHard
        } else {
            boot.profile_config.play.gauge
        };
        let gauge_auto_shift_option =
            if boot.profile_config.play.gauge == GaugeTypeConfig::AutoShift {
                GaugeAutoShiftConfig::BestClear
            } else {
                boot.profile_config.play.gauge_auto_shift
            };
        let bottom_shiftable_gauge_option = boot.profile_config.play.bottom_shiftable_gauge;
        let arrange_option = arrange_option_from_profile(boot.profile_config.play.random);
        let arrange_option_2p = arrange_option_from_profile(boot.profile_config.play.random2);
        let double_option = double_option_from_profile(boot.profile_config.play.double_option);
        let hs_fix_option = hs_fix_option_from_profile(boot.profile_config.play.hs_fix);
        let target_option = target_option_from_profile(boot.profile_config.play.target);
        let select_keys = SelectKeyBindings::from_profile(&boot.profile_config.input);
        let mut renderer = Box::new(Renderer::default());
        renderer.set_default_font_search_paths(vec![boot.app_paths.bundled_noto_cjk_font_root()]);
        renderer.set_default_font_coverage(boot.profile_config.ui.locale().font_coverage());
        renderer
            .set_internal_resolution_mode(config_internal_resolution_mode(&boot.app_config.video));
        let skin_catalog = scan_skin_catalog(&boot.app_paths);
        let mut skin_pipeline = SkinPipelineRuntime::new();
        let (
            default_skin_manifest,
            initial_skin_video_sources,
            pending_select_skin,
            pending_decide_skin,
            pending_result_skin,
        ) = load_initial_skin_textures(
            renderer.as_mut(),
            &boot.app_paths,
            &skin_pipeline,
            0,
            &boot.profile_config.display_name,
            &boot.profile_config.skin,
            options.lua_skin_runtime_mode,
        );
        skin_pipeline.set_pending(SkinKind::Select, pending_select_skin);
        skin_pipeline.set_pending(SkinKind::Decide, pending_decide_skin);
        skin_pipeline.set_pending(SkinKind::Result, pending_result_skin);
        let now = Instant::now();

        let mut gamepad = if boot.app_config.input.gamepad_enabled {
            initialize_gamepad_backend(
                boot.app_config.input.gamepad_backend,
                gamepad_scratch_configs(&boot.profile_config.input),
                raw_input_bridge.clone(),
            )
        } else {
            None
        };
        let gamepad_slots = resolve_gamepad_runtime_slots(&boot.app_config.input, gamepad.as_ref());
        if let Some(backend) = &mut gamepad {
            backend.set_analog_config(
                gamepad_scratch_configs(&boot.profile_config.input),
                crate::input::gamepad::GamepadSlotMap::from_device_ids(gamepad_slots),
            );
        }

        let initial_window_mode = boot.app_config.video.mode.clone();
        let applied_obs_config = boot.app_config.obs.clone();
        let obs_controller = crate::obs::ObsController::spawn(applied_obs_config.clone());

        // システム SE / BGM の候補を起動時に一度だけスキャンする。
        // - `profile.[system_sound].bgm_dir` / `se_dir` が指定されていれば再帰スキャンして
        //   セットを集め、その中からランダム選択する(beatoraja 互換)。選曲画面へ戻る
        //   ときはスキャン済み候補から再抽選する。
        // - 空なら scan を省略し、`default_sound_dir` だけにフォールバックする。
        let system_sound_catalog = system_sound_catalog_from_boot(&boot);
        let select_preview =
            system_audio.as_ref().map(|audio| SelectChartPreview::new(audio.engine()));
        let select_assets =
            SelectAssetRuntime::new(select_preview, boot.app_paths.library_db.clone());
        let audio_output_open_attempted = audio_runtime.is_some();
        let player_stats = player_stats_snapshot(
            &boot.score_db,
            &boot.library_db,
            boot.profile_config.statistics.day_start_hour,
        );
        let initial_result_skin_signature = result_skin_signature_for_config(
            &boot.profile_config.skin,
            ResultSkinSlot::Normal,
            lua_runtime_state_with_mode(
                lua_runtime_state_for_result(
                    false,
                    None,
                    false,
                    false,
                    KeyMode::default(),
                    BTreeMap::new(),
                    &boot.profile_config.display_name,
                ),
                options.lua_skin_runtime_mode,
            ),
        );
        let difficulty_tables = match boot.library_db.list_difficulty_tables() {
            Ok(tables) => tables,
            Err(error) => {
                tracing::warn!(%error, "failed to list difficulty tables for egui");
                Vec::new()
            }
        };
        let select_folder_summary_ln_policy = boot.profile_config.play.ln_mode_policy;
        let select_folder_summary_rule_mode = boot.profile_config.play.rule_mode;
        let select_folder_summaries = SelectFolderSummaryRuntime::new(
            boot.app_paths.library_db.clone(),
            boot.profile_paths.score_db.clone(),
            &folder_stack,
            select_folder_summary_ln_policy,
            select_folder_summary_rule_mode,
        )?;
        let rian_table_identity = RianTableIdentity::from_ir_config(&boot.profile_config.ir);
        let table_fetch = TableFetchRuntime::new(startup_table_fetch_urls, rian_table_identity);
        let queued_update_check = (boot.app_config.updates.enabled
            && boot.app_config.updates.check_on_startup)
            .then_some(("startup update check", false));

        let mut app = Self {
            boot,
            window: None,
            first_frame_startup_completed: false,
            shutdown_requested,
            renderer,
            input: AppInputRuntime::default(),
            raw_input_bridge,
            gamepad,
            event_proxy,
            frame: FrameRuntime::new(now),
            deferred_boot: deferred_boot_action(boot_chart_id, &options),
            select: SelectRuntimeState {
                autoplay_folder: None,
                select_ir: crate::screens::select_ir::SelectIrRanking::default(),
                ir_battle: crate::app::select_ir_battle::SelectIrBattleRuntime::default(),
                player_stats,
                score_refresh: SelectScoreRefreshState::default(),
                course_builder: None,
                select_items,
                replay_slot_cache: RefCell::new(None),
                select_distribution_cache: RefCell::new(HashMap::new()),
                difficulty_tables,
                table_breadcrumb_cache: RefCell::new(HashMap::new()),
                select_folder_summaries,
                selected_index_stack: vec![0; folder_stack.len()],
                folder_stack,
                selected_index: 0,
                arrange_option,
                arrange_option_2p,
                random_trainer: RandomTrainerState::default(),
                target_option,
                gauge_option,
                gauge_auto_shift_option,
                bottom_shiftable_gauge_option,
                double_option,
                hs_fix_option,
                session_mode,
                select_mode_filter,
                select_difficulty_filter,
                selected_replay_slot,
                select_sort,
                select_keys,
                select_bar_scroll_direction: 0,
                select_bar_scroll_duration: Duration::ZERO,
                select_hold_move: None,
                select_hold_started_at: None,
                select_hold_last_trigger_at: None,
                select_hold_control: None,
                select_analog_scroll_buffer: 0,
                select_analog_last_tick_at: None,
                select_analog_suppress_until_idle: false,
                select_scene_timer_armed: false,
                select_scene_started_at: now,
                select_bar_started_at: now,
                option_panel_started_at: now,
                option_panel_off_started_at: [None; 6],
                select_option_panel: 0,
                select_exit_hold_started_at: None,
                select_assets,
                settings_edit: None,
                key_config_edit: None,
                search: SelectSearchRuntime::new(now),
                select_slider_dragging_type: None,
            },
            play: PlayRuntimeState {
                active_play: None,
                active_course: None,
                last_play_snapshot: None,
                pending_decide: None,
                pending_play_start: None,
                pending_play_preload: None,
                pending_course_stage_launch: None,
                pending_course_metrics: None,
                preloaded_play_session: None,
                play_preload_generation: 0,
                course_metrics_generation: 0,
                play_media_cache: None,
                play_ending: None,
                last_started_chart_id: None,
                last_battle_target: None,
                play_table_text_primary: String::new(),
                play_table_text_secondary: String::new(),
                play_table_text_fallback: String::new(),
                play_option_input: None,
                play_analog_scroll_buffer: 0,
                play_analog_last_tick_at: None,
                play_scene_started_at: now,
                play_ready_sound_started_at: None,
                play_ready_last_control_hold_at: None,
                decide_sound_stopped_for_chart_start: false,
                bga_preload: BgaPreloadRuntime::default(),
                play_stagefile_source: None,
                play_stagefile_loaded: false,
                play_stagefile_size: None,
                play_backbmp_source: None,
                play_backbmp_loaded: false,
                last_play_start_press_at: None,
                play_lane_target: PlayLaneTarget::Lift,
                decide_e1_held: false,
                play_e1_held: false,
                play_e2_held: false,
                play_e3_held: false,
                play_exit_hold_started_at: None,
                practice_session: None,
                practice_chart_zero_time: None,
            },
            result: ResultRuntimeState {
                prepared_course_finish: None,
                finished_course: None,
                finished_course_skin_summary: None,
                finished_course_hash: None,
                finished_course_rian_hash_v1: None,
                finished_course_bms_ir_key: None,
                finished_course_ir_attempted: false,
                finished_play: None,
                result_favorite_chart: false,
                result_ir: None,
                last_play_session_mode: SessionMode::Normal,
                result_scene_started_at: now,
                result_skin_audio: None,
                result_exit: None,
                result_key5_held: false,
                result_key7_held: false,
                result_gauge_graph_type: GaugeType::Normal as i32,
                result_panel: 0,
                result_ir_scroll: ResultIrScrollRuntime::default(),
            },
            jobs: AppJobs {
                table_fetch,
                pending_song_scan: None,
                pending_replay_import: None,
                queued_song_scans: VecDeque::new(),
                pending_chart_download: None,
                song_scan_progress: None,
                pending_update_check: None,
                pending_update_check_reports_up_to_date: false,
                queued_update_check,
                pending_update_download: None,
                update_prompt: None,
                update_dismissed_session_version: None,
                startup_rival_sync,
                pending_rival_sync: None,
                startup_course_link_repair: true,
                pending_course_link_repair: None,
                maintenance_select_tx,
            },
            integrations: IntegrationRuntimeState {
                obs_controller,
                applied_obs_config,
                exit_configs_saved: false,
                last_scene_kind: None,
                discord_presence: None,
                discord_presence_config: None,
                last_obs_event_key: None,
            },
            smoke: SmokeRuntime {
                smoke_exit_after_frames: options.smoke_exit_after_frames,
                smoke_exit_after_play_frames: options.smoke_exit_after_play_frames,
                smoke_exit_after_result_frames: options.smoke_exit_after_result_frames,
                smoke_exit_on_result: options.smoke_exit_on_result,
                smoke_screenshot_path: options.smoke_screenshot_path.as_ref().map(PathBuf::from),
                left_overlay_toast: None,
                rendered_frames: 0,
                rendered_play_frames: 0,
                rendered_result_frames: 0,
                app_started_at: now,
                startup_started_at,
                first_present_logged: false,
            },
            skin: SkinRuntimeState {
                lua_runtime_mode: options.lua_skin_runtime_mode,
                skin_catalog,
                skin_defs_cache: BTreeMap::new(),
                default_skin_manifest,
                skin_pipeline,
                skin_video_sources: initial_skin_video_sources,
                pending_skin_render_probe: None,
                last_play_skin_signature: None,
                last_result_skin_signature: Some(initial_result_skin_signature),
            },
            audio: AppAudioRuntimeState {
                draining_audio: None,
                audio_runtime,
                audio_output_open_attempted,
                audio_diagnostics_last_log_at: now,
                audio_diagnostics_last: None,
                input_diagnostics_last_sequence: 0,
                system_audio,
                system_sound_catalog,
                system_sound: None,
                pending_system_sound: None,
                system_sound_generation: 0,
            },
            ui: UiRuntimeState {
                egui: None,
                log_buffer,
                applied_window_mode: initial_window_mode,
                exclusive_fullscreen_fallback_active: false,
                device_events_reconfigure_pending: false,
                focused: true,
                last_cursor_action_at: now,
                last_cursor_position: None,
                cursor_visible: true,
            },
            course_editor_cache: course_editor::CourseEditorDataCache::default(),
        };
        if options.boot_result_sample {
            tracing::info!("booting directly into synthetic result screen");
            app.result.finished_play = Some(debug_boot_finished_play_session());
            app.result.result_gauge_graph_type = app
                .result
                .finished_play
                .as_ref()
                .map(|finished| finished.summary.gauge_type as i32)
                .unwrap_or(GaugeType::Normal as i32);
            app.result.result_key5_held = false;
            app.result.result_key7_held = false;
            app.result.result_scene_started_at = Instant::now();
        }
        if app.audio.system_audio.is_some() {
            app.start_system_sound_load();
        }
        app.sync_discord_presence_config();
        Ok(app)
    }
}
