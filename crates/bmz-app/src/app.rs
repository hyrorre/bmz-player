use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use bmz_chart::model::PlayableChart;
use bmz_core::lane::KeyMode;
use bmz_core::time::TimeUs;
use bmz_gameplay::session::PlaySkinOffset;
use bmz_render::assets::load_static_rgba_image;
use bmz_render::plan::{
    PLAY_BACKBMP_TEXTURE, SELECT_BANNER_TEXTURE, SELECT_STAGE_TEXTURE, TextureId,
};
use bmz_render::renderer::{RenderSurfaceStatus, Renderer, SurfaceSize};
use bmz_render::scene::{AppSceneSnapshot, ResultSnapshot, SelectRowSnapshot, SelectSnapshot};
use bmz_render::snapshot::{
    DisplayJudgeCounts, FastSlowJudgeCounts, OverlaySnapshot, RenderSnapshot,
};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::monitor::{MonitorHandle, VideoModeHandle};
use winit::window::{Fullscreen, Window, WindowAttributes, WindowId};

use crate::audio::AppAudioOutput;
use crate::bootstrap::{self, BootstrappedApp};
use crate::chart_preview::SelectChartPreview;
use crate::cli::{
    AUTOPLAY_ON_START_ARG, AppOptions, SMOKE_EXIT_AFTER_FRAMES_ARG, SMOKE_EXIT_ON_RESULT_ARG,
};
use crate::config::app_config::{PathEntry, WindowMode};
use crate::config::load::load_profile_config;
use crate::config::profile_config::{
    AssistOptionConfig, BgaModeConfig, GaugeTypeConfig, InputActionConfig, LaneConfig,
    ProfileConfig, ProfileInputConfig, RandomOptionConfig, TargetOptionConfig,
};
use crate::config::save::{save_app_config, save_profile_config};
use crate::input::winit::physical_key_to_control;
use crate::screens::course_session::{
    ActiveCourseSession, CourseEntryResult, CourseResultSummary,
};
use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::play_loop::{PlayAdvanceOutcome, advance_running_play_session_until_result};
use crate::screens::play_snapshot::{
    BgaFrameCatalog, bga_texture_id, build_render_snapshot_with_target_and_bga_frames,
    display_bga_frame,
};
use crate::screens::play_start::{
    PlayStartOptions, PreloadedWinitPlaySession, StartedWinitPlaySession,
    apply_course_constraints, open_prepared_winit_play_session,
    prepare_play_session_for_chart_with_winit_input, prepare_winit_play_session_from_preloaded,
};
use crate::screens::result_model::ResultSummary;
use crate::screens::select_model::{
    COURSE_ROOT_PATH, SelectItem, TABLE_ROOT_PATH, TablePath, course_root_item,
    load_select_items_for_courses, load_select_items_in_folder, load_select_items_in_table_level,
    parse_table_path, root_folder_items, song_scan_path_from_context, table_folder_items,
    table_level_folder_items, table_source_url_from_context,
};
use crate::select_options::{ArrangeOption, AssistOption, TargetOption};
use crate::skin_loader::{
    DecodedSkin, PreparedSource, SkinKind, UploadedSkin, apply_skin_from_config,
    decode_beatoraja_skin_with_options, install_decoded_font, install_decoded_skin,
    is_decodable_skin_path, load_default_skin_into_renderer, play_skin_selection_for,
    set_decoded_skin_context, upload_decoded_skin,
};
use crate::songs_cmd::scan_songs;
use crate::storage::library_db::LibraryDatabase;
use crate::storage::migration::migrate_library_db;
use crate::storage::replay::load_replay_for_chart;
use crate::ui::{DebugInfo, EguiLayer, SceneSkinDefs, SkinCandidate, SkinCatalog, SkinConfigMeta};
use bmz_render::skin::{SkinDocument, SkinDocumentTexture, SkinManifest};
use std::collections::BTreeMap;

const SAMPLE_PLAYABLE_TITLE: &str = "BMZ Sample Playable";

pub async fn run() -> Result<()> {
    run_with_options(AppOptions::default()).await
}

pub async fn run_with_options(options: AppOptions) -> Result<()> {
    let mut boot = bootstrap::bootstrap()?;

    if boot.app_config.tables.auto_fetch_on_startup {
        fetch_configured_difficulty_tables(&mut boot).await;
    }

    // システム SE / BGM 用の cpal ストリームを起動する。
    // 開けない環境(ヘッドレス CI 等)はサイレントモードでアプリ起動を継続する。
    let system_audio = match crate::audio::SystemAudio::open(&boot.app_config.audio) {
        Ok(audio) => Some(audio),
        Err(error) => {
            tracing::warn!(%error, "failed to open system audio output; running without system sounds");
            None
        }
    };

    let event_loop = EventLoop::new().context("failed to create event loop")?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = WinitApp::new(boot, options, system_audio)?;
    tracing::info!("starting winit event loop");
    event_loop.run_app(&mut app).context("winit event loop failed")
}

async fn fetch_configured_difficulty_tables(boot: &mut bootstrap::BootstrappedApp) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);

    let sources: Vec<_> = boot
        .app_config
        .tables
        .sources
        .iter()
        .filter(|s| s.enabled)
        .map(|s| s.url.clone())
        .collect();

    for url in sources {
        tracing::info!(%url, "fetching difficulty table");
        match crate::difficulty_table::fetch_difficulty_table(&url, now).await {
            Ok(table) => {
                tracing::info!(
                    name = %table.name,
                    entries = table.entries.len(),
                    courses = table.courses.len(),
                    "difficulty table fetched"
                );
                if let Err(e) = boot.library_db.upsert_difficulty_table(&table) {
                    tracing::warn!(%url, error = %e, "failed to store difficulty table");
                }
                let source = format!("table:{url}");
                for (position, course) in table.courses.iter().enumerate() {
                    if let Err(e) =
                        boot.library_db.upsert_course(&source, course, position as i64, now)
                    {
                        tracing::warn!(%url, course = %course.title, error = %e, "failed to store table course");
                    }
                }
                if !table.courses.is_empty() {
                    tracing::info!(count = table.courses.len(), %url, "stored table courses");
                }
            }
            Err(e) => {
                tracing::warn!(%url, error = %e, "failed to fetch difficulty table");
            }
        }
    }
}

struct WinitApp {
    boot: BootstrappedApp,
    window: Option<Arc<Window>>,
    active_play: Option<StartedWinitPlaySession>,
    /// コースプレイ中のセッション。単曲プレイ時は None。
    active_course: Option<ActiveCourseSession>,
    /// コース全体完了時のリザルト。リザルト画面から抜けるまで保持する。
    finished_course: Option<CourseResultSummary>,
    /// プレイ終了でリザルトへ移った後、曲の余韻を鳴らし切るために保持する音声出力。
    /// ドレインが完了するか、選曲復帰・次プレイ開始で解放される。
    draining_audio: Option<AppAudioOutput>,
    finished_play: Option<FinishedPlaySession>,
    /// 直近のプレイがオートプレイだったか。Result 画面の常時表示に使う。
    last_play_was_autoplay: bool,
    last_play_snapshot: Option<RenderSnapshot>,
    pending_decide: Option<DecideTransition>,
    pending_play_start: Option<PendingPlayStart>,
    pending_play_preload: Option<PendingPlayPreload>,
    play_preload_generation: u64,
    play_ending: Option<PlayEndingTransition>,
    last_started_chart_id: Option<i64>,
    select_items: Vec<SelectItem>,
    folder_stack: Vec<String>,
    /// `folder_stack` の各階層に入る直前の `selected_index`。
    /// フォルダから出た時にカーソル位置を復元するために使う。長さは `folder_stack` と一致。
    selected_index_stack: Vec<usize>,
    selected_index: usize,
    renderer: Renderer,
    skin_catalog: SkinCatalog,
    skin_defs_cache: BTreeMap<String, SceneSkinDefs>,
    pending_table_fetch: Option<Receiver<Result<()>>>,
    last_scene_kind: Option<AppSceneKind>,
    start_held: bool,
    select_held: bool,
    arrange_option: ArrangeOption,
    target_option: TargetOption,
    gauge_option: GaugeTypeConfig,
    assist_option: AssistOption,
    select_keys: SelectKeyBindings,
    smoke_exit_after_frames: Option<u32>,
    smoke_exit_on_result: bool,
    rendered_frames: u32,
    select_scene_started_at: Instant,
    select_bar_started_at: Instant,
    play_scene_started_at: Instant,
    play_ready_sound_started_at: Option<Instant>,
    result_scene_started_at: Instant,
    option_panel_started_at: Instant,
    select_option_panel: u8,
    gilrs: Option<crate::input::gilrs::GilrsBackend>,
    default_skin_manifest: Option<SkinManifest>,
    /// decode worker (CPU) → upload worker への送信端。
    skin_decode_tx: mpsc::Sender<PendingSkinResult>,
    /// decode worker → upload worker の受信端。surface 接続時に upload worker へ
    /// move するため Option で保持する。
    skin_decode_rx: Option<Receiver<PendingSkinResult>>,
    /// upload worker → main への送信端 (upload worker を spawn する際に clone)。
    skin_upload_tx: mpsc::Sender<PendingUploadResult>,
    /// upload worker → main の受信端。GPU アップロード済みスキンを取り込む。
    skin_upload_rx: Receiver<PendingUploadResult>,
    /// upload worker を spawn 済みか (surface 接続時に一度だけ起動)。
    skin_upload_worker_started: bool,
    pending_select_skin: bool,
    pending_decide_skin: bool,
    pending_play_skin: bool,
    pending_result_skin: bool,
    skin_reload_generation: u64,
    pending_skin_reload_at: Option<Instant>,
    /// 直近 install をリクエストしたプレイスキンの key_mode と設定 fingerprint。
    /// 同じ mode かつ同じ path/options/files なら再 decode をスキップする。
    last_play_skin_signature: Option<PlaySkinSignature>,
    /// システム SE / BGM を再生する cpal ストリーム。
    /// 開けない環境では `None` で、システム音はサイレント。
    #[allow(dead_code)]
    system_audio: Option<crate::audio::SystemAudio>,
    /// `system_audio` 上にデコード済みサンプルを乗せて再生・停止する facade。
    /// `system_audio` が `None` の場合や、サウンドセット未指定の場合も `Some` で
    /// 構築されるが id_map が空なので各 play/stop は no-op になる。
    system_sound: Option<crate::system_sound_manager::SystemSoundManager>,
    /// 選曲画面でESCを長押し中の開始時刻。離されたり画面を抜けると None になる。
    select_exit_hold_started_at: Option<Instant>,
    /// 選曲 `#STAGEFILE` のロード済みキャッシュキー (`folder|file`)。
    select_stage_source: Option<String>,
    select_stage_loaded: bool,
    /// 選曲 `#BANNER` のロード済みキャッシュキー (`folder|file`)。
    select_banner_source: Option<String>,
    select_banner_loaded: bool,
    /// 選曲 `#PREVIEW` のロード済みキャッシュキー (`folder|file`)。
    select_preview_source: Option<String>,
    select_preview_playing: bool,
    select_preview: Option<SelectChartPreview>,
    /// プレイ `#BACKBMP` のロード済みキャッシュキー。
    play_backbmp_source: Option<String>,
    play_backbmp_loaded: bool,
    /// プレイ中の Start キー直近の押下時刻。連続押し判定で使用。
    last_play_start_press_at: Option<Instant>,
    /// 本体設定 / スキン設定 / デバッグ表示用の egui レイヤ。
    /// ウィンドウ生成時に初期化される。
    egui: Option<EguiLayer>,
    /// 現在ウィンドウへ適用済みのウィンドウモード。
    /// config 側との差分検出でライブ反映の要否を判定する。
    applied_window_mode: WindowMode,
    /// ウィンドウがフォーカスを持っているか。フレームレート上限の切替に使う。
    focused: bool,
    /// 直近フレームの開始時刻。フレームレート制限のスリープ量算出に使う。
    last_frame_at: Option<Instant>,
    /// RedrawRequested 間隔から平滑化した wgpu 描画 FPS。
    wgpu_fps: f32,
    /// リザルト画面終了アニメーションの進行状態。
    /// Some のあいだは終了フェードアウト中で、入力は受け付けない。
    result_exit: Option<ResultExit>,
}

type PlaySkinSignature = (KeyMode, String, BTreeMap<String, String>, BTreeMap<String, String>);

struct DecideTransition {
    chart_id: i64,
    started_at: Instant,
    fadeout_started_at: Option<Instant>,
    cancel: bool,
    snapshot: RenderSnapshot,
}

struct PendingPlayStart {
    chart_id: i64,
}

struct PendingPlayPreload {
    generation: u64,
    chart_id: i64,
    rx: Receiver<PlayPreloadResult>,
}

struct PlayPreloadResult {
    generation: u64,
    chart_id: i64,
    result: std::result::Result<PreloadedWinitPlaySession, String>,
}

struct PlayEndingTransition {
    started_at: Instant,
    fadeout_started_at: Option<Instant>,
    finished: FinishedPlaySession,
    failed: bool,
    full_combo_elapsed_at_finish_ms: Option<i32>,
}

/// リザルト画面終了フェードアウトの進行状態。
/// フェードアウト時間が経過したら `action` を実行して画面を切り替える。
struct ResultExit {
    started_at: Instant,
    action: ResultExitAction,
}

/// リザルト画面を抜けたあとに実行する遷移。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResultExitAction {
    /// 選曲画面へ戻る。
    Leave,
    /// 直前と同じ譜面をもう一度プレイする。
    Retry,
}

const SELECT_EXIT_HOLD_DURATION: Duration = Duration::from_millis(1_200);
/// プレイ中の Start ボタンを「2回連続押し」と判定する間隔上限。
const PLAY_START_DOUBLE_PRESS_WINDOW: Duration = Duration::from_millis(400);
/// レーンカバー / LIFT を上下キーで動かす際のステップ幅。
const LANE_COVER_STEP: f32 = 0.01;
const SKIN_RELOAD_DEBOUNCE: Duration = Duration::from_millis(300);

struct PendingSkinResult {
    generation: u64,
    path: PathBuf,
    kind: SkinKind,
    result: Result<DecodedSkin>,
}

/// upload worker が GPU アップロードまで終えた結果を main へ返すメッセージ。
/// `UploadedSkin` 内の `PreparedTexture` は `Send` なのでスレッド間で渡せる。
/// main は受信後、テクスチャを差し込んで `SkinContext` を組むだけ (軽量)。
struct PendingUploadResult {
    generation: u64,
    path: PathBuf,
    kind: SkinKind,
    uploaded: Result<UploadedSkin>,
}

