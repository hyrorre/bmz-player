use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use bmz_chart::model::{BgaAssetRef, PlayableChart};
use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::input::{InputKind, ScratchDirection};
use bmz_core::lane::{KeyMode, LANE_COUNT, Lane};
use bmz_core::time::TimeUs;
use bmz_gameplay::input::backend::{
    DeviceId, DeviceInputEvent, InputBackend, InputBouncePolicy, PhysicalControl,
};
use bmz_gameplay::input::binding::LaneBinding;
use bmz_gameplay::rule::RuleMode;
use bmz_gameplay::session::{FloatingPolicy, HispeedMode, PlaySkinOffset};
use bmz_render::assets::{RgbaImageAsset, load_chart_bga_image, load_static_rgba_image};
use bmz_render::plan::{
    PLAY_BACKBMP_TEXTURE, Rect, SELECT_BANNER_TEXTURE, SELECT_STAGE_TEXTURE, TextureId,
};
use bmz_render::renderer::{RenderSurfaceStatus, Renderer, SurfaceSize};
use bmz_render::scene::{
    AppSceneSnapshot, DailyPlayerStatsSnapshot, PlayerStatsSnapshot, ResultSnapshot,
    SelectChartDistributionSecond, SelectRowSnapshot, SelectSnapshot,
};
use bmz_render::skin::{SkinImageSize, SkinTextureId};
use bmz_render::skin_offset::{SkinOffsetValue, SkinOffsetValues};
use bmz_render::snapshot::{
    CourseStageMarker, DisplayJudgeCounts, FastSlowJudgeCounts, OverlaySnapshot, RenderSnapshot,
    SkinLogicalInputSnapshot,
};
use bmz_video::VideoBgaDecoder;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{
    DeviceEvent, ElementState, MouseButton, MouseScrollDelta, StartCause, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop, EventLoopProxy};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::monitor::{MonitorHandle, VideoModeHandle};
use winit::window::{Fullscreen, Icon, Window, WindowAttributes, WindowId};