#[derive(Debug, Clone, PartialEq)]
enum AppViewState {
    Select,
    Decide,
    Play,
    Result(Box<ResultSummary>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppSceneKind {
    Select,
    Decide,
    Play,
    Result,
}

impl WinitApp {
    fn new(
        boot: BootstrappedApp,
        options: AppOptions,
        system_audio: Option<crate::audio::SystemAudio>,
    ) -> Result<Self> {
        let mut boot = boot;
        if let Some(cli_renderer) = options.renderer.clone() {
            tracing::info!(?cli_renderer, "overriding renderer backend via CLI option");
            boot.app_config.video.renderer = cli_renderer;
        }

        let folder_stack = initial_folder_stack(&boot.app_config);
        let select_items = load_items_for_stack(&boot, &folder_stack);
        let boot_chart_id = resolve_boot_chart_id(&boot.library_db, &options);
        log_startup_options(&options);

        let assist_option = if options.autoplay_on_start || boot.profile_config.play.auto_play {
            AssistOption::Autoplay
        } else {
            AssistOption::Normal
        };
        let gauge_option = boot.profile_config.play.gauge;
        let arrange_option = arrange_option_from_profile(boot.profile_config.play.random);
        let target_option = target_option_from_profile(boot.profile_config.play.target);
        let select_keys = SelectKeyBindings::from_profile(&boot.profile_config.input);
        let now = Instant::now();

        let mut renderer = Renderer::default();
        let skin_catalog = scan_skin_catalog();
        let (skin_decode_tx, skin_decode_rx) = mpsc::channel::<PendingSkinResult>();
        let (skin_upload_tx, skin_upload_rx) = mpsc::channel::<PendingUploadResult>();
        let (default_skin_manifest, pending_select_skin, pending_decide_skin, pending_result_skin) =
            load_initial_skin_textures(
                &mut renderer,
                &skin_decode_tx,
                0,
                &boot.profile_config.skin.select,
                &boot.profile_config.skin.decide,
                &boot.profile_config.skin.result,
                &boot.profile_config.skin.select_options,
                &boot.profile_config.skin.decide_options,
                &boot.profile_config.skin.result_options,
                &boot.profile_config.skin.select_files,
                &boot.profile_config.skin.decide_files,
                &boot.profile_config.skin.result_files,
            );
        let pending_play_skin = false;

        let gilrs = if boot.app_config.input.gamepad_enabled {
            let sensitivity = boot.profile_config.input.analog_scratch_sensitivity;
            let timeout_ms = boot.profile_config.input.analog_scratch_timeout_ms;
            match crate::input::gilrs::GilrsBackend::new(sensitivity, timeout_ms) {
                Ok(g) => {
                    tracing::info!("gilrs initialized");
                    Some(g)
                }
                Err(e) => {
                    tracing::warn!(error = %e, "gilrs init failed, gamepad disabled");
                    None
                }
            }
        } else {
            None
        };

        let initial_window_mode = boot.app_config.video.mode.clone();

        // システム SE / BGM facade を構築する。
        // - `profile.[system_sound].bgm_dir` / `se_dir` が指定されていれば再帰スキャンして
        //   セットを集め、その中からランダム選択する(beatoraja 互換)。
        // - 空なら scan を省略し、`default_sound_dir` だけにフォールバックする。
        let system_sound = system_audio.as_ref().map(|audio| {
            let cfg = &boot.profile_config.system_sound;
            let bgm_candidates = if cfg.bgm_dir.is_empty() {
                Vec::new()
            } else {
                crate::system_sound::scan_sound_sets(
                    Path::new(&cfg.bgm_dir),
                    crate::system_sound::SoundType::Select.file_name(),
                )
            };
            let se_candidates = if cfg.se_dir.is_empty() {
                Vec::new()
            } else {
                crate::system_sound::scan_sound_sets(
                    Path::new(&cfg.se_dir),
                    crate::system_sound::SoundType::ResultClear.file_name(),
                )
            };
            let default_dir = if cfg.default_sound_dir.is_empty() {
                None
            } else {
                Some(PathBuf::from(&cfg.default_sound_dir))
            };
            let selection = crate::system_sound::select_random_sound_set(
                &bgm_candidates,
                &se_candidates,
                default_dir,
            );
            crate::system_sound_manager::SystemSoundManager::new(audio.engine(), &selection)
        });
        let select_preview =
            system_audio.as_ref().map(|audio| SelectChartPreview::new(audio.engine()));

        let mut app = Self {
            boot,
            window: None,
            active_play: None,
            active_course: None,
            finished_course: None,
            draining_audio: None,
            finished_play: None,
            last_play_was_autoplay: false,
            last_play_snapshot: None,
            pending_decide: None,
            pending_play_start: None,
            pending_play_preload: None,
            play_preload_generation: 0,
            play_ending: None,
            last_started_chart_id: None,
            select_items,
            selected_index_stack: vec![0; folder_stack.len()],
            folder_stack,
            selected_index: 0,
            renderer,
            skin_catalog,
            skin_defs_cache: BTreeMap::new(),
            pending_table_fetch: None,
            last_scene_kind: None,
            start_held: false,
            select_held: false,
            arrange_option,
            target_option,
            gauge_option,
            assist_option,
            select_keys,
            smoke_exit_after_frames: options.smoke_exit_after_frames,
            smoke_exit_on_result: options.smoke_exit_on_result,
            rendered_frames: 0,
            select_scene_started_at: now,
            select_bar_started_at: now,
            play_scene_started_at: now,
            play_ready_sound_started_at: None,
            result_scene_started_at: now,
            option_panel_started_at: now,
            select_option_panel: 0,
            gilrs,
            default_skin_manifest,
            skin_decode_tx,
            skin_decode_rx: Some(skin_decode_rx),
            skin_upload_tx,
            skin_upload_rx,
            skin_upload_worker_started: false,
            pending_select_skin,
            pending_decide_skin,
            pending_play_skin,
            pending_result_skin,
            last_play_skin_signature: None,
            skin_reload_generation: 0,
            pending_skin_reload_at: None,
            system_audio,
            system_sound,
            select_exit_hold_started_at: None,
            select_stage_source: None,
            select_stage_loaded: false,
            select_banner_source: None,
            select_banner_loaded: false,
            select_preview_source: None,
            select_preview_playing: false,
            select_preview,
            play_backbmp_source: None,
            play_backbmp_loaded: false,
            last_play_start_press_at: None,
            egui: None,
            applied_window_mode: initial_window_mode,
            focused: true,
            last_frame_at: None,
            wgpu_fps: 0.0,
            result_exit: None,
        };
        if let Some(chart_id) = boot_chart_id {
            tracing::info!(chart_id, "booting directly into chart");
            if let Some(slot) = options.boot_replay_slot {
                if !app.try_start_replay_for_chart(chart_id, slot) {
                    tracing::warn!(slot, "boot replay slot empty; falling back to normal play");
                    app.start_chart(chart_id);
                }
            } else {
                app.start_chart(chart_id);
            }
        }

        Ok(app)
    }

    fn ensure_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let video = &self.boot.app_config.video;
        let attributes = window_attributes_from_config(video)
            .with_fullscreen(fullscreen_from_config(&video.mode, event_loop.primary_monitor()));
        match event_loop.create_window(attributes) {
            Ok(window) => {
                let window = Arc::new(window);
                window.set_visible(true);
                let size = surface_size_for_window(&window);
                // サーフェス生成前に VSync とバックエンド設定を反映させておく。
                self.renderer.set_vsync(self.boot.app_config.video.vsync);
                let backend = match self.boot.app_config.video.renderer {
                    crate::config::app_config::RendererBackend::Auto => {
                        bmz_render::WgpuBackend::Auto
                    }
                    crate::config::app_config::RendererBackend::Vulkan => {
                        bmz_render::WgpuBackend::Vulkan
                    }
                    crate::config::app_config::RendererBackend::Metal => {
                        bmz_render::WgpuBackend::Metal
                    }
                    crate::config::app_config::RendererBackend::Dx12 => {
                        bmz_render::WgpuBackend::Dx12
                    }
                    crate::config::app_config::RendererBackend::Gl => bmz_render::WgpuBackend::Gl,
                };
                self.renderer.set_backend(backend);
                if let Err(error) = self.renderer.attach_surface(Arc::clone(&window), size) {
                    tracing::error!(%error, "failed to initialize renderer surface");
                    event_loop.exit();
                    return;
                }
                tracing::info!(
                    width = size.width,
                    height = size.height,
                    "window and renderer surface ready"
                );
                // surface 接続後 (= GPU device/queue 利用可能) に upload worker を起動する。
                // decode 結果はそれまで skin_decode_rx にバッファされ、起動後にドレインされる。
                self.start_skin_upload_worker();
                window.request_redraw();
                self.egui = Some(EguiLayer::new(&window, self.boot.profile_config.ui.show_fps));
                self.window = Some(window);
                self.update_window_title_for_scene(AppSceneKind::Select);
            }
            Err(error) => {
                tracing::error!(%error, "failed to create window");
                event_loop.exit();
            }
        }
    }

    fn view_state(&self) -> AppViewState {
        if self.pending_decide.is_some() {
            return AppViewState::Decide;
        }
        if self.active_play.is_some() || self.pending_play_start.is_some() {
            return AppViewState::Play;
        }

        if let Some(finished) = &self.finished_play {
            return AppViewState::Result(Box::new(finished.summary.clone()));
        }

        AppViewState::Select
    }

    fn scene_snapshot(&self) -> AppSceneSnapshot {
        let mut scene = match self.view_state() {
            AppViewState::Select => AppSceneSnapshot::Select(self.select_snapshot()),
            AppViewState::Decide => AppSceneSnapshot::Decide(
                self.pending_decide
                    .as_ref()
                    .map(|decide| self.decide_snapshot(decide))
                    .or_else(|| self.last_play_snapshot.clone())
                    .unwrap_or_default(),
            ),
            AppViewState::Play => {
                AppSceneSnapshot::Play(self.last_play_snapshot.clone().unwrap_or_default())
            }
            AppViewState::Result(summary) => AppSceneSnapshot::Result(ResultSnapshot {
                clear_type: summary.clear_type,
                ex_score: summary.ex_score,
                ex_score_rate: summary.ex_score_rate(),
                max_combo: summary.max_combo,
                gauge_value: summary.gauge_value,
                gauge_type: summary.gauge_type as i32,
                total_notes: summary.total_notes,
                judge_counts: DisplayJudgeCounts {
                    pgreat: summary.judge_counts.pgreat,
                    great: summary.judge_counts.great,
                    good: summary.judge_counts.good,
                    bad: summary.judge_counts.bad,
                    poor: summary.judge_counts.poor,
                    empty_poor: summary.judge_counts.empty_poor,
                },
                fast_slow_counts: FastSlowJudgeCounts {
                    fast_pgreat: summary.fast_slow_counts.fast_pgreat,
                    slow_pgreat: summary.fast_slow_counts.slow_pgreat,
                    fast_great: summary.fast_slow_counts.fast_great,
                    slow_great: summary.fast_slow_counts.slow_great,
                    fast_good: summary.fast_slow_counts.fast_good,
                    slow_good: summary.fast_slow_counts.slow_good,
                    fast_bad: summary.fast_slow_counts.fast_bad,
                    slow_bad: summary.fast_slow_counts.slow_bad,
                    fast_poor: summary.fast_slow_counts.fast_poor,
                    slow_poor: summary.fast_slow_counts.slow_poor,
                    fast_empty_poor: summary.fast_slow_counts.fast_empty_poor,
                    slow_empty_poor: summary.fast_slow_counts.slow_empty_poor,
                },
                score_history_id: summary.score_history_id,
                replay_saved: !summary.replay_path.is_empty(),
                best_ex_score: summary.best_ex_score,
                best_clear_type: summary.best_clear_type,
                target_ex_score: summary.target_ex_score,
                best_max_combo: summary.best_max_combo,
                target_max_combo: summary.target_max_combo,
                best_misscount: summary.best_misscount,
                target_misscount: summary.target_misscount,
                target_clear_type: summary.target_clear_type,
                elapsed_time: bmz_core::time::TimeUs(
                    self.result_scene_started_at.elapsed().as_micros().min(i64::MAX as u128) as i64,
                ),
                fadeout_elapsed: self.result_exit.as_ref().map(|exit| {
                    bmz_core::time::TimeUs(
                        exit.started_at.elapsed().as_micros().min(i64::MAX as u128) as i64,
                    )
                }),
                title: summary.title.clone(),
                subtitle: summary.subtitle.clone(),
                artist: summary.artist.clone(),
                subartist: summary.subartist.clone(),
                genre: summary.genre.clone(),
                difficulty_name: summary.difficulty_name.clone(),
                play_level: summary.play_level.clone(),
                overlay: OverlaySnapshot::default(),
            }),
        };
        let overlay = self.build_overlay_snapshot();
        self.apply_overlay_to_scene(&mut scene, overlay);
        scene
    }

    fn build_overlay_snapshot(&self) -> OverlaySnapshot {
        OverlaySnapshot { text: self.always_overlay_text(), fps_text: self.wgpu_fps_overlay_text() }
    }

    fn always_overlay_text(&self) -> String {
        let player_name = env!("CARGO_PKG_NAME");
        let player_version = env!("CARGO_PKG_VERSION");
        if self.is_autoplay_for_overlay() {
            format!("{player_name} {player_version} autoplay")
        } else {
            format!("{player_name} {player_version}")
        }
    }

    fn wgpu_fps_overlay_text(&self) -> String {
        if self.wgpu_fps <= 0.0 {
            return String::new();
        }
        format!("FPS {:.1}", self.wgpu_fps)
    }

    fn is_autoplay_for_overlay(&self) -> bool {
        match self.view_state() {
            AppViewState::Result(_) => self.last_play_was_autoplay,
            AppViewState::Play => self
                .active_play
                .as_ref()
                .map(|active| active.running.session.autoplay.is_some())
                .or_else(|| {
                    self.pending_play_start
                        .as_ref()
                        .map(|_| self.assist_option == AssistOption::Autoplay)
                })
                .unwrap_or(self.last_play_was_autoplay),
            AppViewState::Select | AppViewState::Decide => {
                self.assist_option == AssistOption::Autoplay
            }
        }
    }

    fn apply_overlay_to_scene(&self, scene: &mut AppSceneSnapshot, overlay: OverlaySnapshot) {
        match scene {
            AppSceneSnapshot::Select(snapshot) => snapshot.overlay = overlay,
            AppSceneSnapshot::Decide(snapshot) | AppSceneSnapshot::Play(snapshot) => {
                snapshot.overlay = overlay
            }
            AppSceneSnapshot::Result(snapshot) => snapshot.overlay = overlay,
        }
    }

    /// 難易度表のパンくず表示名。テーブルが既知なら `[symbol] name`、
    /// 不明なら URL のファイル名部分にフォールバックする。
    fn table_breadcrumb_name(&self, source_url: &str) -> String {
        self.boot
            .library_db
            .list_difficulty_tables()
            .ok()
            .and_then(|ts| ts.into_iter().find(|t| t.source_url == source_url))
            .map(|t| format!("[{}] {}", t.symbol, t.name))
            .unwrap_or_else(|| {
                std::path::Path::new(source_url)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(source_url)
                    .to_string()
            })
    }

    fn select_snapshot(&self) -> SelectSnapshot {
        let selected = self.select_items.get(self.selected_index);
        let current_folder = match self.folder_stack.last() {
            None => String::new(),
            Some(path) if path.starts_with(TABLE_ROOT_PATH) => match parse_table_path(path) {
                Some(TablePath::Root) | None => "難易度表".to_string(),
                Some(TablePath::Table { source_url }) => self.table_breadcrumb_name(source_url),
                Some(TablePath::Level { source_url, level }) => {
                    let table_name = self.table_breadcrumb_name(source_url);
                    let symbol = self
                        .boot
                        .library_db
                        .list_difficulty_tables()
                        .ok()
                        .and_then(|ts| ts.into_iter().find(|t| t.source_url == source_url))
                        .map(|t| t.symbol)
                        .unwrap_or_default();
                    format!("{table_name} > {symbol}{level}")
                }
            },
            Some(path) => std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string(),
        };
        SelectSnapshot {
            time: self.select_time(),
            selection_time: self.select_bar_time(),
            option_panel_time: self.option_panel_time(),
            option_panel: self.select_option_panel,
            chart_count: self.select_items.len() as u32,
            selected_index: self.selected_index as u32,
            selected_chart_id: match selected {
                Some(SelectItem::Chart(row)) => row.chart.as_ref().map(|chart| chart.chart_id),
                _ => None,
            },
            selected_title: selected.map(|i| i.display_name().to_string()).unwrap_or_default(),
            rows: select_snapshot_rows(&self.select_items, self.selected_index, 25),
            arrange: self.arrange_option.as_str().to_string(),
            target: self.target_option.as_str().to_string(),
            gauge: gauge_option_as_str(self.gauge_option).to_string(),
            assist: self.assist_option.as_str().to_string(),
            bga: bga_mode_as_str(self.boot.profile_config.play.bga).to_string(),
            master_volume: self.boot.profile_config.audio_mix.master_volume,
            key_volume: self.boot.profile_config.audio_mix.key_volume,
            bgm_volume: self.boot.profile_config.audio_mix.bgm_volume,
            current_folder,
            key_hint: self.select_keys.key_hint.clone(),
            option_hint: self.select_keys.option_hint.clone(),
            exit_hold_progress: self.select_exit_hold_progress(),
            overlay: OverlaySnapshot::default(),
            stage_background: self.select_stage_loaded,
            banner_image: self.select_banner_loaded,
        }
    }

    fn sync_select_preview_audio(&mut self) {
        let cache_key = match self.select_items.get(self.selected_index) {
            Some(SelectItem::Chart(row)) => row.chart.as_ref().and_then(|chart| {
                (!chart.preview_file.is_empty())
                    .then(|| format!("{}|{}", chart.folder_path, chart.preview_file))
            }),
            _ => None,
        };
        if cache_key.as_deref() == self.select_preview_source.as_deref() {
            return;
        }
        let had_preview = self.select_preview_playing;
        self.select_preview_source = cache_key.clone();

        let mix = &self.boot.profile_config.audio_mix;
        let volume = (mix.master_volume * mix.preview_volume).clamp(0.0, 1.0);

        let loaded = match (&self.select_preview, cache_key.as_deref()) {
            (Some(preview), Some(key)) => {
                let (folder, file) = key.split_once('|').unwrap_or(("", ""));
                preview.stop();
                crate::chart_asset::resolve_chart_asset_path(folder, file)
                    .is_some_and(|path| preview.load_and_play(&path, volume))
            }
            (Some(preview), None) => {
                preview.stop();
                false
            }
            (None, _) => false,
        };

        self.select_preview_playing = loaded;
        if loaded {
            self.stop_all_system_bgm();
        } else if cache_key.is_some() || had_preview {
            self.play_system_sound(crate::system_sound::SoundType::Select);
        }
    }

    fn stop_select_preview(&mut self) {
        if let Some(preview) = &self.select_preview {
            preview.stop();
        }
        self.select_preview_source = None;
        self.select_preview_playing = false;
    }

    fn sync_select_banner_texture(&mut self) {
        let cache_key = match self.select_items.get(self.selected_index) {
            Some(SelectItem::Chart(row)) => row.chart.as_ref().and_then(|chart| {
                (!chart.banner_file.is_empty())
                    .then(|| format!("{}|{}", chart.folder_path, chart.banner_file))
            }),
            _ => None,
        };
        if cache_key.as_deref() == self.select_banner_source.as_deref() {
            return;
        }
        self.select_banner_source = cache_key.clone();
        self.select_banner_loaded = cache_key.is_some_and(|key| {
            let (folder, file) = key.split_once('|').unwrap_or(("", ""));
            load_chart_meta_texture(&mut self.renderer, SELECT_BANNER_TEXTURE, folder, file)
        });
    }

    fn sync_select_stage_texture(&mut self) {
        let cache_key = match self.select_items.get(self.selected_index) {
            Some(SelectItem::Chart(row)) => row.chart.as_ref().and_then(|chart| {
                (!chart.stage_file.is_empty())
                    .then(|| format!("{}|{}", chart.folder_path, chart.stage_file))
            }),
            _ => None,
        };
        if cache_key.as_deref() == self.select_stage_source.as_deref() {
            return;
        }
        self.select_stage_source = cache_key.clone();
        self.select_stage_loaded = cache_key.is_some_and(|key| {
            let (folder, file) = key.split_once('|').unwrap_or(("", ""));
            load_chart_meta_texture(&mut self.renderer, SELECT_STAGE_TEXTURE, folder, file)
        });
    }

    fn should_exit_via_select_hold(&mut self) -> bool {
        if !matches!(self.view_state(), AppViewState::Select) {
            self.select_exit_hold_started_at = None;
            return false;
        }
        let Some(started) = self.select_exit_hold_started_at else {
            return false;
        };
        started.elapsed() >= SELECT_EXIT_HOLD_DURATION
    }

    fn select_exit_hold_progress(&self) -> f32 {
        let Some(started) = self.select_exit_hold_started_at else {
            return 0.0;
        };
        let elapsed = started.elapsed().as_secs_f32();
        let total = SELECT_EXIT_HOLD_DURATION.as_secs_f32();
        (elapsed / total).clamp(0.0, 1.0)
    }

    fn select_time(&self) -> TimeUs {
        let micros =
            self.select_scene_started_at.elapsed().as_micros().min(i64::MAX as u128) as i64;
        TimeUs(micros)
    }

    fn select_bar_time(&self) -> TimeUs {
        let micros = self.select_bar_started_at.elapsed().as_micros().min(i64::MAX as u128) as i64;
        TimeUs(micros)
    }

    fn play_elapsed_time(&self) -> TimeUs {
        let micros = self.play_scene_started_at.elapsed().as_micros().min(i64::MAX as u128) as i64;
        TimeUs(micros)
    }

    fn decide_snapshot(&self, decide: &DecideTransition) -> RenderSnapshot {
        let mut snapshot = decide.snapshot.clone();
        snapshot.play_elapsed_time = elapsed_since(decide.started_at);
        snapshot.fadeout_elapsed_ms = decide.fadeout_started_at.map(elapsed_since_ms);
        snapshot
    }

    fn option_panel_time(&self) -> TimeUs {
        let micros =
            self.option_panel_started_at.elapsed().as_micros().min(i64::MAX as u128) as i64;
        TimeUs(micros)
    }

    fn set_start_held(&mut self, held: bool) {
        if self.start_held != held {
            self.start_held = held;
            self.update_select_option_panel();
        }
    }

    fn set_select_held(&mut self, held: bool) {
        if self.select_held != held {
            self.select_held = held;
            self.update_select_option_panel();
        }
    }

    fn update_select_option_panel(&mut self) {
        let panel = select_option_panel_for_holds(self.start_held, self.select_held);
        if self.select_option_panel != panel {
            self.select_option_panel = panel;
            self.option_panel_started_at = Instant::now();
        }
    }

    fn cycle_bga_option(&mut self) {
        self.boot.profile_config.play.bga = cycle_bga_option(self.boot.profile_config.play.bga);
        tracing::info!(
            bga = bga_mode_as_str(self.boot.profile_config.play.bga),
            "bga option changed"
        );
    }

    fn apply_play_option_control(&mut self, control: &str) -> bool {
        if self.select_keys.cycle_arrange.as_deref() == Some(control) {
            self.arrange_option = self.arrange_option.cycle();
            tracing::info!(arrange = self.arrange_option.as_str(), "arrange option changed");
            true
        } else if self.select_keys.cycle_gauge.as_deref() == Some(control) {
            self.gauge_option = cycle_gauge_option(self.gauge_option);
            tracing::info!(gauge = ?self.gauge_option, "gauge option changed");
            true
        } else if self.select_keys.cycle_assist.as_deref() == Some(control) {
            self.set_assist_option(self.assist_option.cycle());
            tracing::info!(assist = self.assist_option.as_str(), "assist option changed");
            true
        } else {
            false
        }
    }

    fn set_assist_option(&mut self, assist: AssistOption) {
        self.assist_option = assist;
        self.boot.profile_config.play.auto_play = assist == AssistOption::Autoplay;
    }

    fn apply_assist_option_control(&mut self, control: &str) -> bool {
        if self.select_keys.cycle_assist.as_deref() == Some(control) {
            self.set_assist_option(self.assist_option.cycle());
            tracing::info!(assist = self.assist_option.as_str(), "assist option changed");
            true
        } else {
            false
        }
    }

    fn apply_target_option_cycle(&mut self, cycle: TargetCycle) {
        self.target_option = match cycle {
            TargetCycle::Previous => self.target_option.cycle_prev(),
            TargetCycle::Next => self.target_option.cycle(),
        };
        tracing::info!(target = self.target_option.as_str(), "target option changed");
    }

    fn apply_detail_option_control(&mut self, control: &str) -> bool {
        if self.select_keys.cycle_bga.as_deref() == Some(control) {
            self.cycle_bga_option();
            true
        } else {
            false
        }
    }

    fn route_keyboard_input(&mut self, event: &winit::event::KeyEvent) {
        if let Some(active_play) = &mut self.active_play {
            if let Some(control) = physical_key_name(event.physical_key)
                && self.select_keys.is_start(&control)
                && !event.repeat
            {
                active_play.running.session.lane_cover_changing =
                    event.state == ElementState::Pressed;
            }
            if let Some(change) = hispeed_action(event.physical_key, event.state, event.repeat) {
                // Beatoraja: NoSpeed constraint locks the hispeed during course play.
                let speed_locked = self
                    .active_course
                    .as_ref()
                    .is_some_and(|c| {
                        c.definition.constraints.speed
                            == bmz_core::course::CourseSpeedConstraint::NoSpeed
                    });
                if speed_locked {
                    tracing::debug!("hispeed change ignored: course NoSpeed constraint");
                    return;
                }
                active_play.running.session.hispeed =
                    adjusted_hispeed(active_play.running.session.hispeed, change);
                tracing::info!(hispeed = active_play.running.session.hispeed, "adjusted hispeed");
                return;
            }
            if event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                && event.state == ElementState::Pressed
                && !event.repeat
            {
                let session = &mut active_play.running.session;
                if !session.judge.is_exhausted(&session.chart) {
                    tracing::info!("escape pressed during play; marking session as failed");
                    session.state = bmz_gameplay::session::PlayState::Failed;
                    self.play_system_sound(crate::system_sound::SoundType::PlayStop);
                }
                return;
            }
            if let Some(delta) = lane_cover_step(event.physical_key, event.state, event.repeat) {
                let session = &mut active_play.running.session;
                if session.lane_cover_visible {
                    // SUDDEN+ は ArrowDown で値を大きくする (上下逆操作)。
                    session.lane_cover = (session.lane_cover - delta).clamp(0.0, 1.0);
                    tracing::info!(lane_cover = session.lane_cover, "adjusted lane cover");
                } else {
                    session.lift = (session.lift + delta).clamp(0.0, 1.0);
                    tracing::info!(lift = session.lift, "adjusted lift");
                }
                return;
            }
            // Start ボタンの2回連続押し → レーンカバー表示切替
            if event.state == ElementState::Pressed
                && !event.repeat
                && let Some(control) = physical_key_name(event.physical_key)
                && self.select_keys.is_start(&control)
            {
                let now = Instant::now();
                let is_double = self
                    .last_play_start_press_at
                    .is_some_and(|prev| now.duration_since(prev) <= PLAY_START_DOUBLE_PRESS_WINDOW);
                if is_double {
                    let session = &mut active_play.running.session;
                    session.lane_cover_visible = !session.lane_cover_visible;
                    tracing::info!(
                        lane_cover_visible = session.lane_cover_visible,
                        "toggled lane cover visibility",
                    );
                    self.last_play_start_press_at = None;
                } else {
                    self.last_play_start_press_at = Some(now);
                }
                // Start キーはゲームプレイ入力としても通すのでフォールスルー
            }
            active_play.input.handle_key_event(event);
            return;
        }

        if self.pending_decide.is_some() {
            if self.decide_input_ready()
                && let Some(action) = decide_action(event.physical_key, event.state, event.repeat)
            {
                self.begin_decide_fadeout(matches!(action, DecideAction::Cancel));
            }
            return;
        }

        if self.finished_play.is_some() {
            // 終了アニメーション中 (result_exit=Some) は追加入力を受け付けない。
            if self.result_exit.is_none()
                && self.result_input_ready()
                && let Some(action) = result_action(event.physical_key, event.state, event.repeat)
            {
                match action {
                    ResultAction::Retry => self.begin_result_exit(ResultExitAction::Retry),
                    ResultAction::Leave => self.begin_result_exit(ResultExitAction::Leave),
                }
            }
            return;
        }

        if matches!(self.view_state(), AppViewState::Select)
            && event.physical_key == PhysicalKey::Code(KeyCode::F5)
            && event.state == ElementState::Pressed
            && !event.repeat
        {
            self.reload_from_select_context();
            return;
        }

        // Select 画面で ESC 長押し → アプリ終了 (実際の exit は redraw 時にチェック)。
        if event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
            match event.state {
                ElementState::Pressed => {
                    if self.select_exit_hold_started_at.is_none() {
                        self.select_exit_hold_started_at = Some(Instant::now());
                    }
                }
                ElementState::Released => {
                    self.select_exit_hold_started_at = None;
                }
            }
            return;
        }

        if is_select_start_key(event.physical_key, &self.select_keys) {
            self.set_start_held(event.state == ElementState::Pressed);
            return;
        }

        if is_select_modifier_key(event.physical_key, &self.select_keys) {
            self.set_select_held(event.state == ElementState::Pressed);
            return;
        }

        if self.select_option_panel != 0 {
            if event.state == ElementState::Pressed && !event.repeat {
                match self.select_option_panel {
                    1 => {
                        if let Some(slot) = digit_to_replay_slot(event.physical_key) {
                            if !self.start_replay_for_selected(slot) {
                                tracing::info!(slot, "Start+digit pressed but no replay available");
                            }
                            return;
                        }
                        if let Some(cycle) = target_cycle_from_key(event.physical_key) {
                            self.apply_target_option_cycle(cycle);
                            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                            return;
                        }
                        if let Some(control) = physical_key_name(event.physical_key)
                            && self.apply_play_option_control(&control)
                        {
                            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                        }
                    }
                    2 => {
                        if let Some(control) = physical_key_name(event.physical_key)
                            && self.apply_assist_option_control(&control)
                        {
                            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                        }
                    }
                    3 => {
                        if let Some(control) = physical_key_name(event.physical_key)
                            && self.apply_detail_option_control(&control)
                        {
                            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                        }
                    }
                    _ => {}
                }
            }
            return;
        }

        if let Some(action) =
            select_action(event.physical_key, event.state, event.repeat, &self.select_keys)
        {
            match action {
                SelectAction::EnterOrPlay => self.enter_or_play_selected(),
                SelectAction::ExitFolder => self.exit_folder(),
                SelectAction::Move(select_move) => self.move_selection(select_move),
            }
        }
    }

    fn poll_gamepad_events(&mut self) {
        let Some(gilrs) = &mut self.gilrs else { return };
        let events = gilrs.poll();
        for event in &events {
            let device_event = crate::input::gilrs::to_device_input_event(event);
            if let Some(active_play) = &self.active_play {
                active_play.input.push_shared_event(device_event);
            }
            self.route_gamepad_button(&event.name.clone(), event.pressed);
        }
    }

    fn route_gamepad_button(&mut self, button: &str, pressed: bool) {
        if let Some(active_play) = &mut self.active_play
            && self.select_keys.is_start(button)
        {
            active_play.running.session.lane_cover_changing = pressed;
        }
        if !pressed {
            if self.select_keys.is_start(button) {
                self.set_start_held(false);
            } else if self.select_keys.is_back(button) || matches!(button, "Select" | "DPadLeft") {
                self.set_select_held(false);
            }
            return;
        }

        // プレイ中: プレイ入力は push_shared_event で処理済み
        if self.active_play.is_some() {
            return;
        }

        if self.pending_decide.is_some() {
            if self.decide_input_ready() {
                match button {
                    "Button1" | "Start" => self.begin_decide_fadeout(false),
                    "Button2" | "Select" => self.begin_decide_fadeout(true),
                    _ => {}
                }
            }
            return;
        }

        // リザルト画面
        if self.finished_play.is_some() {
            // 終了アニメーション中 (result_exit=Some) は追加入力を受け付けない。
            if self.result_exit.is_none() && self.result_input_ready() {
                match button {
                    "Button1" | "Start" => self.begin_result_exit(ResultExitAction::Retry),
                    "Button2" | "Select" => self.begin_result_exit(ResultExitAction::Leave),
                    _ => {}
                }
            }
            return;
        }

        if self.select_keys.is_start(button) {
            self.set_start_held(true);
            return;
        }

        if self.select_keys.is_back(button) || matches!(button, "Select" | "DPadLeft") {
            self.set_select_held(true);
            return;
        }

        if self.select_option_panel != 0 {
            let option_changed = match self.select_option_panel {
                1 => self.apply_play_option_control(button),
                2 => self.apply_assist_option_control(button),
                3 => self.apply_detail_option_control(button),
                _ => false,
            };
            if self.select_option_panel == 1
                && let Some(cycle) = target_cycle_from_control(button, &self.select_keys)
            {
                self.apply_target_option_cycle(cycle);
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                return;
            }
            if option_changed {
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            }
            return;
        }

        // セレクト画面: 固定ナビゲーション + プロファイルバインド
        let action = match button {
            "DPadUp" => Some(SelectAction::Move(SelectMove::Previous)),
            "DPadDown" => Some(SelectAction::Move(SelectMove::Next)),
            "DPadLeft" | "Select" => Some(SelectAction::ExitFolder),
            "DPadRight" | "Button1" => Some(SelectAction::EnterOrPlay),
            _ => {
                if self.select_keys.is_enter(button) {
                    Some(SelectAction::EnterOrPlay)
                } else if self.select_keys.is_back(button) {
                    Some(SelectAction::ExitFolder)
                } else {
                    None
                }
            }
        };

        if let Some(action) = action {
            match action {
                SelectAction::EnterOrPlay => self.enter_or_play_selected(),
                SelectAction::ExitFolder => self.exit_folder(),
                SelectAction::Move(m) => self.move_selection(m),
            }
        }
    }