use crate::audio::{AppAudioOutput, AudioOutputDiagnostics, AudioRuntime};
use crate::bootstrap::{self, BootstrappedApp};
use crate::chart_preview::SelectChartPreview;
use crate::cli::{
    AUTOPLAY_ON_START_ARG, AppOptions, BOOT_RESULT_SAMPLE_ARG, LUA_SKIN_RUNTIME_ARG,
    SMOKE_EXIT_AFTER_FRAMES_ARG, SMOKE_EXIT_AFTER_PLAY_FRAMES_ARG,
    SMOKE_EXIT_AFTER_RESULT_FRAMES_ARG, SMOKE_EXIT_ON_RESULT_ARG, SMOKE_SCREENSHOT_ARG,
    VIEWER_BATTLE_ARG,
};
use crate::config::app_config::{
    AppConfig, GamepadBackendKind, GlobalInputConfig, InputBackendKind,
    InternalResolutionModeConfig, ObsConfig, PathEntry, WindowMode,
};
use crate::config::app_settings_registry::{AppSettingsChoices, AppSettingsEntryId};
use crate::config::key_config::{
    KeyBindingSlot, KeyBindingTarget, apply_play_binding, clear_play_binding,
    is_scratch_down_control, is_scratch_up_control,
};
use crate::config::load::load_profile_config;
use crate::config::play::{
    TARGET_GREEN_NUMBER_MAX, TARGET_GREEN_NUMBER_MIN, clamp_hispeed,
    input_bounce_config_from_profile,
};
use crate::config::profile_config::{
    BaseHispeedConfig, BgaExpandConfig, BgaModeConfig, BottomShiftableGaugeConfig,
    DoubleOptionConfig, FloatingPolicyConfig, GaugeAutoShiftConfig, GaugeTypeConfig,
    HispeedConfigPreset, HispeedDirectionConfig, HsFixConfig, InputActionConfig,
    JudgeAlgorithmConfig, KeyModeConversionConfig, LaneEffectConfig, LaneViewConfig,
    PlayDefaultsConfig, PlayModeConfig, ProfileConfig, ProfileInputConfig, RandomOptionConfig,
    RivalSourceConfig, SkinConfig, SkinOffsetConfig, TargetOptionConfig,
    default_classic_hispeed_step, default_floating_hispeed_step, normalize_hispeed_step,
    replay_slot_rule_indices,
};
use crate::config::save::{save_app_config, save_profile_config};
use crate::config::settings_registry::SettingsEntryId;
use crate::discord_presence::{DiscordPresence, DiscordPresenceConfig, DiscordPresenceHandle};
use crate::generated_preview::{fallback_preview_start_ms, generated_preview_cache_key};
use crate::i18n::{FluentArgs, Localizer};
use crate::input::shared::SharedInputBackend;
use crate::input::winit::{
    W_KEYBOARD_DEVICE_ID, key_event_to_device_input, physical_key_to_control,
};
use crate::ir::table::{
    RIAN_TABLE_MANUAL_REFRESH_COOLDOWN, RIAN_TABLE_REFRESH_INTERVAL, RianTableIdentity,
    active_source_urls as active_rian_table_source_urls, is_rian_table_source,
};
use crate::ln_policy::LnPolicySetting;
use crate::logging::LogBuffer;
use crate::paths::AppPaths;
use crate::practice_ui::PracticePanelContext;
use crate::random_trainer::RandomTrainerState;
use crate::screens::course_session::{ActiveCourseSession, CourseEntryResult, CourseResultSummary};
use crate::screens::key_config_edit::KeyConfigEditSession;
use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::play_loop::{
    PlayEndingSkinTimers, apply_play_arrange_to_snapshot, consume_running_play_snapshot,
    refresh_play_ending_snapshot,
};
use crate::screens::play_session::{AppliedArrange, PreparedPlayChart};
use crate::screens::play_snapshot::{
    BgaFrameCatalog, apply_fast_slow_display_filter, apply_prepared_chart_to_render_snapshot,
    bga_texture_id, build_render_snapshot_with_target_and_bga_frames_cached, display_bga_frame,
};
use crate::screens::play_start::{
    PlayStartOptions, PreloadedInputPlaySession, PreparedInputPlaySession, StartedInputPlaySession,
    apply_arrange_override, apply_course_constraints, apply_queued_replay,
    open_prepared_winit_play_session, play_session_options_from_start,
    prepare_play_session_for_chart_with_winit_input,
    prepare_practice_winit_play_session_from_preloaded, prepare_winit_play_session_from_preloaded,
};
use crate::screens::practice::{
    PracticeCliOverrides, PracticePhase, PracticeSession, clamp_practice_property,
    load_practice_property, practice_chart_zero_time, save_practice_property,
};
use crate::screens::result_model::ResultSummary;
use crate::screens::select_model::{
    COURSE_ROOT_PATH, DifficultyTableText, FAVORITE_CHART_PATH, FAVORITE_ROOT_PATH,
    FAVORITE_SONG_PATH, RANDOM_MIX_COURSE_SOURCE, SEARCH_PATH_PREFIX, SelectChartRow,
    SelectExecutableKind, SelectItem, TABLE_ROOT_PATH, TablePath, VIRTUAL_FOLDER_PATH_PREFIX,
    apply_collection_flags, chart_is_in_active_song_roots, course_contents_path, course_root_item,
    difficulty_table_text_for_chart_with_active_sources, favorite_root_item, favorite_root_items,
    favorite_song_representatives_for_folder, load_select_items_for_course_contents,
    load_select_items_for_courses, load_select_items_for_favorite_charts,
    load_select_items_for_favorite_song, load_select_items_for_favorite_songs,
    load_select_items_for_search_for_rule_mode_with_filters,
    load_select_items_in_folder_for_rule_mode_with_filters,
    load_select_items_in_table_level_for_rule_mode, load_select_items_in_virtual_folder,
    new_course_item_for_locale, parse_course_contents_path, parse_favorite_song_detail_path,
    parse_same_folder_path, parse_search_query, parse_table_path, random_mix_item,
    random_select_items_from_items, root_folder_items, same_folder_path,
    search_history_folder_items_for_locale, song_scan_path_from_context,
    table_folder_items_for_active_sources, table_level_folder_items, table_source_url_from_context,
    virtual_folder_breadcrumb, virtual_folder_root_items,
};
use crate::screens::settings_edit::{
    AppSettingsEditSession, SelectSettingsEditSession, SettingsBindings, SettingsEditSession,
    adjust_settings_draft,
};
use crate::screens::settings_model::{
    CONFIG_KEYS_PATH, in_settings_stack, settings_breadcrumb_for_locale,
    settings_root_item_for_locale,
};
use crate::select_options::{
    ArrangeOption, DoubleOption, HsFixOption, ResolvedTarget, SessionMode, TargetOption,
};
use crate::skin_loader::{
    BeatorajaSkinDecodeRequest, DecodedSkin, PreparedSource, SharedSkinGpuTextureCache,
    SkinFontCacheKey, SkinKind, UploadedSkin, decode_beatoraja_skin_request,
    default_play_skin_document_path_from_paths, default_skin_document_path_from_paths,
    enabled_options_from_selections, install_decoded_font, install_decoded_skin,
    is_decodable_skin_path, is_json_skin_path, is_lr2_skin_path, is_lua_skin_path,
    load_default_skin_into_renderer_from_paths, play_skin_selection_for_session,
    set_decoded_skin_context, upload_decoded_skin_with_texture_cache,
};
use crate::song_download::{
    ChartDownloadBatchResult, ChartDownloadRequest, MissingChartAction,
    choose_missing_chart_action, download_charts, open_browser_urls,
};
use crate::songs_cmd::scan_songs_with_progress;
use crate::storage::collection_db::FavoriteHints;
use crate::storage::common::hash_to_hex;
use crate::storage::difficulty_table_db::DifficultyTableRecord;
use crate::storage::library_db::{ChartDistributionSecond, ChartListItem, LibraryDatabase};
use crate::storage::migration::{migrate_library_db, migrate_network_db, migrate_score_db};
use crate::storage::replay::load_replay_for_chart_policy_and_double_option;
use crate::storage::replay_import::{
    ImportBeatorajaReplaysRequest, ReplayImportProgress, ReplayImportReport,
    import_beatoraja_replays_with_progress, write_replay_import_details,
};
use crate::storage::scan::{ScanProgress, ScanReport};
use crate::storage::score_db::{DailyPlayerStats, PlayerStats, ScoreDatabase, ScoreKey};
use crate::storage::score_import::{ScoreImportRequest, import_scores};
use crate::table_cmd::{TableFetchOutcome, TableFetchReport};
use crate::ui::{
    CourseEditorAction, CourseEditorChart, CourseEditorData, DebugInfo, EguiKeyConfigAction,
    EguiKeyConfigInput, EguiKeyConfigSection, EguiLayer, EguiRunContext, SceneSkinDefs,
    SelectCourseBuilderAction, SelectCourseBuilderData, SkinCandidate, SkinCandidateOrigin,
    SkinCatalog, SkinConfigMeta, SkinReloadRequest, SongScanRequest, UpdateDialog,
    UpdateDialogAction,
};
use crate::update::{DownloadedUpdate, UpdateAssetKind, UpdateCandidate};
use crate::window_config::{monitor_config_name, select_monitor};
use bmz_render::skin::{
    DestinationListEntry, SKIN_EVENT_DAILY_STATISTICS_RESET, SKIN_EVENT_IR_SCOPE_GLOBAL,
    SKIN_EVENT_IR_SCOPE_RIVAL, SKIN_EVENT_IR_SCOPE_TOGGLE, SKIN_EVENT_RESULT_PANEL_GRAPH,
    SKIN_EVENT_RESULT_PANEL_IR, SKIN_OPTION_BMZ_DOUBLE_PLAY, SKIN_OPTION_BMZ_KEY_MODE_BASE,
    SKIN_OPTION_BMZ_KEY_MODE_COUNT, SKIN_OPTION_BMZ_NO_SCRATCH, SKIN_OPTION_BMZ_SINGLE_PLAY,
    SKIN_REF_BMZ_ACTIVE_LANE_COUNT, SKIN_REF_BMZ_KEY_MODE, SkinAnimationDef, SkinClickHit,
    SkinClickTarget, SkinContext, SkinDestinationDef, SkinDocument, SkinDocumentRenderExt,
    SkinDocumentTexture, SkinDstEntry, SkinManifest, SkinSliderHit,
};

mod app_support;
mod background_jobs;
mod bga_runtime;
mod chart_assets;
#[path = "app/course_editor.rs"]
mod course_editor;
#[path = "app/course_flow/advance.rs"]
mod course_flow_advance;
#[path = "app/course_flow/finish.rs"]
mod course_flow_finish;
#[path = "app/course_flow/ir.rs"]
mod course_flow_ir;
#[path = "app/course_flow/metrics.rs"]
mod course_flow_metrics;
#[path = "app/course_flow/start.rs"]
mod course_flow_start;
#[path = "app/course_metrics_state.rs"]
mod course_metrics_state;
mod frame_flow;
mod frame_runtime;
mod input_runtime;
mod integration_support;
mod integrations;
mod maintenance;
mod pending_state;
mod play_control;
#[path = "app/play_flow/audio.rs"]
mod play_flow_audio;
#[path = "app/play_flow/launch/bga.rs"]
mod play_flow_launch_bga;
#[path = "app/play_flow/launch/poll.rs"]
mod play_flow_launch_poll;
#[path = "app/play_flow/launch/preload.rs"]
mod play_flow_launch_preload;
#[path = "app/play_flow/launch/start.rs"]
mod play_flow_launch_start;
#[path = "app/play_flow/practice.rs"]
mod play_flow_practice;
#[path = "app/play_flow/replay.rs"]
mod play_flow_replay;
#[path = "app/play_flow/retry.rs"]
mod play_flow_retry;
mod play_loop_flow;
mod play_preload_state;
mod play_support;
mod play_transition_state;
#[path = "app/result_flow/ending.rs"]
mod result_flow_ending;
#[path = "app/result_flow/interaction.rs"]
mod result_flow_interaction;
#[path = "app/result_flow/timing.rs"]
mod result_flow_timing;
#[path = "app/result_flow/transition.rs"]
mod result_flow_transition;
mod result_runtime;
mod result_support;
#[path = "app/result_support/timing.rs"]
mod result_timing_support;
mod rival_sync;
mod runtime_state;
mod scene_input;
mod select_assets;
#[path = "app/select_course_builder.rs"]
mod select_course_builder;
#[path = "app/select_flow/controls.rs"]
mod select_flow_controls;
#[path = "app/select_flow/gamepad.rs"]
mod select_flow_gamepad;
#[path = "app/select_flow/keyboard.rs"]
mod select_flow_keyboard;
#[path = "app/select_flow/mode_config.rs"]
mod select_flow_mode_config;
#[path = "app/select_flow/navigation.rs"]
mod select_flow_navigation;
#[path = "app/select_flow/pointer.rs"]
mod select_flow_pointer;
#[path = "app/select_flow/preview.rs"]
mod select_flow_preview;
#[path = "app/select_flow/skin_events.rs"]
mod select_flow_skin_events;
#[path = "app/select_flow/snapshot.rs"]
mod select_flow_snapshot;
mod select_folder_summary;
#[path = "app/select_ir_battle.rs"]
mod select_ir_battle;
mod select_key_bindings;
#[path = "app/select_random_mix.rs"]
mod select_random_mix;
mod select_search;
mod select_support;
mod skin_catalog;
#[path = "app/skin_flow/profile.rs"]
mod skin_flow_profile;
#[path = "app/skin_flow/reload.rs"]
mod skin_flow_reload;
#[path = "app/skin_flow/upload.rs"]
mod skin_flow_upload;
#[path = "app/skin_flow/video.rs"]
mod skin_flow_video;
mod skin_loading;
mod skin_options;
mod skin_pipeline;
mod skin_runtime_types;
mod skin_video;
mod skin_workers;
mod table_fetch_runtime;
mod update_prompt;