    fn route_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        if !matches!(self.view_state(), AppViewState::Select) {
            return;
        }
        if let Some(select_move) = select_wheel_move(delta) {
            self.move_selection(select_move);
        }
    }

    fn move_selection(&mut self, select_move: SelectMove) {
        if self.select_items.is_empty() {
            self.reload_select_items();
        }
        if self.select_items.is_empty() {
            return;
        }
        let previous_index = self.selected_index;
        self.selected_index =
            moved_select_index(self.selected_index, self.select_items.len(), select_move);
        if self.selected_index != previous_index {
            self.select_bar_started_at = Instant::now();
            self.play_system_sound(crate::system_sound::SoundType::Scratch);
        }
    }

    fn enter_or_play_selected(&mut self) {
        if self.select_items.is_empty() {
            self.reload_select_items();
        }
        match self.select_items.get(self.selected_index).cloned() {
            Some(SelectItem::Folder { path, .. }) => {
                // 入る直前のカーソル位置を覚えておき、出た時に復元できるようにする。
                self.selected_index_stack.push(self.selected_index);
                self.folder_stack.push(path);
                self.reload_select_items();
                self.selected_index = 0;
                self.select_bar_started_at = Instant::now();
                self.play_system_sound(crate::system_sound::SoundType::FolderOpen);
                tracing::info!(folder = ?self.folder_stack.last(), "entered folder");
            }
            Some(SelectItem::Chart(row)) => {
                if row.in_library() {
                    self.start_chart(
                        row.chart.as_ref().expect("in_library row has chart").chart_id,
                    );
                } else {
                    tracing::info!(
                        title = row.display_title(),
                        "skipping play for chart missing from library"
                    );
                }
            }
            Some(SelectItem::Course(row)) => {
                let course_id = row.course_id;
                self.start_course(course_id);
            }
            None => {
                tracing::warn!("no item is available to select");
            }
        }
    }

    fn exit_folder(&mut self) {
        if self.folder_stack.pop().is_some() {
            let restored = self.selected_index_stack.pop().unwrap_or(0);
            self.reload_select_items();
            // 復元先がリスト範囲外なら末尾にクランプする。
            self.selected_index = restored.min(self.select_items.len().saturating_sub(1));
            self.select_bar_started_at = Instant::now();
            self.play_system_sound(crate::system_sound::SoundType::FolderClose);
            tracing::info!(depth = self.folder_stack.len(), "exited folder");
        }
    }

    fn start_chart(&mut self, chart_id: i64) {
        let options = self.play_start_options();
        self.begin_decide_for_chart(chart_id, options);
    }

    fn start_course(&mut self, course_id: i64) {
        let stored = match self.boot.library_db.list_courses() {
            Ok(courses) => courses.into_iter().find(|c| c.id == course_id),
            Err(error) => {
                tracing::error!(%error, course_id, "failed to load courses for start_course");
                return;
            }
        };
        let Some(stored) = stored else {
            tracing::warn!(course_id, "course not found");
            return;
        };
        let definition = stored.definition;
        let first_chart_id = definition.entries.iter().find_map(|e| e.chart_id);
        let Some(first_chart_id) = first_chart_id else {
            tracing::warn!(course_id, "no resolved chart in course");
            return;
        };
        tracing::info!(course_id, title = %definition.title, "starting course");
        let mut options = self.play_start_options();
        apply_course_constraints(&mut options, &definition.constraints);
        self.active_course = Some(ActiveCourseSession {
            course_id,
            definition,
            current_index: 0,
            entry_results: Vec::new(),
        });
        self.start_chart_with_options(first_chart_id, options);
    }

    fn advance_course_after_finish(&mut self, finished: FinishedPlaySession) {
        let Some(course) = &mut self.active_course else {
            return;
        };
        let chart_id = self.last_started_chart_id.unwrap_or(0);
        // Beatoraja behavior: if any chart in the course is Failed, the course
        // ends immediately and remaining charts are skipped.
        let failed = finished.result.clear_type == bmz_core::clear::ClearType::Failed;
        // Carry the gauge value of this chart over to the next chart in the
        // course (beatoraja keeps the gauge between songs).
        let carried_gauge = finished.result.gauge_value;
        course.entry_results.push(CourseEntryResult { chart_id, finished });
        course.current_index += 1;

        let next_index = course.current_index;
        let constraints = course.definition.constraints.clone();
        let next_chart_id =
            course.definition.entries.get(next_index).and_then(|e| e.chart_id);

        if !failed && let Some(next_chart_id) = next_chart_id {
            let mut options = self.play_start_options();
            apply_course_constraints(&mut options, &constraints);
            options.initial_gauge_value = Some(carried_gauge);
            self.start_chart_with_options(next_chart_id, options);
            return;
        }

        // Course is over either because every entry was played or because the
        // most recent chart was Failed (skip remaining entries).
        let course = self.active_course.take().unwrap();
        let last_finished = course.entry_results.last().map(|r| r.finished.clone());
        let course_result = course.into_result();
        tracing::info!(
            title = %course_result.title,
            total_ex_score = course_result.total_ex_score,
            course_clear = course_result.course_clear,
            course_failed = course_result.course_failed,
            played = course_result.played_entries,
            total = course_result.total_entries,
            trophies = ?course_result
                .trophy_results
                .iter()
                .filter(|t| t.achieved)
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>(),
            "course finished"
        );
        self.finished_course = Some(course_result);
        // Use the last chart's result for the standard result skin display.
        if let Some(last) = last_finished {
            self.finished_play = Some(last);
            self.result_scene_started_at = Instant::now();
            self.ensure_skin_ready(SkinKind::Result);
        }
    }

    fn begin_decide_for_chart(&mut self, chart_id: i64, options: PlayStartOptions) {
        self.ensure_skin_ready(SkinKind::Decide);
        self.spawn_play_skin_decode_for(self.key_mode_for_chart(chart_id));
        self.ensure_skin_ready(SkinKind::Play);
        self.start_play_preload(chart_id, options.clone());
        let now = Instant::now();
        self.pending_decide = Some(DecideTransition {
            chart_id,
            started_at: now,
            fadeout_started_at: None,
            cancel: false,
            snapshot: self.decide_snapshot_for_chart(chart_id),
        });
    }

    fn start_play_preload(&mut self, chart_id: i64, options: PlayStartOptions) {
        self.play_preload_generation = self.play_preload_generation.wrapping_add(1);
        let generation = self.play_preload_generation;
        let (tx, rx) = mpsc::channel();
        let library_db_path = self.boot.app_paths.library_db.clone();
        let app_config = self.boot.app_config.clone();
        thread::Builder::new()
            .name(format!("play-preload-{chart_id}"))
            .spawn(move || {
                let result = (|| -> Result<PreloadedWinitPlaySession> {
                    let library_db =
                        crate::storage::library_db::LibraryDatabase::open(&library_db_path)?;
                    let input = crate::input::winit::WinitInputBackend::default();
                    let session_options =
                        crate::screens::play_start::play_session_options_from_start(
                            &app_config,
                            options,
                        );
                    let preloaded = crate::screens::play_session::preload_play_session_for_chart(
                        &library_db,
                        chart_id,
                        session_options.clone(),
                    )?;
                    Ok(PreloadedWinitPlaySession { preloaded, input, session_options })
                })()
                .map_err(|error| format!("{error:#}"));
                let _ = tx.send(PlayPreloadResult { generation, chart_id, result });
            })
            .expect("failed to spawn play preload thread");
        self.pending_play_preload = Some(PendingPlayPreload { generation, chart_id, rx });
        tracing::info!(chart_id, generation, "play preload started");
    }

    fn invalidate_play_preload(&mut self) {
        self.play_preload_generation = self.play_preload_generation.wrapping_add(1);
        self.pending_play_preload = None;
    }

    /// select_items に持っている `ChartListItem.mode` から KeyMode を引く。
    /// 未知 / 見つからない場合はデフォルトの 7K を返す (プレイスキン解決のフォールバック)。
    fn key_mode_for_chart(&self, chart_id: i64) -> KeyMode {
        self.select_items
            .iter()
            .find_map(|item| match item {
                SelectItem::Chart(row) => row.chart.as_ref().and_then(|chart| {
                    (chart.chart_id == chart_id).then(|| KeyMode::from_str_opt(&chart.mode))
                }),
                _ => None,
            })
            .flatten()
            .unwrap_or_default()
    }

    fn decide_snapshot_for_chart(&self, chart_id: i64) -> RenderSnapshot {
        let mut snapshot = RenderSnapshot::default();
        if let Some(SelectItem::Chart(row)) = self.select_items.iter().find(|item| match item {
            SelectItem::Chart(row) => {
                row.chart.as_ref().is_some_and(|chart| chart.chart_id == chart_id)
            }
            SelectItem::Folder { .. } | SelectItem::Course(_) => false,
        }) && let Some(chart) = &row.chart
        {
            snapshot.title = chart.title.clone();
            snapshot.artist = chart.artist.clone();
            snapshot.difficulty_name = chart.difficulty_name.clone();
            snapshot.play_level = chart.play_level.clone();
            snapshot.total_notes = chart.total_notes;
            snapshot.duration = TimeUs(chart.length_ms.saturating_mul(1_000));
            snapshot.min_bpm = chart.min_bpm as f32;
            snapshot.max_bpm = chart.max_bpm as f32;
            snapshot.now_bpm = chart.initial_bpm as f32;
        }
        snapshot
    }

    fn start_chart_with_options(&mut self, chart_id: i64, mut options: PlayStartOptions) {
        self.last_play_was_autoplay = options.autoplay;
        self.ensure_skin_ready(SkinKind::Decide);
        self.spawn_play_skin_decode_for(self.key_mode_for_chart(chart_id));
        self.ensure_skin_ready(SkinKind::Play);
        self.invalidate_play_preload();
        self.play_ending = None;
        self.result_exit = None;
        self.play_ready_sound_started_at = None;
        if options.chart_zero_time == TimeUs(0) {
            options.chart_zero_time = self.play_skin_playstart_offset();
        }
        // 新しいプレイの音声出力を開く前に、前曲の余韻再生を止めて出力を解放する。
        self.draining_audio = None;
        match prepare_play_session_for_chart_with_winit_input(
            &self.boot.library_db,
            &self.boot.app_config,
            &self.boot.profile_config,
            chart_id,
            options.clone(),
        )
        .and_then(|prepared| {
            open_prepared_winit_play_session(&self.boot.score_db, &self.boot.app_config, prepared)
        }) {
            Ok(active_play) => {
                self.enter_play_scene(chart_id, self.decide_snapshot_for_chart(chart_id));
                self.install_active_play(active_play);
            }
            Err(error) => {
                tracing::error!(chart_id, %error, "failed to start play");
            }
        }
    }

    fn play_skin_playstart_offset(&self) -> TimeUs {
        let playstart_ms =
            self.renderer.play_skin_document().map(|document| document.playstart).unwrap_or(0);
        TimeUs(-i64::from(playstart_ms.max(0)) * 1_000)
    }

    fn play_skin_ready_delay(&self) -> Duration {
        let ready_delay_ms = self.renderer.play_skin_document().map_or(0, |document| {
            document.loadstart.max(0).saturating_add(document.loadend.max(0))
        });
        skin_duration_ms(ready_delay_ms)
    }

    fn clear_play_backbmp_state(&mut self) {
        self.play_backbmp_source = None;
        self.play_backbmp_loaded = false;
    }

    fn enter_play_scene(&mut self, chart_id: i64, mut snapshot: RenderSnapshot) {
        self.play_ending = None;
        self.result_exit = None;
        self.play_ready_sound_started_at = None;
        self.active_play = None;
        self.clear_play_backbmp_state();
        self.finished_play = None;
        self.draining_audio = None;
        self.play_scene_started_at = Instant::now();
        snapshot.play_elapsed_time = TimeUs(0);
        snapshot.ready_elapsed_time = None;
        snapshot.time = self.play_skin_playstart_offset();
        self.last_play_snapshot = Some(snapshot.clone());
        self.pending_play_start = Some(PendingPlayStart { chart_id });
        self.last_started_chart_id = Some(chart_id);
    }

    fn install_active_play(&mut self, mut active_play: StartedWinitPlaySession) {
        self.last_play_was_autoplay = active_play.running.session.autoplay.is_some();
        active_play.running.bga_frames =
            load_chart_bga_textures(&mut self.renderer, &active_play.running.session.chart);
        let chart = &active_play.running.session.chart;
        let folder = chart_asset_folder(chart)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let backbmp_key = format!("{}|{}", folder, chart.metadata.backbmp_file);
        if self.play_backbmp_source.as_deref() != Some(backbmp_key.as_str()) {
            self.play_backbmp_source = Some(backbmp_key);
            self.play_backbmp_loaded = load_chart_meta_texture(
                &mut self.renderer,
                PLAY_BACKBMP_TEXTURE,
                &folder,
                &chart.metadata.backbmp_file,
            );
        }
        let render_now = self.play_skin_playstart_offset();
        let mut snapshot = build_render_snapshot_with_target_and_bga_frames(
            &active_play.running.session,
            render_now,
            &active_play.running.session.recent_judgements,
            active_play.running.best_ex_score,
            active_play.running.best_ghost.as_deref(),
            active_play.running.target_ex_score,
            &active_play.running.bga_frames,
        );
        snapshot.backbmp_background = self.play_backbmp_loaded;
        snapshot.play_elapsed_time = self.play_elapsed_time();
        snapshot.ready_elapsed_time = self.play_ready_sound_started_at.map(elapsed_since);
        self.last_play_snapshot = Some(snapshot);
        self.active_play = Some(active_play);
    }

    fn poll_play_preload(&mut self) {
        if self.pending_decide.is_some() && self.pending_play_start.is_none() {
            return;
        }
        let Some(pending) = &self.pending_play_preload else {
            return;
        };
        let result = match pending.rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!(
                    chart_id = pending.chart_id,
                    generation = pending.generation,
                    "play preload worker disconnected"
                );
                self.pending_play_preload = None;
                return;
            }
        };
        self.pending_play_preload = None;

        let Some(play_start) = &self.pending_play_start else {
            tracing::debug!(
                chart_id = result.chart_id,
                generation = result.generation,
                "discarding play preload result with no pending play scene"
            );
            return;
        };
        if result.generation != self.play_preload_generation
            || result.chart_id != play_start.chart_id
        {
            tracing::debug!(
                chart_id = result.chart_id,
                generation = result.generation,
                current_generation = self.play_preload_generation,
                pending_chart_id = play_start.chart_id,
                "discarding stale play preload result"
            );
            return;
        }

        match result.result {
            Ok(prepared) => {
                let prepared =
                    prepare_winit_play_session_from_preloaded(&self.boot.profile_config, prepared);
                match open_prepared_winit_play_session(
                    &self.boot.score_db,
                    &self.boot.app_config,
                    prepared,
                ) {
                    Ok(active_play) => {
                        tracing::info!(
                            chart_id = play_start.chart_id,
                            generation = result.generation,
                            "play preload installed"
                        );
                        self.install_active_play(active_play);
                    }
                    Err(error) => {
                        tracing::error!(
                            chart_id = play_start.chart_id,
                            %error,
                            "failed to open preloaded play audio"
                        );
                        self.abort_pending_play_start();
                    }
                }
            }
            Err(error) => {
                tracing::error!(
                    chart_id = play_start.chart_id,
                    error,
                    "failed to preload play session"
                );
                self.abort_pending_play_start();
            }
        }
    }

    fn abort_pending_play_start(&mut self) {
        self.pending_play_start = None;
        self.active_play = None;
        self.clear_play_backbmp_state();
        self.last_play_snapshot = None;
        let now = Instant::now();
        self.select_scene_started_at = now;
        self.select_bar_started_at = now;
    }

    fn play_start_options(&self) -> PlayStartOptions {
        let arrange_seed = match self.arrange_option {
            ArrangeOption::Random => Some(crate::screens::play_session::generate_arrange_seed()),
            _ => None,
        };
        PlayStartOptions {
            autoplay: self.assist_option == AssistOption::Autoplay,
            gauge: Some(self.gauge_option),
            arrange: self.arrange_option,
            target: self.target_option,
            arrange_seed,
            ..Default::default()
        }
    }

    fn try_start_replay_for_chart(&mut self, chart_id: i64, slot: u8) -> bool {
        let Some(sha) = self.boot.library_db.chart_sha256_by_chart_id(chart_id).ok().flatten()
        else {
            tracing::warn!(chart_id, "replay start failed: chart sha256 not found");
            return false;
        };
        let Some(slot_record) = self.boot.score_db.replay_slot(sha, slot).ok().flatten() else {
            tracing::info!(slot, "no replay saved for slot");
            return false;
        };
        let abs_path = self.boot.profile_paths.root_dir.join(&slot_record.replay_path);
        let replay_file = match load_replay_for_chart(&abs_path, sha) {
            Ok(file) => file,
            Err(error) => {
                tracing::warn!(%error, path = %abs_path.display(), "replay load failed");
                return false;
            }
        };
        let player = bmz_gameplay::replay::ReplayPlayer {
            events: replay_file.events.clone(),
            next_index: 0,
        };
        let options = PlayStartOptions {
            autoplay: false,
            replay_player: Some(player),
            chart_zero_time: TimeUs(0),
            gauge: Some(self.gauge_option),
            arrange: replay_file.arrange_option(),
            target: self.target_option,
            arrange_seed: replay_file.arrange_seed,
            arrange_pattern: replay_file.lane_shuffle_pattern.clone(),
            initial_gauge_value: None,
        };
        self.start_chart_with_options(chart_id, options);
        true
    }

    fn start_replay_for_selected(&mut self, slot: u8) -> bool {
        let Some(chart_id) = self.currently_selected_chart_id() else {
            return false;
        };
        self.try_start_replay_for_chart(chart_id, slot)
    }

    fn currently_selected_chart_id(&self) -> Option<i64> {
        match self.select_items.get(self.selected_index)? {
            SelectItem::Chart(row) => row.chart.as_ref().map(|chart| chart.chart_id),
            SelectItem::Folder { .. } | SelectItem::Course(_) => None,
        }
    }

    fn retry_last_chart(&mut self) {
        let Some(chart_id) = self.last_started_chart_id else {
            tracing::warn!("no previous chart is available to retry");
            return;
        };
        let options = self.play_start_options();
        self.start_chart_with_options(chart_id, options);
    }

    /// リザルト画面の終了アニメーションを開始する。
    /// スキンが宣言するフェードアウト時間が経過したら `advance_result_exit` が
    /// 実際の遷移 (選曲へ戻る / リトライ) を実行する。
    fn begin_result_exit(&mut self, action: ResultExitAction) {
        if self.result_exit.is_some() || self.finished_play.is_none() {
            return;
        }
        tracing::info!(?action, "result screen exit animation started");
        self.result_exit = Some(ResultExit { started_at: Instant::now(), action });
        // ResultClear / ResultFail のループ風長尺音を止めて、close SE を鳴らす。
        self.stop_system_sound(crate::system_sound::SoundType::ResultClear);
        self.stop_system_sound(crate::system_sound::SoundType::ResultFail);
        self.play_system_sound(crate::system_sound::SoundType::ResultClose);
    }

    fn decide_input_ready(&self) -> bool {
        let Some(decide) = &self.pending_decide else {
            return false;
        };
        decide.started_at.elapsed() >= self.decide_input_duration()
    }

    fn begin_decide_fadeout(&mut self, cancel: bool) {
        let Some(decide) = &mut self.pending_decide else {
            return;
        };
        if decide.fadeout_started_at.is_some() {
            return;
        }
        decide.cancel = cancel;
        decide.fadeout_started_at = Some(Instant::now());
    }

    fn advance_decide_transition(&mut self) {
        let Some(decide) = &self.pending_decide else {
            return;
        };
        if decide.fadeout_started_at.is_none()
            && decide.started_at.elapsed() >= self.decide_scene_duration()
        {
            self.begin_decide_fadeout(false);
            return;
        }

        let Some(fadeout_started_at) =
            self.pending_decide.as_ref().and_then(|d| d.fadeout_started_at)
        else {
            return;
        };
        if fadeout_started_at.elapsed() < self.decide_fadeout_duration() {
            return;
        }

        let Some(decide) = self.pending_decide.take() else {
            return;
        };
        if decide.cancel {
            self.invalidate_play_preload();
            let now = Instant::now();
            self.select_scene_started_at = now;
            self.select_bar_started_at = now;
        } else {
            self.enter_play_scene(decide.chart_id, decide.snapshot);
        }
    }

    fn advance_play_ending(&mut self) {
        let Some(ending) = &self.play_ending else {
            return;
        };
        if ending.failed {
            if ending.started_at.elapsed() >= self.play_close_duration() {
                self.finish_play_ending();
            }
            return;
        }

        if ending.fadeout_started_at.is_none()
            && ending.started_at.elapsed() >= self.play_pre_fadeout_duration(ending)
        {
            if let Some(ending) = &mut self.play_ending {
                ending.fadeout_started_at = Some(Instant::now());
            }
            return;
        }

        let Some(fadeout_started_at) = self.play_ending.as_ref().and_then(|e| e.fadeout_started_at)
        else {
            return;
        };
        if fadeout_started_at.elapsed() >= self.play_fadeout_duration() {
            self.finish_play_ending();
        }
    }

    fn finish_play_ending(&mut self) {
        let Some(ending) = self.play_ending.take() else {
            return;
        };
        if let Some(started) = self.active_play.take() {
            self.draining_audio = Some(started.running.audio);
        }
        if self.active_course.is_some() {
            self.advance_course_after_finish(ending.finished);
            return;
        }
        self.finished_play = Some(ending.finished);
        self.result_scene_started_at = Instant::now();
        self.ensure_skin_ready(SkinKind::Result);
    }

    /// 終了フェードアウトの経過を監視し、スキンのフェードアウト時間を過ぎたら
    /// 保留していた遷移を実行する。毎フレーム描画前に呼ぶ。
    fn advance_result_exit(&mut self) {
        if self.finished_play.is_some()
            && self.result_exit.is_none()
            && self.result_scene_started_at.elapsed() >= self.result_scene_duration()
        {
            self.begin_result_exit(ResultExitAction::Leave);
        }
        let Some(exit) = &self.result_exit else {
            return;
        };
        // 何らかの理由でリザルトを抜けていたら終了状態を破棄する。
        if self.finished_play.is_none() {
            self.result_exit = None;
            return;
        }
        let fadeout = Duration::from_millis(self.renderer.result_skin_fadeout_ms().max(0) as u64);
        if exit.started_at.elapsed() < fadeout {
            return;
        }
        let action = exit.action;
        self.result_exit = None;
        match action {
            ResultExitAction::Leave => self.leave_result(),
            ResultExitAction::Retry => self.retry_last_chart(),
        }
    }

    fn leave_result(&mut self) {
        self.finished_play = None;
        self.finished_course = None;
        self.active_course = None;
        self.result_exit = None;
        self.clear_play_backbmp_state();
        // リザルト画面を抜けたら、まだ鳴っていても余韻再生を止める。
        self.draining_audio = None;
        self.last_play_snapshot = None;
        self.reload_select_items();
        let now = Instant::now();
        self.select_scene_started_at = now;
        self.select_bar_started_at = now;
    }

    fn decide_input_duration(&self) -> Duration {
        skin_duration_ms(self.renderer.decide_skin_document().map(|d| d.input).unwrap_or(0))
    }

    fn decide_scene_duration(&self) -> Duration {
        skin_duration_ms(self.renderer.decide_skin_document().map(|d| d.scene).unwrap_or(0))
    }

    fn decide_fadeout_duration(&self) -> Duration {
        skin_duration_ms(self.renderer.decide_skin_document().map(|d| d.fadeout).unwrap_or(0))
    }

    fn play_finishmargin_duration(&self) -> Duration {
        skin_duration_ms(self.renderer.play_skin_document().map(|d| d.finishmargin).unwrap_or(0))
    }

    fn play_pre_fadeout_duration(&self, ending: &PlayEndingTransition) -> Duration {
        let finishmargin = self.play_finishmargin_duration();
        let Some(elapsed_ms) = ending.full_combo_elapsed_at_finish_ms else {
            return finishmargin;
        };
        let full_combo_ms = self
            .renderer
            .play_skin_timer_animation_duration_ms(48)
            .max(self.renderer.play_skin_timer_animation_duration_ms(49));
        let remaining_ms = full_combo_ms.saturating_sub(elapsed_ms.max(0));
        finishmargin.max(skin_duration_ms(remaining_ms))
    }

    fn play_fadeout_duration(&self) -> Duration {
        skin_duration_ms(self.renderer.play_skin_document().map(|d| d.fadeout).unwrap_or(0))
    }

    fn play_close_duration(&self) -> Duration {
        skin_duration_ms(self.renderer.play_skin_document().map(|d| d.close).unwrap_or(0))
    }

    fn result_input_ready(&self) -> bool {
        self.result_scene_started_at.elapsed() >= self.result_input_duration()
    }

    fn result_input_duration(&self) -> Duration {
        skin_duration_ms(self.renderer.result_skin_document().map(|d| d.input).unwrap_or(0))
    }

    fn result_scene_duration(&self) -> Duration {
        skin_duration_ms(self.renderer.result_skin_document().map(|d| d.scene).unwrap_or(0))
    }

    fn reload_select_items(&mut self) {
        let items = load_items_for_stack(&self.boot, &self.folder_stack);
        self.select_items = items;
        if self.selected_index >= self.select_items.len() {
            self.selected_index = self.select_items.len().saturating_sub(1);
        }
    }

    fn load_songs_and_reload(&mut self) {
        let scan_roots = self.song_load_roots_from_stack();

        if !scan_roots.is_empty() {
            match scan_songs(
                &mut self.boot.library_db,
                &scan_roots,
                &self.boot.app_config.scan,
                now_unix_seconds(),
                false,
            ) {
                Ok(report) => tracing::info!(
                    imported = report.summary.imported,
                    skipped = report.summary.skipped,
                    failed = report.summary.failed,
                    "song load complete"
                ),
                Err(error) => tracing::error!(%error, "song load failed"),
            }
        }

        self.reload_select_items();
    }

    fn song_load_roots_from_stack(&self) -> Vec<PathEntry> {
        if let Some(folder) = self.folder_stack.last()
            && !folder.starts_with(TABLE_ROOT_PATH)
        {
            return vec![PathEntry { path: folder.clone(), enabled: true, recursive: true }];
        }
        self.boot.app_config.songs.roots.iter().filter(|p| p.enabled).cloned().collect()
    }

    fn reload_from_select_context(&mut self) {
        let selected = self.select_items.get(self.selected_index);
        if let Some(url) = table_source_url_from_context(&self.folder_stack, selected) {
            self.spawn_table_fetch(url);
            return;
        }
        if let Some(path) = song_scan_path_from_context(&self.folder_stack, selected) {
            let roots = vec![PathEntry { path, enabled: true, recursive: true }];
            match scan_songs(
                &mut self.boot.library_db,
                &roots,
                &self.boot.app_config.scan,
                now_unix_seconds(),
                true,
            ) {
                Ok(report) => tracing::info!(
                    imported = report.summary.imported,
                    skipped = report.summary.skipped,
                    failed = report.summary.failed,
                    "F5 song reload complete"
                ),
                Err(error) => tracing::error!(%error, "F5 song reload failed"),
            }
            self.reload_select_items();
            return;
        }
        tracing::debug!("F5 reload: no applicable target in select context");
    }

    fn spawn_table_fetch(&mut self, url: String) {
        if self.pending_table_fetch.is_some() {
            tracing::debug!(%url, "table fetch already in progress");
            return;
        }
        let library_db_path = self.boot.app_paths.library_db.clone();
        let (tx, rx) = mpsc::channel();
        let fetch_url = url.clone();
        thread::Builder::new()
            .name("table-fetch".to_string())
            .spawn(move || {
                let result = (|| -> Result<()> {
                    migrate_library_db(&library_db_path)?;
                    let mut library_db = LibraryDatabase::open(&library_db_path)?;
                    let rt =
                        tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                    rt.block_on(crate::table_cmd::fetch_table_url(&fetch_url, &mut library_db))
                })();
                let _ = tx.send(result);
            })
            .expect("failed to spawn table fetch thread");
        self.pending_table_fetch = Some(rx);
        tracing::info!(%url, "started table fetch");
    }

    fn poll_pending_table_fetch(&mut self) {
        let Some(rx) = &self.pending_table_fetch else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(())) => {
                tracing::info!("table fetch complete");
                self.pending_table_fetch = None;
                self.reload_select_items();
            }
            Ok(Err(error)) => {
                tracing::error!(%error, "table fetch failed");
                self.pending_table_fetch = None;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("table fetch worker disconnected");
                self.pending_table_fetch = None;
            }
        }
    }

    /// upload worker を起動する。surface 接続後に一度だけ呼ぶ。
    /// decode worker からの receiver (`skin_decode_rx`) と GPU uploader を worker へ
    /// move し、worker は decode 結果を受けて GPU アップロードし `skin_upload_tx` で
    /// main へ返す。
    fn start_skin_upload_worker(&mut self) {
        if self.skin_upload_worker_started {
            return;
        }
        let Some(decode_rx) = self.skin_decode_rx.take() else {
            return;
        };
        let Some(uploader) = self.renderer.gpu_uploader() else {
            // surface 未接続。次回接続時に再試行できるよう receiver を戻す。
            self.skin_decode_rx = Some(decode_rx);
            return;
        };
        let upload_tx = self.skin_upload_tx.clone();
        thread::Builder::new()
            .name("skin-upload".to_string())
            .spawn(move || skin_upload_worker(decode_rx, upload_tx, uploader))
            .expect("failed to spawn skin upload thread");
        self.skin_upload_worker_started = true;
    }

    /// upload worker が GPU アップロードまで終えたスキンを非ブロッキングで取り込む。
    /// 毎フレーム呼ぶ。テクスチャ挿入 + フォント登録 + SkinContext 構築のみで軽量。
    fn drain_pending_skins(&mut self) {
        loop {
            match self.skin_upload_rx.try_recv() {
                Ok(result) => self.apply_uploaded_skin(result),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
    }

    /// 指定された kind のスキンがアップロードされ取り込まれるまでブロックして待つ。
    /// scene 遷移直前 (特にプレイ開始) に呼ぶ。GPU アップロードは upload worker 上で
    /// 進むため、main は worker からの受信を待つだけで重い同期処理は無い。
    /// 先読みが間に合っていれば待ちはゼロ。
    fn ensure_skin_ready(&mut self, kind: SkinKind) {
        while self.is_kind_pending_decode(kind) {
            match self.skin_upload_rx.recv() {
                Ok(result) => self.apply_uploaded_skin(result),
                Err(_) => break,
            }
        }
    }

    fn is_kind_pending_decode(&self, kind: SkinKind) -> bool {
        match kind {
            SkinKind::Select => self.pending_select_skin,
            SkinKind::Decide => self.pending_decide_skin,
            SkinKind::Play => self.pending_play_skin,
            SkinKind::Result => self.pending_result_skin,
        }
    }

    /// upload worker から届いた `UploadedSkin` を Renderer へ取り込む。
    /// stale generation は破棄。GPU アップロードは worker で完了済みなので、
    /// ここではハンドル挿入・フォント登録・SkinContext 構築のみ (軽量)。
    fn apply_uploaded_skin(&mut self, pending: PendingUploadResult) {
        let PendingUploadResult { generation, path, kind, uploaded } = pending;
        if generation != self.skin_reload_generation {
            tracing::debug!(
                path = %path.display(),
                kind = ?kind,
                generation,
                current = self.skin_reload_generation,
                "discarding stale uploaded skin"
            );
            return;
        }
        match kind {
            SkinKind::Select => self.pending_select_skin = false,
            SkinKind::Decide => self.pending_decide_skin = false,
            SkinKind::Play => self.pending_play_skin = false,
            SkinKind::Result => self.pending_result_skin = false,
        }
        let uploaded = match uploaded {
            Ok(uploaded) => uploaded,
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    kind = ?kind,
                    error = %format_error_chain(&error),
                    "failed to decode/upload beatoraja skin in background"
                );
                return;
            }
        };
        let Some(manifest) = self.default_skin_manifest.clone() else {
            tracing::warn!(
                path = %path.display(),
                kind = ?kind,
                "skipping uploaded skin because default skin manifest is unavailable"
            );
            return;
        };
        let UploadedSkin { kind, document, fonts, prepared } = uploaded;
        // フォント登録 (ab_glyph パース。軽量なので main で実施)。
        for font in fonts {
            install_decoded_font(&mut self.renderer, font);
        }
        // アップロード済みテクスチャを差し込み、SkinDocumentTexture を組む。
        let mut document_textures = Vec::with_capacity(prepared.len());
        for source in prepared {
            let PreparedSource { source_id, texture, prepared, size } = source;
            self.renderer.insert_prepared_texture(TextureId(texture.0), prepared);
            document_textures.push(SkinDocumentTexture { source_id, texture, source_size: size });
        }
        tracing::info!(
            path = %path.display(),
            kind = ?kind,
            sources = document_textures.len(),
            "beatoraja skin fully installed"
        );
        let preserve_play_dynamic_timers = kind == SkinKind::Play && self.active_play.is_some();
        set_decoded_skin_context(
            &mut self.renderer,
            kind,
            manifest,
            document,
            document_textures,
            preserve_play_dynamic_timers,
        );
    }

    fn advance_active_play(&mut self) {
        if self.play_ending.is_some() {
            self.update_play_ending_snapshot_timers();
            return;
        }
        if self.pending_play_start.is_some() {
            self.update_pending_play_snapshot_timers();
        }
        if self.active_play.is_none() {
            return;
        }
        self.maybe_start_ready_phase();
        if self.play_ready_sound_started_at.is_none() {
            self.update_pending_play_snapshot_timers();
            return;
        }
        let Some(active_play) = &mut self.active_play else {
            return;
        };

        // 動画BGAテクスチャを更新（前フレームの時刻を使用、1フレーム遅延は許容）
        let video_update_time =
            self.last_play_snapshot.as_ref().map(|s| s.time).unwrap_or(bmz_core::time::TimeUs(0));
        crate::video_bga::update_video_bga_frames(
            &mut self.renderer,
            &mut active_play.running,
            video_update_time,
        );

        match advance_running_play_session_until_result(
            &mut active_play.running,
            &mut self.boot.score_db,
            &self.boot.profile_paths,
            &self.boot.profile_config.replay,
            now_unix_seconds(),
        ) {
            Ok(PlayAdvanceOutcome::Playing(frame)) => {
                let mine_hits = frame.mine_hits.len();
                let mut snapshot = frame.render_snapshot;
                snapshot.play_elapsed_time = self.play_elapsed_time();
                snapshot.ready_elapsed_time = self.play_ready_sound_started_at.map(elapsed_since);
                snapshot.backbmp_background = self.play_backbmp_loaded;
                self.last_play_snapshot = Some(snapshot);
                self.play_landmine_se(mine_hits);
            }
            Ok(PlayAdvanceOutcome::Finished { frame, finished }) => {
                let hispeed = active_play.running.session.hispeed;
                let mine_hits = frame.mine_hits.len();
                let mut snapshot = frame.render_snapshot;
                snapshot.play_elapsed_time = self.play_elapsed_time();
                snapshot.ready_elapsed_time = self.play_ready_sound_started_at.map(elapsed_since);
                snapshot.backbmp_background = self.play_backbmp_loaded;
                let full_combo_elapsed_at_finish_ms = snapshot.full_combo_elapsed_ms;
                self.last_play_snapshot = Some(snapshot);
                self.play_landmine_se(mine_hits);
                // active_play がまだ残っている内に hispeed/lane_cover/lift を profile に保存する。
                self.save_current_play_options(Some(hispeed), "play finished");
                self.play_ending = Some(PlayEndingTransition {
                    started_at: Instant::now(),
                    fadeout_started_at: None,
                    failed: frame.state == bmz_gameplay::session::PlayState::Failed,
                    full_combo_elapsed_at_finish_ms,
                    finished,
                });
                self.update_play_ending_snapshot_timers();
            }
            Err(error) => {
                tracing::error!(%error, "failed to advance play session");
                self.active_play = None;
                self.clear_play_backbmp_state();
                self.last_play_snapshot = None;
            }
        }
    }

    fn maybe_start_ready_phase(&mut self) {
        if self.play_ready_sound_started_at.is_some() {
            return;
        }
        if self.play_elapsed_time().0 < self.play_skin_ready_delay().as_micros() as i64 {
            return;
        }
        let chart_zero_time = self.play_skin_playstart_offset();
        let Some(active_play) = &mut self.active_play else {
            return;
        };
        if let Err(error) = active_play.running.start(chart_zero_time) {
            tracing::error!(%error, "failed to start preloaded play audio");
            self.abort_pending_play_start();
            return;
        }
        self.play_ready_sound_started_at = Some(Instant::now());
        self.pending_play_start = None;
        self.play_system_sound(crate::system_sound::SoundType::PlayReady);
        if let Some(snapshot) = &mut self.last_play_snapshot {
            snapshot.ready_elapsed_time = Some(TimeUs(0));
            snapshot.time = chart_zero_time;
        }
    }

    fn update_pending_play_snapshot_timers(&mut self) {
        let play_elapsed_time = self.play_elapsed_time();
        let ready_elapsed_time = self.play_ready_sound_started_at.map(elapsed_since);
        let Some(snapshot) = &mut self.last_play_snapshot else {
            return;
        };
        snapshot.play_elapsed_time = play_elapsed_time;
        snapshot.ready_elapsed_time = ready_elapsed_time;
    }

    fn update_play_ending_snapshot_timers(&mut self) {
        let Some(ending) = &self.play_ending else {
            return;
        };
        let play_elapsed_time = self.play_elapsed_time();
        let Some(snapshot) = &mut self.last_play_snapshot else {
            return;
        };
        snapshot.play_elapsed_time = play_elapsed_time;
        snapshot.music_end_elapsed_ms =
            (!ending.failed).then_some(elapsed_since_ms(ending.started_at));
        snapshot.failed_elapsed_ms = ending.failed.then_some(elapsed_since_ms(ending.started_at));
        snapshot.fadeout_elapsed_ms = ending.fadeout_started_at.map(elapsed_since_ms);
        snapshot.full_combo_elapsed_ms = ending
            .full_combo_elapsed_at_finish_ms
            .map(|elapsed_ms| elapsed_ms.saturating_add(elapsed_since_ms(ending.started_at)));
    }

    /// `target_fps` (フォアグラウンド) / `frame_limit_in_background`
    /// (非フォーカス時) に従ってフレーム開始間隔を一定に保つ。
    ///
    /// 各 `RedrawRequested` の先頭で呼び、前フレーム開始からの経過が
    /// フレーム予算に満たなければ残りをスリープする。FPS 値が 0 の場合は
    /// 無制限としてスリープしない。
    fn limit_frame_rate(&mut self) {
        let frame_started = Instant::now();
        if let Some(last) = self.last_frame_at {
            let dt = frame_started.duration_since(last).as_secs_f32();
            if dt > 0.0 {
                let instant_fps = 1.0 / dt;
                self.wgpu_fps = if self.wgpu_fps <= 0.0 {
                    instant_fps
                } else {
                    self.wgpu_fps.mul_add(0.9, instant_fps * 0.1)
                };
            }
        }
        let fps = if self.focused {
            self.boot.app_config.video.target_fps
        } else {
            self.boot.app_config.video.frame_limit_in_background
        };
        if fps > 0
            && let Some(last) = self.last_frame_at
        {
            let budget = Duration::from_secs_f64(1.0 / f64::from(fps));
            let elapsed = last.elapsed();
            if elapsed < budget {
                thread::sleep(budget - elapsed);
            }
        }
        self.last_frame_at = Some(frame_started);
    }

    /// egui の 1 フレームを構築し、renderer へ描画データを渡す。
    /// `render_current_scene` の前に呼ぶこと。
    fn run_egui_frame(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let scene = match scene_kind(&self.scene_snapshot()) {
            AppSceneKind::Select => "Select",
            AppSceneKind::Decide => "Decide",
            AppSceneKind::Play => "Play",
            AppSceneKind::Result => "Result",
        };
        let size = window.inner_size();
        let info = DebugInfo { scene, width: size.width, height: size.height };
        let play5_path = self.boot.profile_config.skin.play5.clone();
        let play7_path = self.boot.profile_config.skin.play7.clone();
        let play10_path = self.boot.profile_config.skin.play10.clone();
        let play14_path = self.boot.profile_config.skin.play14.clone();
        let play5_defs = self.play_skin_defs_for_path(&play5_path);
        let play7_defs = self.play_skin_defs_for_path(&play7_path);
        let play10_defs = self.play_skin_defs_for_path(&play10_path);
        let play14_defs = self.play_skin_defs_for_path(&play14_path);
        let skin_meta = SkinConfigMeta {
            select: SceneSkinDefs::from_document(self.renderer.select_skin_document()),
            decide: SceneSkinDefs::from_document(self.renderer.decide_skin_document()),
            play5: play5_defs,
            play7: play7_defs,
            play10: play10_defs,
            play14: play14_defs,
            result: SceneSkinDefs::from_document(self.renderer.result_skin_document()),
        };
        // Clone the course summary so the egui closure can borrow it while
        // `self.egui` is uniquely borrowed.  CourseResultSummary is small —
        // a few strings and Vec<ResultSummary> — so the clone cost is minor.
        let course_result = self.finished_course.clone();
        let Some(egui) = self.egui.as_mut() else {
            return;
        };
        let output = egui.run(
            &window,
            &info,
            &mut self.boot.app_config,
            &mut self.boot.profile_config,
            &skin_meta,
            &self.skin_catalog,
            course_result.as_ref(),
        );
        self.renderer.set_egui_frame(output.frame);
        // デバッグパネルの開閉状態を profile config へ同期する。
        // 永続化は終了時 / プレイ後の save_profile_config に任せる。
        self.boot.profile_config.ui.show_fps = output.debug_panel_visible;
        // 本体設定パネルでの VSync 変更を即座に反映する (set_vsync は変化時のみ再構成)。
        self.renderer.set_vsync(self.boot.app_config.video.vsync);
        // ウィンドウモード変更をライブ反映する (差分があるときのみ適用)。
        let desired_mode = self.boot.app_config.video.mode.clone();
        if desired_mode != self.applied_window_mode {
            window.set_fullscreen(fullscreen_from_config(&desired_mode, window.current_monitor()));
            tracing::info!(mode = ?desired_mode, "window mode updated");
            self.applied_window_mode = desired_mode;
        }
        if output.save_app_config {
            match save_app_config(&self.boot.app_paths.config_toml, &self.boot.app_config) {
                Ok(()) => tracing::info!("app config saved from egui settings panel"),
                Err(error) => tracing::error!(%error, "failed to save app config"),
            }
        }
        if output.trigger_song_rescan {
            self.load_songs_and_reload();
        }
        if output.save_profile_config {
            match save_profile_config(
                &self.boot.profile_paths.profile_toml,
                &self.boot.profile_config,
            ) {
                Ok(()) => tracing::info!("profile config saved from egui skin panel"),
                Err(error) => tracing::error!(%error, "failed to save profile config"),
            }
        }
        if output.reset_skin_config {
            self.pending_skin_reload_at = None;
            self.reset_profile_config_from_disk();
        } else if output.skin_config_changed {
            self.apply_profile_skin_offsets_to_active_play();
            self.pending_skin_reload_at = Some(Instant::now() + SKIN_RELOAD_DEBOUNCE);
        }
        if let Some(reload_at) = self.pending_skin_reload_at
            && Instant::now() >= reload_at
        {
            self.pending_skin_reload_at = None;
            self.reload_skins();
        }
    }

    fn play_skin_defs_for_path(&mut self, path: &str) -> SceneSkinDefs {
        let key = path.trim().to_string();
        if let Some(defs) = self.skin_defs_cache.get(&key) {
            return defs.clone();
        }
        let defs = play_skin_defs_from_path(&key);
        self.skin_defs_cache.insert(key, defs.clone());
        defs
    }

    fn reset_profile_config_from_disk(&mut self) {
        match load_profile_config(&self.boot.profile_paths.profile_toml) {
            Ok(profile) => {
                self.boot.profile_config = profile;
                self.pending_skin_reload_at = None;
                self.apply_profile_skin_offsets_to_active_play();
                self.reload_skins();
                tracing::info!("profile config reset from profile.toml");
            }
            Err(error) => {
                tracing::error!(
                    path = %self.boot.profile_paths.profile_toml.display(),
                    %error,
                    "failed to reset profile config from profile.toml"
                );
            }
        }
    }

    fn apply_profile_skin_offsets_to_active_play(&mut self) {
        let Some(active_play) = &mut self.active_play else {
            return;
        };
        active_play.running.session.skin_offsets = self
            .boot
            .profile_config
            .skin
            .offsets
            .iter()
            .map(|offset| PlaySkinOffset {
                id: offset.id,
                x: offset.x,
                y: offset.y,
                w: offset.w,
                h: offset.h,
                r: offset.r,
                a: offset.a,
            })
            .collect();
    }

    /// 現在の profile config のスキンパスを renderer へ再適用する。
    ///
    /// 起動時と同じ `load_skin_textures` 経路を使い、JSON スキンは
    /// バックグラウンド decode + 段階 install パイプラインへ流す。
    fn reload_skins(&mut self) {
        let skin = self.boot.profile_config.skin.clone();
        self.skin_reload_generation = self.skin_reload_generation.wrapping_add(1);
        let generation = self.skin_reload_generation;
        let (pending_select, pending_decide, pending_result) = reload_skin_textures(
            &mut self.renderer,
            &self.skin_decode_tx,
            generation,
            &skin.select,
            &skin.decide,
            &skin.result,
            &skin.select_options,
            &skin.decide_options,
            &skin.result_options,
            &skin.select_files,
            &skin.decide_files,
            &skin.result_files,
        );
        self.pending_select_skin = pending_select;
        self.pending_decide_skin = pending_decide;
        self.pending_play_skin = false;
        self.pending_result_skin = pending_result;
        self.skin_defs_cache.clear();
        // 旧 generation 分の upload 結果は apply_uploaded_skin の generation
        // チェックで破棄されるため、ここでの明示的なキュー破棄は不要。
        // プレイスキンも egui 設定パネルからライブ反映する。直近 load 済みの
        // key_mode があれば、その mode で強制再 decode を投入する。
        // `spawn_play_skin_decode_for` は signature 一致時に skip するので、
        // 署名を無効化してから呼び、設定変更を確実に反映させる。
        if let Some(key_mode) = self.last_play_skin_signature.as_ref().map(|sig| sig.0) {
            self.last_play_skin_signature = None;
            self.spawn_play_skin_decode_for(key_mode);
        }
        tracing::info!(generation, "skins reload queued from egui skin panel");
    }

    /// 決定対象チャートの key_mode に対応するプレイスキンを background decode に投入する。
    /// 直前と同じ mode かつ path/options/files が同じなら何もしない。
    fn spawn_play_skin_decode_for(&mut self, key_mode: KeyMode) {
        let selection = play_skin_selection_for(&self.boot.profile_config.skin, key_mode);
        let trimmed = selection.path.trim();
        let signature =
            (key_mode, trimmed.to_string(), selection.options.clone(), selection.files.clone());

        if !self.pending_play_skin && self.last_play_skin_signature.as_ref() == Some(&signature) {
            tracing::debug!(?key_mode, "play skin reuse (signature unchanged)");
            return;
        }
        self.last_play_skin_signature = Some(signature);
        self.pending_play_skin = false;

        if trimmed.is_empty() {
            tracing::debug!(?key_mode, "play skin path empty; using default skin only");
            return;
        }
        let path = Path::new(trimmed);
        if !is_decodable_skin_path(path) {
            // TOML directory スキンは同期ロード。デフォルトスキンを下敷きに renderer 直差し替え。
            if let Err(error) = apply_skin_from_config(&mut self.renderer, trimmed) {
                tracing::warn!(
                    ?key_mode,
                    path = trimmed,
                    error = %format_error_chain(&error),
                    "failed to apply play skin from directory; using existing textures"
                );
            }
            return;
        }

        spawn_skin_decode(
            self.skin_decode_tx.clone(),
            self.skin_reload_generation,
            path.to_path_buf(),
            SkinKind::Play,
            selection.options.clone(),
            selection.files.clone(),
        );
        self.pending_play_skin = true;
        tracing::info!(
            ?key_mode,
            path = trimmed,
            generation = self.skin_reload_generation,
            "play skin decode queued"
        );
    }

    /// リザルト遷移後も鳴らし続けている音声出力を監視し、スケジュール済みの
    /// BGM/キー音がすべて鳴り切ったら出力を解放する。
    fn advance_draining_audio(&mut self) {
        let Some(audio) = &self.draining_audio else {
            return;
        };
        let drained = match audio.engine.lock() {
            Ok(engine) => engine.is_idle(),
            // ロック中断時は安全側に倒して出力を解放する。
            Err(_) => true,
        };
        if drained {
            tracing::info!("play audio drained after result; releasing output");
            self.draining_audio = None;
        }
    }

    fn render_current_scene(&mut self) {
        if matches!(self.view_state(), AppViewState::Select) {
            self.sync_select_stage_texture();
            self.sync_select_banner_texture();
            self.sync_select_preview_audio();
        }
        let scene = self.scene_snapshot();
        let scene_kind = scene_kind(&scene);
        self.update_window_title_for_scene(scene_kind);
        match self.renderer.render_scene_status(scene) {
            Ok(RenderSurfaceStatus::Rendered)
            | Ok(RenderSurfaceStatus::SkippedNoSurface)
            | Ok(RenderSurfaceStatus::SkippedZeroSize) => {}
            Ok(RenderSurfaceStatus::Reconfigured) => {
                tracing::debug!("renderer surface reconfigured");
            }
            Ok(RenderSurfaceStatus::TimedOut) => {
                tracing::debug!("renderer surface acquisition timed out");
            }
            Err(error) => {
                tracing::error!(%error, "failed to present render scene");
            }
        }
    }

    fn handle_smoke_exit_after_redraw(&mut self, event_loop: &ActiveEventLoop) {
        if self.smoke_exit_on_result && self.finished_play.is_some() {
            self.smoke_exit_on_result = false;
            tracing::info!("smoke result reached; leaving event loop");
            self.save_current_play_options(None, "game exit");
            event_loop.exit();
            return;
        }

        let Some(exit_after_frames) = self.smoke_exit_after_frames else {
            return;
        };

        self.rendered_frames = self.rendered_frames.saturating_add(1);
        if self.rendered_frames >= exit_after_frames {
            self.smoke_exit_after_frames = None;
            tracing::info!(
                frames = self.rendered_frames,
                "smoke exit frame count reached; leaving event loop"
            );
            self.save_current_play_options(self.active_hispeed(), "game exit");
            event_loop.exit();
        }
    }

    fn active_hispeed(&self) -> Option<f32> {
        self.active_play.as_ref().map(|active| active.running.session.hispeed)
    }

    fn active_lane_state(&self) -> Option<ActiveLaneState> {
        self.active_play.as_ref().map(|active| {
            let session = &active.running.session;
            ActiveLaneState { lane_cover: session.lane_cover, lift: session.lift }
        })
    }

    fn save_current_play_options(&mut self, hispeed: Option<f32>, reason: &'static str) {
        let lane_state = self.active_lane_state();
        apply_current_play_options_to_profile(
            &mut self.boot.profile_config,
            hispeed,
            lane_state,
            self.arrange_option,
            self.target_option,
            self.gauge_option,
            self.assist_option,
            now_unix_seconds(),
        );
        if let Err(error) =
            save_profile_config(&self.boot.profile_paths.profile_toml, &self.boot.profile_config)
        {
            tracing::error!(%error, reason, "failed to save profile play options");
        } else {
            tracing::info!(reason, "saved profile play options");
        }
    }

    fn update_window_title_for_scene(&mut self, scene_kind: AppSceneKind) {
        if self.last_scene_kind == Some(scene_kind) {
            return;
        }

        let previous = self.last_scene_kind;
        self.last_scene_kind = Some(scene_kind);
        if previous == Some(AppSceneKind::Select) && scene_kind != AppSceneKind::Select {
            self.stop_select_preview();
        }
        self.fire_scene_transition_sounds(scene_kind);
        if scene_kind == AppSceneKind::Select {
            let now = Instant::now();
            self.select_scene_started_at = now;
            self.select_bar_started_at = now;
        }
        if scene_kind == AppSceneKind::Result {
            self.result_scene_started_at = Instant::now();
        }
        if let Some(window) = &self.window {
            window.set_title(window_title_for_scene(scene_kind));
        }
        tracing::info!(scene = ?scene_kind, title = window_title_for_scene(scene_kind), "app scene active");
    }

    /// シーン遷移時のシステム SE / BGM を発火する。
    /// 入る前に進行中の BGM をすべて停止してから、新しい BGM / SE を鳴らす。
    fn fire_scene_transition_sounds(&self, scene_kind: AppSceneKind) {
        use crate::system_sound::SoundType;
        self.stop_all_system_bgm();
        match scene_kind {
            AppSceneKind::Select => self.play_system_sound(SoundType::Select),
            AppSceneKind::Decide => self.play_system_sound(SoundType::Decide),
            AppSceneKind::Play => {}
            AppSceneKind::Result => {
                let clear = self
                    .finished_play
                    .as_ref()
                    .map(|finished| finished.summary.clear_type)
                    .unwrap_or(bmz_core::clear::ClearType::Failed);
                let sound = if matches!(clear, bmz_core::clear::ClearType::Failed) {
                    SoundType::ResultFail
                } else {
                    SoundType::ResultClear
                };
                self.play_system_sound(sound);
            }
        }
    }

    /// `profile.[system_sound].volume` を反映してシステム音を鳴らす。
    /// ボリュームは AudioEngine 側で 0.0..=1.0 にクランプされる。
    fn play_system_sound(&self, sound_type: crate::system_sound::SoundType) {
        if let Some(manager) = &self.system_sound {
            manager.play(sound_type, self.boot.profile_config.system_sound.volume);
        }
    }

    /// 当該フレームで踏んだ Mine の数だけ地雷 SE を鳴らす。
    /// 連続ヒットを重ね鳴らししないよう、複数同時ヒットでも1回にまとめる
    /// (`hits == 0` のときは no-op)。
    fn play_landmine_se(&self, hits: usize) {
        if hits == 0 {
            return;
        }
        self.play_system_sound(crate::system_sound::SoundType::Landmine);
    }

    fn stop_system_sound(&self, sound_type: crate::system_sound::SoundType) {
        if let Some(manager) = &self.system_sound {
            manager.stop(sound_type);
        }
    }

    fn stop_all_system_bgm(&self) {
        if let Some(manager) = &self.system_sound {
            manager.stop_all_bgm();
        }
    }
}