use app_support::*;
use course_metrics_state::*;
use pending_state::*;
use play_preload_state::*;
use play_transition_state::*;
use runtime_state::*;
use skin_runtime_types::*;
use update_prompt::*;

use select_key_bindings::{
    SelectKeyBindings, play_analog_lane_cover_delta, select_analog_scroll_delta,
    take_analog_scroll_steps, update_analog_scroll_buffer,
};

#[cfg(test)]
use crate::config::profile_config::{LaneConfig, SelectInputModeConfig};

use bga_runtime::{
    BgaImageLoadStatus, BgaPreloadRuntime, PendingBgaImageResult, RESOURCE_LOAD_PROGRESS_SCALE,
    combined_resource_load_progress, load_worker as chart_bga_texture_load_worker,
    resource_load_progress_units,
};
use chart_assets::*;
use frame_runtime::{
    AppLoopFrameTimings, FramePacingState, FrameProfileKind, FrameRuntime, FrameSchedule,
    FrameWindowMode, SceneFrameProfileSample, SkinVideoFrameProfile,
};
use input_runtime::{
    AppInputRuntime, ControlInputEvent, should_route_gamepad_event_while_discarding,
};
use integration_support::*;
use play_control::{
    GreenNumberChange, HispeedChange, LaneCoverChange, PlayAnalogOptionMode, PlayLaneAction,
    PlayLaneTarget, PlayOptionControl, keyboard_lane_action, lane_action_from_option,
    resolved_play_lane_target,
};
use play_support::*;
use result_runtime::{
    course_result_skin_snapshot, course_result_summary_for_skin, debug_boot_finished_play_session,
    mark_course_replay_slot_saved, result_main_bpm, result_max_bpm, result_min_bpm,
};
use result_support::*;
use result_timing_support::*;
use rival_sync::*;
use scene_input::{
    DecideAction, ResultAction, SelectAction, SelectMove, configurable_select_shortcut_action,
    decide_action as scene_decide_action, result_action as scene_result_action,
    select_action as scene_select_action,
};
use select_assets::{
    PreparedSelectPreview, SelectAssetRuntime, SelectMetaImageSlot, SelectPreviewFade,
    SelectPreviewSyncAction, select_preview_fade_factor,
};
use select_folder_summary::SelectFolderSummaryRuntime;
use select_search::{SearchInputAction, SelectSearchRuntime};
use select_support::*;
use skin_catalog::*;
use skin_loading::*;
use skin_options::*;
use skin_pipeline::SkinPipelineRuntime;
use skin_video::*;
use skin_workers::*;
use table_fetch_runtime::{
    RianTableFetchOutcome, RianTableFetchWorkerResult, TableFetchProgress, TableFetchRuntime,
    TableFetchWorkerEvent, startup_difficulty_table_fetch_urls_for_boot,
};