fn window_attributes_from_config(
    video: &crate::config::app_config::VideoConfig,
) -> WindowAttributes {
    WindowAttributes::default()
        .with_title("bmz-player")
        .with_inner_size(PhysicalSize::new(video.width.max(1), video.height.max(1)))
}

/// 設定のウィンドウモードに対応する winit の `Fullscreen` を返す。
///
/// 排他フルスクリーンはモニタの video mode が必要で、取得できない場合は
/// ボーダレスへフォールバックする。
fn fullscreen_from_config(mode: &WindowMode, monitor: Option<MonitorHandle>) -> Option<Fullscreen> {
    match mode {
        WindowMode::Windowed => None,
        WindowMode::BorderlessFullscreen => Some(Fullscreen::Borderless(monitor)),
        WindowMode::ExclusiveFullscreen => {
            let monitor = monitor?;
            match pick_exclusive_video_mode(&monitor) {
                Some(video_mode) => Some(Fullscreen::Exclusive(video_mode)),
                None => {
                    tracing::warn!("no exclusive video mode available; using borderless");
                    Some(Fullscreen::Borderless(Some(monitor)))
                }
            }
        }
    }
}

/// 排他フルスクリーン用に、解像度とリフレッシュレートが最大の video mode を選ぶ。
fn pick_exclusive_video_mode(monitor: &MonitorHandle) -> Option<VideoModeHandle> {
    monitor.video_modes().max_by_key(|mode| {
        let size = mode.size();
        (u64::from(size.width) * u64::from(size.height), mode.refresh_rate_millihertz())
    })
}