#[cfg(test)]
use crate::input::winit::physical_key_to_device_input;
#[cfg(test)]
use crate::screens::result_model::ResultFastSlowJudgeCounts;
#[cfg(test)]
use bmz_audio::sample::DecodedSample;
#[cfg(test)]
use result_runtime::debug_boot_result_summary;
#[cfg(test)]
use select_assets::{SELECT_PREVIEW_FADE_DURATION, SelectPreviewLoadQueue, prepare_select_preview};
#[cfg(test)]
use skin_pipeline::MAX_PENDING_SKIN_UPLOADS;

const SAMPLE_PLAYABLE_TITLE: &str = "BMZ Sample Playable";

#[derive(Debug, Clone)]
enum AppUserEvent {
    SkinUpload { sent_at: Instant },
    SystemSoundReady { generation: u64 },
    CourseLinkRepair,
    TableFetch,
    RivalSync,
    ViewerCommand(crate::viewer_ipc::ViewerCommand),
}

pub async fn run() -> Result<()> {
    run_with_options(AppOptions::default()).await
}

pub async fn run_with_options(options: AppOptions) -> Result<()> {
    run_with_options_and_log_buffer(options, LogBuffer::default()).await
}

pub async fn run_with_options_and_log_buffer(
    options: AppOptions,
    log_buffer: LogBuffer,
) -> Result<()> {
    let app_paths = crate::paths::resolve_app_paths()?;
    run_with_options_log_buffer_and_paths(options, log_buffer, app_paths).await
}

pub async fn run_with_options_log_buffer_and_paths(
    options: AppOptions,
    log_buffer: LogBuffer,
    app_paths: AppPaths,
) -> Result<()> {
    run_with_options_log_buffer_paths_and_profile(options, log_buffer, app_paths, None).await
}

pub async fn run_with_options_log_buffer_paths_and_profile(
    mut options: AppOptions,
    log_buffer: LogBuffer,
    app_paths: AppPaths,
    profile_id: Option<&str>,
) -> Result<()> {
    let startup_started_at = Instant::now();
    let (mut boot, viewer_cleanup) = if options.viewer_play {
        let path = options
            .boot_play_path
            .as_deref()
            .map(Path::new)
            .context("viewer play requires a chart path")?;
        let bms_random_seed = crate::random_option_seed::fresh_bms_random_seed();
        let viewer = bootstrap::bootstrap_viewer_with_paths(
            app_paths,
            path,
            options.start_measure.unwrap_or(0),
            bms_random_seed,
            profile_id,
        )?;
        options.boot_play_path = Some(viewer.chart_path.to_string_lossy().into_owned());
        options.boot_start_time_us = Some(viewer.start_time.0);
        options.boot_bms_random_seed = Some(bms_random_seed);
        tracing::info!(
            chart_id = viewer.chart_id,
            path = %viewer.chart_path.display(),
            start_time_us = viewer.start_time.0,
            "prepared transient viewer chart"
        );
        (viewer.app, Some(viewer.cleanup))
    } else {
        (bootstrap::bootstrap_with_paths_profile(app_paths, profile_id)?, None)
    };
    prepare_boot_chart_options(&mut boot, &mut options)?;
    tracing::info!(
        startup_elapsed_ms = startup_started_at.elapsed().as_millis(),
        "application bootstrap complete"
    );

    // Raw Input へ実行中に切り替えられるよう、Windows message hook は起動時から
    // 常設する。デバイス usage の登録は RawInputBackend の attach 時まで行わない。
    let raw_input_bridge = cfg!(windows).then(crate::input::rawinput::RawInputBridge::new);
    let mut event_loop_builder = EventLoop::<AppUserEvent>::with_user_event();
    #[cfg(windows)]
    if let Some(bridge) = raw_input_bridge.clone() {
        use winit::platform::windows::EventLoopBuilderExtWindows;

        event_loop_builder.with_msg_hook(move |message| {
            bridge.handle_message(message);
            false
        });
    }
    let event_loop = event_loop_builder.build().context("failed to create event loop")?;
    // 描画間隔は `FramePacer` の deadline を `WaitUntil` へ渡して制御する。
    // event loop thread 自体を sleep させず、フレーム待機中も入力イベントを処理する。
    event_loop.set_control_flow(ControlFlow::Wait);
    let event_proxy = event_loop.create_proxy();
    if options.viewer_play {
        let viewer_event_proxy = event_proxy.clone();
        crate::viewer_ipc::start_listener(move |command| {
            let _ = viewer_event_proxy.send_event(AppUserEvent::ViewerCommand(command));
        })?;
    }

    // Ctrl-C(SIGINT)で event loop を正常終了させ、cpal/ASIO ストリームの Drop を
    // 走らせる。捕捉しないと既定ハンドラがプロセスを即殺し、ASIO の停止処理が走らず
    // ドライバがノイズを流し続ける。
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    {
        let shutdown_requested = Arc::clone(&shutdown_requested);
        if let Err(error) =
            ctrlc::set_handler(move || shutdown_requested.store(true, Ordering::SeqCst))
        {
            tracing::warn!(%error, "failed to install Ctrl-C handler");
        }
    }

    let (maintenance_select_tx, maintenance_select_rx) = tokio::sync::watch::channel(false);
    if !options.viewer_play {
        spawn_ir_sync_worker(&boot, maintenance_select_rx);
    }

    let mut app = Box::new(WinitApp::new(
        boot,
        options,
        startup_started_at,
        None,
        None,
        shutdown_requested,
        event_proxy,
        log_buffer,
        maintenance_select_tx,
        raw_input_bridge,
    )?);
    tracing::info!("starting winit event loop");
    let result = event_loop.run_app(app.as_mut()).context("winit event loop failed");
    drop(app);
    drop(viewer_cleanup);
    result
}

fn prepare_boot_chart_options(
    boot: &mut bootstrap::BootstrappedApp,
    options: &mut AppOptions,
) -> Result<()> {
    let Some(path) = options.boot_play_path.as_deref().map(Path::new) else {
        return Ok(());
    };
    if !path.is_file() {
        return Ok(());
    }
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to resolve boot chart: {}", path.display()))?;
    if !crate::storage::scan::is_chart_file(&canonical) {
        bail!("unsupported chart extension: {}", canonical.display());
    }
    options.boot_play_path = Some(canonical.to_string_lossy().into_owned());
    let chart_id = match boot.library_db.chart_id_by_chart_file_path(&canonical)? {
        Some(chart_id) => chart_id,
        None => {
            crate::storage::import::import_chart_file(
                &mut boot.library_db,
                &canonical,
                None,
                None,
                now_unix_seconds(),
            )?
            .chart_id
        }
    };
    if options.boot_start_time_us.is_none()
        && let Some(measure) = options.start_measure
    {
        let chart = crate::screens::play_session::load_source_chart_for_chart(
            &boot.library_db,
            chart_id,
            None,
        )?;
        options.boot_start_time_us = Some(bootstrap::viewer_measure_start_time(&chart, measure)?.0);
    }
    Ok(())
}