/// 起動時のスキンロード処理。
///
/// - default skin は必ず一度だけ renderer にアップロードする。
/// - select の JSON skin は同期デコード+install（Select 画面を最短で表示するためクリティカルパス）。
/// - decide / result の JSON skin はバックグラウンドスレッドで Phase A (decode) を実行。
///   完了したものは main thread の `try_recv` で順次 Phase B (install) する。
/// - select/decide/result の各パスが JSON 以外 (空文字または非対応) の場合は警告ログのみ。
/// - プレイスキンは決定画面でチャートの key_mode から個別に decode するためここでは扱わない。
#[allow(clippy::too_many_arguments)]
fn load_initial_skin_textures(
    renderer: &mut Renderer,
    skin_decode_tx: &mpsc::Sender<PendingSkinResult>,
    generation: u64,
    select_skin_path: &str,
    decide_skin_path: &str,
    result_skin_path: &str,
    select_options: &BTreeMap<String, String>,
    decide_options: &BTreeMap<String, String>,
    result_options: &BTreeMap<String, String>,
    select_files: &BTreeMap<String, String>,
    decide_files: &BTreeMap<String, String>,
    result_files: &BTreeMap<String, String>,
) -> (Option<SkinManifest>, bool, bool, bool) {
    // Decide / Result の JSON skin は Select の同期ロードより**前**に decode スレッドを起動して
    // CPU をフル活用する。Select の sync 処理 (PNG GPU upload など) と並列に decode が進む。
    let pending_select = false;
    let mut pending_decide = false;
    let mut pending_result = false;

    let decide_trimmed = decide_skin_path.trim().to_string();
    let result_trimmed = result_skin_path.trim().to_string();

    if !decide_trimmed.is_empty() {
        let decide_path = Path::new(&decide_trimmed);
        if is_decodable_skin_path(decide_path) {
            spawn_skin_decode(
                skin_decode_tx.clone(),
                generation,
                decide_path.to_path_buf(),
                SkinKind::Decide,
                decide_options.clone(),
                decide_files.clone(),
            );
            pending_decide = true;
        }
    }
    if !result_trimmed.is_empty() {
        let result_path = Path::new(&result_trimmed);
        if is_decodable_skin_path(result_path) {
            spawn_skin_decode(
                skin_decode_tx.clone(),
                generation,
                result_path.to_path_buf(),
                SkinKind::Result,
                result_options.clone(),
                result_files.clone(),
            );
            pending_result = true;
        }
    }

    let default_manifest = match load_default_skin_into_renderer(renderer) {
        Ok(manifest) => Some(manifest),
        Err(error) => {
            tracing::warn!(
                error = %format_error_chain(&error),
                "failed to load default skin; using fallback drawing"
            );
            None
        }
    };

    // Select skin (クリティカルパス: 起動直後に表示される)
    let select_trimmed = select_skin_path.trim();
    if !select_trimmed.is_empty() {
        let path = Path::new(select_trimmed);
        if is_decodable_skin_path(path) {
            apply_json_skin_sync(
                renderer,
                path,
                SkinKind::Select,
                default_manifest.as_ref(),
                select_options,
                select_files,
            );
        } else {
            tracing::warn!(
                path = %path.display(),
                "select skin path is not a supported beatoraja skin file; ignoring"
            );
        }
    }

    if !result_trimmed.is_empty() && !is_decodable_skin_path(Path::new(&result_trimmed)) {
        tracing::warn!(
            path = %result_trimmed,
            "result skin path is not a supported beatoraja skin file; ignoring"
        );
    }

    if !decide_trimmed.is_empty() && !is_decodable_skin_path(Path::new(&decide_trimmed)) {
        tracing::warn!(
            path = %decide_trimmed,
            "decide skin path is not a supported beatoraja skin file; ignoring"
        );
    }

    (default_manifest, pending_select, pending_decide, pending_result)
}

#[allow(clippy::too_many_arguments)]
fn reload_skin_textures(
    _renderer: &mut Renderer,
    skin_decode_tx: &mpsc::Sender<PendingSkinResult>,
    generation: u64,
    select_skin_path: &str,
    decide_skin_path: &str,
    result_skin_path: &str,
    select_options: &BTreeMap<String, String>,
    decide_options: &BTreeMap<String, String>,
    result_options: &BTreeMap<String, String>,
    select_files: &BTreeMap<String, String>,
    decide_files: &BTreeMap<String, String>,
    result_files: &BTreeMap<String, String>,
) -> (bool, bool, bool) {
    let mut pending_select = false;
    let mut pending_decide = false;
    let mut pending_result = false;

    for (path_text, kind, options, files) in [
        (select_skin_path, SkinKind::Select, select_options, select_files),
        (decide_skin_path, SkinKind::Decide, decide_options, decide_files),
        (result_skin_path, SkinKind::Result, result_options, result_files),
    ] {
        let trimmed = path_text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = Path::new(trimmed);
        if is_decodable_skin_path(path) {
            spawn_skin_decode(
                skin_decode_tx.clone(),
                generation,
                path.to_path_buf(),
                kind,
                options.clone(),
                files.clone(),
            );
            match kind {
                SkinKind::Select => pending_select = true,
                SkinKind::Decide => pending_decide = true,
                SkinKind::Result => pending_result = true,
                SkinKind::Play => unreachable!("play skin handled via spawn_play_skin_decode_for"),
            }
        } else {
            tracing::warn!(
                path = %path.display(),
                kind = ?kind,
                "skin path is not a supported beatoraja skin file; ignoring"
            );
        }
    }

    (pending_select, pending_decide, pending_result)
}

fn apply_json_skin_sync(
    renderer: &mut Renderer,
    path: &Path,
    kind: SkinKind,
    default_manifest: Option<&SkinManifest>,
    options: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) {
    let Some(manifest) = default_manifest else {
        tracing::warn!(
            path = %path.display(),
            kind = ?kind,
            "skipping skin install because default skin manifest is unavailable"
        );
        return;
    };
    let decoded = match decode_beatoraja_skin_with_options(path, kind, options, files) {
        Ok(decoded) => decoded,
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                kind = ?kind,
                error = %format_error_chain(&error),
                "failed to decode beatoraja skin"
            );
            return;
        }
    };
    if let Err(error) = install_decoded_skin(renderer, decoded, manifest.clone()) {
        tracing::warn!(
            path = %path.display(),
            kind = ?kind,
            error = %format_error_chain(&error),
            "failed to install beatoraja skin"
        );
    }
}

fn spawn_skin_decode(
    tx: mpsc::Sender<PendingSkinResult>,
    generation: u64,
    path: PathBuf,
    kind: SkinKind,
    options: BTreeMap<String, String>,
    files: BTreeMap<String, String>,
) {
    let send_path = path.clone();
    thread::Builder::new()
        .name(format!("skin-decode-{:?}", kind))
        .spawn(move || {
            let result = decode_beatoraja_skin_with_options(&path, kind, &options, &files);
            let _ = tx.send(PendingSkinResult { generation, path: send_path, kind, result });
        })
        .expect("failed to spawn skin decode thread");
}

/// upload worker のループ。decode 結果を受け取り、GPU アップロードして main へ返す。
/// decode 側 (`decode_rx`) が全て drop されるとループを抜ける (アプリ終了時)。
fn skin_upload_worker(
    decode_rx: Receiver<PendingSkinResult>,
    upload_tx: mpsc::Sender<PendingUploadResult>,
    uploader: bmz_render::renderer::GpuUploader,
) {
    while let Ok(PendingSkinResult { generation, path, kind, result }) = decode_rx.recv() {
        let uploaded = result.map(|decoded| upload_decoded_skin(&uploader, decoded));
        if upload_tx.send(PendingUploadResult { generation, path, kind, uploaded }).is_err() {
            // main 側受信端が drop された (アプリ終了)。
            break;
        }
    }
}

fn scan_skin_catalog() -> SkinCatalog {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo_root.join("data/skins");
    let mut catalog = SkinCatalog::default();
    scan_skin_catalog_dir(&repo_root, &root, &mut catalog);
    sort_skin_catalog(&mut catalog);
    catalog
}

fn scan_skin_catalog_dir(repo_root: &Path, dir: &Path, catalog: &mut SkinCatalog) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_skin_catalog_dir(repo_root, &path, catalog);
            continue;
        }
        if !is_skin_candidate_file(&path) {
            continue;
        }
        match load_skin_candidate(repo_root, &path) {
            Some((skin_type, candidate)) => push_skin_candidate(catalog, skin_type, candidate),
            None => {
                tracing::debug!(path = %path.display(), "skipping skin candidate without readable header")
            }
        }
    }
}

fn play_skin_defs_from_path(path: &str) -> SceneSkinDefs {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return SceneSkinDefs::from_play_document(None);
    }
    let document = load_skin_header_document(Path::new(trimmed));
    SceneSkinDefs::from_play_document(document.as_ref())
}

fn is_skin_candidate_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "json" | "luaskin"))
        .unwrap_or(false)
}

fn load_skin_header_document(path: &Path) -> Option<SkinDocument> {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("luaskin"))
    {
        bmz_skin::load_lua_skin_header_value(path)
            .ok()
            .and_then(|loaded| serde_json::from_value::<SkinDocument>(loaded.value).ok())
    } else {
        SkinDocument::load_beatoraja_json(path).ok()
    }
}

fn load_skin_candidate(repo_root: &Path, path: &Path) -> Option<(i32, SkinCandidate)> {
    let document = load_skin_header_document(path)?;
    let relative = path.strip_prefix(repo_root).unwrap_or(path);
    let name = if document.name.trim().is_empty() {
        relative.file_stem().and_then(|name| name.to_str()).unwrap_or("").to_string()
    } else {
        document.name
    };
    Some((
        document.skin_type,
        SkinCandidate { name, path: relative.to_string_lossy().replace('\\', "/") },
    ))
}

fn push_skin_candidate(catalog: &mut SkinCatalog, skin_type: i32, candidate: SkinCandidate) {
    match skin_type {
        0 => catalog.play7.push(candidate),
        1 => catalog.play5.push(candidate),
        2 => catalog.play14.push(candidate),
        3 => catalog.play10.push(candidate),
        5 => catalog.select.push(candidate),
        6 => catalog.decide.push(candidate),
        7 => catalog.result.push(candidate),
        _ => {}
    }
}

fn sort_skin_catalog(catalog: &mut SkinCatalog) {
    for candidates in [
        &mut catalog.select,
        &mut catalog.decide,
        &mut catalog.play5,
        &mut catalog.play7,
        &mut catalog.play10,
        &mut catalog.play14,
        &mut catalog.result,
    ] {
        candidates.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
                .then_with(|| a.path.cmp(&b.path))
        });
        candidates.dedup_by(|a, b| a.path == b.path);
    }
}

fn chart_asset_folder(chart: &PlayableChart) -> Option<PathBuf> {
    chart
        .sounds
        .iter()
        .find_map(|asset| asset.path.parent())
        .or_else(|| chart.bga_assets.iter().find_map(|asset| asset.path.parent()))
        .map(Path::to_path_buf)
}

fn load_chart_meta_texture(
    renderer: &mut Renderer,
    texture_id: TextureId,
    folder_path: &str,
    relative: &str,
) -> bool {
    let Some(path) = crate::chart_asset::resolve_chart_asset_path(folder_path, relative) else {
        return false;
    };
    match load_static_rgba_image(&path) {
        Ok(image) => {
            if let Err(error) = renderer.upsert_image_asset(texture_id, &image) {
                tracing::warn!(path = %path.display(), %error, "failed to upload chart meta image");
                false
            } else {
                true
            }
        }
        Err(error) => {
            tracing::debug!(path = %path.display(), %error, "skipping chart meta image");
            false
        }
    }
}

fn load_chart_bga_textures(renderer: &mut Renderer, chart: &PlayableChart) -> BgaFrameCatalog {
    use bmz_chart::model::BgaAssetKind;

    let mut frames = BgaFrameCatalog::new();
    for asset in &chart.bga_assets {
        let path = &asset.path;
        if asset.kind != BgaAssetKind::Static {
            tracing::debug!(
                asset_id = asset.id.0,
                path = %path.display(),
                "skipping non-static BGA asset (will be decoded at play time)"
            );
            continue;
        }

        match load_static_rgba_image(path) {
            Ok(image) => {
                let texture_id = TextureId(bga_texture_id(asset.id));
                let frame = display_bga_frame(asset.id, image.width, image.height);
                if let Err(error) = renderer.upsert_image_asset(texture_id, &image) {
                    tracing::warn!(
                        asset_id = asset.id.0,
                        texture_id = texture_id.0,
                        path = %path.display(),
                        %error,
                        "failed to upload BGA image"
                    );
                } else {
                    tracing::info!(
                        asset_id = asset.id.0,
                        texture_id = texture_id.0,
                        width = image.width,
                        height = image.height,
                        path = %path.display(),
                        "loaded BGA image"
                    );
                    frames.insert(asset.id, frame);
                }
            }
            Err(error) => {
                tracing::debug!(
                    asset_id = asset.id.0,
                    path = %path.display(),
                    %error,
                    "skipping unreadable BGA image"
                );
            }
        }
    }
    frames
}

fn format_error_chain(error: &anyhow::Error) -> String {
    error.chain().map(ToString::to_string).collect::<Vec<_>>().join(": ")
}

impl ApplicationHandler for WinitApp {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if cause == StartCause::Init {
            tracing::info!("winit app init");
            self.ensure_window(event_loop);
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

        // すべてのウィンドウイベントを egui へ供給する。RedrawRequested など
        // egui が関知しないイベントは egui_winit 側で無視される。
        let egui_consumed = match (self.window.clone(), self.egui.as_mut()) {
            (Some(window), Some(egui)) => egui.on_window_event(&window, &event),
            _ => false,
        };

        match event {
            WindowEvent::CloseRequested => {
                self.save_current_play_options(self.active_hispeed(), "game exit");
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // F1 で egui メニューを開閉する。
                if event.physical_key == PhysicalKey::Code(KeyCode::F1)
                    && event.state == ElementState::Pressed
                    && !event.repeat
                {
                    if let Some(egui) = self.egui.as_mut() {
                        egui.toggle();
                    }
                    return;
                }
                // egui がフォーカスを持つ間はゲーム入力へ伝播させない。
                if egui_consumed {
                    return;
                }
                self.route_keyboard_input(&event);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if egui_consumed {
                    return;
                }
                self.route_mouse_wheel(delta);
            }
            WindowEvent::Resized(size) => {
                self.renderer
                    .resize_surface(SurfaceSize { width: size.width, height: size.height });
            }
            WindowEvent::Focused(focused) => {
                self.focused = focused;
            }
            WindowEvent::RedrawRequested => {
                self.limit_frame_rate();
                self.poll_gamepad_events();
                self.drain_pending_skins();
                self.poll_play_preload();
                self.poll_pending_table_fetch();
                self.advance_decide_transition();
                self.advance_play_ending();
                self.advance_result_exit();
                self.run_egui_frame();
                self.render_current_scene();
                self.advance_active_play();
                self.advance_draining_audio();
                // 次フレームの再描画をここで要求して描画ループを自走させる。
                // about_to_wait から要求すると、Windows のウィンドウ移動/リサイズ中に
                // 発生するモーダルループ (WM_ENTERSIZEMOVE..WM_EXITSIZEMOVE) では
                // about_to_wait が呼ばれず、メインループが停止してしまう。
                // RedrawRequested 内から request_redraw すると実 WM_PAINT が生成され、
                // モーダルループのメッセージ処理でも拾われるためループが止まらない。
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                if self.should_exit_via_select_hold() {
                    tracing::info!("escape held for 2s on select screen; exiting app");
                    self.save_current_play_options(self.active_hispeed(), "select exit hold");
                    event_loop.exit();
                    return;
                }
                self.handle_smoke_exit_after_redraw(event_loop);
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.save_current_play_options(self.active_hispeed(), "game exit");
    }
}

fn surface_size_for_window(window: &Window) -> SurfaceSize {
    let size = window.inner_size();
    SurfaceSize { width: size.width, height: size.height }
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn resolve_boot_chart_id(
    library_db: &crate::storage::library_db::LibraryDatabase,
    options: &AppOptions,
) -> Option<i64> {
    if let Some(path) = options.boot_play_path.as_deref() {
        return lookup_boot_chart_id(library_db, path);
    }
    if options.boot_play_sample {
        return library_db.chart_id_by_title(SAMPLE_PLAYABLE_TITLE).ok().flatten();
    }
    None
}

fn lookup_boot_chart_id(
    library_db: &crate::storage::library_db::LibraryDatabase,
    path: &str,
) -> Option<i64> {
    let path_obj = Path::new(path);
    if !path_obj.is_file() {
        tracing::warn!(path, "boot chart path not found; starting normally");
        return None;
    }
    match library_db.chart_id_by_chart_file_path(path_obj) {
        Ok(Some(chart_id)) => Some(chart_id),
        Ok(None) => {
            tracing::warn!(path, "boot chart path is not in library; starting normally");
            None
        }
        Err(error) => {
            tracing::error!(%error, path, "failed to resolve boot chart path; starting normally");
            None
        }
    }
}

fn log_startup_options(options: &AppOptions) {
    if let Some(path) = &options.boot_play_path {
        tracing::info!(boot_play_path = %path, "boot chart path specified");
    }
    if options.autoplay_on_start {
        tracing::info!(arg = AUTOPLAY_ON_START_ARG, "autoplay enabled for started charts");
    }
    if let Some(frames) = options.smoke_exit_after_frames {
        tracing::info!(arg = SMOKE_EXIT_AFTER_FRAMES_ARG, frames, "smoke auto-exit enabled");
    }
    if options.smoke_exit_on_result {
        tracing::info!(arg = SMOKE_EXIT_ON_RESULT_ARG, "smoke auto-exit on result enabled");
    }
}

fn initial_folder_stack(app_config: &crate::config::app_config::AppConfig) -> Vec<String> {
    let enabled: Vec<String> =
        app_config.songs.roots.iter().filter(|p| p.enabled).map(|p| p.path.clone()).collect();
    if enabled.len() == 1 { enabled } else { Vec::new() }
}

fn enabled_root_paths(app_config: &crate::config::app_config::AppConfig) -> Vec<String> {
    app_config.songs.roots.iter().filter(|p| p.enabled).map(|p| p.path.clone()).collect()
}

fn load_items_for_stack(
    boot: &crate::bootstrap::BootstrappedApp,
    stack: &[String],
) -> Vec<SelectItem> {
    match stack.last() {
        Some(path) if path == COURSE_ROOT_PATH => {
            match load_select_items_for_courses(&boot.library_db) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(%error, "failed to load course list");
                    Vec::new()
                }
            }
        }
        Some(path) if path.starts_with(TABLE_ROOT_PATH) => match parse_table_path(path) {
            Some(TablePath::Root) => match table_folder_items(&boot.library_db) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(%error, "failed to load difficulty table list");
                    Vec::new()
                }
            },
            Some(TablePath::Table { source_url }) => {
                match table_level_folder_items(&boot.library_db, source_url) {
                    Ok(items) => items,
                    Err(error) => {
                        tracing::error!(%error, "failed to load difficulty table levels");
                        Vec::new()
                    }
                }
            }
            Some(TablePath::Level { source_url, level }) => {
                match load_select_items_in_table_level(
                    &boot.library_db,
                    &boot.score_db,
                    source_url,
                    level,
                ) {
                    Ok(items) => items,
                    Err(error) => {
                        tracing::error!(%error, "failed to load difficulty table charts");
                        Vec::new()
                    }
                }
            }
            None => Vec::new(),
        },
        Some(folder) => {
            match load_select_items_in_folder(&boot.library_db, &boot.score_db, folder) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(%error, "failed to load select items");
                    Vec::new()
                }
            }
        }
        None => {
            // ルートには曲フォルダに続けて、コースフォルダ・各難易度表フォルダを並べる。
            // 難易度表由来のコースは各テーブルフォルダ内に表示されるため、
            // 手動インポート分（source が "table:..." でないもの）がある場合のみ COURSE フォルダを表示する。
            let mut items = root_folder_items(&enabled_root_paths(&boot.app_config));
            match boot.library_db.list_courses() {
                Ok(courses)
                    if courses.iter().any(|c| !c.source.starts_with("table:")) =>
                {
                    items.push(course_root_item());
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::error!(%error, "failed to check course list for root");
                }
            }
            match table_folder_items(&boot.library_db) {
                Ok(tables) => items.extend(tables),
                Err(error) => {
                    tracing::error!(%error, "failed to load difficulty table folders");
                }
            }
            items
        }
    }
}

fn cycle_gauge_option(current: GaugeTypeConfig) -> GaugeTypeConfig {
    match current {
        GaugeTypeConfig::AssistEasy => GaugeTypeConfig::Easy,
        GaugeTypeConfig::Easy => GaugeTypeConfig::Normal,
        GaugeTypeConfig::Normal => GaugeTypeConfig::Hard,
        GaugeTypeConfig::Hard => GaugeTypeConfig::ExHard,
        GaugeTypeConfig::ExHard => GaugeTypeConfig::Hazard,
        GaugeTypeConfig::Hazard => GaugeTypeConfig::AssistEasy,
    }
}

fn gauge_option_as_str(gauge: GaugeTypeConfig) -> &'static str {
    match gauge {
        GaugeTypeConfig::AssistEasy => "A-EASY",
        GaugeTypeConfig::Easy => "EASY",
        GaugeTypeConfig::Normal => "NORMAL",
        GaugeTypeConfig::Hard => "HARD",
        GaugeTypeConfig::ExHard => "EX-HARD",
        GaugeTypeConfig::Hazard => "HAZARD",
    }
}

fn bga_mode_as_str(bga: BgaModeConfig) -> &'static str {
    match bga {
        BgaModeConfig::On => "ON",
        BgaModeConfig::Auto => "AUTO",
        BgaModeConfig::Off => "OFF",
    }
}

fn cycle_bga_option(current: BgaModeConfig) -> BgaModeConfig {
    match current {
        BgaModeConfig::On => BgaModeConfig::Auto,
        BgaModeConfig::Auto => BgaModeConfig::Off,
        BgaModeConfig::Off => BgaModeConfig::On,
    }
}

fn select_option_panel_for_holds(start_held: bool, select_held: bool) -> u8 {
    match (start_held, select_held) {
        (true, true) => 3,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 0,
    }
}

fn is_select_start_key(physical_key: PhysicalKey, bindings: &SelectKeyBindings) -> bool {
    physical_key_name(physical_key).is_some_and(|control| bindings.is_start(&control))
}

fn is_select_modifier_key(physical_key: PhysicalKey, bindings: &SelectKeyBindings) -> bool {
    physical_key_name(physical_key).is_some_and(|control| bindings.is_back(&control))
}

fn arrange_option_from_profile(random: RandomOptionConfig) -> ArrangeOption {
    match random {
        RandomOptionConfig::Mirror => ArrangeOption::Mirror,
        RandomOptionConfig::Random | RandomOptionConfig::RRandom | RandomOptionConfig::SRandom => {
            ArrangeOption::Random
        }
        RandomOptionConfig::Off => ArrangeOption::Normal,
    }
}

fn random_config_from_arrange(arrange: ArrangeOption) -> RandomOptionConfig {
    match arrange {
        ArrangeOption::Normal => RandomOptionConfig::Off,
        ArrangeOption::Mirror => RandomOptionConfig::Mirror,
        ArrangeOption::Random => RandomOptionConfig::Random,
    }
}

fn target_option_from_profile(target: TargetOptionConfig) -> TargetOption {
    match target {
        TargetOptionConfig::None => TargetOption::None,
        TargetOptionConfig::Max => TargetOption::Max,
        TargetOptionConfig::Aaa => TargetOption::Aaa,
        TargetOptionConfig::Aa => TargetOption::Aa,
        TargetOptionConfig::A => TargetOption::A,
        TargetOptionConfig::B => TargetOption::B,
        TargetOptionConfig::C => TargetOption::C,
        TargetOptionConfig::D => TargetOption::D,
        TargetOptionConfig::E => TargetOption::E,
    }
}

fn target_config_from_option(target: TargetOption) -> TargetOptionConfig {
    match target {
        TargetOption::None => TargetOptionConfig::None,
        TargetOption::Max => TargetOptionConfig::Max,
        TargetOption::Aaa => TargetOptionConfig::Aaa,
        TargetOption::Aa => TargetOptionConfig::Aa,
        TargetOption::A => TargetOptionConfig::A,
        TargetOption::B => TargetOptionConfig::B,
        TargetOption::C => TargetOptionConfig::C,
        TargetOption::D => TargetOptionConfig::D,
        TargetOption::E => TargetOptionConfig::E,
    }
}

#[derive(Debug, Clone, Copy)]
struct ActiveLaneState {
    lane_cover: f32,
    lift: f32,
}

fn apply_current_play_options_to_profile(
    profile: &mut ProfileConfig,
    hispeed: Option<f32>,
    lane_state: Option<ActiveLaneState>,
    arrange: ArrangeOption,
    target: TargetOption,
    gauge: GaugeTypeConfig,
    assist: AssistOption,
    updated_at: i64,
) {
    if let Some(hispeed) = hispeed {
        profile.lane.hispeed = clamp_hispeed_for_profile(hispeed);
    }
    if let Some(state) = lane_state {
        profile.lane.lane_cover = state.lane_cover.clamp(0.0, 1.0);
        profile.lane.lift = state.lift.clamp(0.0, 1.0);
    }
    profile.play.random = random_config_from_arrange(arrange);
    profile.play.target = target_config_from_option(target);
    profile.play.gauge = gauge;
    profile.play.auto_play = assist == AssistOption::Autoplay;
    profile.play.assist = AssistOptionConfig::None;
    profile.updated_at = updated_at;
}

fn clamp_hispeed_for_profile(hispeed: f32) -> f32 {
    (hispeed * 4.0).round().clamp(2.0, 40.0) / 4.0
}