/// IR スコアジョブをバックグラウンドで定期送信する。
///
/// メインスレッドの DB connection とは別 connection を開く (DB は WAL)。
/// IR が未設定なら何もしない。
fn spawn_ir_sync_worker(
    boot: &bootstrap::BootstrappedApp,
    mut select_rx: tokio::sync::watch::Receiver<bool>,
) {
    let ir_config = boot.profile_config.ir.clone();
    if !ir_config.providers.iter().any(|provider| provider.enabled && !provider.base_url.is_empty())
    {
        return;
    }
    let profile_root = boot.profile_paths.root_dir.clone();
    let logs_dir = boot.app_paths.logs_dir.clone();
    let score_db_path = boot.profile_paths.score_db.clone();
    let network_db_path = boot.profile_paths.network_db.clone();
    tokio::spawn(async move {
        let interval = std::time::Duration::from_secs(crate::ir::sync::IR_SYNC_LOOP_INTERVAL_SECS);
        let mut next_run_at = tokio::time::Instant::now();
        loop {
            while !*select_rx.borrow() {
                if select_rx.changed().await.is_err() {
                    return;
                }
            }
            if tokio::time::Instant::now() < next_run_at {
                tokio::select! {
                    _ = tokio::time::sleep_until(next_run_at) => {}
                    changed = select_rx.changed() => {
                        if changed.is_err() {
                            return;
                        }
                        continue;
                    }
                }
            }
            if !*select_rx.borrow() {
                continue;
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if let Err(error) = migrate_network_db(&network_db_path) {
                tracing::warn!(%error, "failed to migrate network db for IR sync");
                next_run_at = tokio::time::Instant::now() + interval;
                continue;
            }
            match crate::storage::network_db::NetworkDatabase::open(&network_db_path) {
                Ok(mut network_db) => {
                    let mut submitted = 0_u32;
                    let mut failed = 0_u32;
                    for index in 0..crate::ir::sync::IR_SYNC_BATCH_LIMIT {
                        if !*select_rx.borrow() {
                            break;
                        }
                        let job_now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(now);
                        match crate::ir::sync::sync_pending_ir_jobs(
                            &mut network_db,
                            &score_db_path,
                            &profile_root,
                            &logs_dir,
                            &ir_config,
                            job_now,
                            1,
                            false,
                            crate::ir::sync::IrSyncThrottle::none(),
                        )
                        .await
                        {
                            Ok(report) => {
                                submitted = submitted.saturating_add(report.submitted);
                                failed = failed.saturating_add(report.failed);
                                if report.submitted == 0 && report.failed == 0 {
                                    break;
                                }
                            }
                            Err(error) => {
                                tracing::warn!(%error, "IR score sync failed");
                                break;
                            }
                        }
                        if index + 1 < crate::ir::sync::IR_SYNC_BATCH_LIMIT {
                            tokio::select! {
                                _ = tokio::time::sleep(std::time::Duration::from_millis(
                                    crate::ir::sync::IR_SYNC_JOB_SPACING_MS,
                                )) => {}
                                changed = select_rx.changed() => {
                                    if changed.is_err() {
                                        return;
                                    }
                                    if !*select_rx.borrow() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    if submitted > 0 || failed > 0 {
                        tracing::info!(submitted, failed, "IR score sync finished");
                    }
                }
                Err(error) => tracing::warn!(%error, "failed to open network db for IR sync"),
            }
            next_run_at = tokio::time::Instant::now() + interval;
        }
    });
}

struct WinitApp {
    boot: BootstrappedApp,
    window: Option<Arc<Window>>,
    first_frame_startup_completed: bool,
    /// Ctrl-C(SIGINT)受信フラグ。セットされたら `about_to_wait` で event loop を
    /// 正常終了させ、cpal/ASIO ストリームの Drop(停止・後処理)を確実に走らせる。
    shutdown_requested: Arc<AtomicBool>,
    renderer: Box<Renderer>,
    /// device共通の押下集合とkeyboard bounce状態。
    input: AppInputRuntime,
    /// 実行中に Raw Input backend を生成し直すための常設 message bridge。
    raw_input_bridge: Option<crate::input::rawinput::RawInputBridge>,
    gamepad: Option<crate::input::capture::InputCapture>,
    /// worker 完了時に main thread の redraw を起こすための winit user event proxy。
    event_proxy: EventLoopProxy<AppUserEvent>,
    /// frame pacing、確定FPS、scene別profile集計をまとめた描画runtime。
    frame: FrameRuntime,
    deferred_boot: Option<DeferredBoot>,
    /// uBMplay互換の外部ビューワーとして起動したプロセスか。
    viewer_mode: bool,
    /// ビューワーの単曲再生が終わり、次のIPC命令を待っている状態。
    viewer_waiting: bool,
    /// 現在の外部ビューワー譜面。F5 reload と同じ #RANDOM seed の再利用に使う。
    viewer_chart_path: Option<PathBuf>,
    viewer_bms_random_seed: Option<u64>,
    /// Spaceで停止したviewerの譜面時刻とPlay skin経過時刻を凍結する。
    viewer_paused: bool,
    viewer_paused_play_elapsed: Option<TimeUs>,
    skip_result: bool,
    select: SelectRuntimeState,
    play: PlayRuntimeState,
    result: ResultRuntimeState,
    jobs: AppJobs,
    integrations: IntegrationRuntimeState,
    smoke: SmokeRuntime,
    skin: SkinRuntimeState,
    audio: AppAudioRuntimeState,
    ui: UiRuntimeState,
    course_editor_cache: course_editor::CourseEditorDataCache,
}

#[path = "app/audio_helpers.rs"]
mod audio_helpers;
#[path = "app/constructor.rs"]
mod constructor;
#[path = "app/input_lifecycle.rs"]
mod input_lifecycle;
mod key_config_flow;
#[path = "app/lifecycle.rs"]
mod lifecycle;
#[path = "app/platform.rs"]
mod platform;
#[path = "app/runtime_config.rs"]
mod runtime_config;
#[path = "app/runtime_helpers.rs"]
mod runtime_helpers;
#[path = "app/scene_state.rs"]
mod scene_state;
#[path = "app/viewer.rs"]
mod viewer;

use audio_helpers::*;
use platform::*;
use runtime_config::*;
use runtime_helpers::*;

#[cfg(test)]
#[path = "app/tests.rs"]
mod tests;
/// Shared profile option mapping for a noninteractive export session.
pub(crate) fn offline_play_options(
    profile: &crate::config::profile_config::ProfileConfig,
) -> crate::screens::play_session::PlaySessionOptions {
    let selected = select_play_options_from_profile(&profile.play);
    crate::screens::play_session::PlaySessionOptions {
        session_mode: SessionMode::Autoplay,
        autoplay: true,
        score_save_disabled: true,
        sample_rate: 48_000,
        playback_rate_percent: 100,
        gauge_override: Some(crate::config::play::gauge_type_from_config(selected.gauge)),
        gauge_auto_shift: crate::config::play::gauge_auto_shift_from_config(
            selected.gauge,
            selected.gauge_auto_shift,
        ),
        bottom_shiftable_gauge: crate::config::play::bottom_shiftable_gauge_from_config(
            selected.bottom_shiftable_gauge,
        ),
        arrange: selected.arrange,
        arrange_2p: selected.arrange_2p,
        double_option: selected.double_option,
        hs_fix: selected.hs_fix,
        target: selected.target,
        key_mode_conversion: profile.play.key_mode_conversion,
        seven_to_nine_pattern: profile.play.seven_to_nine_pattern,
        seven_to_nine_type: profile.play.seven_to_nine_type,
        seven_to_nine_rule_mode: profile.play.seven_to_nine_rule_mode,
        assist: profile.play.assist,
        ln_policy_setting: profile.play.ln_mode_policy,
        rule_mode: profile.play.rule_mode,
        ..Default::default()
    }
}

pub(crate) fn offline_skin_load_state(
    play: &crate::screens::play_session::PreparedPlaySession,
    profile: &crate::config::profile_config::ProfileConfig,
    best: Option<u32>,
) -> bmz_skin::LuaLoadRuntimeState {
    let replay = play.session.replay_player.clone();
    let mode = if replay.is_some() { SessionMode::Normal } else { SessionMode::Autoplay };
    let options = PlayStartOptions {
        session_mode: mode,
        autoplay: replay.is_none(),
        replay_player: replay,
        score_save_disabled: true,
        target: play.target_option,
        ..Default::default()
    };
    let runtime = lua_runtime_state_for_play(
        &options,
        false,
        play.session.chart.metadata.key_mode,
        best,
        &profile.display_name,
        play.skin_attempt,
    );
    let selection = crate::skin_loader::play_skin_selection_for_session(
        &profile.skin,
        play.session.chart.metadata.key_mode,
        mode,
    );
    lua_runtime_state_with_skin_offsets(runtime, selection.offsets)
}

pub(crate) fn offline_skin_video_gating(
    document: &SkinDocument,
    source: &str,
) -> (bool, Vec<Vec<i32>>) {
    let gating = skin_video_source_gating(document, source);
    (gating.active, gating.op_sets)
}

pub(crate) fn offline_skin_video_state(
    snapshot: &RenderSnapshot,
    document: &SkinDocument,
) -> bmz_render::skin::SkinDrawState {
    play_skin_video_draw_state(snapshot, Some(document.h), None, document.input)
}