fn select_snapshot_rows(
    items: &[SelectItem],
    selected_index: usize,
    visible_limit: usize,
) -> Vec<SelectRowSnapshot> {
    if items.is_empty() || visible_limit == 0 {
        return Vec::new();
    }

    let row_count = visible_limit;
    let selected_index = selected_index.min(items.len() - 1);
    let half_window = row_count / 2;
    let start = (selected_index + items.len() - (half_window % items.len())) % items.len();

    (0..row_count)
        .map(|offset| {
            let index = (start + offset) % items.len();
            let item = &items[index];
            match item {
                SelectItem::Folder { name, kind, .. } => SelectRowSnapshot {
                    index: index as u32,
                    title: name.clone(),
                    artist: String::new(),
                    difficulty_name: String::new(),
                    play_level: String::new(),
                    table_level: String::new(),
                    total_notes: 0,
                    initial_bpm: 0.0,
                    min_bpm: 0.0,
                    max_bpm: 0.0,
                    length_ms: 0,
                    clear_type: String::new(),
                    ex_score: None,
                    max_combo: None,
                    gauge_value: None,
                    replay_slots: [false; 4],
                    is_folder: true,
                    kind: *kind,
                    in_library: true,
                },
                SelectItem::Chart(row) => SelectRowSnapshot {
                    index: index as u32,
                    title: row.display_title().to_string(),
                    artist: row.display_artist().to_string(),
                    difficulty_name: row
                        .chart
                        .as_ref()
                        .map(|chart| chart.difficulty_name.clone())
                        .unwrap_or_default(),
                    play_level: row
                        .chart
                        .as_ref()
                        .map(|chart| chart.play_level.clone())
                        .unwrap_or_default(),
                    table_level: row.table_level.clone(),
                    total_notes: row.chart.as_ref().map(|chart| chart.total_notes).unwrap_or(0),
                    initial_bpm: row
                        .chart
                        .as_ref()
                        .map(|chart| chart.initial_bpm as f32)
                        .unwrap_or(0.0),
                    min_bpm: row.chart.as_ref().map(|chart| chart.min_bpm as f32).unwrap_or(0.0),
                    max_bpm: row.chart.as_ref().map(|chart| chart.max_bpm as f32).unwrap_or(0.0),
                    length_ms: row.chart.as_ref().map(|chart| chart.length_ms).unwrap_or(0),
                    clear_type: row
                        .best_score
                        .as_ref()
                        .map(|score| score.clear_type.clone())
                        .unwrap_or_default(),
                    ex_score: row.best_score.as_ref().map(|score| score.ex_score),
                    max_combo: row.best_score.as_ref().map(|score| score.max_combo),
                    gauge_value: row.best_score.as_ref().map(|score| score.gauge_value),
                    replay_slots: row.replay_slots,
                    is_folder: false,
                    kind: bmz_render::scene::SelectRowKind::Song,
                    in_library: row.in_library(),
                },
                SelectItem::Course(row) => SelectRowSnapshot {
                    index: index as u32,
                    title: row.title.clone(),
                    // Use the trophy names joined as "subtitle" so the artist
                    // slot shows e.g. "silvermedal / goldmedal".
                    artist: row.trophy_names.join(" / "),
                    // Beatoraja-style category tag (DAN / COURSE).
                    difficulty_name: row.category_label.clone(),
                    // Show "N stages" in the play_level slot.
                    play_level: format!("{} stages", row.entry_count),
                    table_level: String::new(),
                    total_notes: row.total_notes,
                    initial_bpm: row.min_bpm,
                    min_bpm: row.min_bpm,
                    max_bpm: row.max_bpm,
                    length_ms: row.total_length_ms,
                    clear_type: String::new(),
                    ex_score: None,
                    max_combo: None,
                    gauge_value: None,
                    replay_slots: [false; 4],
                    is_folder: false,
                    kind: bmz_render::scene::SelectRowKind::Course,
                    in_library: row.resolved_count > 0,
                },
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectAction {
    EnterOrPlay,
    ExitFolder,
    Move(SelectMove),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectMove {
    Previous,
    Next,
    PagePrevious,
    PageNext,
    First,
    Last,
}

fn select_action(
    physical_key: PhysicalKey,
    state: ElementState,
    repeat: bool,
    bindings: &SelectKeyBindings,
) -> Option<SelectAction> {
    if state != ElementState::Pressed || repeat {
        return None;
    }

    // Fixed system keys always apply regardless of key config
    match physical_key {
        PhysicalKey::Code(KeyCode::Enter | KeyCode::Space | KeyCode::ArrowRight) => {
            return Some(SelectAction::EnterOrPlay);
        }
        PhysicalKey::Code(KeyCode::ArrowLeft) => return Some(SelectAction::ExitFolder),
        PhysicalKey::Code(KeyCode::ArrowUp) => {
            return Some(SelectAction::Move(SelectMove::Previous));
        }
        PhysicalKey::Code(KeyCode::ArrowDown) => {
            return Some(SelectAction::Move(SelectMove::Next));
        }
        PhysicalKey::Code(KeyCode::PageUp) => {
            return Some(SelectAction::Move(SelectMove::PagePrevious));
        }
        PhysicalKey::Code(KeyCode::PageDown) => {
            return Some(SelectAction::Move(SelectMove::PageNext));
        }
        PhysicalKey::Code(KeyCode::Home) => return Some(SelectAction::Move(SelectMove::First)),
        PhysicalKey::Code(KeyCode::End) => return Some(SelectAction::Move(SelectMove::Last)),
        _ => {}
    }

    // Binding-based lane keys
    let control = physical_key_name(physical_key)?;
    if bindings.is_enter(&control) {
        Some(SelectAction::EnterOrPlay)
    } else if bindings.is_back(&control) {
        Some(SelectAction::ExitFolder)
    } else {
        None
    }
}

fn select_wheel_move(delta: MouseScrollDelta) -> Option<SelectMove> {
    let y = match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(position) => position.y as f32,
    };

    if y > 0.0 {
        Some(SelectMove::Previous)
    } else if y < 0.0 {
        Some(SelectMove::Next)
    } else {
        None
    }
}

fn moved_select_index(current_index: usize, row_count: usize, select_move: SelectMove) -> usize {
    if row_count == 0 {
        return 0;
    }

    match select_move {
        SelectMove::Previous => (current_index + row_count - 1) % row_count,
        SelectMove::Next => (current_index + 1) % row_count,
        SelectMove::PagePrevious => (current_index + row_count - (7 % row_count)) % row_count,
        SelectMove::PageNext => (current_index + 7) % row_count,
        SelectMove::First => 0,
        SelectMove::Last => row_count - 1,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResultAction {
    Retry,
    Leave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecideAction {
    Confirm,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HispeedChange {
    Down,
    Up,
}

fn hispeed_action(
    physical_key: PhysicalKey,
    state: ElementState,
    _repeat: bool,
) -> Option<HispeedChange> {
    if state != ElementState::Pressed {
        return None;
    }

    match physical_key {
        PhysicalKey::Code(KeyCode::ArrowLeft) => Some(HispeedChange::Down),
        PhysicalKey::Code(KeyCode::ArrowRight) => Some(HispeedChange::Up),
        _ => None,
    }
}

fn lane_cover_step(physical_key: PhysicalKey, state: ElementState, _repeat: bool) -> Option<f32> {
    if state != ElementState::Pressed {
        return None;
    }
    match physical_key {
        // 上キー: カバー位置を上げる(下方向への余白を縮める = 値を増やす)
        PhysicalKey::Code(KeyCode::ArrowUp) => Some(LANE_COVER_STEP),
        PhysicalKey::Code(KeyCode::ArrowDown) => Some(-LANE_COVER_STEP),
        _ => None,
    }
}

fn adjusted_hispeed(current: f32, change: HispeedChange) -> f32 {
    let delta = match change {
        HispeedChange::Down => -0.25,
        HispeedChange::Up => 0.25,
    };
    ((current + delta) * 4.0).round().clamp(2.0, 40.0) / 4.0
}

fn result_action(
    physical_key: PhysicalKey,
    state: ElementState,
    repeat: bool,
) -> Option<ResultAction> {
    if state != ElementState::Pressed || repeat {
        return None;
    }

    match physical_key {
        PhysicalKey::Code(KeyCode::KeyR) => Some(ResultAction::Retry),
        PhysicalKey::Code(KeyCode::Enter | KeyCode::Escape) => Some(ResultAction::Leave),
        _ => None,
    }
}

fn decide_action(
    physical_key: PhysicalKey,
    state: ElementState,
    repeat: bool,
) -> Option<DecideAction> {
    if state != ElementState::Pressed || repeat {
        return None;
    }

    match physical_key {
        PhysicalKey::Code(KeyCode::Enter | KeyCode::Space) => Some(DecideAction::Confirm),
        PhysicalKey::Code(KeyCode::Escape) => Some(DecideAction::Cancel),
        _ => None,
    }
}

fn elapsed_since(started_at: Instant) -> TimeUs {
    TimeUs(started_at.elapsed().as_micros().min(i64::MAX as u128) as i64)
}

fn elapsed_since_ms(started_at: Instant) -> i32 {
    (started_at.elapsed().as_millis().min(i32::MAX as u128)) as i32
}

fn skin_duration_ms(ms: i32) -> Duration {
    Duration::from_millis(ms.max(0) as u64)
}

fn scene_kind(scene: &AppSceneSnapshot) -> AppSceneKind {
    match scene {
        AppSceneSnapshot::Select(_) => AppSceneKind::Select,
        AppSceneSnapshot::Decide(_) => AppSceneKind::Decide,
        AppSceneSnapshot::Play(_) => AppSceneKind::Play,
        AppSceneSnapshot::Result(_) => AppSceneKind::Result,
    }
}

fn window_title_for_scene(scene_kind: AppSceneKind) -> &'static str {
    match scene_kind {
        AppSceneKind::Select => "bmz-player - Select",
        AppSceneKind::Decide => "bmz-player - Decide",
        AppSceneKind::Play => "bmz-player - Play",
        AppSceneKind::Result => "bmz-player - Result",
    }
}

fn physical_key_name(physical_key: PhysicalKey) -> Option<String> {
    use bmz_gameplay::input::backend::PhysicalControl;
    match physical_key_to_control(physical_key)? {
        PhysicalControl::KeyboardKey(name) => Some(name),
        _ => None,
    }
}

fn digit_to_replay_slot(physical_key: PhysicalKey) -> Option<u8> {
    match physical_key {
        PhysicalKey::Code(KeyCode::Digit1) => Some(0),
        PhysicalKey::Code(KeyCode::Digit2) => Some(1),
        PhysicalKey::Code(KeyCode::Digit3) => Some(2),
        PhysicalKey::Code(KeyCode::Digit4) => Some(3),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetCycle {
    Previous,
    Next,
}

fn target_cycle_from_key(physical_key: PhysicalKey) -> Option<TargetCycle> {
    match physical_key {
        PhysicalKey::Code(KeyCode::ArrowUp) => Some(TargetCycle::Previous),
        PhysicalKey::Code(KeyCode::ArrowDown) => Some(TargetCycle::Next),
        _ => None,
    }
}

fn target_cycle_from_control(control: &str, bindings: &SelectKeyBindings) -> Option<TargetCycle> {
    match control {
        "ScratchUp" => return Some(TargetCycle::Previous),
        "ScratchDown" => return Some(TargetCycle::Next),
        _ => {}
    }
    if !bindings.scratch_controls.iter().any(|scratch| scratch == control) {
        return None;
    }
    if control.ends_with('-') {
        Some(TargetCycle::Previous)
    } else if control.ends_with('+') {
        Some(TargetCycle::Next)
    } else {
        None
    }
}

struct SelectKeyBindings {
    start: Vec<String>,
    enter: Vec<String>,
    back: Vec<String>,
    scratch_controls: Vec<String>,
    cycle_arrange: Option<String>,
    cycle_gauge: Option<String>,
    cycle_assist: Option<String>,
    cycle_bga: Option<String>,
    key_hint: String,
    option_hint: String,
}

impl SelectKeyBindings {
    fn from_profile(input: &ProfileInputConfig) -> Self {
        let kb: Vec<_> = input.bindings.iter().filter(|e| e.device == "keyboard").collect();
        let all_input: Vec<_> = input
            .bindings
            .iter()
            .filter(|e| e.device == "keyboard" || e.device == "gamepad")
            .collect();

        // キーボード専用（ヒント文字列表示用）
        let kb_keys_for = |lane: LaneConfig| -> Vec<String> {
            kb.iter().filter(|e| e.lane == Some(lane)).map(|e| e.control.clone()).collect()
        };

        // キーボード + ゲームパッド（is_enter / is_back ルックアップ用）
        let keys_for = |lane: LaneConfig| -> Vec<String> {
            all_input.iter().filter(|e| e.lane == Some(lane)).map(|e| e.control.clone()).collect()
        };
        let kb_actions_for = |action: InputActionConfig| -> Vec<String> {
            kb.iter().filter(|e| e.action == Some(action)).map(|e| e.control.clone()).collect()
        };
        let actions_for = |action: InputActionConfig| -> Vec<String> {
            all_input
                .iter()
                .filter(|e| e.action == Some(action))
                .map(|e| e.control.clone())
                .collect()
        };

        let lane_enter: Vec<String> =
            [LaneConfig::Key1, LaneConfig::Key3, LaneConfig::Key5, LaneConfig::Key7]
                .iter()
                .flat_map(|&l| keys_for(l))
                .collect();
        let enter = select_controls_with_lane_fallback(
            actions_for(InputActionConfig::SelectEnter),
            lane_enter,
        );
        let back = select_controls_with_lane_fallback(
            actions_for(InputActionConfig::E2),
            keys_for(LaneConfig::Key2),
        );
        let scratch_controls = keys_for(LaneConfig::Scratch);
        let cycle_arrange = select_control_with_lane_fallback(
            actions_for(InputActionConfig::SelectOptionArrange),
            keys_for(LaneConfig::Key1),
        );
        let cycle_gauge = select_control_with_lane_fallback(
            actions_for(InputActionConfig::SelectOptionGauge),
            keys_for(LaneConfig::Key3),
        );
        let cycle_assist = select_control_with_lane_fallback(
            actions_for(InputActionConfig::SelectOptionAssist),
            keys_for(LaneConfig::Key5),
        );
        let cycle_bga = select_control_with_lane_fallback(
            actions_for(InputActionConfig::SelectOptionBga),
            keys_for(LaneConfig::Key1),
        );
        let mut start = actions_for(InputActionConfig::E1);
        if let Some(legacy_start) = input.start_key.clone()
            && !start.iter().any(|control| control == &legacy_start)
        {
            start.push(legacy_start);
        }
        if start.is_empty() {
            start.push("Q".to_string());
        }

        // ヒント文字列はキーボードバインドのみ使用
        let kb_lane_enter: Vec<String> =
            [LaneConfig::Key1, LaneConfig::Key3, LaneConfig::Key5, LaneConfig::Key7]
                .iter()
                .flat_map(|&l| kb_keys_for(l))
                .collect();
        let kb_enter = select_controls_with_lane_fallback(
            kb_actions_for(InputActionConfig::SelectEnter),
            kb_lane_enter,
        );
        let kb_back = select_controls_with_lane_fallback(
            kb_actions_for(InputActionConfig::E2),
            kb_keys_for(LaneConfig::Key2),
        );
        let enter_str =
            if kb_enter.is_empty() { String::new() } else { format!("/{}", kb_enter.join("/")) };
        let back_str = kb_back.first().map(|k| format!("/{k}")).unwrap_or_default();
        let start_str = kb_actions_for(InputActionConfig::E1)
            .into_iter()
            .next()
            .or_else(|| input.start_key.clone())
            .unwrap_or_else(|| start.first().cloned().unwrap_or_else(|| "Q".to_string()));
        let key_hint =
            format!("UP DOWN  RIGHT{enter_str}:ENTER  LEFT{back_str}:BACK  ENTER {start_str}");

        let kb_arrange_str = select_control_with_lane_fallback(
            kb_actions_for(InputActionConfig::SelectOptionArrange),
            kb_keys_for(LaneConfig::Key1),
        );
        let kb_gauge_str = select_control_with_lane_fallback(
            kb_actions_for(InputActionConfig::SelectOptionGauge),
            kb_keys_for(LaneConfig::Key3),
        );
        let kb_assist_str = select_control_with_lane_fallback(
            kb_actions_for(InputActionConfig::SelectOptionAssist),
            kb_keys_for(LaneConfig::Key5),
        );
        let kb_bga_str = select_control_with_lane_fallback(
            kb_actions_for(InputActionConfig::SelectOptionBga),
            kb_keys_for(LaneConfig::Key1),
        );
        let arrange_str = kb_arrange_str.as_deref().unwrap_or("?");
        let gauge_str = kb_gauge_str.as_deref().unwrap_or("?");
        let assist_str = kb_assist_str.as_deref().unwrap_or("?");
        let bga_str = kb_bga_str.as_deref().unwrap_or("?");
        let option_hint = format!(
            "F1 MENU  F5 RELOAD   \
             {start_str}:PLAY OPT  BACK:ASSIST OPT  {start_str}+BACK:DETAIL OPT  \
             {start_str}+{arrange_str}:ARRANGE  {start_str}+{gauge_str}:GAUGE  {start_str}+{assist_str}:ASSIST  \
             {start_str}+UP/DOWN:TARGET  {start_str}+{bga_str}:BGA  {start_str}+1..4:REPLAY"
        );

        Self {
            start,
            enter,
            back,
            scratch_controls,
            cycle_arrange,
            cycle_gauge,
            cycle_assist,
            cycle_bga,
            key_hint,
            option_hint,
        }
    }

    fn is_enter(&self, control: &str) -> bool {
        self.enter.iter().any(|k| k == control)
    }

    fn is_back(&self, control: &str) -> bool {
        self.back.iter().any(|k| k == control)
    }

    fn is_start(&self, control: &str) -> bool {
        self.start.iter().any(|k| k == control)
    }
}

fn select_controls_with_lane_fallback(
    configured: Vec<String>,
    lane_fallback: Vec<String>,
) -> Vec<String> {
    if configured.is_empty() { lane_fallback } else { configured }
}

fn select_control_with_lane_fallback(
    configured: Vec<String>,
    lane_fallback: Vec<String>,
) -> Option<String> {
    configured.into_iter().next().or_else(|| lane_fallback.into_iter().next())
}

#[cfg(test)]
mod tests {
    use bmz_render::skin::SkinManifest;

    use crate::config::profile_config::ProfileInputConfig;
    use crate::screens::select_model::SelectChartRow;
    use crate::skin_loader::default_skin_root;
    use crate::storage::library_db::ChartListItem;
    use crate::storage::score_db::BestScoreSummary;

    use super::*;

    #[test]
    fn default_skin_note_texture_exists() {
        assert!(default_skin_root().join("note.png").is_file());
        assert!(default_skin_root().join("note-blue.png").is_file());
        assert!(default_skin_root().join("note-red.png").is_file());
        assert!(default_skin_root().join("receptor.png").is_file());
        assert!(default_skin_root().join("receptor-blue.png").is_file());
        assert!(default_skin_root().join("receptor-red.png").is_file());
        assert!(default_skin_root().join("judge-line.png").is_file());
        assert!(default_skin_root().join("gauge-frame.png").is_file());
        assert!(default_skin_root().join("gauge-fill.png").is_file());
        assert!(default_skin_root().join("combo-panel.png").is_file());
        assert!(default_skin_root().join("combo-panel-inactive.png").is_file());
    }

    #[test]
    fn default_skin_manifest_exists() {
        let manifest_path = default_skin_root().join("skin.toml");
        let manifest = SkinManifest::load(&manifest_path).unwrap();

        assert!(
            manifest.textures.iter().any(|texture| texture.id == 1 && texture.path == "note.png")
        );
        assert!(
            manifest
                .textures
                .iter()
                .any(|texture| texture.id == 2 && texture.path == "note-blue.png")
        );
        assert!(
            manifest
                .textures
                .iter()
                .any(|texture| texture.id == 3 && texture.path == "note-red.png")
        );
        assert!(
            manifest
                .textures
                .iter()
                .any(|texture| texture.id == 4 && texture.path == "receptor.png")
        );
        assert!(
            manifest
                .textures
                .iter()
                .any(|texture| texture.id == 5 && texture.path == "receptor-blue.png")
        );
        assert!(
            manifest
                .textures
                .iter()
                .any(|texture| texture.id == 6 && texture.path == "receptor-red.png")
        );
        assert!(
            manifest
                .textures
                .iter()
                .any(|texture| texture.id == 7 && texture.path == "judge-line.png")
        );
        assert!(
            manifest
                .textures
                .iter()
                .any(|texture| texture.id == 8 && texture.path == "gauge-frame.png")
        );
        assert!(
            manifest
                .textures
                .iter()
                .any(|texture| texture.id == 9 && texture.path == "gauge-fill.png")
        );
        assert!(
            manifest
                .textures
                .iter()
                .any(|texture| texture.id == 10 && texture.path == "combo-panel.png")
        );
        assert!(
            manifest
                .textures
                .iter()
                .any(|texture| texture.id == 11 && texture.path == "combo-panel-inactive.png")
        );
    }

    #[test]
    fn skin_catalog_scan_ignores_lua_parts_files() {
        assert!(is_skin_candidate_file(Path::new("data/skins/ECFN/play/play7.luaskin")));
        assert!(is_skin_candidate_file(Path::new("data/skins/ECFN/play/play7-1p.json")));
        assert!(!is_skin_candidate_file(Path::new("data/skins/ECFN/play/play_parts.lua")));
    }

    #[test]
    fn skin_catalog_maps_play_key_modes_by_exact_skin_type() {
        let mut catalog = SkinCatalog::default();
        push_skin_candidate(
            &mut catalog,
            0,
            SkinCandidate {
                name: "Seven".to_string(),
                path: "data/skins/example/play7.luaskin".to_string(),
            },
        );
        push_skin_candidate(
            &mut catalog,
            1,
            SkinCandidate {
                name: "Five".to_string(),
                path: "data/skins/example/play5.luaskin".to_string(),
            },
        );
        push_skin_candidate(
            &mut catalog,
            2,
            SkinCandidate {
                name: "Fourteen".to_string(),
                path: "data/skins/example/play14.luaskin".to_string(),
            },
        );
        push_skin_candidate(
            &mut catalog,
            3,
            SkinCandidate {
                name: "Ten".to_string(),
                path: "data/skins/example/play10.luaskin".to_string(),
            },
        );

        assert_eq!(catalog.play5.len(), 1);
        assert_eq!(catalog.play7.len(), 1);
        assert_eq!(catalog.play10.len(), 1);
        assert_eq!(catalog.play14.len(), 1);
        assert_eq!(catalog.play5[0].path, "data/skins/example/play5.luaskin");
        assert_eq!(catalog.play7[0].path, "data/skins/example/play7.luaskin");
        assert_eq!(catalog.play10[0].path, "data/skins/example/play10.luaskin");
        assert_eq!(catalog.play14[0].path, "data/skins/example/play14.luaskin");
    }

    #[test]
    fn play_skin_defs_load_from_configured_path_without_renderer_install() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = repo.join("data/skins/ECFN/play/play7.luaskin");
        if !path.is_file() {
            return;
        }

        let defs = play_skin_defs_from_path(&path.to_string_lossy());

        assert!(!defs.property.is_empty());
        assert!(!defs.filepath.is_empty());
        assert!(defs.offset.iter().any(|offset| offset.id == 10));
    }

    fn default_select_keys() -> SelectKeyBindings {
        use crate::config::profile_config::default_keyboard_bindings;
        SelectKeyBindings::from_profile(&ProfileInputConfig {
            scratch_mode: crate::config::profile_config::ScratchInputMode::Normal,
            start_key: None,
            bindings: default_keyboard_bindings(),
            analog_scratch_sensitivity: 1.0,
            analog_scratch_timeout_ms: 500,
        })
    }

    #[test]
    fn select_action_maps_start_and_vertical_movement() {
        let keys = default_select_keys();
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::Enter), ElementState::Pressed, false, &keys),
            Some(SelectAction::EnterOrPlay)
        );
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::ArrowUp), ElementState::Pressed, false, &keys),
            Some(SelectAction::Move(SelectMove::Previous))
        );
        assert_eq!(
            select_action(
                PhysicalKey::Code(KeyCode::ArrowDown),
                ElementState::Pressed,
                false,
                &keys
            ),
            Some(SelectAction::Move(SelectMove::Next))
        );
    }

    #[test]
    fn select_action_maps_page_and_edge_movement() {
        let keys = default_select_keys();
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::PageUp), ElementState::Pressed, false, &keys),
            Some(SelectAction::Move(SelectMove::PagePrevious))
        );
        assert_eq!(
            select_action(
                PhysicalKey::Code(KeyCode::PageDown),
                ElementState::Pressed,
                false,
                &keys
            ),
            Some(SelectAction::Move(SelectMove::PageNext))
        );
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::Home), ElementState::Pressed, false, &keys),
            Some(SelectAction::Move(SelectMove::First))
        );
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::End), ElementState::Pressed, false, &keys),
            Some(SelectAction::Move(SelectMove::Last))
        );
    }

    #[test]
    fn select_action_maps_configured_lane_keys() {
        let keys = default_select_keys();
        // Key1(Z), Key3(X), Key5(C), Key7(V) → EnterOrPlay
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::KeyZ), ElementState::Pressed, false, &keys),
            Some(SelectAction::EnterOrPlay)
        );
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::KeyV), ElementState::Pressed, false, &keys),
            Some(SelectAction::EnterOrPlay)
        );
        // Key2(S) → ExitFolder
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::KeyS), ElementState::Pressed, false, &keys),
            Some(SelectAction::ExitFolder)
        );
    }

    #[test]
    fn select_action_rejects_releases_repeats_and_other_keys() {
        let keys = default_select_keys();
        assert_eq!(
            select_action(
                PhysicalKey::Code(KeyCode::ArrowDown),
                ElementState::Released,
                false,
                &keys
            ),
            None
        );
        assert_eq!(
            select_action(
                PhysicalKey::Code(KeyCode::ArrowDown),
                ElementState::Pressed,
                true,
                &keys
            ),
            None
        );
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::KeyA), ElementState::Pressed, false, &keys),
            None
        );
    }

    #[test]
    fn select_wheel_move_maps_vertical_scroll_to_selection_movement() {
        assert_eq!(
            select_wheel_move(MouseScrollDelta::LineDelta(0.0, 1.0)),
            Some(SelectMove::Previous)
        );
        assert_eq!(
            select_wheel_move(MouseScrollDelta::LineDelta(0.0, -1.0)),
            Some(SelectMove::Next)
        );
        assert_eq!(select_wheel_move(MouseScrollDelta::LineDelta(3.0, 0.0)), None);
    }

    #[test]
    fn select_wheel_move_supports_pixel_delta() {
        assert_eq!(
            select_wheel_move(MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(
                0.0, 12.0
            ))),
            Some(SelectMove::Previous)
        );
        assert_eq!(
            select_wheel_move(MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(
                0.0, -12.0
            ))),
            Some(SelectMove::Next)
        );
    }

    #[test]
    fn select_key_bindings_builds_correct_hints() {
        let keys = default_select_keys();
        assert!(keys.key_hint.contains("Z/X/C/V"), "enter keys in hint: {}", keys.key_hint);
        assert!(keys.key_hint.contains("/S:BACK"), "back key in hint: {}", keys.key_hint);
        assert!(keys.key_hint.contains(" Q"), "start key in hint: {}", keys.key_hint);
        assert!(keys.option_hint.contains("F1 MENU"), "menu in hint: {}", keys.option_hint);
        assert!(keys.option_hint.contains("F5 RELOAD"), "reload in hint: {}", keys.option_hint);
        assert!(keys.option_hint.contains("Q+Z:ARRANGE"), "arrange in hint: {}", keys.option_hint);
        assert!(
            keys.option_hint.contains("Q+UP/DOWN:TARGET"),
            "target in hint: {}",
            keys.option_hint
        );
    }

    #[test]
    fn select_option_panel_maps_start_and_select_holds() {
        assert_eq!(select_option_panel_for_holds(false, false), 0);
        assert_eq!(select_option_panel_for_holds(true, false), 1);
        assert_eq!(select_option_panel_for_holds(false, true), 2);
        assert_eq!(select_option_panel_for_holds(true, true), 3);
    }

    #[test]
    fn target_cycle_maps_start_arrow_and_scratch_controls() {
        let keys = default_select_keys();
        let gamepad_keys = SelectKeyBindings::from_profile(
            &ProfileConfig::new_default("default", "Default", 1).input,
        );

        assert_eq!(
            target_cycle_from_key(PhysicalKey::Code(KeyCode::ArrowUp)),
            Some(TargetCycle::Previous)
        );
        assert_eq!(
            target_cycle_from_key(PhysicalKey::Code(KeyCode::ArrowDown)),
            Some(TargetCycle::Next)
        );
        assert_eq!(target_cycle_from_control("ScratchUp", &keys), Some(TargetCycle::Previous));
        assert_eq!(target_cycle_from_control("ScratchDown", &keys), Some(TargetCycle::Next));
        assert_eq!(
            target_cycle_from_control("AxisLeftX-", &gamepad_keys),
            Some(TargetCycle::Previous)
        );
        assert_eq!(target_cycle_from_control("AxisLeftX+", &gamepad_keys), Some(TargetCycle::Next));
    }

    #[test]
    fn select_modifier_keys_are_handled_before_folder_back() {
        let keys = default_select_keys();
        assert!(!is_select_modifier_key(PhysicalKey::Code(KeyCode::ArrowLeft), &keys));
        assert!(is_select_modifier_key(PhysicalKey::Code(KeyCode::KeyS), &keys));
        assert_eq!(
            select_action(
                PhysicalKey::Code(KeyCode::ArrowLeft),
                ElementState::Pressed,
                false,
                &keys
            ),
            Some(SelectAction::ExitFolder)
        );
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::KeyS), ElementState::Pressed, false, &keys),
            Some(SelectAction::ExitFolder)
        );
    }

    #[test]
    fn select_start_key_uses_profile_start_binding() {
        let keys = default_select_keys();
        assert!(is_select_start_key(PhysicalKey::Code(KeyCode::KeyQ), &keys));
        assert!(!is_select_start_key(PhysicalKey::Code(KeyCode::KeyS), &keys));
    }

    #[test]
    fn select_key_bindings_map_e1_plus_key5_to_assist_option() {
        let keys = default_select_keys();

        assert!(keys.is_start("Q"));
        assert_eq!(keys.cycle_assist.as_deref(), Some("C"));
        assert!(keys.is_enter("C"));
    }

    #[test]
    fn bga_option_cycles_on_auto_off() {
        assert!(matches!(cycle_bga_option(BgaModeConfig::On), BgaModeConfig::Auto));
        assert!(matches!(cycle_bga_option(BgaModeConfig::Auto), BgaModeConfig::Off));
        assert!(matches!(cycle_bga_option(BgaModeConfig::Off), BgaModeConfig::On));
    }

    #[test]
    fn result_action_accepts_retry_and_leave_keys() {
        assert_eq!(
            result_action(PhysicalKey::Code(KeyCode::KeyR), ElementState::Pressed, false),
            Some(ResultAction::Retry)
        );
        assert_eq!(
            result_action(PhysicalKey::Code(KeyCode::Enter), ElementState::Pressed, false),
            Some(ResultAction::Leave)
        );
        assert_eq!(
            result_action(PhysicalKey::Code(KeyCode::Escape), ElementState::Pressed, false),
            Some(ResultAction::Leave)
        );
    }

    #[test]
    fn result_action_rejects_releases_repeats_and_other_keys() {
        assert_eq!(
            result_action(PhysicalKey::Code(KeyCode::KeyR), ElementState::Released, false),
            None
        );
        assert_eq!(
            result_action(PhysicalKey::Code(KeyCode::Escape), ElementState::Pressed, true),
            None
        );
        assert_eq!(
            result_action(PhysicalKey::Code(KeyCode::Space), ElementState::Pressed, false),
            None
        );
    }

    #[test]
    fn hispeed_action_maps_left_and_right_presses() {
        assert_eq!(
            hispeed_action(PhysicalKey::Code(KeyCode::ArrowLeft), ElementState::Pressed, false),
            Some(HispeedChange::Down)
        );
        assert_eq!(
            hispeed_action(PhysicalKey::Code(KeyCode::ArrowRight), ElementState::Pressed, false),
            Some(HispeedChange::Up)
        );
    }

    #[test]
    fn hispeed_action_rejects_releases_and_other_keys() {
        assert_eq!(
            hispeed_action(PhysicalKey::Code(KeyCode::ArrowLeft), ElementState::Released, false),
            None
        );
        assert_eq!(
            hispeed_action(PhysicalKey::Code(KeyCode::ArrowUp), ElementState::Pressed, false),
            None
        );
    }

    #[test]
    fn adjusted_hispeed_steps_by_quarter_and_clamps_range() {
        assert_eq!(adjusted_hispeed(2.0, HispeedChange::Up), 2.25);
        assert_eq!(adjusted_hispeed(2.0, HispeedChange::Down), 1.75);
        assert_eq!(adjusted_hispeed(10.0, HispeedChange::Up), 10.0);
        assert_eq!(adjusted_hispeed(0.5, HispeedChange::Down), 0.5);
    }

    #[test]
    fn apply_current_play_options_updates_profile_defaults() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);

        apply_current_play_options_to_profile(
            &mut profile,
            Some(3.37),
            Some(ActiveLaneState { lane_cover: 0.42, lift: 0.1 }),
            ArrangeOption::Mirror,
            TargetOption::Aaa,
            GaugeTypeConfig::Hard,
            AssistOption::Autoplay,
            42,
        );

        assert_eq!(profile.lane.hispeed, 3.25);
        assert!((profile.lane.lane_cover - 0.42).abs() < 1e-6);
        assert!((profile.lane.lift - 0.1).abs() < 1e-6);
        assert!(matches!(profile.play.random, RandomOptionConfig::Mirror));
        assert!(matches!(profile.play.target, TargetOptionConfig::Aaa));
        assert!(matches!(profile.play.gauge, GaugeTypeConfig::Hard));
        assert!(profile.play.auto_play);
        assert!(matches!(profile.play.assist, AssistOptionConfig::None));
        assert_eq!(profile.updated_at, 42);
    }

    #[test]
    fn arrange_option_maps_profile_random_defaults() {
        assert_eq!(arrange_option_from_profile(RandomOptionConfig::Off), ArrangeOption::Normal);
        assert_eq!(arrange_option_from_profile(RandomOptionConfig::Mirror), ArrangeOption::Mirror);
        assert_eq!(arrange_option_from_profile(RandomOptionConfig::Random), ArrangeOption::Random);
        assert!(matches!(
            random_config_from_arrange(ArrangeOption::Normal),
            RandomOptionConfig::Off
        ));
        assert!(matches!(
            random_config_from_arrange(ArrangeOption::Mirror),
            RandomOptionConfig::Mirror
        ));
        assert!(matches!(
            random_config_from_arrange(ArrangeOption::Random),
            RandomOptionConfig::Random
        ));
    }

    #[test]
    fn window_title_uses_scene_name() {
        assert_eq!(window_title_for_scene(AppSceneKind::Select), "bmz-player - Select");
        assert_eq!(window_title_for_scene(AppSceneKind::Play), "bmz-player - Play");
        assert_eq!(window_title_for_scene(AppSceneKind::Result), "bmz-player - Result");
    }

    #[test]
    fn window_attributes_use_configured_video_size() {
        let mut config = crate::config::app_config::AppConfig::default().video;
        config.width = 1920;
        config.height = 1080;

        let attributes = window_attributes_from_config(&config);

        assert_eq!(attributes.inner_size, Some(PhysicalSize::new(1920, 1080).into()));
    }

    #[test]
    fn select_snapshot_rows_centers_selection_and_copies_score_summary() {
        let rows: Vec<SelectItem> = (0..10)
            .map(|index| {
                let mut row = select_chart_row(index);
                if index == 5 {
                    row.best_score = Some(best_score_with_replay(1234, "replay/test.toml"));
                    row.replay_slots = [true, false, false, false];
                }
                SelectItem::Chart(row)
            })
            .collect();

        let snapshot_rows = select_snapshot_rows(&rows, 5, 7);

        assert_eq!(snapshot_rows.len(), 7);
        assert_eq!(snapshot_rows[0].index, 2);
        assert_eq!(snapshot_rows[3].index, 5);
        assert_eq!(snapshot_rows[3].title, "Title 5");
        assert_eq!(snapshot_rows[3].clear_type, "Normal");
        assert_eq!(snapshot_rows[3].ex_score, Some(1234));
        assert_eq!(snapshot_rows[3].replay_slots, [true, false, false, false]);
    }

    #[test]
    fn select_snapshot_rows_wraps_near_edges() {
        let rows: Vec<SelectItem> =
            (0..4).map(|i| SelectItem::Chart(select_chart_row(i))).collect();

        let snapshot_rows = select_snapshot_rows(&rows, 0, 7);

        assert_eq!(snapshot_rows.len(), 7);
        assert_eq!(
            snapshot_rows.iter().map(|row| row.index).collect::<Vec<_>>(),
            vec![1, 2, 3, 0, 1, 2, 3]
        );
    }

    #[test]
    fn select_snapshot_rows_keeps_twelve_rows_around_selection() {
        let rows: Vec<SelectItem> =
            (0..30).map(|i| SelectItem::Chart(select_chart_row(i))).collect();

        let snapshot_rows = select_snapshot_rows(&rows, 2, 25);

        assert_eq!(snapshot_rows.len(), 25);
        assert_eq!(snapshot_rows[0].index, 20);
        assert_eq!(snapshot_rows[12].index, 2);
        assert_eq!(snapshot_rows[24].index, 14);
    }

    #[test]
    fn moved_select_index_moves_by_single_page_and_wraps_edges() {
        assert_eq!(moved_select_index(4, 10, SelectMove::Previous), 3);
        assert_eq!(moved_select_index(4, 10, SelectMove::Next), 5);
        assert_eq!(moved_select_index(9, 10, SelectMove::Next), 0);
        assert_eq!(moved_select_index(0, 10, SelectMove::Previous), 9);
        assert_eq!(moved_select_index(8, 10, SelectMove::PagePrevious), 1);
        assert_eq!(moved_select_index(4, 10, SelectMove::PagePrevious), 7);
        assert_eq!(moved_select_index(7, 10, SelectMove::PageNext), 4);
        assert_eq!(moved_select_index(0, 10, SelectMove::Last), 9);
        assert_eq!(moved_select_index(9, 10, SelectMove::First), 0);
    }

    #[test]
    fn moved_select_index_handles_empty_rows() {
        assert_eq!(moved_select_index(9, 0, SelectMove::Last), 0);
    }

    fn select_chart_row(index: usize) -> SelectChartRow {
        SelectChartRow {
            chart: Some(ChartListItem {
                chart_id: index as i64,
                md5: [0u8; 16],
                sha256: [index as u8; 32],
                title: format!("Title {index}"),
                subtitle: String::new(),
                artist: format!("Artist {index}"),
                difficulty_name: String::new(),
                play_level: index.to_string(),
                mode: "7K".to_string(),
                total_notes: 100,
                initial_bpm: 128.0,
                min_bpm: 128.0,
                max_bpm: 128.0,
                length_ms: 90_000,
                folder_path: String::new(),
                stage_file: String::new(),
                banner_file: String::new(),
                backbmp_file: String::new(),
                preview_file: String::new(),
            }),
            fallback_title: String::new(),
            fallback_artist: String::new(),
            entry_sha256: None,
            best_score: None,
            replay_slots: [false; 4],
            table_level: String::new(),
        }
    }

    fn best_score_with_replay(ex_score: u32, replay_path: &str) -> BestScoreSummary {
        BestScoreSummary {
            chart_sha256: [0; 32],
            clear_type: "Normal".to_string(),
            gauge_type: "Normal".to_string(),
            gauge_value: 80.0,
            ex_score,
            max_combo: 100,
            played_at: 1,
            replay_path: replay_path.to_string(),
        }
    }
}
