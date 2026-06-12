use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use bmz_audio::ffmpeg_loader::FfmpegSampleLoader;
use bmz_audio::loader::SampleLoader;
use bmz_audio::sample::DecodedSample;
use bmz_chart::model::PlayableChart;
use bmz_core::clear::{ClearType, GaugeType};
use bmz_core::input::InputDeviceKind;
use bmz_core::lane::{KeyMode, Lane};
use bmz_core::time::TimeUs;
use bmz_gameplay::input::backend::{DeviceId, PhysicalControl};
use bmz_gameplay::result::PlayResult;
use bmz_gameplay::score::{JudgeCounts, ScoreState};
use bmz_gameplay::session::compute_frame_times;
use bmz_gameplay::session::{HispeedMode, PlaySkinOffset};
use bmz_render::assets::{RgbaImageAsset, load_static_rgba_image};
use bmz_render::plan::{
    PLAY_BACKBMP_TEXTURE, Rect, SELECT_BANNER_TEXTURE, SELECT_STAGE_TEXTURE, TextureId,
};
use bmz_render::renderer::{RenderFrameTimings, RenderSurfaceStatus, Renderer, SurfaceSize};
use bmz_render::scene::{
    AppSceneSnapshot, ResultSnapshot, SelectChartDistributionSecond, SelectRowSnapshot,
    SelectSnapshot,
};
use bmz_render::skin::{SkinImageSize, SkinTextureId};
use bmz_render::snapshot::{
    CourseStageMarker, DisplayJudgeCounts, FastSlowJudgeCounts, OverlaySnapshot, RenderSnapshot,
};
use bmz_video::VideoBgaDecoder;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::monitor::{MonitorHandle, VideoModeHandle};
use winit::window::{Fullscreen, Window, WindowAttributes, WindowId};

use crate::audio::{AppAudioOutput, AudioRuntime};
use crate::bootstrap::{self, BootstrappedApp};
use crate::chart_preview::SelectChartPreview;
use crate::cli::{
    AUTOPLAY_ON_START_ARG, AppOptions, BOOT_RESULT_SAMPLE_ARG, SMOKE_EXIT_AFTER_FRAMES_ARG,
    SMOKE_EXIT_AFTER_RESULT_FRAMES_ARG, SMOKE_EXIT_ON_RESULT_ARG, SMOKE_SCREENSHOT_ARG,
};
use crate::config::app_config::{AppConfig, PathEntry, WindowMode};
use crate::config::key_config::{
    KeyBindingSlot, KeyBindingTarget, apply_play_binding, clear_play_binding,
    is_scratch_down_control, is_scratch_up_control,
};
use crate::config::load::load_profile_config;
use crate::config::profile_config::{
    AssistOptionConfig, BgaExpandConfig, BgaModeConfig, BottomShiftableGaugeConfig,
    GaugeAutoShiftConfig, GaugeTypeConfig, HispeedModeConfig, InputActionConfig, LaneConfig,
    ProfileConfig, ProfileInputConfig, RandomOptionConfig, ScratchDirectionConfig,
    TargetOptionConfig,
};
use crate::config::save::{save_app_config, save_profile_config};
use crate::config::settings_registry::SettingsEntryId;
use crate::input::winit::physical_key_to_control;
use crate::practice_ui::PracticePanelContext;
use crate::screens::course_session::{ActiveCourseSession, CourseEntryResult, CourseResultSummary};
use crate::screens::key_config_edit::KeyConfigEditSession;
use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::play_loop::{
    PlayAdvanceOutcome, PlayEndingSkinTimers, advance_running_play_session_until_result,
    refresh_play_ending_snapshot,
};
use crate::screens::play_session::AppliedArrange;
use crate::screens::play_session::build_practice_prepared_from_preloaded;
use crate::screens::play_snapshot::{
    BgaFrameCatalog, apply_fast_slow_display_filter, bga_texture_id,
    build_render_snapshot_with_target_and_bga_frames, display_bga_frame,
};
use crate::screens::play_start::{
    PlayStartOptions, PreloadedWinitPlaySession, PreparedWinitPlaySession, StartedWinitPlaySession,
    apply_arrange_override, apply_course_constraints, apply_queued_replay,
    open_prepared_winit_play_session, play_session_options_from_start,
    prepare_play_session_for_chart_with_winit_input, prepare_winit_play_session_from_preloaded,
};
use crate::screens::practice::{
    PracticeCliOverrides, PracticePhase, PracticeSession, clamp_practice_property,
    load_practice_property, practice_chart_zero_time, save_practice_property,
};
use crate::screens::result_model::{ResultFastSlowJudgeCounts, ResultSummary};
use crate::screens::select_model::{
    COURSE_ROOT_PATH, MAX_SEARCH_HISTORY, SEARCH_PATH_PREFIX, SelectFolderSummary, SelectItem,
    TABLE_ROOT_PATH, TablePath, course_root_item, load_select_items_for_courses,
    load_select_items_for_search, load_select_items_in_folder, load_select_items_in_table_level,
    parse_search_query, parse_table_path, root_folder_items, search_history_folder_items,
    select_folder_summary, song_scan_path_from_context, table_folder_items,
    table_level_folder_items, table_source_url_from_context,
};
use crate::screens::settings_edit::{SettingsBindings, SettingsEditSession, adjust_settings_draft};
use crate::screens::settings_model::{
    in_settings_stack, load_settings_items, settings_breadcrumb, settings_root_item,
};
use crate::select_options::{ArrangeOption, AssistOption, TargetOption};
use crate::skin_loader::{
    DecodedSkin, PreparedSource, SkinKind, UploadedSkin, apply_skin_from_config,
    decode_beatoraja_skin_with_options, install_decoded_font, install_decoded_skin,
    is_decodable_skin_path, load_default_skin_into_renderer, play_skin_selection_for,
    set_decoded_skin_context, upload_decoded_skin,
};
use crate::songs_cmd::scan_songs_with_progress;
use crate::storage::library_db::{ChartDistributionSecond, LibraryDatabase};
use crate::storage::migration::{migrate_library_db, migrate_score_db};
use crate::storage::play_result::StoredPlayResult;
use crate::storage::replay::load_replay_for_chart_and_policy;
use crate::storage::scan::{ScanProgress, ScanReport};
use crate::storage::score_db::ScoreDatabase;
use crate::storage::score_import::{ScoreImportRequest, import_scores};
use crate::ui::{
    DebugInfo, EguiLayer, EguiRunContext, SceneSkinDefs, SkinCandidate, SkinCatalog, SkinConfigMeta,
};
use bmz_render::skin::{
    DestinationListEntry, SkinAnimationDef, SkinClickHit, SkinClickTarget, SkinDestinationDef,
    SkinDocument, SkinDocumentTexture, SkinDstEntry, SkinManifest, SkinSliderHit,
};
const SAMPLE_PLAYABLE_TITLE: &str = "BMZ Sample Playable";

pub async fn run() -> Result<()> {
    run_with_options(AppOptions::default()).await
}

pub async fn run_with_options(options: AppOptions) -> Result<()> {
    let mut boot = bootstrap::bootstrap()?;

    if boot.app_config.tables.auto_fetch_on_startup {
        fetch_configured_difficulty_tables(&mut boot).await;
    }

    let event_loop = EventLoop::new().context("failed to create event loop")?;
    event_loop.set_control_flow(ControlFlow::Poll);

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

    spawn_ir_sync_worker(&boot);

    let mut app = WinitApp::new(boot, options, None, None, shutdown_requested)?;
    tracing::info!("starting winit event loop");
    event_loop.run_app(&mut app).context("winit event loop failed")
}

/// IR スコアジョブをバックグラウンドで定期送信する。
///
/// メインスレッドの `ScoreDatabase` とは別 connection を開く (score.db は WAL)。
/// IR が未設定なら何もしない。
fn spawn_ir_sync_worker(boot: &bootstrap::BootstrappedApp) {
    let ir_config = boot.profile_config.ir.clone();
    if !ir_config.providers.iter().any(|provider| provider.enabled && !provider.base_url.is_empty())
    {
        return;
    }
    let profile_root = boot.profile_paths.root_dir.clone();
    let score_db_path = boot.profile_paths.score_db.clone();
    tokio::spawn(async move {
        loop {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            match crate::storage::score_db::ScoreDatabase::open(&score_db_path) {
                Ok(mut score_db) => {
                    match crate::ir::sync::sync_pending_ir_jobs(
                        &mut score_db,
                        &profile_root,
                        &ir_config,
                        now,
                        20,
                    )
                    .await
                    {
                        Ok(report) if report.submitted > 0 || report.failed > 0 => {
                            tracing::info!(
                                submitted = report.submitted,
                                failed = report.failed,
                                "IR score sync finished"
                            );
                        }
                        Ok(_) => {}
                        Err(error) => tracing::warn!(%error, "IR score sync failed"),
                    }
                }
                Err(error) => tracing::warn!(%error, "failed to open score db for IR sync"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });
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
    audio_runtime: Option<AudioRuntime>,
    audio_output_open_attempted: bool,
    first_frame_startup_completed: bool,
    /// Ctrl-C(SIGINT)受信フラグ。セットされたら `about_to_wait` で event loop を
    /// 正常終了させ、cpal/ASIO ストリームの Drop(停止・後処理)を確実に走らせる。
    shutdown_requested: Arc<AtomicBool>,
    finished_play: Option<FinishedPlaySession>,
    /// リザルト画面の IR 送信・ランキング表示状態。リザルト以外では None。
    result_ir: Option<crate::screens::result_ir::ResultIrState>,
    /// 選曲カーソル譜面の IR ランキングキャッシュ。
    select_ir: crate::screens::select_ir::SelectIrRanking,
    /// 直近のプレイがオートプレイだったか。Result 画面の常時表示に使う。
    last_play_was_autoplay: bool,
    last_play_snapshot: Option<RenderSnapshot>,
    pending_decide: Option<DecideTransition>,
    pending_play_start: Option<PendingPlayStart>,
    pending_play_preload: Option<PendingPlayPreload>,
    /// Decide 演出中に preload worker から受け取った結果を退避し、
    /// `start_chart_with_options` で再利用するためのバッファ。
    /// 既に裏で完了している譜面/音源ロードを main で再度同期実行するのを避ける。
    preloaded_play_session: Option<PreloadedWinitPlaySession>,
    play_preload_generation: u64,
    play_ending: Option<PlayEndingTransition>,
    last_started_chart_id: Option<i64>,
    /// プレイ開始時点の難易度表テキスト (beatoraja TEXT_TABLE1..3)。
    play_table_text_primary: String,
    play_table_text_secondary: String,
    play_table_text_fallback: String,
    select_items: Vec<SelectItem>,
    select_distribution_cache: RefCell<HashMap<i64, Vec<ChartDistributionSecond>>>,
    table_breadcrumb_cache: RefCell<HashMap<String, TableBreadcrumb>>,
    select_folder_summary_cache: HashMap<String, SelectFolderSummaryCacheEntry>,
    select_folder_summary_tx: mpsc::Sender<SelectFolderSummaryResult>,
    select_folder_summary_rx: Receiver<SelectFolderSummaryResult>,
    folder_stack: Vec<String>,
    /// `folder_stack` の各階層に入る直前の `selected_index`。
    /// フォルダから出た時にカーソル位置を復元するために使う。長さは `folder_stack` と一致。
    selected_index_stack: Vec<usize>,
    selected_index: usize,
    renderer: Renderer,
    skin_catalog: SkinCatalog,
    skin_defs_cache: BTreeMap<String, SceneSkinDefs>,
    pending_table_fetch: Option<Receiver<Result<()>>>,
    pending_song_scan: Option<Receiver<SongScanEvent>>,
    song_scan_progress: Option<ScanProgress>,
    last_scene_kind: Option<AppSceneKind>,
    start_held: bool,
    select_held: bool,
    arrange_option: ArrangeOption,
    target_option: TargetOption,
    gauge_option: GaugeTypeConfig,
    gauge_auto_shift_option: GaugeAutoShiftConfig,
    bottom_shiftable_gauge_option: BottomShiftableGaugeConfig,
    assist_option: AssistOption,
    select_mode_filter: SelectModeFilter,
    select_sort: SelectSort,
    select_keys: SelectKeyBindings,
    select_bar_scroll_direction: i32,
    select_bar_scroll_duration: Duration,
    select_hold_move: Option<SelectMove>,
    select_hold_started_at: Option<Instant>,
    select_hold_last_trigger_at: Option<Instant>,
    select_hold_control: Option<String>,
    select_analog_scroll_buffer: i32,
    select_analog_last_tick_at: Option<Instant>,
    /// キーコンフィグ確定/キャンセル直後、スクラッチが止まるまでアナログスクロールを抑止する。
    select_analog_suppress_until_idle: bool,
    smoke_exit_after_frames: Option<u32>,
    smoke_exit_after_result_frames: Option<u32>,
    smoke_exit_on_result: bool,
    smoke_screenshot_path: Option<PathBuf>,
    rendered_frames: u32,
    rendered_result_frames: u32,
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
    skin_video_sources: HashMap<SkinKind, Vec<ActiveSkinVideoSource>>,
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
    select_stage_size: Option<SkinImageSize>,
    /// 選曲 skin 用 `#BACKBMP` のロード済みキャッシュキー (`folder|file`)。
    select_backbmp_source: Option<String>,
    select_backbmp_loaded: bool,
    select_backbmp_size: Option<SkinImageSize>,
    /// 選曲 `#BANNER` のロード済みキャッシュキー (`folder|file`)。
    select_banner_source: Option<String>,
    select_banner_loaded: bool,
    select_banner_size: Option<SkinImageSize>,
    /// 選曲 `#PREVIEW` のロード済みキャッシュキー (`folder|file`)。
    select_preview_source: Option<String>,
    select_preview_playing: bool,
    select_preview_fade: SelectPreviewFade,
    select_preview: Option<SelectChartPreview>,
    select_meta_image_cache: HashMap<String, SelectMetaImageCacheEntry>,
    select_meta_image_tx: mpsc::Sender<SelectMetaImageResult>,
    select_meta_image_rx: Receiver<SelectMetaImageResult>,
    select_preview_cache: HashMap<String, SelectPreviewCacheEntry>,
    select_preview_tx: mpsc::Sender<SelectPreviewResult>,
    select_preview_rx: Receiver<SelectPreviewResult>,
    /// プレイ `#BACKBMP` のロード済みキャッシュキー。
    play_backbmp_source: Option<String>,
    play_backbmp_loaded: bool,
    /// プレイ中の Start キー直近の押下時刻。連続押し判定で使用。
    last_play_start_press_at: Option<Instant>,
    /// Decide 中の E1 押下状態。E1+E2 長押しキャンセルに使う。
    decide_e1_held: bool,
    /// プレイ中の E2 押下状態。E2+E3 即終了 / E1+E2 長押し終了に使う。
    play_e2_held: bool,
    /// プレイ中の E3 押下状態。E2+E3 即終了に使う。
    play_e3_held: bool,
    /// E1+E2 が押され続けている開始時刻。beatoraja 既定 1000ms で途中終了。
    play_exit_hold_started_at: Option<Instant>,
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
    /// 設定画面で編集中の項目。`None` なら一覧操作モード。
    settings_edit: Option<SettingsEditSession>,
    /// キー設定の待ち受け状態。
    key_config_edit: Option<KeyConfigEditSession>,
    /// リザルト画面終了アニメーションの進行状態。
    /// Some のあいだは終了フェードアウト中で、入力は受け付けない。
    result_exit: Option<ResultExit>,
    /// リザルト画面で Key5 が現在押されているか。
    /// 終了アニメーション終了時に retry arrange を決める判定に使う。
    result_key5_held: bool,
    /// リザルト画面で Key7 が現在押されているか。
    result_key7_held: bool,
    result_gauge_graph_type: i32,
    deferred_boot: Option<DeferredBoot>,
    /// 選曲画面で楽曲検索の入力モード中か。
    search_mode: bool,
    /// 現在入力中の検索クエリ。検索モード中はそのまま skin の search_word に渡る。
    search_query: String,
    /// 直近のマウスカーソル位置。select skin のクリック hit-test に使う。
    last_cursor_position: Option<PhysicalPosition<f64>>,
    /// ドラッグ中の select skin slider type。
    select_slider_dragging_type: Option<i32>,
    /// IME 変換中の未確定文字列 (Preedit)。Commit で空になり search_query に追加される。
    search_preedit: String,
    /// 直近の検索クエリ履歴 (古い順)。`bmz-search:<q>` 仮想フォルダとしてルートに並ぶ。
    search_history: std::collections::VecDeque<String>,
    /// 直近の検索で「0 件ヒット」になった等のフィードバック文字列。
    /// 検索モード解除時にクリアされる。
    search_message: Option<String>,
    /// CLI から入ったプラクティスセッション。選曲 UI からは未対応。
    practice_session: Option<PracticeSession>,
    /// 次の `RunningPlaySession::start` で使う chart zero（区間先頭の 1 秒前）。
    practice_chart_zero_time: Option<TimeUs>,
    select_frame_profiler: SceneFrameProfiler,
    result_frame_profiler: SceneFrameProfiler,
    /// 直近のマウスカーソル移動 / 操作時刻。カーソル非表示判定に使う。
    last_cursor_action_at: Instant,
    /// 現在マウスカーソルが表示されているか。
    cursor_visible: bool,
}

struct ActiveSkinVideoSource {
    texture: SkinTextureId,
    path: PathBuf,
    decoder: Option<VideoBgaDecoder>,
    last_pts: Option<i64>,
    loop_start_us: i64,
    /// スキン config の option による静的な有効判定。
    active: bool,
    /// このソースを参照する各 destination の op 条件。実行時 state に対して
    /// 評価し、現在のシーン状態 (例: リザルトのランク) で実際に表示されるソース
    /// だけをデコードするために使う。空なら参照されておらず常時可視扱い。
    gating_op_sets: Vec<Vec<i32>>,
    /// `gating_op_sets` 評価に必要な document の有効 option 一覧。
    enabled_options: Vec<i32>,
    /// リザルト draw state 構築に使う document の ranktime。
    result_ranktime_ms: i32,
    failed: bool,
}

#[derive(Debug, Default)]
struct SceneFrameProfiler {
    frames: u32,
    video_us: u128,
    snapshot_us: u128,
    render_us: u128,
    plan_us: u128,
    draw_us: u128,
    text_us: u128,
    geometry_us: u128,
    upload_us: u128,
    submit_us: u128,
    surface_us: u128,
    bind_us: u128,
    encode_us: u128,
    queue_us: u128,
    present_us: u128,
    commands: u128,
    steps: u128,
    rect_steps: u128,
    image_steps: u128,
    text_steps: u128,
    rect_instances: u128,
    image_instances: u128,
    text_instances: u128,
}

impl SceneFrameProfiler {
    const LOG_EVERY_FRAMES: u32 = 120;

    fn record(
        &mut self,
        profile: FrameProfileKind,
        video_us: u128,
        snapshot_us: u128,
        render_us: u128,
        timings: Option<RenderFrameTimings>,
    ) {
        self.frames += 1;
        self.video_us += video_us;
        self.snapshot_us += snapshot_us;
        self.render_us += render_us;
        if let Some(timings) = timings {
            self.plan_us += timings.plan_us;
            self.draw_us += timings.draw_us;
            self.text_us += timings.text_us;
            self.geometry_us += timings.geometry_us;
            self.upload_us += timings.upload_us;
            self.submit_us += timings.submit_us;
            self.surface_us += timings.surface_us;
            self.bind_us += timings.bind_us;
            self.encode_us += timings.encode_us;
            self.queue_us += timings.queue_us;
            self.present_us += timings.present_us;
            self.commands += timings.commands as u128;
            self.steps += timings.steps as u128;
            self.rect_steps += timings.rect_steps as u128;
            self.image_steps += timings.image_steps as u128;
            self.text_steps += timings.text_steps as u128;
            self.rect_instances += timings.rect_instances as u128;
            self.image_instances += timings.image_instances as u128;
            self.text_instances += timings.text_instances as u128;
        }
        if self.frames >= Self::LOG_EVERY_FRAMES {
            self.log_and_reset(profile);
        }
    }

    fn log_and_reset(&mut self, profile: FrameProfileKind) {
        let frames = self.frames.max(1) as u128;
        let commands = (self.commands / frames) as u64;
        let steps = (self.steps / frames) as u64;
        let rect_steps = (self.rect_steps / frames) as u64;
        let image_steps = (self.image_steps / frames) as u64;
        let text_steps = (self.text_steps / frames) as u64;
        let rect_instances = (self.rect_instances / frames) as u64;
        let image_instances = (self.image_instances / frames) as u64;
        let text_instances = (self.text_instances / frames) as u64;
        let video_ms = fmt_profile_ms(self.video_us, frames);
        let snapshot_ms = fmt_profile_ms(self.snapshot_us, frames);
        let render_ms = fmt_profile_ms(self.render_us, frames);
        let plan_ms = fmt_profile_ms(self.plan_us, frames);
        let draw_ms = fmt_profile_ms(self.draw_us, frames);
        let text_ms = fmt_profile_ms(self.text_us, frames);
        let geometry_ms = fmt_profile_ms(self.geometry_us, frames);
        let upload_ms = fmt_profile_ms(self.upload_us, frames);
        let submit_ms = fmt_profile_ms(self.submit_us, frames);
        let surface_ms = fmt_profile_ms(self.surface_us, frames);
        let bind_ms = fmt_profile_ms(self.bind_us, frames);
        let encode_ms = fmt_profile_ms(self.encode_us, frames);
        let queue_ms = fmt_profile_ms(self.queue_us, frames);
        let present_ms = fmt_profile_ms(self.present_us, frames);
        match profile {
            FrameProfileKind::Select => {
                tracing::debug!(
                    target: "bmz_player::select_profile",
                    frames = self.frames,
                    video_ms,
                    snapshot_ms,
                    render_ms,
                    plan_ms,
                    draw_ms,
                    text_ms,
                    geometry_ms,
                    upload_ms,
                    submit_ms,
                    surface_ms,
                    bind_ms,
                    encode_ms,
                    queue_ms,
                    present_ms,
                    commands,
                    steps,
                    rect_steps,
                    image_steps,
                    text_steps,
                    rect_instances,
                    image_instances,
                    text_instances,
                    "select frame profile"
                );
            }
            FrameProfileKind::Result => {
                tracing::debug!(
                    target: "bmz_player::result_profile",
                    frames = self.frames,
                    video_ms,
                    snapshot_ms,
                    render_ms,
                    plan_ms,
                    draw_ms,
                    text_ms,
                    geometry_ms,
                    upload_ms,
                    submit_ms,
                    surface_ms,
                    bind_ms,
                    encode_ms,
                    queue_ms,
                    present_ms,
                    commands,
                    steps,
                    rect_steps,
                    image_steps,
                    text_steps,
                    rect_instances,
                    image_instances,
                    text_instances,
                    "result frame profile"
                );
            }
        }
        *self = Self::default();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameProfileKind {
    Select,
    Result,
}

fn fmt_profile_ms(total_us: u128, frames: u128) -> String {
    format!("{:.3}", total_us as f64 / frames as f64 / 1000.0)
}

type PlaySkinSignature = (KeyMode, String, BTreeMap<String, String>, BTreeMap<String, String>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectMetaImageSlot {
    Stage,
    Backbmp,
    Banner,
}

enum SelectMetaImageCacheEntry {
    Loading,
    Ready(RgbaImageAsset),
    Missing,
}

struct SelectMetaImageResult {
    slot: SelectMetaImageSlot,
    key: String,
    path: Option<PathBuf>,
    result: std::result::Result<RgbaImageAsset, String>,
}

enum SelectPreviewCacheEntry {
    Loading,
    Ready(DecodedSample),
    Missing,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum SelectPreviewFade {
    #[default]
    Silent,
    FadingIn {
        started_at: Instant,
    },
    Playing,
    FadingOut {
        started_at: Instant,
    },
}

struct SelectPreviewResult {
    key: String,
    path: Option<PathBuf>,
    result: std::result::Result<DecodedSample, String>,
}

enum SelectFolderSummaryCacheEntry {
    Loading,
    Ready(Option<SelectFolderSummary>),
    Missing,
}

struct SelectFolderSummaryResult {
    key: String,
    result: std::result::Result<Option<SelectFolderSummary>, String>,
}

#[derive(Debug, Clone)]
struct TableBreadcrumb {
    name: String,
    symbol: String,
}

struct DecideTransition {
    chart_id: i64,
    options: PlayStartOptions,
    started_at: Instant,
    fadeout_started_at: Option<Instant>,
    cancel: bool,
    snapshot: RenderSnapshot,
}

struct PendingPlayStart {
    chart_id: i64,
    options: PlayStartOptions,
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

enum SongScanEvent {
    Progress(ScanProgress),
    Finished(Result<ScanReport>),
}

struct PracticeChartDefaults {
    property: crate::screens::practice::PracticeProperty,
    title: String,
    sha256: [u8; 32],
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
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResultExitAction {
    /// 選曲画面へ戻る。
    Leave,
    /// 直前と同じ譜面を、指定した arrange でもう一度プレイする。
    Retry(ResultRetryMode),
    /// レーンキー (Key1-4 / Key5 / Key7) 押下で開始した遷移。
    /// フェードアウト終了時の Key5/Key7 押下状態で、retry(arrange) か
    /// 選曲へ戻るかを決める (beatoraja の REPLAY_SAME / REPLAY_DIFFERENT / OK 相当)。
    HeldLanes,
    /// コース（段位）リザルトから、コース全体を同配置で再プレイする。
    RetryCourseSameArrange,
    /// コース曲間の中間リザルトを閉じて、コースの次の曲を開始する。
    /// リトライは発生させず次譜面へ進むだけ (beatoraja の MusicResult コース分岐相当)。
    AdvanceCourse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResultRetryMode {
    SameArrange,
    DifferentArrange,
}

const SELECT_EXIT_HOLD_DURATION: Duration = Duration::from_millis(1_200);
/// プレイ中の Start ボタンを「2回連続押し」と判定する間隔上限。
const PLAY_START_DOUBLE_PRESS_WINDOW: Duration = Duration::from_millis(400);
/// リザルト退出時にプレイ残響(draining_audio)を絞り切るまでの上限時間。
/// スキンの終了アニメーション (`fadeout`) が長くても (例: Starseeker は 3000ms)、
/// 音声はこの時間内でフェードし切る。スキンの fadeout がこれより短ければそちらを優先。
const RESULT_EXIT_AUDIO_FADE: Duration = Duration::from_millis(1_500);
/// beatoraja PreviewMusicProcessor fades select BGM over 10 * 15ms steps.
const SELECT_PREVIEW_FADE_DURATION: Duration = Duration::from_millis(150);
/// beatoraja MusicSelector waits this long after a song-bar change before preview starts.
const SELECT_PREVIEW_START_DELAY: Duration = Duration::from_millis(400);
/// レーンカバー / LIFT を上下キーで動かす際のステップ幅。
const LANE_COVER_STEP: f32 = 0.001;
const LANE_COVER_REPEAT_STEP: f32 = 0.01;
const SKIN_RELOAD_DEBOUNCE: Duration = Duration::from_millis(300);
/// アナログスクラッチの tick が途切れたとみなし、端数バッファを捨てるまでの時間 (ms)。
/// beatoraja の `getAnalogDiffAndReset(i, 200)` の tolerance に相当。
const SELECT_ANALOG_SCROLL_TOLERANCE_MS: u64 = 200;

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum DeferredBoot {
    Chart {
        chart_id: i64,
        replay_slot: Option<u8>,
    },
    Practice {
        chart_id: i64,
        start_time_ms: Option<u32>,
        end_time_ms: Option<u32>,
    },
    /// `--boot-replay-file <PATH>`: リプレイファイル直接指定の再生。
    ReplayFile {
        path: String,
    },
    CourseReplay {
        course_id: i64,
    },
    Course {
        course_id: i64,
    },
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

fn course_result_summary_for_skin(course: &CourseResultSummary) -> ResultSummary {
    let last = course.entry_summaries.last();
    let clear_type = if course.course_failed {
        ClearType::Failed
    } else {
        last.map(|summary| summary.clear_type).unwrap_or(ClearType::NoPlay)
    };
    let max_combo =
        course.entry_summaries.iter().map(|summary| summary.max_combo).max().unwrap_or(0);
    let bp = course.entry_summaries.iter().map(|summary| summary.bp).sum();
    let cb = course.entry_summaries.iter().map(|summary| summary.cb).sum();
    let fast_slow_counts =
        course.entry_summaries.iter().fold(ResultFastSlowJudgeCounts::default(), |acc, summary| {
            ResultFastSlowJudgeCounts {
                fast_pgreat: acc.fast_pgreat + summary.fast_slow_counts.fast_pgreat,
                slow_pgreat: acc.slow_pgreat + summary.fast_slow_counts.slow_pgreat,
                fast_great: acc.fast_great + summary.fast_slow_counts.fast_great,
                slow_great: acc.slow_great + summary.fast_slow_counts.slow_great,
                fast_good: acc.fast_good + summary.fast_slow_counts.fast_good,
                slow_good: acc.slow_good + summary.fast_slow_counts.slow_good,
                fast_bad: acc.fast_bad + summary.fast_slow_counts.fast_bad,
                slow_bad: acc.slow_bad + summary.fast_slow_counts.slow_bad,
                fast_poor: acc.fast_poor + summary.fast_slow_counts.fast_poor,
                slow_poor: acc.slow_poor + summary.fast_slow_counts.slow_poor,
                fast_empty_poor: acc.fast_empty_poor + summary.fast_slow_counts.fast_empty_poor,
                slow_empty_poor: acc.slow_empty_poor + summary.fast_slow_counts.slow_empty_poor,
            }
        });
    let best_clear_type =
        course.best_score.as_ref().and_then(|best| ClearType::from_label(&best.clear_type));

    ResultSummary {
        clear_type,
        arrange: "NORMAL".to_string(),
        lane_shuffle_pattern: Vec::new(),
        ex_score: course.total_ex_score,
        max_combo,
        bp,
        cb,
        gauge_value: last.map(|summary| summary.gauge_value).unwrap_or(0.0),
        gauge_type: last.map(|summary| summary.gauge_type).unwrap_or(GaugeType::Normal),
        total_notes: course.total_notes,
        duration_ms: last.map(|summary| summary.duration_ms).unwrap_or(0),
        initial_bpm: last.map(|summary| summary.initial_bpm).unwrap_or(0.0),
        min_bpm: course
            .entry_summaries
            .iter()
            .map(|summary| summary.min_bpm)
            .filter(|bpm| *bpm > 0.0)
            .reduce(f32::min)
            .unwrap_or(0.0),
        max_bpm: course
            .entry_summaries
            .iter()
            .map(|summary| summary.max_bpm)
            .filter(|bpm| *bpm > 0.0)
            .reduce(f32::max)
            .unwrap_or(0.0),
        main_bpm: last.map(|summary| summary.main_bpm).unwrap_or(0.0),
        total_gauge: last.map(|summary| summary.total_gauge).unwrap_or(0.0),
        judge_rank: last.and_then(|summary| summary.judge_rank),
        key_mode: last.map(|summary| summary.key_mode).unwrap_or_default(),
        judge_counts: course.judge_counts.clone(),
        fast_slow_counts,
        replay_path: String::new(),
        replay_slots: course.replay_slots,
        saved_replay_slots: course.saved_replay_slots,
        score_history_id: course.best_score.as_ref().map(|best| best.course_score_id).unwrap_or(0),
        best_ex_score: course.best_score.as_ref().map(|best| best.ex_score),
        best_clear_type,
        best_max_combo: course.best_score.as_ref().map(|best| best.max_combo),
        best_bp: course.best_score.as_ref().map(|best| best.bp),
        previous_best_ex_score: None,
        previous_best_clear_type: None,
        previous_best_max_combo: None,
        previous_best_bp: None,
        target_ex_score: None,
        target_max_combo: None,
        target_bp: None,
        target_clear_type: None,
        ir_queued_jobs: course.entry_summaries.iter().map(|summary| summary.ir_queued_jobs).sum(),
        ir_last_error: course
            .entry_summaries
            .iter()
            .find_map(|summary| summary.ir_last_error.clone()),
        title: course.title.clone(),
        subtitle: String::new(),
        artist: String::new(),
        subartist: String::new(),
        genre: match course.kind {
            bmz_core::course::CourseKind::Dan => "DAN".to_string(),
            bmz_core::course::CourseKind::Course => "COURSE".to_string(),
        },
        difficulty_name: String::new(),
        play_level: String::new(),
        graph: aggregate_course_result_graph(&course.entry_summaries),
    }
}

fn aggregate_course_result_graph(
    entries: &[ResultSummary],
) -> bmz_render::snapshot::ResultGraphSnapshot {
    let durations: Vec<i32> =
        entries.iter().map(|entry| result_graph_duration_ms(&entry.graph)).collect();
    let total_duration = durations.iter().copied().sum::<i32>().max(1);
    let mut offset_ms = 0_i32;
    let mut graph = bmz_render::snapshot::ResultGraphSnapshot::default();

    for (entry, duration_ms) in entries.iter().zip(durations) {
        graph.gauge_points.extend(entry.graph.gauge_points.iter().map(|point| {
            let mut point = *point;
            point.time_ms = point.time_ms.saturating_add(offset_ms);
            point
        }));
        graph.timing_points.extend(entry.graph.timing_points.iter().map(|point| {
            bmz_render::snapshot::ResultTimingPoint {
                time_ms: point.time_ms.saturating_add(offset_ms),
                delta_us: point.delta_us,
                judge: point.judge,
            }
        }));
        graph.judge_graph_buckets.extend_from_slice(&entry.graph.judge_graph_buckets);
        graph.early_late_graph_buckets.extend_from_slice(&entry.graph.early_late_graph_buckets);
        graph.judge_graph_density.extend_from_slice(&entry.graph.judge_graph_density);
        graph.bpm_graph_segments.extend(entry.graph.bpm_graph_segments.iter().map(|segment| {
            let start = offset_ms as f32 + segment.start_ratio * duration_ms as f32;
            let end = offset_ms as f32 + segment.end_ratio * duration_ms as f32;
            bmz_render::snapshot::BpmGraphSegment {
                start_ratio: (start / total_duration as f32).clamp(0.0, 1.0),
                end_ratio: (end / total_duration as f32).clamp(0.0, 1.0),
                bpm: segment.bpm,
                is_stop: segment.is_stop,
            }
        }));
        if entry.graph.hit_error_ring != Default::default() {
            graph.hit_error_ring = entry.graph.hit_error_ring;
        }
        offset_ms = offset_ms.saturating_add(duration_ms);
    }

    graph.timing_distribution = bmz_render::snapshot::ResultTimingDistribution::default();
    for point in &graph.timing_points {
        graph.timing_distribution.add((point.delta_us / 1_000) as i32);
    }

    graph
}

fn result_graph_duration_ms(graph: &bmz_render::snapshot::ResultGraphSnapshot) -> i32 {
    let gauge_ms = graph.gauge_points.last().map(|point| point.time_ms).unwrap_or(0);
    let timing_ms = graph.timing_points.last().map(|point| point.time_ms).unwrap_or(0);
    let density_ms = i32::try_from(graph.judge_graph_density.len()).unwrap_or(i32::MAX / 1_000);
    let judge_ms = i32::try_from(graph.judge_graph_buckets.len())
        .unwrap_or(i32::MAX / 1_000)
        .saturating_mul(1_000);
    let early_late_ms = i32::try_from(graph.early_late_graph_buckets.len())
        .unwrap_or(i32::MAX / 1_000)
        .saturating_mul(1_000);
    gauge_ms
        .max(timing_ms)
        .max(density_ms.saturating_mul(1_000))
        .max(judge_ms)
        .max(early_late_ms)
        .max(1)
}

fn debug_boot_finished_play_session() -> FinishedPlaySession {
    let summary = debug_boot_result_summary();
    let judge_counts = JudgeCounts {
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
    };
    let result = PlayResult {
        chart_sha256: [0; 32],
        clear_type: summary.clear_type,
        gauge_type: summary.gauge_type,
        gauge_value: summary.gauge_value,
        total_notes: summary.total_notes,
        score: ScoreState {
            judges: judge_counts,
            combo: 0,
            max_combo: summary.max_combo,
            past_notes: summary.total_notes,
            ghost: Vec::new(),
        },
        autoplay: false,
    };
    FinishedPlaySession {
        result,
        stored: StoredPlayResult {
            score_history_id: 0,
            replay_path: String::new(),
            slot_paths: [None, None, None, None],
            device_type: InputDeviceKind::Keyboard,
        },
        summary,
        replay_playback: false,
        arrange: ArrangeOption::Normal,
        applied_arrange: AppliedArrange::default(),
        ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
    }
}

fn debug_boot_result_summary() -> ResultSummary {
    let fast_slow_counts = ResultFastSlowJudgeCounts {
        fast_pgreat: 128,
        slow_pgreat: 92,
        fast_great: 31,
        slow_great: 69,
        fast_good: 9,
        slow_good: 20,
        fast_bad: 3,
        slow_bad: 5,
        fast_poor: 2,
        slow_poor: 8,
        fast_empty_poor: 1,
        slow_empty_poor: 2,
    };
    let judge_counts = crate::screens::result_model::ResultJudgeCounts {
        pgreat: fast_slow_counts.fast_pgreat + fast_slow_counts.slow_pgreat,
        great: fast_slow_counts.fast_great + fast_slow_counts.slow_great,
        good: fast_slow_counts.fast_good + fast_slow_counts.slow_good,
        bad: fast_slow_counts.fast_bad + fast_slow_counts.slow_bad,
        poor: fast_slow_counts.fast_poor + fast_slow_counts.slow_poor,
        empty_poor: fast_slow_counts.fast_empty_poor + fast_slow_counts.slow_empty_poor,
    };
    let total_notes = 594;
    let duration_ms = 180_000;
    ResultSummary {
        clear_type: ClearType::Failed,
        arrange: "RANDOM".to_string(),
        lane_shuffle_pattern: vec![3, 1, 4, 2, 7, 5, 6],
        ex_score: judge_counts.pgreat * 2 + judge_counts.great,
        max_combo: 239,
        bp: 30,
        cb: 345,
        gauge_value: 39.4,
        gauge_type: GaugeType::Normal,
        total_notes,
        duration_ms,
        initial_bpm: 171.0,
        min_bpm: 128.0,
        max_bpm: 192.0,
        main_bpm: 171.0,
        total_gauge: 363.0,
        judge_rank: Some(2),
        key_mode: KeyMode::K7,
        judge_counts,
        fast_slow_counts,
        replay_path: String::new(),
        replay_slots: [true, false, true, false],
        saved_replay_slots: [false, false, false, false],
        score_history_id: 0,
        best_ex_score: Some(780),
        best_clear_type: Some(ClearType::Easy),
        best_max_combo: Some(412),
        best_bp: Some(24),
        previous_best_ex_score: Some(760),
        previous_best_clear_type: Some(ClearType::Normal),
        previous_best_max_combo: Some(390),
        previous_best_bp: Some(36),
        target_ex_score: Some(1_056),
        target_max_combo: Some(594),
        target_bp: Some(10),
        target_clear_type: Some(ClearType::Hard),
        ir_queued_jobs: 0,
        ir_last_error: None,
        title: "Debug Result Boot [ANOTHER]".to_string(),
        subtitle: "synthetic result".to_string(),
        artist: "bmz-player".to_string(),
        subartist: "Codex".to_string(),
        genre: "DEBUG".to_string(),
        difficulty_name: "ANOTHER".to_string(),
        play_level: "12".to_string(),
        graph: debug_boot_result_graph(duration_ms),
    }
}

fn debug_boot_result_graph(duration_ms: i32) -> bmz_render::snapshot::ResultGraphSnapshot {
    let mut graph = bmz_render::snapshot::ResultGraphSnapshot {
        gauge_points: (0..=18)
            .map(|index| bmz_render::snapshot::ResultGaugeGraphPoint {
                time_ms: index * 10_000,
                value: (100.0 - index as f32 * 3.2).max(12.0),
                border: 20.0,
                gauge_type: GaugeType::Normal as i32,
            })
            .collect(),
        judge_graph_buckets: (0..360)
            .map(|index| bmz_render::snapshot::ResultJudgeGraphBucket {
                values: [
                    0,
                    1 + (index % 5) as u32,
                    (index % 4) as u32,
                    (index % 3) as u32,
                    (index % 2) as u32,
                    ((index + 1) % 2) as u32,
                ],
            })
            .collect(),
        early_late_graph_buckets: (0..360)
            .map(|index| bmz_render::snapshot::ResultEarlyLateGraphBucket {
                values: [
                    0,
                    1 + (index % 5) as u32,
                    (index % 4) as u32,
                    ((index + 2) % 3) as u32,
                    (index % 2) as u32,
                    0,
                    ((index + 1) % 5) as u32,
                    ((index + 3) % 4) as u32,
                    ((index + 1) % 3) as u32,
                    ((index + 1) % 2) as u32,
                ],
            })
            .collect(),
        bpm_graph_segments: vec![
            bmz_render::snapshot::BpmGraphSegment {
                start_ratio: 0.0,
                end_ratio: 0.35,
                bpm: 171.0,
                is_stop: false,
            },
            bmz_render::snapshot::BpmGraphSegment {
                start_ratio: 0.35,
                end_ratio: 0.55,
                bpm: 128.0,
                is_stop: false,
            },
            bmz_render::snapshot::BpmGraphSegment {
                start_ratio: 0.55,
                end_ratio: 0.56,
                bpm: 0.0,
                is_stop: true,
            },
            bmz_render::snapshot::BpmGraphSegment {
                start_ratio: 0.56,
                end_ratio: 1.0,
                bpm: 192.0,
                is_stop: false,
            },
        ],
        ..Default::default()
    };
    graph.judge_graph_density =
        graph.judge_graph_buckets.iter().map(|bucket| bucket.total().min(255) as u8).collect();
    graph.timing_points = (-60..=60)
        .map(|index| {
            let delta_ms: i32 = if index % 7 == 0 { index / 2 } else { index / 4 };
            let judge = if delta_ms.abs() <= 8 {
                bmz_core::judge::Judge::PGreat
            } else if delta_ms.abs() <= 24 {
                bmz_core::judge::Judge::Great
            } else {
                bmz_core::judge::Judge::Good
            };
            bmz_render::snapshot::ResultTimingPoint {
                time_ms: ((index + 60) * duration_ms / 120).clamp(0, duration_ms),
                delta_us: i64::from(delta_ms) * 1_000,
                judge,
            }
        })
        .collect();
    graph.timing_distribution = bmz_render::snapshot::ResultTimingDistribution::default();
    for point in &graph.timing_points {
        graph.timing_distribution.add((point.delta_us / 1_000) as i32);
    }
    graph
}

fn result_min_bpm(summary: &ResultSummary) -> f32 {
    summary
        .graph
        .bpm_graph_segments
        .iter()
        .filter(|segment| !segment.is_stop && segment.bpm > 0.0)
        .map(|segment| segment.bpm)
        .reduce(f32::min)
        .unwrap_or(summary.min_bpm)
}

fn result_max_bpm(summary: &ResultSummary) -> f32 {
    summary
        .graph
        .bpm_graph_segments
        .iter()
        .filter(|segment| !segment.is_stop && segment.bpm > 0.0)
        .map(|segment| segment.bpm)
        .reduce(f32::max)
        .unwrap_or(summary.max_bpm)
}

fn result_main_bpm(summary: &ResultSummary) -> f32 {
    summary
        .graph
        .bpm_graph_segments
        .iter()
        .filter(|segment| !segment.is_stop && segment.bpm > 0.0)
        .max_by(|a, b| {
            let a_width = a.end_ratio - a.start_ratio;
            let b_width = b.end_ratio - b.start_ratio;
            a_width.total_cmp(&b_width)
        })
        .map(|segment| segment.bpm)
        .unwrap_or(summary.main_bpm)
}

impl WinitApp {
    fn new(
        boot: BootstrappedApp,
        options: AppOptions,
        audio_runtime: Option<AudioRuntime>,
        system_audio: Option<crate::audio::SystemAudio>,
        shutdown_requested: Arc<AtomicBool>,
    ) -> Result<Self> {
        let mut boot = boot;
        if let Some(cli_renderer) = options.renderer.clone() {
            tracing::info!(?cli_renderer, "overriding renderer backend via CLI option");
            boot.app_config.video.renderer = cli_renderer;
        }

        let folder_stack = initial_folder_stack(&boot.app_config);
        let initial_mode_filter =
            SelectModeFilter::from_str_or_default(&boot.profile_config.select.mode_filter);
        let select_sort = SelectSort::from_str_or_default(&boot.profile_config.select.sort);
        let (select_items, select_mode_filter) =
            load_items_for_stack(&boot, &folder_stack, &[], initial_mode_filter, select_sort);
        boot.profile_config.select.mode_filter = select_mode_filter.as_str().to_string();
        let boot_chart_id = resolve_boot_chart_id(&boot.library_db, &options);
        log_startup_options(&options);

        let assist_option = if options.autoplay_on_start || boot.profile_config.play.auto_play {
            AssistOption::Autoplay
        } else {
            AssistOption::Normal
        };
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
        let target_option = target_option_from_profile(boot.profile_config.play.target);
        let select_keys = SelectKeyBindings::from_profile(&boot.profile_config.input);
        let mut renderer = Renderer::default();
        let skin_catalog = scan_skin_catalog();
        let (skin_decode_tx, skin_decode_rx) = mpsc::channel::<PendingSkinResult>();
        let (skin_upload_tx, skin_upload_rx) = mpsc::channel::<PendingUploadResult>();
        let (select_meta_image_tx, select_meta_image_rx) = mpsc::channel::<SelectMetaImageResult>();
        let (select_preview_tx, select_preview_rx) = mpsc::channel::<SelectPreviewResult>();
        let (select_folder_summary_tx, select_folder_summary_rx) =
            mpsc::channel::<SelectFolderSummaryResult>();
        let (
            default_skin_manifest,
            initial_skin_video_sources,
            pending_select_skin,
            pending_decide_skin,
            pending_result_skin,
        ) = load_initial_skin_textures(
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
        let now = Instant::now();
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
        let system_sound =
            system_audio.as_ref().map(|audio| system_sound_manager_from_boot(&boot, audio));
        let select_preview =
            system_audio.as_ref().map(|audio| SelectChartPreview::new(audio.engine()));
        let audio_output_open_attempted = audio_runtime.is_some();

        let mut app = Self {
            boot,
            window: None,
            active_play: None,
            active_course: None,
            finished_course: None,
            draining_audio: None,
            audio_runtime,
            audio_output_open_attempted,
            first_frame_startup_completed: false,
            shutdown_requested,
            finished_play: None,
            result_ir: None,
            select_ir: crate::screens::select_ir::SelectIrRanking::default(),
            last_play_was_autoplay: false,
            last_play_snapshot: None,
            pending_decide: None,
            pending_play_start: None,
            pending_play_preload: None,
            preloaded_play_session: None,
            play_preload_generation: 0,
            play_ending: None,
            last_started_chart_id: None,
            play_table_text_primary: String::new(),
            play_table_text_secondary: String::new(),
            play_table_text_fallback: String::new(),
            select_items,
            select_distribution_cache: RefCell::new(HashMap::new()),
            table_breadcrumb_cache: RefCell::new(HashMap::new()),
            select_folder_summary_cache: HashMap::new(),
            select_folder_summary_tx,
            select_folder_summary_rx,
            selected_index_stack: vec![0; folder_stack.len()],
            folder_stack,
            selected_index: 0,
            renderer,
            skin_catalog,
            skin_defs_cache: BTreeMap::new(),
            pending_table_fetch: None,
            pending_song_scan: None,
            song_scan_progress: None,
            last_scene_kind: None,
            start_held: false,
            select_held: false,
            arrange_option,
            target_option,
            gauge_option,
            gauge_auto_shift_option,
            bottom_shiftable_gauge_option,
            assist_option,
            select_mode_filter,
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
            smoke_exit_after_frames: options.smoke_exit_after_frames,
            smoke_exit_after_result_frames: options.smoke_exit_after_result_frames,
            smoke_exit_on_result: options.smoke_exit_on_result,
            smoke_screenshot_path: options.smoke_screenshot_path.as_ref().map(PathBuf::from),
            rendered_frames: 0,
            rendered_result_frames: 0,
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
            skin_video_sources: initial_skin_video_sources,
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
            select_stage_size: None,
            select_backbmp_source: None,
            select_backbmp_loaded: false,
            select_backbmp_size: None,
            select_banner_source: None,
            select_banner_loaded: false,
            select_banner_size: None,
            select_preview_source: None,
            select_preview_playing: false,
            select_preview_fade: SelectPreviewFade::Silent,
            select_preview,
            select_meta_image_cache: HashMap::new(),
            select_meta_image_tx,
            select_meta_image_rx,
            select_preview_cache: HashMap::new(),
            select_preview_tx,
            select_preview_rx,
            play_backbmp_source: None,
            play_backbmp_loaded: false,
            last_play_start_press_at: None,
            decide_e1_held: false,
            play_e2_held: false,
            play_e3_held: false,
            play_exit_hold_started_at: None,
            egui: None,
            applied_window_mode: initial_window_mode,
            focused: true,
            last_frame_at: None,
            wgpu_fps: 0.0,
            settings_edit: None,
            key_config_edit: None,
            result_exit: None,
            result_key5_held: false,
            result_key7_held: false,
            result_gauge_graph_type: GaugeType::Normal as i32,
            deferred_boot: deferred_boot_action(boot_chart_id, &options),
            search_mode: false,
            search_query: String::new(),
            last_cursor_position: None,
            select_slider_dragging_type: None,
            search_preedit: String::new(),
            search_history: std::collections::VecDeque::new(),
            search_message: None,
            practice_session: None,
            practice_chart_zero_time: None,
            select_frame_profiler: SceneFrameProfiler::default(),
            result_frame_profiler: SceneFrameProfiler::default(),
            last_cursor_action_at: now,
            cursor_visible: true,
        };
        if options.boot_result_sample {
            tracing::info!("booting directly into synthetic result screen");
            app.finished_play = Some(debug_boot_finished_play_session());
            app.result_gauge_graph_type = app
                .finished_play
                .as_ref()
                .map(|finished| finished.summary.gauge_type as i32)
                .unwrap_or(GaugeType::Normal as i32);
            app.result_key5_held = false;
            app.result_key7_held = false;
            app.result_scene_started_at = Instant::now();
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
                // サーフェス生成前に present mode とバックエンド設定を反映させておく。
                self.renderer.set_present_mode(config_present_mode(&self.boot.app_config.video));
                let backend = config_renderer_backend(self.boot.app_config.video.renderer.clone());
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
            }
            Err(error) => {
                tracing::error!(%error, "failed to create window");
                event_loop.exit();
            }
        }
    }

    fn start_deferred_boot(&mut self) {
        let Some(boot) = self.deferred_boot.take() else {
            return;
        };
        match boot {
            DeferredBoot::Chart { chart_id, replay_slot } => {
                tracing::info!(chart_id, "booting directly into chart");
                if let Some(slot) = replay_slot {
                    if !self.try_start_replay_for_chart(chart_id, slot) {
                        tracing::warn!(slot, "boot replay slot empty; falling back to normal play");
                        self.start_chart(chart_id);
                    }
                } else {
                    self.start_chart(chart_id);
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
                match self.boot.library_db.latest_course_score_id(course_id) {
                    Ok(Some(course_score_id)) => {
                        tracing::info!(course_id, course_score_id, "booting into course replay");
                        self.start_course_replay(course_id, course_score_id);
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
                self.start_course(course_id);
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

        if let Some(course) = &self.finished_course {
            return AppViewState::Result(Box::new(course_result_summary_for_skin(course)));
        }

        if let Some(finished) = &self.finished_play {
            return AppViewState::Result(Box::new(finished.summary.clone()));
        }

        AppViewState::Select
    }

    fn current_scene_kind(&self) -> AppSceneKind {
        if self.pending_decide.is_some() {
            return AppSceneKind::Decide;
        }
        if self.active_play.is_some() || self.pending_play_start.is_some() {
            return AppSceneKind::Play;
        }
        if self.finished_course.is_some() || self.finished_play.is_some() {
            return AppSceneKind::Result;
        }
        AppSceneKind::Select
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
                arrange: summary.arrange.as_str().to_string(),
                lane_shuffle_pattern: summary.lane_shuffle_pattern.clone(),
                ex_score: summary.ex_score,
                ex_score_rate: summary.ex_score_rate(),
                max_combo: summary.max_combo,
                bp: summary.bp,
                cb: summary.cb,
                gauge_value: summary.gauge_value,
                gauge_type: summary.gauge_type as i32,
                total_notes: summary.total_notes,
                grade_diff_display: self.boot.profile_config.play.grade_diff_display,
                duration_ms: summary.duration_ms,
                initial_bpm: summary.initial_bpm,
                min_bpm: result_min_bpm(&summary),
                max_bpm: result_max_bpm(&summary),
                main_bpm: result_main_bpm(&summary),
                total_gauge: summary.total_gauge,
                judge_rank: summary.judge_rank,
                key_mode: summary.key_mode,
                result_gauge_graph_type: self.result_gauge_graph_type,
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
                replay_slots: summary.replay_slots,
                saved_replay_slots: summary.saved_replay_slots,
                best_ex_score: summary.best_ex_score,
                best_clear_type: summary.best_clear_type,
                target_ex_score: summary.target_ex_score,
                best_max_combo: summary.best_max_combo,
                target_max_combo: summary.target_max_combo,
                best_bp: summary.best_bp,
                target_bp: summary.target_bp,
                previous_best_ex_score: summary.previous_best_ex_score,
                previous_best_clear_type: summary.previous_best_clear_type,
                previous_best_max_combo: summary.previous_best_max_combo,
                previous_best_bp: summary.previous_best_bp,
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
                graph: summary.graph.clone(),
                overlay: OverlaySnapshot::default(),
                ir: self.result_ir.as_ref().map(|state| state.skin_snapshot()).unwrap_or_default(),
            }),
        };
        let overlay = self.build_overlay_snapshot();
        self.apply_overlay_to_scene(&mut scene, overlay);
        scene
    }

    fn build_overlay_snapshot(&self) -> OverlaySnapshot {
        OverlaySnapshot {
            left_text: self.song_scan_overlay_text(),
            text: self.always_overlay_text(),
            fps_text: self.wgpu_fps_overlay_text(),
        }
    }

    fn song_scan_overlay_text(&self) -> String {
        self.song_scan_progress
            .map(|progress| format!("SCAN {} / {}", progress.done, progress.total))
            .unwrap_or_default()
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

    fn fallback_table_breadcrumb(source_url: &str) -> TableBreadcrumb {
        TableBreadcrumb {
            name: std::path::Path::new(source_url)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(source_url)
                .to_string(),
            symbol: String::new(),
        }
    }

    fn table_breadcrumb(&self, source_url: &str) -> TableBreadcrumb {
        if let Some(cached) = self.table_breadcrumb_cache.borrow().get(source_url) {
            return cached.clone();
        }

        let mut cache = self.table_breadcrumb_cache.borrow_mut();
        if let Ok(tables) = self.boot.library_db.list_difficulty_tables() {
            for table in tables {
                cache.insert(
                    table.source_url,
                    TableBreadcrumb {
                        name: format!("[{}] {}", table.symbol, table.name),
                        symbol: table.symbol,
                    },
                );
            }
        }

        cache
            .entry(source_url.to_string())
            .or_insert_with(|| Self::fallback_table_breadcrumb(source_url))
            .clone()
    }

    /// 難易度表のパンくず表示名。テーブルが既知なら `[symbol] name`、
    /// 不明なら URL のファイル名部分にフォールバックする。
    fn table_breadcrumb_name(&self, source_url: &str) -> String {
        self.table_breadcrumb(source_url).name
    }

    fn table_text_context_for_chart(&self, chart_id: i64) -> (String, String, String) {
        let table_level = self
            .select_items
            .iter()
            .find_map(|item| match item {
                SelectItem::Chart(row)
                    if row.chart.as_ref().is_some_and(|chart| chart.chart_id == chart_id) =>
                {
                    Some(row.table_level.clone())
                }
                _ => None,
            })
            .unwrap_or_default();

        let selected = self.select_items.get(self.selected_index);
        let source_url = table_source_url_from_context(&self.folder_stack, selected);
        let primary =
            source_url.as_ref().map(|url| self.table_breadcrumb_name(url)).unwrap_or_default();
        let secondary = table_level;
        let fallback = primary.clone();
        (primary, secondary, fallback)
    }

    fn capture_play_table_text_for_chart(&mut self, chart_id: i64) {
        let (primary, secondary, fallback) = self.table_text_context_for_chart(chart_id);
        self.play_table_text_primary = primary;
        self.play_table_text_secondary = secondary;
        self.play_table_text_fallback = fallback;
    }

    fn apply_play_table_text(&self, snapshot: &mut RenderSnapshot) {
        snapshot.table_text_primary = self.play_table_text_primary.clone();
        snapshot.table_text_secondary = self.play_table_text_secondary.clone();
        snapshot.table_text_fallback = self.play_table_text_fallback.clone();
    }

    fn select_snapshot(&self) -> SelectSnapshot {
        let selected = self.select_items.get(self.selected_index);
        let current_folder = match self.folder_stack.last() {
            None => String::new(),
            Some(path) if path.starts_with(TABLE_ROOT_PATH) => match parse_table_path(path) {
                Some(TablePath::Root) | None => "難易度表".to_string(),
                Some(TablePath::Table { source_url }) => self.table_breadcrumb_name(source_url),
                Some(TablePath::Level { source_url, level }) => {
                    let table = self.table_breadcrumb(source_url);
                    format!("{} > {}{}", table.name, table.symbol, level)
                }
            },
            Some(path) if in_settings_stack(std::slice::from_ref(path)) => {
                settings_breadcrumb(path)
            }
            Some(path) => std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string(),
        };
        let (search_word, search_word_alpha) = self.display_search_word();
        self.ensure_visible_select_chart_distributions(25);
        let chart_distributions = self.select_distribution_cache.borrow();
        SelectSnapshot {
            time: self.select_time(),
            selection_time: self.select_bar_time(),
            option_panel_time: self.option_panel_time(),
            option_panel: self.select_option_panel,
            chart_count: self.select_items.len() as u32,
            selected_index: self.selected_index as u32,
            bar_scroll_direction: self.select_bar_scroll_direction,
            bar_scroll_progress: self.select_bar_scroll_progress(),
            selected_chart_id: match selected {
                Some(SelectItem::Chart(row)) => row.chart.as_ref().map(|chart| chart.chart_id),
                _ => None,
            },
            selected_title: selected.map(|i| i.display_name()).unwrap_or_default(),
            rows: select_snapshot_rows(
                &self.select_items,
                self.selected_index,
                25,
                &self.boot.profile_config,
                self.key_config_edit.as_ref(),
                &chart_distributions,
            ),
            arrange: self.arrange_option.as_str().to_string(),
            target: self.target_option.as_string(),
            gauge: gauge_option_as_str(self.gauge_option).to_string(),
            gauge_auto_shift: gauge_auto_shift_as_str(self.gauge_auto_shift_option).to_string(),
            bottom_shiftable_gauge: bottom_shiftable_gauge_as_str(
                self.bottom_shiftable_gauge_option,
            )
            .to_string(),
            assist: self.assist_option.as_str().to_string(),
            select_mode: self.select_mode_filter.as_str().to_string(),
            select_sort: self.select_sort.as_str().to_string(),
            select_ln_mode: self
                .boot
                .profile_config
                .play
                .ln_mode_policy
                .display_label()
                .to_string(),
            bga: bga_mode_as_str(self.boot.profile_config.play.bga).to_string(),
            grade_diff_display: self.boot.profile_config.play.grade_diff_display,
            judge_timing_offset_ms: (self.boot.profile_config.judge.visual_offset_us / 1_000)
                .clamp(i32::MIN as i64, i32::MAX as i64) as i32,
            master_volume: crate::config::play::volume_unit_to_f32(
                self.boot.profile_config.audio_mix.master_volume,
            ),
            key_volume: crate::config::play::volume_unit_to_f32(
                self.boot.profile_config.audio_mix.key_volume,
            ),
            bgm_volume: crate::config::play::volume_unit_to_f32(
                self.boot.profile_config.audio_mix.bgm_volume,
            ),
            current_folder,
            key_hint: self.select_keys.key_hint.clone(),
            option_hint: self.select_keys.option_hint.clone(),
            exit_hold_progress: self.select_exit_hold_progress(),
            overlay: OverlaySnapshot::default(),
            stage_background: self.select_stage_loaded,
            stage_image_size: self.select_stage_size,
            backbmp_image: self.select_backbmp_loaded,
            backbmp_image_size: self.select_backbmp_size,
            banner_image: self.select_banner_loaded,
            banner_image_size: self.select_banner_size,
            in_settings: in_settings_stack(&self.folder_stack),
            settings_editing: self.settings_edit.is_some() || self.key_config_edit.is_some(),
            search_word,
            search_word_alpha,
            mouse_position: self.cursor_position_normalized(),
            ir: self
                .select_ir
                .snapshot_for(&self.boot.profile_config.ir, self.selected_chart_sha256()),
            rival: self
                .select_ir
                .rival_for(&self.boot.profile_config.ir, self.selected_chart_sha256()),
        }
    }

    /// 選曲カーソルが曲行のときの chart SHA256。フォルダ / コース行は None。
    fn selected_chart_sha256(&self) -> Option<[u8; 32]> {
        match self.select_items.get(self.selected_index)? {
            SelectItem::Chart(row) => row.score_sha256(),
            _ => None,
        }
    }

    fn ensure_visible_select_chart_distributions(&self, visible_limit: usize) {
        let chart_ids: Vec<i64> = select_visible_item_indices(
            self.select_items.len(),
            self.selected_index,
            visible_limit,
        )
        .into_iter()
        .filter_map(|index| match self.select_items.get(index) {
            Some(SelectItem::Chart(row)) => row.chart.as_ref().map(|chart| chart.chart_id),
            _ => None,
        })
        .collect();
        if chart_ids.is_empty() {
            return;
        }

        let missing_ids: Vec<i64> = {
            let cache = self.select_distribution_cache.borrow();
            chart_ids.iter().copied().filter(|chart_id| !cache.contains_key(chart_id)).collect()
        };
        if !missing_ids.is_empty() {
            match self.boot.library_db.chart_distributions_by_chart_ids(&missing_ids) {
                Ok(distributions) => {
                    let mut cache = self.select_distribution_cache.borrow_mut();
                    for (chart_id, distribution) in distributions {
                        cache.insert(chart_id, distribution);
                    }
                    for chart_id in missing_ids {
                        cache.entry(chart_id).or_default();
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "failed to load visible chart distributions");
                }
            }
        }
        self.select_distribution_cache
            .borrow_mut()
            .retain(|chart_id, _| chart_ids.contains(chart_id));
    }

    /// Returns the string to render in the skin's `STRING_SEARCHWORD` (ref=30)
    /// slot along with an alpha multiplier (0.0..=1.0). beatoraja's libgdx
    /// `TextField` uses `messageFontColor=GRAY` for placeholder; we approximate
    /// that by multiplying skin-resolved alpha by `< 1.0` for placeholder /
    /// feedback states.
    fn display_search_word(&self) -> (String, f32) {
        const PLACEHOLDER_ALPHA: f32 = 0.45;
        const MESSAGE_ALPHA: f32 = 0.6;
        let blink_on = (self.select_time().0 / 500_000) % 2 == 0;
        let caret = if blink_on { "_" } else { " " };
        if self.search_mode {
            if self.search_query.is_empty()
                && self.search_preedit.is_empty()
                && let Some(message) = &self.search_message
            {
                return (message.clone(), MESSAGE_ALPHA);
            }
            (format!("{}{}{}", self.search_query, self.search_preedit, caret), 1.0)
        } else if let Some(message) = &self.search_message {
            (message.clone(), MESSAGE_ALPHA)
        } else {
            ("type / to search song".to_string(), PLACEHOLDER_ALPHA)
        }
    }

    fn poll_select_asset_loads(&mut self) {
        while let Ok(result) = self.select_meta_image_rx.try_recv() {
            let is_current = match result.slot {
                SelectMetaImageSlot::Stage => {
                    self.select_stage_source.as_deref() == Some(result.key.as_str())
                }
                SelectMetaImageSlot::Backbmp => {
                    self.select_backbmp_source.as_deref() == Some(result.key.as_str())
                }
                SelectMetaImageSlot::Banner => {
                    self.select_banner_source.as_deref() == Some(result.key.as_str())
                }
            };
            match result.result {
                Ok(image) => {
                    if is_current {
                        let loaded = self.upload_select_meta_image(result.slot, &image);
                        self.set_select_meta_image_loaded(result.slot, loaded);
                    }
                    self.select_meta_image_cache
                        .insert(result.key, SelectMetaImageCacheEntry::Ready(image));
                }
                Err(error) => {
                    if let Some(path) = result.path {
                        tracing::debug!(path = %path.display(), %error, "skipping select meta image");
                    } else {
                        tracing::debug!(%error, "skipping select meta image");
                    }
                    if is_current {
                        self.set_select_meta_image_loaded(result.slot, false);
                    }
                    self.select_meta_image_cache
                        .insert(result.key, SelectMetaImageCacheEntry::Missing);
                }
            }
        }

        while let Ok(result) = self.select_preview_rx.try_recv() {
            let is_current = self.select_preview_source.as_deref() == Some(result.key.as_str());
            match result.result {
                Ok(sample) => {
                    if is_current {
                        let loaded = self.play_select_preview_sample(sample.clone(), 0.0);
                        self.select_preview_playing = loaded;
                        if loaded {
                            self.begin_select_preview_fade_in();
                        }
                    }
                    self.select_preview_cache
                        .insert(result.key, SelectPreviewCacheEntry::Ready(sample));
                }
                Err(error) => {
                    if let Some(path) = result.path {
                        tracing::debug!(path = %path.display(), %error, "skipping chart preview audio");
                    } else {
                        tracing::debug!(%error, "skipping chart preview audio");
                    }
                    if is_current {
                        self.select_preview_playing = false;
                    }
                    self.select_preview_cache.insert(result.key, SelectPreviewCacheEntry::Missing);
                }
            }
        }
    }

    fn sync_select_preview_audio(&mut self) {
        let selected_cache_key = match self.select_items.get(self.selected_index) {
            Some(SelectItem::Chart(row)) => row.chart.as_ref().and_then(|chart| {
                (!chart.preview_file.is_empty())
                    .then(|| format!("{}|{}", chart.folder_path, chart.preview_file))
            }),
            _ => None,
        };
        let cache_key = select_preview_key_after_delay(
            selected_cache_key,
            self.select_bar_started_at.elapsed(),
            SELECT_PREVIEW_START_DELAY,
        );
        if cache_key.as_deref() == self.select_preview_source.as_deref() {
            if !self.select_preview_playing
                && let Some(key) = cache_key.as_deref()
                && let Some(SelectPreviewCacheEntry::Ready(sample)) =
                    self.select_preview_cache.get(key)
            {
                self.select_preview_playing = self.play_select_preview_sample(sample.clone(), 0.0);
                if self.select_preview_playing {
                    self.begin_select_preview_fade_in();
                }
            }
            return;
        }
        let had_preview = self.select_preview_playing;
        self.select_preview_source = cache_key.clone();

        let mut fading_out = false;
        let loaded = match cache_key.as_deref() {
            Some(_) if self.select_preview.is_none() => false,
            Some(key) => match self.select_preview_cache.get(key) {
                Some(SelectPreviewCacheEntry::Ready(_)) if had_preview => {
                    self.begin_select_preview_fade_out();
                    fading_out = true;
                    false
                }
                Some(SelectPreviewCacheEntry::Ready(sample)) => {
                    let loaded = self.play_select_preview_sample(sample.clone(), 0.0);
                    if loaded {
                        self.begin_select_preview_fade_in();
                    }
                    loaded
                }
                Some(SelectPreviewCacheEntry::Loading) | Some(SelectPreviewCacheEntry::Missing) => {
                    if had_preview {
                        self.begin_select_preview_fade_out();
                        fading_out = true;
                    } else if let Some(preview) = &self.select_preview {
                        preview.stop();
                    }
                    false
                }
                None => {
                    if had_preview {
                        self.begin_select_preview_fade_out();
                        fading_out = true;
                    } else if let Some(preview) = &self.select_preview {
                        preview.stop();
                    }
                    self.spawn_select_preview_load(key.to_string());
                    false
                }
            },
            None => {
                if had_preview {
                    self.begin_select_preview_fade_out();
                    fading_out = true;
                } else if let Some(preview) = &self.select_preview {
                    preview.stop();
                }
                false
            }
        };

        self.select_preview_playing = loaded || fading_out;
    }

    fn stop_select_preview(&mut self) {
        if let Some(preview) = &self.select_preview {
            preview.stop();
        }
        self.select_preview_source = None;
        self.select_preview_playing = false;
        self.select_preview_fade = SelectPreviewFade::Silent;
        self.set_select_bgm_volume_factor(1.0);
    }

    fn sync_select_banner_texture(&mut self) {
        self.sync_select_meta_image_texture(SelectMetaImageSlot::Banner);
    }

    fn sync_select_stage_texture(&mut self) {
        self.sync_select_meta_image_texture(SelectMetaImageSlot::Stage);
    }

    fn sync_select_backbmp_texture(&mut self) {
        self.sync_select_meta_image_texture(SelectMetaImageSlot::Backbmp);
    }

    fn sync_select_meta_image_texture(&mut self, slot: SelectMetaImageSlot) {
        let cache_key = match self.select_items.get(self.selected_index) {
            Some(SelectItem::Chart(row)) => row.chart.as_ref().and_then(|chart| {
                let file = match slot {
                    SelectMetaImageSlot::Stage => &chart.stage_file,
                    SelectMetaImageSlot::Backbmp => &chart.backbmp_file,
                    SelectMetaImageSlot::Banner => &chart.banner_file,
                };
                (!file.is_empty()).then(|| format!("{}|{}", chart.folder_path, file))
            }),
            _ => None,
        };
        if cache_key.as_deref() == self.select_meta_image_source(slot).as_deref() {
            if !self.select_meta_image_loaded(slot)
                && let Some(key) = cache_key.as_deref()
                && let Some(SelectMetaImageCacheEntry::Ready(image)) =
                    self.select_meta_image_cache.get(key)
            {
                let image = image.clone();
                let loaded = self.upload_select_meta_image(slot, &image);
                self.set_select_meta_image_loaded(slot, loaded);
            }
            return;
        }
        self.set_select_meta_image_source(slot, cache_key.clone());
        self.set_select_meta_image_loaded(slot, false);
        self.set_select_meta_image_size(slot, None);
        let Some(key) = cache_key else {
            return;
        };

        match self.select_meta_image_cache.get(&key) {
            Some(SelectMetaImageCacheEntry::Ready(image)) => {
                let image = image.clone();
                let loaded = self.upload_select_meta_image(slot, &image);
                self.set_select_meta_image_loaded(slot, loaded);
            }
            Some(SelectMetaImageCacheEntry::Loading) | Some(SelectMetaImageCacheEntry::Missing) => {
            }
            None => self.spawn_select_meta_image_load(slot, key),
        }
    }

    fn select_meta_image_source(&self, slot: SelectMetaImageSlot) -> &Option<String> {
        match slot {
            SelectMetaImageSlot::Stage => &self.select_stage_source,
            SelectMetaImageSlot::Backbmp => &self.select_backbmp_source,
            SelectMetaImageSlot::Banner => &self.select_banner_source,
        }
    }

    fn set_select_meta_image_source(&mut self, slot: SelectMetaImageSlot, source: Option<String>) {
        match slot {
            SelectMetaImageSlot::Stage => self.select_stage_source = source,
            SelectMetaImageSlot::Backbmp => self.select_backbmp_source = source,
            SelectMetaImageSlot::Banner => self.select_banner_source = source,
        }
    }

    fn select_meta_image_loaded(&self, slot: SelectMetaImageSlot) -> bool {
        match slot {
            SelectMetaImageSlot::Stage => self.select_stage_loaded,
            SelectMetaImageSlot::Backbmp => self.select_backbmp_loaded,
            SelectMetaImageSlot::Banner => self.select_banner_loaded,
        }
    }

    fn set_select_meta_image_loaded(&mut self, slot: SelectMetaImageSlot, loaded: bool) {
        match slot {
            SelectMetaImageSlot::Stage => self.select_stage_loaded = loaded,
            SelectMetaImageSlot::Backbmp => self.select_backbmp_loaded = loaded,
            SelectMetaImageSlot::Banner => self.select_banner_loaded = loaded,
        }
    }

    fn set_select_meta_image_size(
        &mut self,
        slot: SelectMetaImageSlot,
        size: Option<SkinImageSize>,
    ) {
        match slot {
            SelectMetaImageSlot::Stage => self.select_stage_size = size,
            SelectMetaImageSlot::Backbmp => self.select_backbmp_size = size,
            SelectMetaImageSlot::Banner => self.select_banner_size = size,
        }
    }

    fn upload_select_meta_image(
        &mut self,
        slot: SelectMetaImageSlot,
        image: &RgbaImageAsset,
    ) -> bool {
        let texture_id = match slot {
            SelectMetaImageSlot::Stage => SELECT_STAGE_TEXTURE,
            SelectMetaImageSlot::Backbmp => PLAY_BACKBMP_TEXTURE,
            SelectMetaImageSlot::Banner => SELECT_BANNER_TEXTURE,
        };
        if let Err(error) = self.renderer.upsert_image_asset(texture_id, image) {
            tracing::warn!(%error, "failed to upload select meta image");
            self.set_select_meta_image_size(slot, None);
            false
        } else {
            self.set_select_meta_image_size(
                slot,
                Some(SkinImageSize { width: image.width as f32, height: image.height as f32 }),
            );
            true
        }
    }

    fn spawn_select_meta_image_load(&mut self, slot: SelectMetaImageSlot, key: String) {
        self.select_meta_image_cache.insert(key.clone(), SelectMetaImageCacheEntry::Loading);
        let tx = self.select_meta_image_tx.clone();
        thread::spawn(move || {
            let (folder, file) = key.split_once('|').unwrap_or(("", ""));
            let path = crate::chart_asset::resolve_chart_asset_path(folder, file);
            let result = match path.as_ref() {
                Some(path) => load_static_rgba_image(path).map_err(|error| error.to_string()),
                None => Err("select meta image file not found".to_string()),
            };
            let _ = tx.send(SelectMetaImageResult { slot, key, path, result });
        });
    }

    fn select_preview_volume(&self) -> f32 {
        let mix = &self.boot.profile_config.audio_mix;
        let volume = crate::config::play::volume_unit_to_f32(mix.master_volume)
            * crate::config::play::volume_unit_to_f32(mix.preview_volume);
        volume.clamp(0.0, 1.0)
    }

    fn play_select_preview_sample(&self, sample: DecodedSample, volume_factor: f32) -> bool {
        let loaded = self.select_preview.as_ref().is_some_and(|preview| {
            preview
                .play_sample(sample, self.select_preview_volume() * volume_factor.clamp(0.0, 1.0))
        });
        if loaded {
            self.start_audio_output_stream();
        }
        loaded
    }

    fn begin_select_preview_fade_in(&mut self) {
        self.select_preview_fade = SelectPreviewFade::FadingIn { started_at: Instant::now() };
        self.apply_select_preview_audio_mix();
    }

    fn begin_select_preview_fade_out(&mut self) {
        self.select_preview_fade = SelectPreviewFade::FadingOut { started_at: Instant::now() };
        self.apply_select_preview_audio_mix();
    }

    fn update_select_preview_fade(&mut self) {
        let now = Instant::now();
        match self.select_preview_fade {
            SelectPreviewFade::FadingIn { started_at }
                if now.duration_since(started_at) >= SELECT_PREVIEW_FADE_DURATION =>
            {
                self.select_preview_fade = SelectPreviewFade::Playing;
            }
            SelectPreviewFade::FadingOut { started_at }
                if now.duration_since(started_at) >= SELECT_PREVIEW_FADE_DURATION =>
            {
                if let Some(preview) = &self.select_preview {
                    preview.stop();
                }
                self.select_preview_playing = false;
                self.select_preview_fade = SelectPreviewFade::Silent;
            }
            _ => {}
        }
        self.apply_select_preview_audio_mix();
    }

    fn apply_select_preview_audio_mix(&self) {
        let preview_factor = select_preview_fade_factor(self.select_preview_fade, Instant::now());
        if let Some(preview) = &self.select_preview {
            preview.set_volume(self.select_preview_volume() * preview_factor);
        }
        self.set_select_bgm_volume_factor(1.0 - preview_factor);
    }

    fn set_select_bgm_volume_factor(&self, factor: f32) {
        let Some(manager) = &self.system_sound else {
            return;
        };
        let volume = system_sound_volume_from_mix(
            &self.boot.profile_config.audio_mix,
            crate::system_sound::SoundType::Select,
        ) * factor.clamp(0.0, 1.0);
        manager.set_volume(crate::system_sound::SoundType::Select, volume);
    }

    fn spawn_select_preview_load(&mut self, key: String) {
        self.select_preview_cache.insert(key.clone(), SelectPreviewCacheEntry::Loading);
        let tx = self.select_preview_tx.clone();
        thread::spawn(move || {
            let (folder, file) = key.split_once('|').unwrap_or(("", ""));
            let path = crate::chart_asset::resolve_preview_file(Path::new(folder), file);
            let result = match path.as_ref() {
                Some(path) => {
                    let mut loader = FfmpegSampleLoader;
                    loader.load(path).map_err(|error| error.to_string())
                }
                None => Err("chart preview audio file not found".to_string()),
            };
            let _ = tx.send(SelectPreviewResult { key, path, result });
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

    fn restart_select_bar_timer_without_scroll(&mut self, now: Instant) {
        self.select_bar_started_at = now;
        self.select_bar_scroll_direction = 0;
        self.select_bar_scroll_duration = Duration::ZERO;
    }

    fn select_bar_scroll_progress(&self) -> f32 {
        if self.select_bar_scroll_direction == 0 || self.select_bar_scroll_duration.is_zero() {
            return 0.0;
        }
        let elapsed = self.select_bar_started_at.elapsed();
        if elapsed >= self.select_bar_scroll_duration {
            return 0.0;
        }
        1.0 - elapsed.as_secs_f32() / self.select_bar_scroll_duration.as_secs_f32()
    }

    fn select_scroll_duration_low(&self) -> Duration {
        Duration::from_millis(u64::from(select_scroll_duration_low_ms(&self.boot.app_config)))
    }

    fn select_scroll_duration_high(&self) -> Duration {
        Duration::from_millis(u64::from(select_scroll_duration_high_ms(&self.boot.app_config)))
    }

    fn play_elapsed_time(&self) -> TimeUs {
        let micros = self.play_scene_started_at.elapsed().as_micros().min(i64::MAX as u128) as i64;
        TimeUs(micros)
    }

    fn decide_snapshot(&self, decide: &DecideTransition) -> RenderSnapshot {
        let mut snapshot = decide.snapshot.clone();
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
        if in_settings_stack(&self.folder_stack) {
            self.select_option_panel = 0;
            return;
        }
        let panel = select_option_panel_for_holds(self.start_held, self.select_held);
        if self.select_option_panel != panel {
            self.select_option_panel = panel;
            self.option_panel_started_at = Instant::now();
        }
    }

    fn begin_settings_edit(&mut self, entry_id: SettingsEntryId) {
        self.settings_edit =
            Some(SettingsEditSession::capture(&self.boot.profile_config, entry_id));
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
        tracing::info!(?entry_id, "settings edit mode started");
    }

    fn cancel_settings_edit(&mut self) {
        let Some(session) = self.settings_edit.take() else {
            return;
        };
        let entry_id = session.entry_id;
        session.restore(&mut self.boot.profile_config);
        self.sync_select_settings_from_profile_if_needed(entry_id);
        self.play_system_sound(crate::system_sound::SoundType::FolderClose);
        tracing::info!(?entry_id, "settings edit cancelled");
    }

    fn commit_settings_edit(&mut self) {
        let Some(session) = self.settings_edit.take() else {
            return;
        };
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
                session.restore(&mut self.boot.profile_config);
                self.sync_select_settings_from_profile_if_needed(entry_id);
            }
        }
    }

    fn begin_key_config_edit(
        &mut self,
        key_mode: bmz_core::lane::KeyMode,
        target: KeyBindingTarget,
    ) {
        self.key_config_edit =
            Some(KeyConfigEditSession::begin(key_mode, target, &self.boot.profile_config));
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
        tracing::info!(?key_mode, ?target, "key config listen started");
    }

    fn cancel_key_config_edit(&mut self) {
        let Some(session) = self.key_config_edit.take() else {
            return;
        };
        let target = session.target;
        session.cancel(&mut self.boot.profile_config);
        self.suppress_select_analog_until_idle();
        self.play_system_sound(crate::system_sound::SoundType::FolderClose);
        tracing::info!(?target, "key config cancelled");
    }

    fn commit_key_config_edit(&mut self) {
        let Some(session) = self.key_config_edit.take() else {
            return;
        };
        let target = session.target;
        self.suppress_select_analog_until_idle();
        self.boot.profile_config.updated_at = now_unix_seconds();
        match save_profile_config(&self.boot.profile_paths.profile_toml, &self.boot.profile_config)
        {
            Ok(()) => {
                self.select_keys = SelectKeyBindings::from_profile(&self.boot.profile_config.input);
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                tracing::info!(?target, "key config saved");
            }
            Err(error) => {
                tracing::error!(%error, ?target, "failed to save key config");
                session.cancel(&mut self.boot.profile_config);
            }
        }
    }

    fn apply_key_config_control(&mut self, control: &str) {
        let Some(session) = self.key_config_edit.as_ref() else {
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

    fn apply_key_config_gamepad(&mut self, control: &str) {
        let Some(session) = self.key_config_edit.as_ref() else {
            return;
        };
        if !session.listening || session.target.slot() != KeyBindingSlot::Controller {
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

    fn clear_key_config_binding(&mut self) {
        let Some(session) = self.key_config_edit.as_ref() else {
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

    fn adjust_settings_edit(&mut self, direction: i32) {
        if direction == 0 {
            return;
        }
        let Some(session) = self.settings_edit.as_ref() else {
            return;
        };
        let entry_id = session.entry_id;
        let delta = direction * crate::config::settings_registry::settings_adjust_step(entry_id);
        if adjust_settings_draft(&mut self.boot.profile_config, session, delta) {
            self.sync_select_settings_from_profile_if_needed(entry_id);
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
        }
    }

    fn sync_select_settings_from_profile_if_needed(&mut self, entry_id: SettingsEntryId) {
        self.sync_select_play_options_from_profile_if_needed(entry_id);
        if SettingsEntryId::VOLUME_ENTRIES.contains(&entry_id) {
            self.sync_realtime_profile_settings();
        }
    }

    fn sync_select_play_options_from_profile_if_needed(&mut self, entry_id: SettingsEntryId) {
        if !SettingsEntryId::PLAY_ENTRIES.contains(&entry_id) {
            return;
        }
        self.sync_select_play_options_from_profile();
    }

    fn sync_select_play_options_from_profile(&mut self) {
        let play = &self.boot.profile_config.play;
        self.gauge_option = if play.gauge == GaugeTypeConfig::AutoShift {
            GaugeTypeConfig::ExHard
        } else {
            play.gauge
        };
        self.gauge_auto_shift_option = if play.gauge == GaugeTypeConfig::AutoShift {
            GaugeAutoShiftConfig::BestClear
        } else {
            play.gauge_auto_shift
        };
        self.bottom_shiftable_gauge_option = play.bottom_shiftable_gauge;
        self.arrange_option = arrange_option_from_profile(play.random);
        self.target_option = target_option_from_profile(play.target);
        self.assist_option =
            if play.auto_play { AssistOption::Autoplay } else { AssistOption::Normal };
    }

    fn route_settings_control(&mut self, control: &str) -> bool {
        let bindings = SettingsBindings::from_profile(&self.boot.profile_config.input);

        if self.key_config_edit.is_some() {
            if bindings.is_back(control) {
                self.cancel_key_config_edit();
            }
            return true;
        }

        if self.settings_edit.is_some() {
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
        if let Some(select_move) = settings_browse_move_control(control, &bindings) {
            self.move_selection(select_move);
            return true;
        }
        if bindings.is_confirm(control) {
            return match self.select_items.get(self.selected_index) {
                Some(SelectItem::Config(row)) => {
                    self.begin_settings_edit(row.entry_id);
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
                Some(SelectItem::Back) => {
                    self.exit_folder();
                    true
                }
                Some(SelectItem::AdvancedSettings) => {
                    self.open_advanced_settings_from_select();
                    true
                }
                _ => false,
            };
        }
        false
    }

    fn cycle_bga_option(&mut self) {
        self.boot.profile_config.play.bga = cycle_bga_option(self.boot.profile_config.play.bga);
        tracing::info!(
            bga = bga_mode_as_str(self.boot.profile_config.play.bga),
            "bga option changed"
        );
    }

    fn toggle_gauge_auto_shift(&mut self) {
        self.gauge_auto_shift_option = cycle_gauge_auto_shift_option(self.gauge_auto_shift_option);
        tracing::info!(
            gauge_auto_shift = gauge_auto_shift_as_str(self.gauge_auto_shift_option),
            "gauge auto shift changed"
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
        } else if let Some(delta_ms) = visual_offset_delta_control(control, &self.select_keys) {
            self.adjust_visual_offset_ms(delta_ms)
        } else {
            false
        }
    }

    fn adjust_visual_offset_ms(&mut self, delta_ms: i32) -> bool {
        let changed = crate::config::settings_registry::adjust_settings_value(
            &mut self.boot.profile_config,
            SettingsEntryId::VisualOffsetMs,
            delta_ms,
        );
        if changed {
            self.boot.profile_config.updated_at = now_unix_seconds();
            self.sync_realtime_profile_settings();
            tracing::info!(
                visual_offset_ms = self.boot.profile_config.judge.visual_offset_us / 1_000,
                "visual judge offset changed"
            );
        }
        changed
    }

    fn route_keyboard_input(&mut self, event: &winit::event::KeyEvent) {
        if self.active_play.is_some()
            && let Some(control) = physical_key_name(event.physical_key)
            && !event.repeat
            && self.update_play_exit_control_state(&control, event.state == ElementState::Pressed)
        {
            return;
        }
        if let Some(active_play) = &mut self.active_play {
            if let Some(control) = physical_key_name(event.physical_key)
                && self.select_keys.is_start(&control)
                && !event.repeat
            {
                active_play.running.session.lane_cover_changing =
                    event.state == ElementState::Pressed;
                update_pre_ready_play_snapshot_options_for_session(
                    self.play_ready_sound_started_at,
                    &mut self.last_play_snapshot,
                    &active_play.running.session,
                    active_play.running.applied_arrange.arrange,
                );
                update_play_exit_hold_started_at(
                    &mut self.play_exit_hold_started_at,
                    active_play.running.session.lane_cover_changing,
                    self.play_e2_held,
                    Instant::now(),
                );
            }
            if event.state == ElementState::Pressed
                && !event.repeat
                && active_play.running.session.lane_cover_changing
                && let Some(control) = physical_key_name(event.physical_key)
                && let Some(action) = play_option_control(&control, &self.select_keys)
            {
                let speed_locked = self.active_course.as_ref().is_some_and(|c| {
                    c.definition.constraints.speed
                        == bmz_core::course::CourseSpeedConstraint::NoSpeed
                });
                if apply_play_option_control_to_session(
                    &mut active_play.running.session,
                    action,
                    speed_locked,
                ) {
                    tracing::info!(
                        hispeed = active_play.running.session.hispeed,
                        hispeed_mode = ?active_play.running.session.hispeed_mode,
                        target_green_number = active_play.running.session.target_green_number,
                        lane_cover = active_play.running.session.lane_cover,
                        "adjusted play option"
                    );
                } else {
                    tracing::debug!("play option change ignored: course NoSpeed constraint");
                }
                self.update_pre_ready_play_snapshot_options();
                return;
            }
            if let Some(change) = hispeed_action(event.physical_key, event.state, event.repeat) {
                // Beatoraja: NoSpeed constraint locks the hispeed during course play.
                let speed_locked = self.active_course.as_ref().is_some_and(|c| {
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
                self.update_pre_ready_play_snapshot_options();
                return;
            }
            if event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                && event.state == ElementState::Pressed
                && !event.repeat
            {
                self.stop_active_play_like_escape("escape pressed during play");
                return;
            }
            if let Some(delta) = lane_cover_step(event.physical_key, event.state, event.repeat) {
                let session = &mut active_play.running.session;
                let speed_locked = self.active_course.as_ref().is_some_and(|c| {
                    c.definition.constraints.speed
                        == bmz_core::course::CourseSpeedConstraint::NoSpeed
                });
                apply_lane_cover_step_to_session(session, delta, speed_locked);
                tracing::info!(
                    lane_cover = session.lane_cover,
                    lift = session.lift,
                    hispeed = session.hispeed,
                    "adjusted lane cover"
                );
                self.update_pre_ready_play_snapshot_options();
                return;
            }
            if event.physical_key == PhysicalKey::Code(KeyCode::KeyH)
                && event.state == ElementState::Pressed
                && !event.repeat
            {
                let session = &mut active_play.running.session;
                session.hsfix_index = (session.hsfix_index + 1) % 5;
                tracing::info!(hsfix_index = session.hsfix_index, "adjusted HSFIX mode");
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
                    let was_visible = session.lane_cover_visible;
                    session.lane_cover_visible = !session.lane_cover_visible;
                    if !was_visible && session.lane_cover_visible {
                        let speed_locked = self.active_course.as_ref().is_some_and(|c| {
                            c.definition.constraints.speed
                                == bmz_core::course::CourseSpeedConstraint::NoSpeed
                        });
                        reset_floating_hispeed_if_enabled(session, speed_locked);
                    }
                    tracing::info!(
                        lane_cover_visible = session.lane_cover_visible,
                        "toggled lane cover visibility",
                    );
                    self.last_play_start_press_at = None;
                    update_pre_ready_play_snapshot_options_for_session(
                        self.play_ready_sound_started_at,
                        &mut self.last_play_snapshot,
                        &active_play.running.session,
                        active_play.running.applied_arrange.arrange,
                    );
                } else {
                    self.last_play_start_press_at = Some(now);
                }
                // Start キーはゲームプレイ入力としても通すのでフォールスルー
            }
            active_play.input.handle_key_event(event);
            return;
        }

        if self.pending_decide.is_some() {
            if let Some(control) = physical_key_name(event.physical_key)
                && !event.repeat
                && self.update_decide_cancel_control_state(
                    &control,
                    event.state == ElementState::Pressed,
                )
            {
                return;
            }
            if let Some(action) = decide_action(event.physical_key, event.state, event.repeat) {
                self.begin_decide_fadeout(matches!(action, DecideAction::Cancel));
            }
            return;
        }

        if self.pending_play_start.is_some() {
            return;
        }

        // コース曲間の中間リザルト: リトライ無効、次の曲へ進むだけ。Key6 の
        // ゲージグラフ切替のみ単曲リザルト同様に許可する。retry を持つ単曲
        // リザルト分岐より先に評価し、R/Key5/Key7 等での誤 retry を防ぐ。
        if self.is_course_intermediate_result() {
            let pressed = event.state == ElementState::Pressed;
            if self.result_exit.is_none()
                && let Some(control) = physical_key_to_control(event.physical_key)
                && self.handle_course_intermediate_control(&control, pressed, event.repeat)
            {
                return;
            }
            if self.result_exit.is_none()
                && self.result_input_ready()
                && result_action(event.physical_key, event.state, event.repeat).is_some()
            {
                // R / Enter / Escape いずれも次の曲へ進むだけ (retry/leave 区別なし)。
                self.begin_result_exit(ResultExitAction::AdvanceCourse);
            }
            return;
        }

        if self.finished_play.is_some() && self.finished_course.is_none() {
            let pressed = event.state == ElementState::Pressed;
            if let Some(control) = physical_key_to_control(event.physical_key) {
                // フェードアウト中でも Key5/Key7 の押下状態は追跡し、
                // アニメーション終了時の retry arrange 判定に使う。
                self.track_result_lane_hold(&control, pressed);
                // 終了アニメーション中 (result_exit=Some) は held 追跡のみで、
                // 新しいアクションは受け付けない。
                if self.result_exit.is_none()
                    && self.handle_result_control(&control, pressed, event.repeat)
                {
                    return;
                }
            }
            if self.result_exit.is_none()
                && self.result_input_ready()
                && let Some(action) = result_action(event.physical_key, event.state, event.repeat)
            {
                match action {
                    ResultAction::Retry => self
                        .begin_result_exit(ResultExitAction::Retry(ResultRetryMode::SameArrange)),
                    ResultAction::Leave => self.begin_result_exit(ResultExitAction::Leave),
                }
            }
            return;
        }

        // コース（段位）リザルト: コース全体を同配置で再プレイ、または選曲へ戻る。
        // 同配置リトライ: R / Key5 / Key7。退出: Enter / Escape / Key1-4。
        if self.finished_course.is_some() {
            if self.result_exit.is_none() && self.result_input_ready() {
                let action = result_action(event.physical_key, event.state, event.repeat);
                let lane = (event.state == ElementState::Pressed && !event.repeat)
                    .then(|| physical_key_to_control(event.physical_key))
                    .flatten()
                    .and_then(|control| self.result_lane_for_control(&control));
                if matches!(action, Some(ResultAction::Retry))
                    || matches!(lane, Some(Lane::Key5 | Lane::Key7))
                {
                    self.begin_result_exit(ResultExitAction::RetryCourseSameArrange);
                } else if matches!(action, Some(ResultAction::Leave))
                    || matches!(lane, Some(Lane::Key1 | Lane::Key2 | Lane::Key3 | Lane::Key4))
                {
                    self.begin_result_exit(ResultExitAction::Leave);
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

        // 検索モード中はテキスト入力を最優先で処理し、通常ナビゲーションは抑制する。
        // モード入りトリガ (`/`) も同じ select 画面チェックの直後に処理する。
        if matches!(self.view_state(), AppViewState::Select) && self.handle_search_key(event) {
            return;
        }

        // Select 画面で ESC 長押し → アプリ終了 (実際の exit は redraw 時にチェック)。
        if event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
            if in_settings_stack(&self.folder_stack)
                && event.state == ElementState::Pressed
                && !event.repeat
            {
                if self.key_config_edit.is_some() {
                    self.cancel_key_config_edit();
                    return;
                }
                if self.settings_edit.is_some() {
                    self.cancel_settings_edit();
                    return;
                }
            }
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

        if in_settings_stack(&self.folder_stack) {
            if self.key_config_edit.is_some()
                && event.state == ElementState::Pressed
                && !event.repeat
            {
                if event.physical_key == PhysicalKey::Code(KeyCode::Delete)
                    || event.physical_key == PhysicalKey::Code(KeyCode::Backspace)
                {
                    self.clear_key_config_binding();
                    return;
                }
                if let Some(control) = physical_key_name(event.physical_key) {
                    if control == "Escape" {
                        self.cancel_key_config_edit();
                    } else if control == "Delete" || control == "Backspace" {
                        self.clear_key_config_binding();
                    } else {
                        self.apply_key_config_control(&control);
                    }
                }
                return;
            }
            if !should_route_settings_key_event(
                event.state,
                event.repeat,
                self.settings_edit.is_some(),
            ) {
                return;
            }
            if let Some(control) = physical_key_name(event.physical_key) {
                self.route_settings_control(&control);
            } else {
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::ArrowUp) => {
                        let _ = self.route_settings_control("ArrowUp");
                    }
                    PhysicalKey::Code(KeyCode::ArrowDown) => {
                        let _ = self.route_settings_control("ArrowDown");
                    }
                    PhysicalKey::Code(KeyCode::ArrowLeft) => {
                        let _ = self.route_settings_control("ArrowLeft");
                    }
                    PhysicalKey::Code(KeyCode::ArrowRight) => {
                        let _ = self.route_settings_control("ArrowRight");
                    }
                    PhysicalKey::Code(KeyCode::Enter) => {
                        let _ = self.route_settings_control("Enter");
                    }
                    PhysicalKey::Code(KeyCode::Space) => {
                        let _ = self.route_settings_control("Space");
                    }
                    PhysicalKey::Code(KeyCode::Escape) => {
                        let _ = self.route_settings_control("Escape");
                    }
                    _ => {}
                }
            }
            return;
        }

        if is_select_start_key(event.physical_key, &self.select_keys) {
            self.set_start_held(event.state == ElementState::Pressed);
            return;
        }

        if event.state == ElementState::Pressed
            && !event.repeat
            && let Some(control) = physical_key_name(event.physical_key)
            && should_toggle_select_gauge_auto_shift(
                &control,
                self.start_held,
                self.select_held,
                &self.select_keys,
            )
        {
            self.toggle_gauge_auto_shift();
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            if is_select_modifier_key(event.physical_key, &self.select_keys) {
                self.set_select_held(true);
            }
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

        if matches!(self.view_state(), AppViewState::Select) {
            if event.state == ElementState::Pressed && !event.repeat {
                if let Some(action) =
                    select_action(event.physical_key, event.state, event.repeat, &self.select_keys)
                {
                    match action {
                        SelectAction::EnterOrPlay => self.enter_or_play_selected(),
                        SelectAction::ExitFolder => self.exit_folder(),
                        SelectAction::Move(select_move) => {
                            self.move_selection(select_move);
                            if matches!(
                                select_move,
                                SelectMove::Previous
                                    | SelectMove::Next
                                    | SelectMove::PagePrevious
                                    | SelectMove::PageNext
                            ) {
                                let control_name = physical_key_name(event.physical_key);
                                self.select_hold_move = Some(select_move);
                                self.select_hold_started_at = Some(Instant::now());
                                self.select_hold_last_trigger_at = Some(Instant::now());
                                self.select_hold_control = control_name;
                            }
                        }
                    }
                }
            } else if event.state == ElementState::Released
                && let Some(control_name) = physical_key_name(event.physical_key)
                && self.select_hold_control.as_ref() == Some(&control_name)
            {
                self.select_hold_move = None;
                self.select_hold_started_at = None;
                self.select_hold_last_trigger_at = None;
                self.select_hold_control = None;
            }
        }
    }

    fn poll_gamepad_events(&mut self) {
        let Some(gilrs) = &mut self.gilrs else { return };
        let output = gilrs.poll();
        for event in &output.buttons {
            let device_event = crate::input::gilrs::to_device_input_event(event);
            if let Some(active_play) = &self.active_play {
                active_play.input.push_shared_event(device_event);
            }
            self.route_gamepad_button(&event.name.clone(), event.pressed);
        }
        for tick in &output.axis_ticks {
            // キーコンフィグ待ち受け中は合成 Press を待たず、生 tick から直接捕捉する。
            // 軸が active のままでも (押しっぱなし扱いで Press が出なくても) 確実に拾える。
            if self.key_config_edit.as_ref().is_some_and(|session| session.listening) {
                let control = format!("{}{}", tick.name, if tick.ticks > 0 { "+" } else { "-" });
                self.apply_key_config_gamepad(&control);
                continue;
            }
            self.accumulate_select_analog_ticks(tick.name, tick.ticks);
        }
    }

    /// 選曲画面のアナログスクラッチ tick を蓄積する。回転量比例スクロール用。
    fn accumulate_select_analog_ticks(&mut self, axis: &str, ticks: i32) {
        if !matches!(self.view_state(), AppViewState::Select)
            || self.active_play.is_some()
            || self.pending_decide.is_some()
            || self.pending_play_start.is_some()
            || self.select_option_panel != 0
            || self.key_config_edit.is_some()
            || self.settings_edit.is_some()
        {
            return;
        }
        let Some(delta) = select_analog_scroll_delta(axis, ticks, &self.select_keys) else {
            return;
        };
        let now = Instant::now();
        // tick が途切れていたら古い端数を捨てる (beatoraja の 200ms tolerance 相当)
        let idle = self.select_analog_last_tick_at.is_none_or(|t| {
            now.duration_since(t) > Duration::from_millis(SELECT_ANALOG_SCROLL_TOLERANCE_MS)
        });
        self.select_analog_last_tick_at = Some(now);
        update_analog_scroll_buffer(
            &mut self.select_analog_scroll_buffer,
            &mut self.select_analog_suppress_until_idle,
            idle,
            delta,
        );
    }

    /// キーコンフィグ確定/キャンセル後、回転中のスクラッチが止まるまで
    /// アナログスクロールを無効化する。
    fn suppress_select_analog_until_idle(&mut self) {
        self.select_analog_suppress_until_idle = true;
        self.select_analog_scroll_buffer = 0;
        self.select_analog_last_tick_at = Some(Instant::now());
    }

    /// 蓄積したアナログ tick を analog_ticks_per_scroll ごとに 1 移動へ変換する。
    /// beatoraja MusicSelectInputProcessor の analogScrollBuffer と同じ仕組み。
    fn advance_select_analog_scroll(&mut self) {
        if !matches!(self.view_state(), AppViewState::Select) {
            self.select_analog_scroll_buffer = 0;
            self.select_analog_last_tick_at = None;
            return;
        }
        let ticks_per_scroll = self.boot.profile_config.input.analog_ticks_per_scroll.max(1) as i32;
        let mov = take_analog_scroll_steps(&mut self.select_analog_scroll_buffer, ticks_per_scroll);
        for _ in 0..mov.abs() {
            self.move_selection_with_duration(
                if mov > 0 { SelectMove::Next } else { SelectMove::Previous },
                select_analog_scroll_duration(mov),
            );
        }
    }

    fn route_gamepad_button(&mut self, button: &str, pressed: bool) {
        if self.active_play.is_some() && self.update_play_exit_control_state(button, pressed) {
            return;
        }
        if let Some(active_play) = &mut self.active_play
            && self.select_keys.is_start(button)
        {
            active_play.running.session.lane_cover_changing = pressed;
            update_pre_ready_play_snapshot_options_for_session(
                self.play_ready_sound_started_at,
                &mut self.last_play_snapshot,
                &active_play.running.session,
                active_play.running.applied_arrange.arrange,
            );
            update_play_exit_hold_started_at(
                &mut self.play_exit_hold_started_at,
                active_play.running.session.lane_cover_changing,
                self.play_e2_held,
                Instant::now(),
            );
        }
        if pressed {
            let speed_locked = self.active_course.as_ref().is_some_and(|c| {
                c.definition.constraints.speed == bmz_core::course::CourseSpeedConstraint::NoSpeed
            });
            if let Some(active_play) = &mut self.active_play
                && active_play.running.session.lane_cover_changing
                && let Some(action) = play_option_control(button, &self.select_keys)
            {
                if apply_play_option_control_to_session(
                    &mut active_play.running.session,
                    action,
                    speed_locked,
                ) {
                    tracing::info!(
                        hispeed = active_play.running.session.hispeed,
                        hispeed_mode = ?active_play.running.session.hispeed_mode,
                        target_green_number = active_play.running.session.target_green_number,
                        lane_cover = active_play.running.session.lane_cover,
                        "adjusted play option"
                    );
                } else {
                    tracing::debug!("play option change ignored: course NoSpeed constraint");
                }
                self.update_pre_ready_play_snapshot_options();
                return;
            }
        }
        if !pressed {
            if in_settings_stack(&self.folder_stack) {
                return;
            }
            if self.select_keys.is_start(button) {
                self.set_start_held(false);
            } else if self.select_keys.is_e2_action(button)
                || matches!(button, "Select" | "DPadLeft")
            {
                self.set_select_held(false);
            }
            return;
        }

        // プレイ中: プレイ入力は push_shared_event で処理済み
        if self.active_play.is_some() {
            return;
        }

        if self.pending_decide.is_some() {
            if self.update_decide_cancel_control_state(button, pressed) {
                return;
            }
            match button {
                "Button1" => self.begin_decide_fadeout(false),
                _ => {
                    if self.select_keys.is_enter(button) {
                        self.begin_decide_fadeout(false);
                    }
                }
            }
            return;
        }

        if self.pending_play_start.is_some() {
            return;
        }

        // コース曲間の中間リザルト: リトライ無効、次の曲へ進むだけ。
        // retry を持つ単曲リザルト分岐より先に評価する。
        if self.is_course_intermediate_result() {
            if self.result_exit.is_none() {
                let control = PhysicalControl::GamepadButton(button.to_string());
                if self.handle_course_intermediate_control(&control, pressed, false) {
                    return;
                }
                if pressed
                    && self.result_input_ready()
                    && matches!(button, "Button1" | "Start" | "Button2" | "Select")
                {
                    self.begin_result_exit(ResultExitAction::AdvanceCourse);
                }
            }
            return;
        }

        // リザルト画面
        if self.finished_play.is_some() && self.finished_course.is_none() {
            let control = PhysicalControl::GamepadButton(button.to_string());
            // フェードアウト中でも Key5/Key7 の押下状態は追跡する。
            self.track_result_lane_hold(&control, pressed);
            // 終了アニメーション中 (result_exit=Some) は held 追跡のみ行う。
            if self.result_exit.is_none() {
                if self.handle_result_control(&control, pressed, false) {
                    return;
                }
                if self.result_input_ready() {
                    match button {
                        "Button1" | "Start" if pressed => self.begin_result_exit(
                            ResultExitAction::Retry(ResultRetryMode::SameArrange),
                        ),
                        "Button2" | "Select" if pressed => {
                            self.begin_result_exit(ResultExitAction::Leave)
                        }
                        _ => {}
                    }
                }
            }
            return;
        }

        // コース（段位）リザルト: コース全体を同配置で再プレイ、または選曲へ戻る。
        // 同配置リトライ: Start/Button1 / Key5 / Key7。退出: Button2/Select / Key1-4。
        if self.finished_course.is_some() {
            if pressed && self.result_exit.is_none() && self.result_input_ready() {
                let control = PhysicalControl::GamepadButton(button.to_string());
                let lane = self.result_lane_for_control(&control);
                if matches!(button, "Button1" | "Start")
                    || matches!(lane, Some(Lane::Key5 | Lane::Key7))
                {
                    self.begin_result_exit(ResultExitAction::RetryCourseSameArrange);
                } else if matches!(button, "Button2" | "Select")
                    || matches!(lane, Some(Lane::Key1 | Lane::Key2 | Lane::Key3 | Lane::Key4))
                {
                    self.begin_result_exit(ResultExitAction::Leave);
                }
            }
            return;
        }

        if in_settings_stack(&self.folder_stack) {
            if self.key_config_edit.as_ref().is_some_and(|session| session.listening) {
                if pressed {
                    self.apply_key_config_gamepad(button);
                }
                return;
            }
            let _ = self.route_settings_control(button);
            return;
        }

        if should_toggle_select_gauge_auto_shift(
            button,
            self.start_held,
            self.select_held,
            &self.select_keys,
        ) {
            self.toggle_gauge_auto_shift();
            self.play_system_sound(crate::system_sound::SoundType::OptionChange);
            if self.select_keys.is_e2_action(button) {
                self.set_select_held(true);
            }
            return;
        }

        if self.select_keys.is_start(button) {
            self.set_start_held(true);
            return;
        }

        if self.select_keys.is_e2_action(button) || matches!(button, "Select" | "DPadLeft") {
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

        if matches!(self.view_state(), AppViewState::Select) {
            // アナログ軸にバインドされたスクラッチは tick 比例スクロール
            // (advance_select_analog_scroll) で処理する。beatoraja の isNonAnalogPressed 相当。
            if button.starts_with("Axis")
                && (self.select_keys.is_scratch_up(button)
                    || self.select_keys.is_scratch_down(button))
            {
                return;
            }
            if pressed {
                let action = match button {
                    "DPadUp" => Some(SelectAction::Move(SelectMove::Previous)),
                    "DPadDown" => Some(SelectAction::Move(SelectMove::Next)),
                    "DPadLeft" | "Select" => Some(SelectAction::ExitFolder),
                    "DPadRight" | "Button1" => Some(SelectAction::EnterOrPlay),
                    _ => {
                        if self.select_keys.is_scratch_up(button) {
                            if self.select_keys.is_scratch_down(button) {
                                Some(SelectAction::Move(SelectMove::Next))
                            } else {
                                Some(SelectAction::Move(SelectMove::Previous))
                            }
                        } else if self.select_keys.is_scratch_down(button) {
                            Some(SelectAction::Move(SelectMove::Next))
                        } else if self.select_keys.is_enter(button) {
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
                        SelectAction::Move(select_move) => {
                            self.move_selection(select_move);
                            if matches!(
                                select_move,
                                SelectMove::Previous
                                    | SelectMove::Next
                                    | SelectMove::PagePrevious
                                    | SelectMove::PageNext
                            ) {
                                self.select_hold_move = Some(select_move);
                                self.select_hold_started_at = Some(Instant::now());
                                self.select_hold_last_trigger_at = Some(Instant::now());
                                self.select_hold_control = Some(button.to_string());
                            }
                        }
                    }
                }
            } else {
                if self.select_hold_control.as_deref() == Some(button) {
                    self.select_hold_move = None;
                    self.select_hold_started_at = None;
                    self.select_hold_last_trigger_at = None;
                    self.select_hold_control = None;
                }
            }
        }
    }

    fn route_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        if let Some(change) = lane_cover_wheel_change(delta)
            && let Some(active_play) = &mut self.active_play
        {
            let speed_locked = self.active_course.as_ref().is_some_and(|c| {
                c.definition.constraints.speed == bmz_core::course::CourseSpeedConstraint::NoSpeed
            });
            let session = &mut active_play.running.session;
            apply_lane_cover_step_to_session(session, lane_cover_change_step(change), speed_locked);
            tracing::info!(
                lane_cover = session.lane_cover,
                lift = session.lift,
                hispeed = session.hispeed,
                "adjusted lane cover from mouse wheel"
            );
            self.update_pre_ready_play_snapshot_options();
            return;
        }
        if !matches!(self.view_state(), AppViewState::Select) {
            return;
        }
        if let Some(select_move) = select_wheel_move(delta) {
            self.move_selection(select_move);
        }
    }

    fn route_mouse_input(&mut self, state: ElementState, button: MouseButton) {
        if !matches!(self.view_state(), AppViewState::Select) {
            self.select_slider_dragging_type = None;
            return;
        }
        if state == ElementState::Released {
            self.select_slider_dragging_type = None;
            return;
        }
        if state != ElementState::Pressed {
            return;
        }
        let Some((x, y)) = self.cursor_position_normalized() else {
            return;
        };
        let snapshot = self.select_snapshot();
        if button == MouseButton::Left
            && let Some(hit) = self.renderer.select_skin_slider_hit(&snapshot, x, y)
        {
            self.select_slider_dragging_type = Some(hit.slider_type);
            self.apply_select_slider_hit(hit);
            return;
        }
        let Some(hit) = self.renderer.select_skin_click_hit(&snapshot, x, y) else {
            return;
        };
        self.handle_select_skin_click(hit, button, x, y);
    }

    fn route_select_slider_drag(&mut self) {
        if self.select_slider_dragging_type.is_none()
            || !matches!(self.view_state(), AppViewState::Select)
        {
            return;
        }
        let Some((x, y)) = self.cursor_position_normalized() else {
            return;
        };
        let snapshot = self.select_snapshot();
        if let Some(hit) = self.renderer.select_skin_slider_hit(&snapshot, x, y) {
            self.apply_select_slider_hit(hit);
        }
    }

    fn cursor_position_normalized(&self) -> Option<(f32, f32)> {
        let window = self.window.as_ref()?;
        let position = self.last_cursor_position?;
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return None;
        }
        Some((
            (position.x as f32 / size.width as f32).clamp(0.0, 1.0),
            (position.y as f32 / size.height as f32).clamp(0.0, 1.0),
        ))
    }

    fn apply_select_slider_hit(&mut self, hit: SkinSliderHit) {
        match hit.slider_type {
            1 => self.apply_select_scroll_slider(hit.value),
            17..=19 => {
                let value = volume_f32_to_unit(hit.value);
                let mix = &mut self.boot.profile_config.audio_mix;
                match hit.slider_type {
                    17 if mix.master_volume != value => {
                        mix.master_volume = value;
                        self.sync_realtime_profile_settings();
                        tracing::info!(value, "select skin master volume changed");
                    }
                    18 if mix.key_volume != value => {
                        mix.key_volume = value;
                        self.sync_realtime_profile_settings();
                        tracing::info!(value, "select skin key volume changed");
                    }
                    19 if mix.bgm_volume != value => {
                        mix.bgm_volume = value;
                        self.sync_realtime_profile_settings();
                        tracing::info!(value, "select skin bgm volume changed");
                    }
                    _ => {}
                }
            }
            _ => {
                tracing::debug!(slider_type = hit.slider_type, "unsupported select skin slider");
            }
        }
    }

    fn apply_select_scroll_slider(&mut self, value: f32) {
        let Some(next) = select_scroll_slider_index(value, self.select_items.len()) else {
            return;
        };
        if self.selected_index != next {
            self.selected_index = next;
            self.restart_select_bar_timer_without_scroll(Instant::now());
            self.play_system_sound(crate::system_sound::SoundType::Scratch);
        }
    }

    fn handle_select_skin_click(&mut self, hit: SkinClickHit, button: MouseButton, x: f32, y: f32) {
        match hit.target {
            SkinClickTarget::SelectRow { row_index } => {
                self.handle_select_row_click(row_index, button);
            }
            SkinClickTarget::Event { event_id, click } => {
                let Some(arg) = select_click_event_arg(click, button, hit.rect, x, y) else {
                    return;
                };
                self.execute_select_skin_event(event_id, arg);
            }
        }
    }

    fn handle_select_row_click(&mut self, row_index: u32, button: MouseButton) {
        match select_row_click_action(
            row_index,
            button,
            self.selected_index,
            self.select_items.len(),
        ) {
            Some(SelectRowClickAction::Select(next)) => {
                self.selected_index = next;
                self.restart_select_bar_timer_without_scroll(Instant::now());
                self.play_system_sound(crate::system_sound::SoundType::Scratch);
            }
            Some(SelectRowClickAction::EnterOrPlay) => self.enter_or_play_selected(),
            Some(SelectRowClickAction::ExitFolder) => self.exit_folder(),
            None => {}
        }
    }

    fn execute_select_skin_event(&mut self, event_id: i32, arg: i32) {
        match event_id {
            // beatoraja EventFactory: play / autoplay / practice.
            15 => {
                self.set_assist_option(AssistOption::Normal);
                self.enter_or_play_selected();
            }
            16 => {
                self.set_assist_option(AssistOption::Autoplay);
                self.enter_or_play_selected();
            }
            315 => {
                if let Some(chart_id) = self.currently_selected_chart_id() {
                    self.enter_practice(chart_id, PracticeCliOverrides::default());
                }
            }
            19 | 316 | 317 | 318 => {
                let slot = match event_id {
                    19 => 0,
                    316 => 1,
                    317 => 2,
                    318 => 3,
                    _ => unreachable!(),
                };
                if !self.start_replay_for_selected(slot) {
                    tracing::info!(slot, "select skin replay click ignored; slot is empty");
                }
            }
            11 => self.cycle_select_mode_filter(arg),
            12 => self.cycle_select_sort(arg),
            40 => self.cycle_select_gauge(arg),
            42 | 43 | 54 => self.cycle_select_arrange(arg),
            72 => self.cycle_select_bga(arg),
            73 => self.cycle_select_bga_expand(arg),
            77 => self.cycle_select_target(arg),
            78 => self.cycle_select_gauge_auto_shift(arg),
            341 => self.cycle_select_bottom_shiftable_gauge(arg),
            308 => self.cycle_select_ln_mode(arg),
            312 => {
                // BMZ only exposes beatoraja's default sorter set for now.
                self.cycle_select_sort(arg);
            }
            _ => {
                tracing::debug!(event_id, arg, "unsupported select skin event");
            }
        }
    }

    fn cycle_select_mode_filter(&mut self, arg: i32) {
        self.select_mode_filter = if arg >= 0 {
            self.select_mode_filter.next()
        } else {
            self.select_mode_filter.previous()
        };
        // reload_select_items 内で beatoraja 準拠の自動送りと profile config への
        // 永続化（退出 / プレイ後の save_current_play_options 用）を行う。
        let previous_len = self.select_items.len();
        self.reload_select_items();
        tracing::info!(
            mode = self.select_mode_filter.as_str(),
            previous_len,
            current_len = self.select_items.len(),
            "select mode filter changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    fn cycle_select_gauge(&mut self, arg: i32) {
        self.gauge_option = cycle_gauge_option_with_direction(self.gauge_option, arg);
        tracing::info!(gauge = ?self.gauge_option, "gauge option changed");
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    fn cycle_select_arrange(&mut self, arg: i32) {
        self.arrange_option = cycle_arrange_option_with_direction(self.arrange_option, arg);
        tracing::info!(arrange = self.arrange_option.as_str(), "arrange option changed");
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    fn cycle_select_bga(&mut self, arg: i32) {
        self.boot.profile_config.play.bga =
            cycle_bga_option_with_direction(self.boot.profile_config.play.bga, arg);
        tracing::info!(
            bga = bga_mode_as_str(self.boot.profile_config.play.bga),
            "bga option changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    fn cycle_select_bga_expand(&mut self, arg: i32) {
        self.boot.profile_config.play.bga_expand =
            cycle_bga_expand_with_direction(self.boot.profile_config.play.bga_expand, arg);
        tracing::info!(
            bga_expand = ?self.boot.profile_config.play.bga_expand,
            "bga expand changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    fn cycle_select_target(&mut self, arg: i32) {
        let cycle = if arg >= 0 { TargetCycle::Next } else { TargetCycle::Previous };
        self.apply_target_option_cycle(cycle);
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    fn cycle_select_gauge_auto_shift(&mut self, arg: i32) {
        self.gauge_auto_shift_option =
            cycle_gauge_auto_shift_option_with_direction(self.gauge_auto_shift_option, arg);
        tracing::info!(
            gauge_auto_shift = gauge_auto_shift_as_str(self.gauge_auto_shift_option),
            "gauge auto shift changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    fn cycle_select_bottom_shiftable_gauge(&mut self, arg: i32) {
        self.bottom_shiftable_gauge_option =
            cycle_bottom_shiftable_gauge_with_direction(self.bottom_shiftable_gauge_option, arg);
        tracing::info!(
            bottom_shiftable_gauge =
                bottom_shiftable_gauge_as_str(self.bottom_shiftable_gauge_option),
            "bottom shiftable gauge changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    fn cycle_select_sort(&mut self, arg: i32) {
        self.select_sort =
            if arg >= 0 { self.select_sort.next() } else { self.select_sort.previous() };
        // 退出 / プレイ後の save_current_play_options で永続化されるよう、
        // profile config をメモリ上で先に更新しておく。
        self.boot.profile_config.select.sort = self.select_sort.as_str().to_string();
        self.reload_select_items();
        tracing::info!(sort = self.select_sort.as_str(), "select sort changed");
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    fn cycle_select_ln_mode(&mut self, arg: i32) {
        self.boot.profile_config.play.ln_mode_policy = if arg >= 0 {
            self.boot.profile_config.play.ln_mode_policy.next()
        } else {
            self.boot.profile_config.play.ln_mode_policy.previous()
        };
        self.reload_select_items();
        self.invalidate_play_preload();
        tracing::info!(
            ln_mode = self.boot.profile_config.play.ln_mode_policy.display_label(),
            "select LN mode policy changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
    }

    fn move_selection(&mut self, select_move: SelectMove) {
        self.move_selection_with_duration(select_move, self.select_scroll_duration_low());
    }

    fn move_selection_with_duration(&mut self, select_move: SelectMove, duration: Duration) {
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
            self.select_bar_scroll_direction = select_move_scroll_direction(select_move);
            self.select_bar_scroll_duration = duration;
            self.play_system_sound(crate::system_sound::SoundType::Scratch);
        }
    }

    fn advance_select_hold_move(&mut self) {
        if !matches!(self.view_state(), AppViewState::Select) {
            self.select_hold_move = None;
            self.select_hold_started_at = None;
            self.select_hold_last_trigger_at = None;
            self.select_hold_control = None;
            return;
        }
        let (Some(select_move), Some(started_at), Some(last_trigger_at)) =
            (self.select_hold_move, self.select_hold_started_at, self.select_hold_last_trigger_at)
        else {
            return;
        };
        let now = Instant::now();
        let elapsed = now.duration_since(started_at);
        if elapsed < self.select_scroll_duration_low() {
            return;
        }
        let since_last = now.duration_since(last_trigger_at);
        if since_last >= self.select_scroll_duration_high() {
            self.select_hold_last_trigger_at = Some(now);
            self.move_selection_with_duration(select_move, self.select_scroll_duration_high());
        }
    }

    fn open_advanced_settings_from_select(&mut self) {
        if let Some(egui) = self.egui.as_mut() {
            egui.open_advanced_settings();
        }
        self.play_system_sound(crate::system_sound::SoundType::FolderOpen);
        tracing::info!("opened egui advanced settings from select");
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
                self.restart_select_bar_timer_without_scroll(Instant::now());
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
                if row.exists_all_songs() {
                    self.start_course(row.course_id);
                } else {
                    tracing::info!(
                        course_id = row.course_id,
                        title = %row.title,
                        resolved = row.resolved_count,
                        total = row.entry_count,
                        "skipping play for course missing entries"
                    );
                }
            }
            Some(SelectItem::Config(_)) => {}
            Some(SelectItem::KeyBinding(row)) => {
                self.begin_key_config_edit(row.key_mode, row.target);
            }
            Some(SelectItem::Back) => {
                self.exit_folder();
            }
            Some(SelectItem::AdvancedSettings) => {
                self.open_advanced_settings_from_select();
            }
            None => {
                tracing::warn!("no item is available to select");
            }
        }
    }

    /// Returns true when the event was consumed by the search input layer
    /// (either because the user is in search mode or pressed the search-toggle
    /// hotkey), which suppresses normal m-select navigation for this event.
    /// Applies a winit IME event (Preedit / Commit / Enabled / Disabled) to the
    /// search query state. Only acts while the user is in search mode on the
    /// select screen — IME events received otherwise are ignored.
    fn route_ime_event(&mut self, ime: &winit::event::Ime) {
        if !matches!(self.view_state(), AppViewState::Select) || !self.search_mode {
            return;
        }
        use winit::event::Ime;
        match ime {
            Ime::Enabled | Ime::Disabled => {
                self.search_preedit.clear();
            }
            Ime::Preedit(text, _cursor) => {
                self.search_preedit = text.clone();
            }
            Ime::Commit(text) => {
                self.search_query.push_str(text);
                self.search_preedit.clear();
                self.search_message = None;
            }
        }
    }

    /// Toggles search mode and synchronizes IME enablement on the window.
    /// IME is only enabled while search mode is active to avoid macOS / Linux
    /// IMEs swallowing gameplay keypresses.
    fn set_search_mode(&mut self, enabled: bool) {
        self.search_mode = enabled;
        self.search_query.clear();
        self.search_preedit.clear();
        if !enabled {
            self.search_message = None;
        }
        if let Some(window) = self.window.as_ref() {
            window.set_ime_allowed(enabled);
        }
        if enabled {
            self.update_search_ime_cursor_area();
        }
    }

    /// Positions the OS IME candidate window over the search input region of
    /// the active select skin (beatoraja `STRING_SEARCHWORD`, ref=30). No-op
    /// when not in search mode or when the skin does not define such a text
    /// element. Pixel coords are derived from the current window size and the
    /// skin canvas; letterboxing is approximated by direct proportional scale,
    /// which is close enough for IME candidate positioning.
    fn update_search_ime_cursor_area(&self) {
        if !self.search_mode {
            return;
        }
        let Some(window) = self.window.as_ref() else { return };
        let Some(document) = self.renderer.select_skin_document() else { return };
        let Some((x_norm, y_norm, w_norm, h_norm)) = document.text_destination_rect_for_ref(30)
        else {
            return;
        };
        // egui_winit と同じ規約で物理ピクセル top-left を渡す。winit 側で各
        // バックエンドの座標系 (macOS は内部で `to_logical`) に変換される。
        let size = window.inner_size();
        let width = size.width as f32;
        let height = size.height as f32;
        let x = (x_norm * width).round() as i32;
        let y = (y_norm * height).round() as i32;
        let w = (w_norm * width).round().max(1.0) as u32;
        let h = (h_norm * height).round().max(1.0) as u32;
        window.set_ime_cursor_area(
            winit::dpi::PhysicalPosition::new(x, y),
            winit::dpi::PhysicalSize::new(w, h),
        );
    }

    fn handle_search_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        // 起動トリガ: 検索モードでない時に `/` 押下 → モード ON、クエリリセット。
        if !self.search_mode {
            if event.physical_key == PhysicalKey::Code(KeyCode::Slash)
                && event.state == ElementState::Pressed
                && !event.repeat
            {
                self.set_search_mode(true);
                tracing::info!("entered song search mode");
                return true;
            }
            return false;
        }

        // 以下、検索モード中の処理。Release は無視する (Press / Repeat のみ反応)。
        if event.state != ElementState::Pressed {
            return true;
        }

        match event.physical_key {
            PhysicalKey::Code(KeyCode::Escape) => {
                self.set_search_mode(false);
                tracing::info!("exited song search mode");
            }
            PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => {
                if event.repeat {
                    return true;
                }
                // IME 変換中の Enter は確定キー (IME が処理) なので検索実行しない。
                if !self.search_preedit.is_empty() {
                    return true;
                }
                self.execute_song_search();
            }
            PhysicalKey::Code(KeyCode::Backspace) => {
                // IME 変換中の Backspace は IME に渡す (preedit の文字削除)。
                if !self.search_preedit.is_empty() {
                    return true;
                }
                self.search_query.pop();
            }
            _ => {
                // テキスト入力: winit が解決した text (キーレイアウト適用後) を採用。
                // 制御文字 (\r, \t, \x08 等) は除外する。IME 入力は WindowEvent::Ime
                // 経由で別途必要だが v1 では未対応。
                if let Some(text) = event.text.as_ref() {
                    // メッセージ表示中 ("no song found" 等) に `/` (検索モード
                    // 起動キー) を押した場合は、メッセージのみクリアして文字
                    // としては入力しない。`/` 連打でモード再起動感を出すため。
                    if self.search_message.is_some()
                        && self.search_query.is_empty()
                        && text.as_str() == "/"
                    {
                        self.search_message = None;
                        return true;
                    }
                    for ch in text.chars() {
                        if !ch.is_control() {
                            self.search_query.push(ch);
                            self.search_message = None;
                        }
                    }
                }
            }
        }
        true
    }

    /// Runs the current `search_query` against the library DB. On hit: appends
    /// to history (dedupe + bounded), pushes a virtual folder onto the stack,
    /// and exits search mode. On miss: leaves the query intact and updates the
    /// feedback message.
    fn execute_song_search(&mut self) {
        let query = self.search_query.trim().to_string();
        if query.is_empty() {
            return;
        }
        let hit_count = match self.boot.library_db.search_charts(&query) {
            Ok(charts) => charts.len(),
            Err(error) => {
                tracing::error!(%error, %query, "song search failed");
                0
            }
        };
        if hit_count == 0 {
            // クエリをクリアして次入力を待つ。display_search_word はクエリ空 +
            // メッセージ有りの組み合わせで "no song found" を流す。
            self.search_query.clear();
            self.search_message = Some("no song found".to_string());
            tracing::info!(%query, "song search returned no results");
            return;
        }

        // dedupe + FIFO eviction
        self.search_history.retain(|existing| existing != &query);
        while self.search_history.len() >= MAX_SEARCH_HISTORY {
            self.search_history.pop_front();
        }
        self.search_history.push_back(query.clone());

        self.set_search_mode(false);
        self.search_message = Some(format!("{hit_count} song(s) found"));

        // 検索結果フォルダへ入る。`enter_or_play_selected` と同じ流儀でカーソル
        // 位置を退避してから push する。
        self.selected_index_stack.push(self.selected_index);
        self.folder_stack.push(format!("{SEARCH_PATH_PREFIX}{query}"));
        self.reload_select_items();
        self.selected_index = 0;
        self.restart_select_bar_timer_without_scroll(Instant::now());
        self.play_system_sound(crate::system_sound::SoundType::FolderOpen);
        tracing::info!(%query, hit_count, "entered search result folder");
    }

    fn exit_folder(&mut self) {
        if self.key_config_edit.is_some() {
            self.cancel_key_config_edit();
        }
        if self.settings_edit.is_some() {
            self.cancel_settings_edit();
        }
        if self.folder_stack.pop().is_some() {
            let restored = self.selected_index_stack.pop().unwrap_or(0);
            self.reload_select_items();
            // 復元先がリスト範囲外なら末尾にクランプする。
            self.selected_index = restored.min(self.select_items.len().saturating_sub(1));
            self.restart_select_bar_timer_without_scroll(Instant::now());
            self.play_system_sound(crate::system_sound::SoundType::FolderClose);
            tracing::info!(depth = self.folder_stack.len(), "exited folder");
        }
    }

    /// Returns the beatoraja-compatible course stage marker for the currently
    /// playing chart in the active course (1, 2, 3, 4 or Final).  None when no
    /// course is active.
    ///
    /// The final entry always maps to `Final` (OPTION_COURSE_STAGE_FINAL=289);
    /// earlier entries map to Stage1..4 by their 1-based index, clamped to
    /// Stage4 for courses longer than 4 + final entry.
    fn current_course_stage_marker(&self) -> Option<CourseStageMarker> {
        let course = self.active_course.as_ref()?;
        let total = course.definition.entries.len();
        if total == 0 {
            return None;
        }
        let index = course.current_index.min(total - 1);
        let is_final = index + 1 == total;
        if is_final {
            return Some(CourseStageMarker::Final);
        }
        Some(match index {
            0 => CourseStageMarker::Stage1,
            1 => CourseStageMarker::Stage2,
            2 => CourseStageMarker::Stage3,
            _ => CourseStageMarker::Stage4,
        })
    }

    fn current_course_titles(&self) -> [String; 10] {
        let Some(course) = self.active_course.as_ref() else {
            return Default::default();
        };
        course_titles_from_entries(
            course
                .definition
                .entries
                .iter()
                .map(|entry| (entry.title_hint.as_str(), entry.chart_id.is_some())),
        )
    }

    fn apply_course_skin_context(&self, snapshot: &mut RenderSnapshot) {
        snapshot.course_stage = self.current_course_stage_marker();
        snapshot.course_titles = self.current_course_titles();
    }

    fn start_chart(&mut self, chart_id: i64) {
        let options = self.play_start_options();
        self.begin_decide_for_chart(chart_id, options);
    }

    fn enter_practice(&mut self, chart_id: i64, cli: PracticeCliOverrides) {
        let defaults = match self.load_practice_defaults_for_chart(chart_id, &cli) {
            Ok(defaults) => defaults,
            Err(error) => {
                tracing::error!(%error, chart_id, "failed to load practice configuration");
                return;
            }
        };
        let max_end_time_ms = defaults.property.end_time_ms;
        self.practice_session = Some(PracticeSession {
            chart_id,
            chart_title: defaults.title,
            chart_sha256: defaults.sha256,
            property: defaults.property,
            phase: PracticePhase::Config,
            max_end_time_ms,
        });
        self.finished_play = None;
        self.play_ending = None;
        self.result_exit = None;
        self.clear_active_course_state();

        let preload_options = PlayStartOptions {
            autoplay: false,
            practice_mode: false,
            arrange: ArrangeOption::Normal,
            ..Default::default()
        };
        self.start_play_preload(chart_id, preload_options.clone());
        self.enter_play_scene(chart_id, preload_options, self.decide_snapshot_for_chart(chart_id));
        tracing::info!(chart_id, "practice configuration screen ready");
    }

    fn load_practice_defaults_for_chart(
        &self,
        chart_id: i64,
        cli: &PracticeCliOverrides,
    ) -> Result<PracticeChartDefaults> {
        let Some(path) = self.boot.library_db.primary_chart_file_path(chart_id)? else {
            anyhow::bail!("chart file not found for chart id {chart_id}");
        };
        let import = bmz_chart::import::import_bms_chart(Path::new(&path), None, true)
            .with_context(|| format!("import chart for practice defaults: {path}"))?;
        let property = load_practice_property(
            &self.boot.profile_paths,
            &import.chart.identity.file_sha256,
            &import.chart,
            self.gauge_option,
            cli,
        )?;
        let title = if import.chart.metadata.title.is_empty() {
            format!("chart {chart_id}")
        } else {
            import.chart.metadata.title.clone()
        };
        Ok(PracticeChartDefaults { property, title, sha256: import.chart.identity.file_sha256 })
    }

    fn practice_media_ready(&self) -> bool {
        self.practice_session.is_some()
            && self.preloaded_play_session.is_some()
            && self.pending_play_preload.is_none()
    }

    fn leave_practice(&mut self) {
        if let Some(practice) = &self.practice_session {
            let _ = save_practice_property(
                &self.boot.profile_paths,
                &practice.chart_sha256,
                &practice.property,
            );
        }
        self.practice_session = None;
        self.practice_chart_zero_time = None;
        self.active_play = None;
        self.pending_play_start = None;
        self.preloaded_play_session = None;
        self.invalidate_play_preload();
        self.play_ending = None;
        self.finished_play = None;
        self.play_ready_sound_started_at = None;
        self.draining_audio = None;
        self.clear_play_backbmp_state();
        self.last_play_snapshot = None;
        self.reload_select_items();
        let now = Instant::now();
        self.select_scene_started_at = now;
        self.restart_select_bar_timer_without_scroll(now);
        tracing::info!("left practice mode");
    }

    fn start_practice_round(&mut self) {
        if !self.practice_media_ready() {
            tracing::debug!("practice start ignored: media not ready");
            return;
        }
        let (chart_id, property, chart_sha256) = {
            let Some(practice) = &mut self.practice_session else {
                return;
            };
            if let Some(preloaded) = &self.preloaded_play_session {
                clamp_practice_property(&mut practice.property, &preloaded.preloaded.chart);
                practice.max_end_time_ms =
                    crate::screens::practice::default_end_time_ms(&preloaded.preloaded.chart);
            }
            (practice.chart_id, practice.property.clone(), practice.chart_sha256)
        };
        if let Err(error) =
            save_practice_property(&self.boot.profile_paths, &chart_sha256, &property)
        {
            tracing::warn!(%error, "failed to save practice property");
        }
        self.practice_chart_zero_time =
            Some(practice_chart_zero_time(&property, self.play_skin_playstart_offset()));
        if let Some(practice) = &mut self.practice_session {
            practice.phase = PracticePhase::Playing;
        }

        let chart_zero = self.practice_chart_zero_time.unwrap_or(TimeUs(0));
        let preloaded = match self.preloaded_play_session.take() {
            Some(preloaded) => preloaded,
            None => {
                tracing::error!(chart_id, "practice start without preloaded session");
                self.practice_chart_zero_time = None;
                if let Some(practice) = &mut self.practice_session {
                    practice.phase = PracticePhase::Config;
                }
                return;
            }
        };

        let app_config = self.play_session_app_config();
        let mut session_options = play_session_options_from_start(
            &app_config,
            PlayStartOptions {
                autoplay: false,
                practice_mode: true,
                gauge: Some(property.gauge),
                gauge_auto_shift: GaugeAutoShiftConfig::Off,
                arrange: property.arrange,
                chart_zero_time: chart_zero,
                ..Default::default()
            },
        );
        session_options.ln_policy_setting = self.boot.profile_config.play.ln_mode_policy;
        let prepared = build_practice_prepared_from_preloaded(
            preloaded.preloaded,
            &self.boot.profile_config,
            &property,
            session_options,
            Box::new(preloaded.input.clone()),
        );
        let prepared_winit = crate::screens::play_start::PreparedWinitPlaySession {
            prepared,
            input: preloaded.input,
        };
        match self.open_prepared_winit_play_session(prepared_winit) {
            Ok(active_play) => {
                self.pending_play_start = None;
                self.install_active_play(active_play);
                tracing::info!(chart_id, "practice round started");
            }
            Err(error) => {
                tracing::error!(%error, chart_id, "failed to open practice play session");
                self.practice_chart_zero_time = None;
                if let Some(practice) = &mut self.practice_session {
                    practice.phase = PracticePhase::Config;
                }
            }
        }
    }

    fn finish_practice_round(&mut self) {
        let (chart_id, chart_sha256, property) = {
            let Some(practice) = &self.practice_session else {
                return;
            };
            (practice.chart_id, practice.chart_sha256, practice.property.clone())
        };
        if let Err(error) =
            save_practice_property(&self.boot.profile_paths, &chart_sha256, &property)
        {
            tracing::warn!(%error, "failed to save practice property after round");
        }
        if let Some(started) = self.active_play.take() {
            self.draining_audio = Some(started.running.audio);
        }
        self.play_ending = None;
        self.finished_play = None;
        self.play_ready_sound_started_at = None;
        self.practice_chart_zero_time = None;
        if let Some(practice) = &mut self.practice_session {
            practice.phase = PracticePhase::Config;
        }

        let preload_options = PlayStartOptions {
            autoplay: false,
            practice_mode: false,
            arrange: ArrangeOption::Normal,
            ..Default::default()
        };
        self.invalidate_play_preload();
        self.start_play_preload(chart_id, preload_options.clone());
        self.pending_play_start = Some(PendingPlayStart { chart_id, options: preload_options });
        tracing::info!(chart_id, "practice round finished; back to configuration");
    }

    fn start_course(&mut self, course_id: i64) {
        self.start_course_with_arrange(course_id, Vec::new());
    }

    /// Start a course in PLAY mode.  When `arrange_overrides` is non-empty, the
    /// recorded per-entry arrange (seed/pattern) is reapplied so the whole
    /// course replays with the same arrangement; entries without an override at
    /// their index get a fresh arrange.  A fresh course start passes an empty
    /// vec.
    fn start_course_with_arrange(
        &mut self,
        course_id: i64,
        arrange_overrides: Vec<AppliedArrange>,
    ) {
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
        if definition.entries.is_empty()
            || definition.entries.iter().any(|entry| entry.chart_id.is_none())
        {
            let resolved =
                definition.entries.iter().filter(|entry| entry.chart_id.is_some()).count();
            tracing::warn!(
                course_id,
                resolved,
                total = definition.entries.len(),
                "course is missing entries"
            );
            return;
        }
        let first_chart_id = definition.entries.first().and_then(|e| e.chart_id);
        let Some(first_chart_id) = first_chart_id else {
            tracing::warn!(course_id, "no resolved chart in course");
            return;
        };
        tracing::info!(
            course_id,
            title = %definition.title,
            same_arrange = !arrange_overrides.is_empty(),
            "starting course"
        );
        let mut options = self.play_start_options();
        apply_course_constraints(&mut options, &definition.constraints);
        // Reapply the first chart's recorded arrange after constraints so the
        // constraint clamp doesn't overwrite it (same ordering as replay).
        if let Some(arrange) = arrange_overrides.first() {
            apply_arrange_override(&mut options, arrange);
        }
        self.active_course = Some(ActiveCourseSession {
            course_id,
            definition,
            current_index: 0,
            entry_results: Vec::new(),
            queued_replays: Vec::new(),
            arrange_overrides,
        });
        self.start_chart_with_options(first_chart_id, options);
    }

    /// Start a course in replay mode, replaying the saved per-chart inputs of
    /// the given `course_score_id`.  Each chart of the course is launched in
    /// sequence with its saved ReplayPlayer attached, so the user can watch
    /// the entire course attempt back to back.
    ///
    /// If `course_score_id` refers to a partial course attempt (e.g. failed
    /// at chart 2 of 4), only the played charts replay; the queue ends there
    /// and the course session naturally finishes the same way the original
    /// attempt did.
    ///
    /// Errors during replay load (missing file, chart re-imported with
    /// different bytes) abort with a logged warning rather than crashing.
    pub fn start_course_replay(&mut self, course_id: i64, course_score_id: i64) {
        let stored = match self.boot.library_db.list_courses() {
            Ok(courses) => courses.into_iter().find(|c| c.id == course_id),
            Err(error) => {
                tracing::error!(
                    %error,
                    course_id,
                    "failed to load courses for start_course_replay"
                );
                return;
            }
        };
        let Some(stored) = stored else {
            tracing::warn!(course_id, "course not found");
            return;
        };

        let entries = match self.boot.library_db.list_course_replays(course_score_id) {
            Ok(rows) => rows,
            Err(error) => {
                tracing::error!(
                    %error,
                    course_id,
                    course_score_id,
                    "failed to list course_replays rows"
                );
                return;
            }
        };
        if entries.is_empty() {
            tracing::warn!(course_id, course_score_id, "no replays saved for this attempt");
            return;
        }

        let entry_tuples: Vec<(i64, i64, String)> =
            entries.iter().map(|r| (r.position, r.chart_id, r.replay_path.clone())).collect();
        let replay_root = self.boot.profile_paths.root_dir.clone();
        let lookup = |chart_id: i64| -> anyhow::Result<Option<[u8; 32]>> {
            self.boot.library_db.chart_sha256_by_chart_id(chart_id)
        };
        let queued = match crate::storage::replay::load_course_replays(
            &entry_tuples,
            &replay_root,
            lookup,
        ) {
            Ok(q) => q,
            Err(error) => {
                tracing::warn!(
                    %error,
                    course_id,
                    course_score_id,
                    "failed to load queued course replays"
                );
                return;
            }
        };

        let definition = stored.definition;
        let first_chart_id = definition.entries.iter().find_map(|e| e.chart_id);
        let Some(first_chart_id) = first_chart_id else {
            tracing::warn!(course_id, "no resolved chart in course");
            return;
        };
        tracing::info!(
            course_id,
            course_score_id,
            title = %definition.title,
            replays = queued.len(),
            "starting course replay"
        );
        let mut options = self.play_start_options();
        apply_course_constraints(&mut options, &definition.constraints);
        // The first chart starts with its queued replay if available.
        if let Some(first) = queued.first()
            && first.chart_id == first_chart_id
        {
            apply_queued_replay(&mut options, first);
        }
        self.active_course = Some(ActiveCourseSession {
            course_id,
            definition,
            current_index: 0,
            entry_results: Vec::new(),
            queued_replays: queued,
            arrange_overrides: Vec::new(),
        });
        self.start_chart_with_options(first_chart_id, options);
    }

    /// コース曲間の中間リザルト状態かどうか。active_course を保持したまま
    /// finished_play だけが立ち、finished_course はまだ無い状態を指す。
    fn is_course_intermediate_result(&self) -> bool {
        is_course_intermediate_result(
            self.active_course.is_some(),
            self.finished_course.is_some(),
            self.finished_play.is_some(),
        )
    }

    /// コース曲間の中間リザルト画面を表示する。直前に終わった曲の結果を
    /// finished_play に入れて Result スキンを出すが、active_course は保持し
    /// finished_course は立てないので「中間リザルト」状態になる。
    fn show_course_intermediate_result(&mut self) {
        let last = self
            .active_course
            .as_ref()
            .and_then(|course| course.entry_results.last())
            .map(|entry| entry.finished.clone());
        let Some(last) = last else {
            // 直前結果が無い異常系では中間リザルトを出さず、次の曲へ進む。
            self.start_next_course_chart();
            return;
        };
        self.result_gauge_graph_type = last.summary.gauge_type as i32;
        self.finished_play = Some(last);
        self.result_exit = None;
        self.result_key5_held = false;
        self.result_key7_held = false;
        self.result_scene_started_at = Instant::now();
        self.ensure_skin_ready(SkinKind::Result);
    }

    /// 中間リザルトを閉じて次の曲へ進む。finished_play をクリアして中間リザルト
    /// 状態を抜け、active_course はそのまま次の曲を開始する。
    fn advance_to_next_course_chart(&mut self) {
        self.finished_play = None;
        self.result_exit = None;
        self.result_key5_held = false;
        self.result_key7_held = false;
        self.start_next_course_chart();
    }

    /// コースの (current_index が指す) 次の曲を開始する。ゲージ持ち越しや
    /// replay / 同配置 arrange の適用は元の advance_course_after_finish と同じ。
    fn start_next_course_chart(&mut self) {
        let Some(course) = &self.active_course else {
            return;
        };
        let next_index = course.current_index;
        let Some(next_chart_id) =
            course.definition.entries.get(next_index).and_then(|e| e.chart_id)
        else {
            return;
        };
        let constraints = course.definition.constraints.clone();
        // Carry the gauge value of the previous chart over to the next chart in
        // the course (beatoraja keeps the gauge between songs).
        let carried_gauge =
            course.entry_results.last().map(|r| r.finished.result.gauge_value).unwrap_or(0.0);
        let mut options = self.play_start_options();
        apply_course_constraints(&mut options, &constraints);
        options.initial_gauge_value = Some(carried_gauge);
        // If the course is being replayed, attach the next queued replay
        // (when it exists and matches the next chart's id).  Mismatches
        // are silently skipped so the chart still plays normally.
        if let Some(course) = &self.active_course
            && let Some(replay) = course.queued_replays.get(next_index)
            && replay.chart_id == next_chart_id
        {
            apply_queued_replay(&mut options, replay);
        } else if let Some(course) = &self.active_course
            && let Some(arrange) = course.arrange_overrides.get(next_index)
        {
            // Same-arrange course retry: reproduce this chart's arrange.
            apply_arrange_override(&mut options, arrange);
        }
        self.start_chart_with_options(next_chart_id, options);
    }

    /// コース中間リザルトのコントロール処理。Key6 はゲージグラフ切替のみ許可し、
    /// それ以外の終了レーン (Key1-4/Key5/Key7) は retry せず次の曲へ進む。
    fn handle_course_intermediate_control(
        &mut self,
        control: &PhysicalControl,
        pressed: bool,
        repeat: bool,
    ) -> bool {
        let Some(lane) = self.result_lane_for_control(control) else {
            return false;
        };
        match lane {
            Lane::Key6 => {
                if pressed && !repeat && self.result_input_ready() {
                    self.cycle_result_gauge_graph_type();
                }
                true
            }
            lane if lane_starts_result_exit(lane) => {
                if pressed && self.result_input_ready() {
                    self.begin_result_exit(ResultExitAction::AdvanceCourse);
                }
                true
            }
            _ => false,
        }
    }

    fn advance_course_after_finish(&mut self, finished: FinishedPlaySession) {
        let Some(course) = &mut self.active_course else {
            return;
        };
        let chart_id = self.last_started_chart_id.unwrap_or(0);
        // Beatoraja behavior: if any chart in the course is Failed, the course
        // ends immediately and remaining charts are skipped.
        let failed = finished.result.clear_type == bmz_core::clear::ClearType::Failed;
        course.entry_results.push(CourseEntryResult { chart_id, finished });
        course.current_index += 1;

        let next_chart_id =
            course.definition.entries.get(course.current_index).and_then(|e| e.chart_id);

        if !failed && next_chart_id.is_some() {
            // 次の曲をすぐ始めず、まず直前の曲の単曲リザルト (中間リザルト) を出す。
            // active_course を保持したまま finished_play に直前結果を入れることで、
            // view_state は Result を返し、入力は中間リザルト分岐へ入る。実際の次曲
            // 開始 (ゲージ持ち越し / replay / 同配置 arrange の適用を含む) は、結果画面
            // を閉じたとき advance_to_next_course_chart まで遅延する。
            self.show_course_intermediate_result();
            return;
        }

        // Course is over either because every entry was played or because the
        // most recent chart was Failed (skip remaining entries).
        let course = self.active_course.take().unwrap();
        let course_id = course.course_id;

        // Extract data needed to persist the course score before `into_result`
        // consumes `entry_results`.
        let chart_records: Vec<crate::storage::library_db::CourseScoreChartRecord> = course
            .entry_results
            .iter()
            .enumerate()
            .map(|(i, r)| crate::storage::library_db::CourseScoreChartRecord {
                position: i as i64,
                chart_id: r.chart_id,
                ex_score: r.finished.result.score.ex_score(),
                max_combo: r.finished.result.score.max_combo,
                clear_type: r.finished.result.clear_type.as_str().to_string(),
                gauge_value: r.finished.result.gauge_value,
            })
            .collect();
        let replay_records: Vec<crate::storage::library_db::CourseReplayRecord> = course
            .entry_results
            .iter()
            .enumerate()
            .map(|(i, r)| crate::storage::library_db::CourseReplayRecord {
                position: i as i64,
                chart_id: r.chart_id,
                replay_path: r.finished.stored.replay_path.clone(),
            })
            .collect();
        let any_autoplay = course.entry_results.iter().any(|r| r.finished.result.autoplay);
        let any_replay_playback = course.entry_results.iter().any(|r| r.finished.replay_playback);
        // Collect score_history row ids written by per-chart store_play_result
        // so they can be tagged with the new course_score_id after insert.
        // Autoplay charts have score_history_id == 0 and are filtered out.
        let history_ids: Vec<i64> = course
            .entry_results
            .iter()
            .map(|r| r.finished.stored.score_history_id)
            .filter(|id| *id > 0)
            .collect();
        let last_finished = course.entry_results.last().map(|r| r.finished.clone());
        let last_clear_type = course
            .entry_results
            .last()
            .map(|r| r.finished.result.clear_type)
            .unwrap_or(bmz_core::clear::ClearType::NoPlay);
        let last_gauge_type = course
            .entry_results
            .last()
            .map(|r| r.finished.result.gauge_type)
            .unwrap_or(bmz_core::clear::GaugeType::Normal);
        let last_gauge_value =
            course.entry_results.last().map(|r| r.finished.result.gauge_value).unwrap_or(0.0);
        let max_combo: u32 = course
            .entry_results
            .iter()
            .map(|r| r.finished.result.score.max_combo)
            .max()
            .unwrap_or(0);
        let course_arrange = course
            .entry_results
            .first()
            .map(|entry| entry.finished.arrange.to_persistent_str().to_string())
            .unwrap_or_else(|| "Normal".to_string());

        let mut course_result = course.into_result();
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
        // Persist course score + per-chart replay paths.
        //
        // - Autoplay / replay playback courses are not saved, matching the
        //   per-chart policy in `finish_session_result`.
        // - The course clear type is taken from the last played chart's
        //   gauge survival result; a Failed at any point forces Failed.
        // - The per-chart replay files have already been written by
        //   `store_play_result` for each chart in the course; we only record
        //   the relative paths here so the course can be replayed back to back
        //   in a future iteration.
        // - TODO(course-replay-reload): launching a course via a "replay slot"
        //   from the select screen is out of scope for this change; only the
        //   save path is wired up.
        if !any_autoplay && !any_replay_playback {
            let final_clear_type = if course_result.course_failed {
                bmz_core::clear::ClearType::Failed
            } else {
                last_clear_type
            };
            let bp = course_result.judge_counts.bad
                + course_result.judge_counts.poor
                + course_result.judge_counts.empty_poor;
            let played_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            // Store the names of trophies that were achieved on this attempt
            // as a JSON array of strings (for round-trip / audit) and
            // separately as structured rows in course_trophy_achievements via
            // CourseScoreInsert.achieved_trophies, which is what powers
            // per-trophy best queries.
            let achieved_trophies: Vec<String> = course_result
                .trophy_results
                .iter()
                .filter(|t| t.achieved)
                .map(|t| t.name.clone())
                .collect();
            let trophies_json =
                serde_json::to_string(&achieved_trophies).unwrap_or_else(|_| "[]".to_string());
            let insert = crate::storage::library_db::CourseScoreInsert {
                course_id,
                ex_score: course_result.total_ex_score,
                max_ex_score: course_result.max_ex_score,
                clear_type: final_clear_type.as_str().to_string(),
                gauge_type: last_gauge_type.as_str().to_string(),
                gauge_value: last_gauge_value,
                max_combo,
                bp,
                course_failed: course_result.course_failed,
                course_clear: course_result.course_clear,
                arrange: course_arrange,
                trophies_json,
                played_at,
                charts: chart_records,
                replays: replay_records,
                achieved_trophies,
            };
            match self.boot.library_db.insert_course_score(&insert) {
                Ok(course_score_id) => {
                    // Backfill the per-chart `score_history` rows with the
                    // course attempt id so they can be filtered as part of
                    // this course play later.
                    if let Err(error) = self
                        .boot
                        .score_db
                        .tag_score_history_with_course(&history_ids, course_score_id)
                    {
                        tracing::warn!(
                            %error,
                            course_id,
                            course_score_id,
                            "failed to tag score_history rows with course_score_id"
                        );
                    }

                    // IR コーススコア送信ジョブを enqueue する (IR 未設定なら no-op)。
                    self.enqueue_ir_course_job(
                        course_id,
                        course_score_id,
                        &course_result,
                        last_finished.as_ref().map(|f| f.stored.device_type),
                        &insert.gauge_type,
                        played_at,
                        &insert.arrange,
                        course_result.entry_arranges.first().and_then(|arrange| arrange.seed),
                    );

                    // Update the four course replay slots that pass their
                    // configured rule.  Reuses the per-chart slot_rule_passes
                    // helper for identical semantics (Always overwrites
                    // unconditionally; Score / Bp / MaxCombo / Clear
                    // require strict improvement; empty slot always wins).
                    course_result.saved_replay_slots = self.update_course_replay_slots(
                        course_id,
                        course_score_id,
                        played_at,
                        course_result.total_ex_score,
                        bp,
                        max_combo,
                        final_clear_type as u8,
                    );
                    course_result.replay_slots =
                        self.boot.library_db.course_replay_slot_presence(course_id).unwrap_or_else(
                            |error| {
                                tracing::warn!(
                                    %error,
                                    course_id,
                                    "failed to read course replay slot presence"
                                );
                                [false; 4]
                            },
                        );
                    for (index, saved) in course_result.saved_replay_slots.iter().enumerate() {
                        if *saved {
                            course_result.replay_slots[index] = true;
                        }
                    }
                }
                Err(error) => {
                    tracing::error!(%error, course_id, "failed to persist course score");
                }
            }

            // Look up the best score *after* the insert above so the just-
            // saved attempt is reflected when it improved the record.  The
            // result overlay reads this to show a "BEST" section.
            course_result.best_score =
                self.boot.library_db.best_course_score(course_id).unwrap_or_else(|error| {
                    tracing::warn!(%error, course_id, "failed to read best course score");
                    None
                });
        }

        self.finished_course = Some(course_result);
        // Use the last chart's result for the standard result skin display.
        if let Some(last) = last_finished {
            self.result_gauge_graph_type = last.summary.gauge_type as i32;
            self.finished_play = Some(last);
            self.result_key5_held = false;
            self.result_key7_held = false;
            self.result_scene_started_at = Instant::now();
            self.ensure_skin_ready(SkinKind::Result);
        }
    }

    /// コース定義から IR 用の identity (charts sha256 + constraints) を解決する。
    /// 未解決の譜面 (sha256 不明) があるコースは IR 送信対象外。
    fn ir_course_definition(
        &self,
        course_id: i64,
    ) -> Option<crate::ir::course_payload::IrCourseDefinition> {
        let stored = self
            .boot
            .library_db
            .list_courses()
            .ok()?
            .into_iter()
            .find(|course| course.id == course_id)?;
        let mut charts = Vec::with_capacity(stored.definition.entries.len());
        for entry in &stored.definition.entries {
            let sha = entry.sha256.clone().or_else(|| {
                let md5 = entry.md5.as_ref()?;
                let md5 = crate::storage::common::hex_to_hash::<16>(md5).ok()?;
                let sha = self.boot.library_db.chart_sha256_by_md5(md5).ok().flatten()?;
                Some(crate::storage::common::hash_to_hex(&sha))
            })?;
            charts.push(sha);
        }
        Some(crate::ir::course_payload::IrCourseDefinition {
            charts,
            constraints: serde_json::to_value(&stored.definition.constraints).ok()?,
            title: stored.definition.title.clone(),
            kind: match stored.definition.kind {
                bmz_core::course::CourseKind::Dan => "dan".to_string(),
                bmz_core::course::CourseKind::Course => "course".to_string(),
            },
        })
    }

    /// コーススコアの IR 送信ジョブを enqueue する。IR 未設定 / 定義未解決なら no-op。
    #[allow(clippy::too_many_arguments)]
    fn enqueue_ir_course_job(
        &mut self,
        course_id: i64,
        course_score_id: i64,
        course_result: &crate::screens::course_session::CourseResultSummary,
        device_type: Option<bmz_core::input::InputDeviceKind>,
        gauge: &str,
        played_at: i64,
        arrange: &str,
        random_seed: Option<i64>,
    ) {
        let enabled: Vec<_> = self
            .boot
            .profile_config
            .ir
            .providers
            .iter()
            .filter(|provider| provider.enabled && !provider.base_url.is_empty())
            .cloned()
            .collect();
        if enabled.is_empty() {
            return;
        }
        let Some(definition) = self.ir_course_definition(course_id) else {
            tracing::info!(course_id, "course has unresolved charts; skipping IR submission");
            return;
        };
        let ln_setting = serde_json::to_value(self.boot.profile_config.play.ln_mode_policy)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| "AutoLn".to_string());
        let payload = crate::ir::course_payload::build_course_submission(
            &definition,
            course_result,
            &crate::ir::course_payload::IrCourseSubmissionContext {
                played_at,
                ln_policy_setting: ln_setting.clone(),
                gauge: gauge.to_string(),
                device_type: device_type.unwrap_or(bmz_core::input::InputDeviceKind::Keyboard),
                arrange: arrange.to_string(),
                random_seed,
                idempotency_key: format!("bmz-course-{course_score_id}"),
            },
        );
        let Ok(payload_json) = serde_json::to_string(&payload) else {
            return;
        };
        let first_chart = definition
            .charts
            .first()
            .and_then(|sha| crate::storage::common::hex_to_hash::<32>(sha).ok())
            .unwrap_or([0; 32]);
        let ln_policy = crate::ln_policy::score_ln_policy(
            self.boot.profile_config.play.ln_mode_policy,
            crate::ln_policy::ChartLnProfile::default(),
        );
        for provider in enabled {
            if let Err(error) =
                self.boot.score_db.enqueue_ir_score_job(&crate::storage::score_db::NewIrScoreJob {
                    provider: provider.provider.clone(),
                    account_id: provider.account_id.clone(),
                    kind: crate::storage::score_db::IrJobKind::Course,
                    local_score_id: course_score_id,
                    chart_sha256: first_chart,
                    ln_policy,
                    payload_json: payload_json.clone(),
                    now: played_at,
                })
            {
                tracing::warn!(provider = provider.provider, %error, "failed to enqueue IR course job");
            }
        }
    }

    fn update_course_replay_slots(
        &mut self,
        course_id: i64,
        course_score_id: i64,
        played_at: i64,
        ex_score: u32,
        bp: u32,
        max_combo: u32,
        clear_rank: u8,
    ) -> [bool; 4] {
        let slot_rules = self.boot.profile_config.replay.slot_rules;
        let candidate = crate::storage::play_result::CandidateMetrics {
            ex_score,
            bp,
            cb: bp,
            max_combo,
            clear_rank,
        };
        let mut saved_slots = [false; 4];
        for (slot_index, &rule) in slot_rules.iter().enumerate() {
            let slot = slot_index as u8;
            let prev = match self.boot.library_db.course_replay_slot(course_id, slot) {
                Ok(record) => record,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        course_id,
                        slot,
                        "failed to read course_replay_slot; skipping rule eval"
                    );
                    continue;
                }
            };
            let prev_metrics = prev.as_ref().map(|p| (p.ex_score, p.bp, p.max_combo, p.clear_rank));
            if !crate::storage::play_result::slot_rule_passes(rule, prev_metrics, &candidate) {
                continue;
            }
            let record = crate::storage::library_db::CourseReplaySlotRecord {
                course_id,
                slot,
                rule: rule.as_str().to_string(),
                course_score_id,
                played_at,
                ex_score,
                bp,
                max_combo,
                clear_rank,
            };
            match self.boot.library_db.upsert_course_replay_slot(&record) {
                Ok(()) => saved_slots[slot_index] = true,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        course_id,
                        slot,
                        "failed to upsert course_replay_slot"
                    );
                }
            }
        }
        saved_slots
    }

    fn begin_decide_for_chart(&mut self, chart_id: i64, options: PlayStartOptions) {
        self.ensure_skin_ready(SkinKind::Decide);
        // Play スキンは裏で decode+upload を進めるが、Decide 入場では待たない。
        // 実際の Play 入場 (`start_chart_with_options`) で `ensure_skin_ready` が保険として残る。
        self.spawn_play_skin_decode_for(self.key_mode_for_chart(chart_id));
        self.start_play_preload(chart_id, options.clone());
        let now = Instant::now();
        self.pending_decide = Some(DecideTransition {
            chart_id,
            options,
            started_at: now,
            fadeout_started_at: None,
            cancel: false,
            snapshot: self.decide_snapshot_for_chart(chart_id),
        });
    }

    fn start_play_preload(&mut self, chart_id: i64, options: PlayStartOptions) {
        self.play_preload_generation = self.play_preload_generation.wrapping_add(1);
        let generation = self.play_preload_generation;
        self.preloaded_play_session = None;
        let (tx, rx) = mpsc::channel();
        let library_db_path = self.boot.app_paths.library_db.clone();
        let app_config = self.play_session_app_config();
        let ln_policy_setting = self.boot.profile_config.play.ln_mode_policy;
        thread::Builder::new()
            .name(format!("play-preload-{chart_id}"))
            .spawn(move || {
                let result = (|| -> Result<PreloadedWinitPlaySession> {
                    let library_db =
                        crate::storage::library_db::LibraryDatabase::open(&library_db_path)?;
                    let input = crate::input::winit::WinitInputBackend::default();
                    let mut session_options =
                        crate::screens::play_start::play_session_options_from_start(
                            &app_config,
                            options,
                        );
                    session_options.ln_policy_setting = ln_policy_setting;
                    let preloaded = crate::screens::play_session::preload_play_session_for_chart(
                        &library_db,
                        chart_id,
                        session_options.clone(),
                    )?;
                    Ok(PreloadedWinitPlaySession { chart_id, preloaded, input, session_options })
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
        // 裏で完成して退避していた結果も無効化する (decide キャンセル / 譜面差し替え)。
        self.preloaded_play_session = None;
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

    fn open_prepared_winit_play_session(
        &self,
        prepared: PreparedWinitPlaySession,
    ) -> Result<StartedWinitPlaySession> {
        let runtime = self.audio_runtime.as_ref().context("audio output is not available")?;
        open_prepared_winit_play_session(&self.boot.score_db, runtime, prepared)
    }

    fn play_output_sample_rate(&self) -> u32 {
        self.audio_runtime
            .as_ref()
            .map(AudioRuntime::sample_rate)
            .unwrap_or(self.boot.app_config.audio.sample_rate)
    }

    fn play_session_app_config(&self) -> AppConfig {
        let mut app_config = self.boot.app_config.clone();
        app_config.audio.sample_rate = self.play_output_sample_rate();
        app_config
    }

    /// ウィンドウと renderer surface の準備後に、初めて共有 cpal ストリームを開く。
    /// 起動ロード中に音声デバイスを start して、デバイス側の初期化音が先に鳴るのを避ける。
    fn ensure_audio_output(&mut self) {
        if self.audio_runtime.is_some() || self.audio_output_open_attempted {
            return;
        }
        self.audio_output_open_attempted = true;

        match AudioRuntime::open(&self.boot.app_config.audio) {
            Ok(runtime) => {
                self.install_system_audio(&runtime, None);
                self.audio_runtime = Some(runtime);
                tracing::info!("audio output opened after window initialization");
            }
            Err(error) => {
                tracing::warn!(%error, "failed to open shared audio output; running without audio");
            }
        }
    }

    fn install_system_audio(
        &mut self,
        runtime: &AudioRuntime,
        system_engine: Option<bmz_audio::backend::cpal::SharedAudioEngine>,
    ) {
        let system_audio = match system_engine {
            Some(engine) => crate::audio::SystemAudio::reattach(runtime, engine),
            None => crate::audio::SystemAudio::open(runtime),
        };

        if self.system_sound.is_none() {
            self.system_sound = Some(system_sound_manager_from_boot(&self.boot, &system_audio));
        }
        if self.select_preview.is_none() {
            self.select_preview = Some(SelectChartPreview::new(system_audio.engine()));
        }
        self.system_audio = Some(system_audio);
    }

    /// 設定パネルの「適用」で、現在の `AppConfig` の音声設定を使って共有 cpal
    /// ストリームを開き直す。ASIO は排他なので新ストリームを開く前に旧ストリームを
    /// 完全に閉じる。プレイ中・プレイ開始待ち中はストリーム差し替えが危険なため何もしない。
    fn reopen_audio_output(&mut self) {
        if self.active_play.is_some() || self.pending_play_start.is_some() {
            tracing::warn!("ignoring audio apply while a play session is active");
            return;
        }

        // SystemSoundManager / SelectChartPreview と共有しているシステムエンジン
        // Arc を保持し、新ストリームへそのまま載せ替える(samples を再ロードしない)。
        let system_engine = self.system_audio.as_ref().map(crate::audio::SystemAudio::engine);

        // 旧ストリームを参照する全ハンドルを drop し、ASIO デバイスを解放する。
        self.draining_audio = None;
        self.system_audio = None;
        self.audio_runtime = None;

        match AudioRuntime::open(&self.boot.app_config.audio) {
            Ok(runtime) => {
                self.install_system_audio(&runtime, system_engine);
                self.audio_runtime = Some(runtime);
                tracing::info!("audio output reopened with current settings");
            }
            Err(error) => {
                tracing::error!(
                    %error,
                    "failed to reopen audio output; audio disabled until restart"
                );
            }
        }
    }

    fn decide_snapshot_for_chart(&self, chart_id: i64) -> RenderSnapshot {
        let mut snapshot = RenderSnapshot::default();
        if let Some(SelectItem::Chart(row)) = self.select_items.iter().find(|item| match item {
            SelectItem::Chart(row) => {
                row.chart.as_ref().is_some_and(|chart| chart.chart_id == chart_id)
            }
            SelectItem::Folder { .. }
            | SelectItem::Course(_)
            | SelectItem::Config(_)
            | SelectItem::KeyBinding(_)
            | SelectItem::Back
            | SelectItem::AdvancedSettings => false,
        }) && let Some(chart) = &row.chart
        {
            snapshot.title = chart.title.clone();
            snapshot.subtitle = chart.subtitle.clone();
            snapshot.artist = chart.artist.clone();
            snapshot.subartist = chart.subartist.clone();
            snapshot.genre = chart.genre.clone();
            snapshot.difficulty_name = chart.difficulty_name.clone();
            snapshot.play_level = chart.play_level.clone();
            snapshot.judge_rank = chart.judge_rank;
            snapshot.total_notes = chart.total_notes;
            snapshot.duration = TimeUs(chart.length_ms.saturating_mul(1_000));
            snapshot.min_bpm = chart.min_bpm as f32;
            snapshot.max_bpm = chart.max_bpm as f32;
            snapshot.now_bpm = chart.initial_bpm as f32;
            // PACEMAKER の MyBest 表示。projected (ghost 進行値) は進捗 0 なので 0。
            snapshot.best_ex_score = row.best_score.as_ref().map(|best| best.ex_score);
            snapshot.projected_best_ex_score = snapshot.best_ex_score.map(|_| 0);
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
        self.result_key5_held = false;
        self.result_key7_held = false;
        self.play_ready_sound_started_at = None;
        if options.chart_zero_time == TimeUs(0) {
            options.chart_zero_time = self.play_skin_playstart_offset();
        }
        // 新しいプレイの音声出力を開く前に、前曲の余韻再生を止めて出力を解放する。
        self.draining_audio = None;

        // Decide 演出中に preload worker が完成させていればそれを使う。
        // 譜面/音源は別スレッドでロード済みなので、ここでは音声出力 open 等の軽量処理だけ。
        // バッファが無ければ (course モード / preload 不発時) 従来通り main で同期ロードする。
        let opened = match self.preloaded_play_session.take() {
            Some(preloaded) => {
                tracing::debug!(chart_id, "using buffered play preload");
                let prepared =
                    prepare_winit_play_session_from_preloaded(&self.boot.profile_config, preloaded);
                self.open_prepared_winit_play_session(prepared)
            }
            None => {
                let app_config = self.play_session_app_config();
                prepare_play_session_for_chart_with_winit_input(
                    &self.boot.library_db,
                    &app_config,
                    &self.boot.profile_config,
                    chart_id,
                    options.clone(),
                )
                .and_then(|prepared| self.open_prepared_winit_play_session(prepared))
            }
        };
        match opened {
            Ok(active_play) => {
                self.enter_play_scene(
                    chart_id,
                    options.clone(),
                    self.decide_snapshot_for_chart(chart_id),
                );
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

    fn enter_play_scene(
        &mut self,
        chart_id: i64,
        options: PlayStartOptions,
        mut snapshot: RenderSnapshot,
    ) {
        self.play_ending = None;
        self.result_exit = None;
        self.play_ready_sound_started_at = None;
        self.active_play = None;
        self.clear_play_control_holds();
        self.clear_play_backbmp_state();
        self.finished_play = None;
        self.draining_audio = None;
        self.play_scene_started_at = Instant::now();
        snapshot.arrange = options.arrange.as_str().to_string();
        snapshot.play_elapsed_time = TimeUs(0);
        snapshot.ready_elapsed_time = None;
        snapshot.time = self.play_skin_playstart_offset();
        // preload 完了で install_active_play がフル snapshot に置き換えるまでの間、
        // 初期ゲージや緑数字が空表示にならないようセッション開始時相当の値を埋める。
        crate::screens::play_session::apply_placeholder_session_visuals(
            &mut snapshot,
            &self.boot.profile_config,
            self.key_mode_for_chart(chart_id),
            &play_session_options_from_start(&self.play_session_app_config(), options.clone()),
        );
        self.capture_play_table_text_for_chart(chart_id);
        self.apply_course_skin_context(&mut snapshot);
        self.apply_play_table_text(&mut snapshot);
        self.last_play_snapshot = Some(snapshot.clone());
        self.pending_play_start = Some(PendingPlayStart { chart_id, options });
        self.last_started_chart_id = Some(chart_id);
    }

    /// FAST/SLOW 表示モード (Auto / ThresholdMs) を snapshot へ適用する。
    /// プレイ snapshot を `last_play_snapshot` に入れる全パスで呼ぶこと。
    fn apply_profile_fast_slow_filter(&self, snapshot: &mut RenderSnapshot) {
        apply_fast_slow_display_filter(
            snapshot,
            self.boot.profile_config.judge.fast_slow_display_threshold_ms,
            self.boot.profile_config.judge.fast_slow_display_scope,
        );
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
        self.apply_profile_fast_slow_filter(&mut snapshot);
        snapshot.arrange = active_play.running.applied_arrange.arrange.as_str().to_string();
        snapshot.backbmp_background = self.play_backbmp_loaded;
        snapshot.play_elapsed_time = self.play_elapsed_time();
        snapshot.ready_elapsed_time = self.play_ready_sound_started_at.map(elapsed_since);
        self.apply_course_skin_context(&mut snapshot);
        self.apply_play_table_text(&mut snapshot);
        crate::screens::play_snapshot::refresh_play_skin_visuals(
            &mut snapshot,
            &active_play.running.session,
        );
        self.last_play_snapshot = Some(snapshot);
        self.active_play = Some(active_play);
    }

    fn poll_play_preload(&mut self) {
        // 1) preload worker からの結果を受け取り (Decide 演出中でも受信して退避する)。
        if let Some(pending) = &self.pending_play_preload {
            match pending.rx.try_recv() {
                Ok(result) => {
                    self.pending_play_preload = None;
                    if result.generation != self.play_preload_generation {
                        tracing::debug!(
                            chart_id = result.chart_id,
                            generation = result.generation,
                            current_generation = self.play_preload_generation,
                            "discarding stale play preload result"
                        );
                    } else {
                        match result.result {
                            Ok(prepared) => {
                                tracing::info!(
                                    chart_id = result.chart_id,
                                    generation = result.generation,
                                    "play preload ready (buffered)"
                                );
                                self.preloaded_play_session = Some(prepared);
                            }
                            Err(error) => {
                                // preload 全体の失敗は譜面パース不能など再生不能なケースのみ
                                // (個別音源の欠落は load_chart_samples が warning で続行する)。
                                // Play 画面へ入場済みなら選曲へ戻す。course モード等の
                                // start_chart_with_options 経路は同期 fallback で再試行される。
                                tracing::error!(
                                    chart_id = result.chart_id,
                                    error,
                                    "play preload failed"
                                );
                                if self.pending_play_start.is_some() {
                                    self.abort_pending_play_start();
                                    return;
                                }
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    tracing::warn!(
                        chart_id = pending.chart_id,
                        generation = pending.generation,
                        "play preload worker disconnected"
                    );
                    self.pending_play_preload = None;
                }
            }
        }

        // 2) Play 入場が確定 (pending_play_start) しており、バッファに preload があれば install。
        if self
            .practice_session
            .as_ref()
            .is_some_and(|practice| practice.phase == PracticePhase::Config)
        {
            return;
        }
        let Some(play_start) = self.pending_play_start.as_ref() else {
            return;
        };
        let Some(prepared) = self.preloaded_play_session.take() else {
            return;
        };
        let chart_id = play_start.chart_id;
        let start_options = play_start.options.clone();
        let opened = if preloaded_matches_start(&prepared, chart_id, &start_options) {
            let prepared =
                prepare_winit_play_session_from_preloaded(&self.boot.profile_config, prepared);
            self.open_prepared_winit_play_session(prepared)
        } else {
            tracing::warn!(chart_id, "discarding mismatched play preload");
            let app_config = self.play_session_app_config();
            prepare_play_session_for_chart_with_winit_input(
                &self.boot.library_db,
                &app_config,
                &self.boot.profile_config,
                chart_id,
                start_options,
            )
            .and_then(|prepared| self.open_prepared_winit_play_session(prepared))
        };
        match opened {
            Ok(active_play) => {
                tracing::info!(chart_id, "play preload installed");
                self.install_active_play(active_play);
                // スキン宣言のロード演出時間を既に超えていれば、同一フレーム内で
                // READY を開始して op 80→81 切り替えと timer 40 発火を揃える
                // (次フレームの advance_active_play まで待つと 1 フレーム
                // 曲名表示が途切れる)。
                self.maybe_start_ready_phase();
            }
            Err(error) => {
                tracing::error!(chart_id, %error, "failed to open preloaded play audio");
                self.abort_pending_play_start();
            }
        }
    }

    fn abort_pending_play_start(&mut self) {
        self.pending_play_start = None;
        self.active_play = None;
        self.clear_play_backbmp_state();
        self.last_play_snapshot = None;
        // An audio-open / audio-start failure bounces the user back to the
        // select screen.  If they were in a course at the time, the course
        // session is no longer valid — otherwise the next chart they pick
        // would be treated as the next entry of a stale course (route
        // through advance_course_after_finish with mismatched chart_id).
        self.clear_active_course_state();
        let now = Instant::now();
        self.select_scene_started_at = now;
        self.restart_select_bar_timer_without_scroll(now);
    }

    /// Clears any active course session and the cached finished-course
    /// summary.  Call from any path that returns to the select screen
    /// without completing the course naturally.
    fn clear_active_course_state(&mut self) {
        if self.active_course.is_some() || self.finished_course.is_some() {
            tracing::info!(
                had_active = self.active_course.is_some(),
                had_finished = self.finished_course.is_some(),
                "clearing course session state (abort or cancel)"
            );
        }
        self.active_course = None;
        self.finished_course = None;
    }

    fn play_start_options(&self) -> PlayStartOptions {
        let arrange_seed = self
            .arrange_option
            .uses_seed()
            .then(crate::screens::play_session::generate_arrange_seed);
        PlayStartOptions {
            autoplay: self.assist_option == AssistOption::Autoplay,
            gauge: Some(self.gauge_option),
            gauge_auto_shift: self.gauge_auto_shift_option,
            bottom_shiftable_gauge: self.bottom_shiftable_gauge_option,
            arrange: self.arrange_option,
            target: self.target_option,
            target_ex_score_override: self.select_target_ex_score_override(),
            arrange_seed,
            ..Default::default()
        }
    }

    fn select_target_ex_score_override(&self) -> Option<u32> {
        let selected = self.select_items.get(self.selected_index);
        let (local_best_ex_score, total_notes) = match selected {
            Some(SelectItem::Chart(row)) => (
                row.best_score.as_ref().map(|best| best.ex_score),
                row.chart.as_ref().map(|chart| chart.total_notes),
            ),
            _ => (None, None),
        };
        if self.target_option == TargetOption::RankNext {
            return total_notes.map(|total_notes| {
                TargetOption::rank_next_ex_score(total_notes, local_best_ex_score.unwrap_or(0))
            });
        }
        self.select_ir.target_ex_score_for(
            &self.boot.profile_config.ir,
            self.selected_chart_sha256(),
            self.target_option,
            local_best_ex_score,
        )
    }

    /// リプレイファイル (例: `bmz ir replay` でダウンロードした IR リプレイ) を
    /// 直接指定して再生する。譜面はファイル内の chart_sha256 から library を引く。
    fn try_start_replay_from_file(&mut self, path: &std::path::Path) -> bool {
        let replay_file = match crate::storage::replay::load_replay(path) {
            Ok(file) => file,
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "replay file load failed");
                return false;
            }
        };
        let Ok(sha) = crate::storage::common::hex_to_hash::<32>(&replay_file.chart_sha256) else {
            tracing::warn!(sha = %replay_file.chart_sha256, "replay file has invalid chart sha256");
            return false;
        };
        let Some(chart_id) = self.boot.library_db.chart_id_by_sha256(sha).ok().flatten() else {
            tracing::warn!(
                sha = %replay_file.chart_sha256,
                "replay chart is not in the library; load the song first"
            );
            return false;
        };
        let player = bmz_gameplay::replay::ReplayPlayer {
            events: replay_file.events.clone(),
            next_index: 0,
        };
        let options = PlayStartOptions {
            autoplay: false,
            practice_mode: false,
            replay_player: Some(player),
            chart_zero_time: TimeUs(0),
            gauge: Some(self.gauge_option),
            gauge_auto_shift: self.gauge_auto_shift_option,
            bottom_shiftable_gauge: self.bottom_shiftable_gauge_option,
            arrange: replay_file.arrange_option(),
            target: self.target_option,
            arrange_seed: replay_file.arrange_seed,
            arrange_pattern: replay_file.lane_shuffle_pattern.clone(),
            initial_gauge_value: None,
            judge_constraint: bmz_core::course::CourseJudgeConstraint::Normal,
            ln_mode_override: None,
            course_gauge_override: None,
            course_gauge_property_override: None,
            target_ex_score_override: None,
        };
        self.start_chart_with_options(chart_id, options);
        true
    }

    fn try_start_replay_for_chart(&mut self, chart_id: i64, slot: u8) -> bool {
        let Some(chart) = self
            .boot
            .library_db
            .list_charts_by_ids(&[chart_id])
            .ok()
            .and_then(|mut charts| charts.pop())
        else {
            tracing::warn!(chart_id, "replay start failed: chart not found");
            return false;
        };
        let sha = chart.sha256;
        let key = crate::storage::score_db::ScoreKey::new(
            sha,
            crate::ln_policy::score_ln_policy(
                self.boot.profile_config.play.ln_mode_policy,
                chart.ln_profile,
            ),
        );
        let Some(slot_record) = self.boot.score_db.replay_slot(key, slot).ok().flatten() else {
            tracing::info!(slot, "no replay saved for slot");
            return false;
        };
        let abs_path = self.boot.profile_paths.root_dir.join(&slot_record.replay_path);
        let replay_file =
            match load_replay_for_chart_and_policy(&abs_path, sha, slot_record.ln_policy) {
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
            practice_mode: false,
            replay_player: Some(player),
            chart_zero_time: TimeUs(0),
            gauge: Some(self.gauge_option),
            gauge_auto_shift: self.gauge_auto_shift_option,
            bottom_shiftable_gauge: self.bottom_shiftable_gauge_option,
            arrange: replay_file.arrange_option(),
            target: self.target_option,
            arrange_seed: replay_file.arrange_seed,
            arrange_pattern: replay_file.lane_shuffle_pattern.clone(),
            initial_gauge_value: None,
            judge_constraint: bmz_core::course::CourseJudgeConstraint::Normal,
            ln_mode_override: None,
            course_gauge_override: None,
            course_gauge_property_override: None,
            target_ex_score_override: None,
        };
        self.start_chart_with_options(chart_id, options);
        true
    }

    fn start_replay_for_selected(&mut self, slot: u8) -> bool {
        // Prefer the chart path when the cursor is on a chart row.
        if let Some(chart_id) = self.currently_selected_chart_id() {
            return self.try_start_replay_for_chart(chart_id, slot);
        }
        // Otherwise, if the cursor is on a course row, try to launch the
        // course replay stored in the requested slot.
        if let Some(course_id) = self.currently_selected_course_id() {
            return self.try_start_course_replay_for_slot(course_id, slot);
        }
        false
    }

    fn currently_selected_chart_id(&self) -> Option<i64> {
        match self.select_items.get(self.selected_index)? {
            SelectItem::Chart(row) => row.chart.as_ref().map(|chart| chart.chart_id),
            SelectItem::Folder { .. }
            | SelectItem::Course(_)
            | SelectItem::Config(_)
            | SelectItem::KeyBinding(_)
            | SelectItem::Back
            | SelectItem::AdvancedSettings => None,
        }
    }

    fn currently_selected_course_id(&self) -> Option<i64> {
        match self.select_items.get(self.selected_index)? {
            SelectItem::Course(row) => Some(row.course_id),
            SelectItem::Chart(_)
            | SelectItem::Folder { .. }
            | SelectItem::Config(_)
            | SelectItem::KeyBinding(_)
            | SelectItem::Back
            | SelectItem::AdvancedSettings => None,
        }
    }

    fn try_start_course_replay_for_slot(&mut self, course_id: i64, slot: u8) -> bool {
        match self.boot.library_db.course_replay_slot(course_id, slot) {
            Ok(Some(record)) => {
                tracing::info!(
                    course_id,
                    course_score_id = record.course_score_id,
                    slot,
                    "starting course replay from select"
                );
                self.start_course_replay(course_id, record.course_score_id);
                true
            }
            Ok(None) => {
                tracing::info!(course_id, slot, "no saved course attempt in this replay slot");
                false
            }
            Err(error) => {
                tracing::error!(
                    %error,
                    course_id,
                    slot,
                    "failed to look up course_replay_slot"
                );
                false
            }
        }
    }

    fn retry_last_chart_with_mode(&mut self, mode: ResultRetryMode) {
        let Some(chart_id) = self.last_started_chart_id else {
            tracing::warn!("no previous chart is available to retry");
            return;
        };
        let options = match mode {
            ResultRetryMode::SameArrange => self.result_retry_same_arrange_options(),
            ResultRetryMode::DifferentArrange => self.result_retry_different_arrange_options(),
        };
        self.start_chart_with_options(chart_id, options);
    }

    /// Replay the whole course from its first chart, reproducing each chart's
    /// recorded arrange.  Reads the just-finished course result for the course
    /// id and per-entry arranges, then re-enters the course in PLAY mode.
    fn retry_course_same_arrange(&mut self) {
        let Some(course) = self.finished_course.as_ref() else {
            tracing::warn!("no finished course is available to retry");
            return;
        };
        let course_id = course.course_id;
        let arrange_overrides = course.entry_arranges.clone();
        tracing::info!(
            course_id,
            entries = arrange_overrides.len(),
            "retrying course (same arrange)"
        );
        // Drop the finished-course/result state before re-entering the course;
        // start_course_with_arrange installs a fresh active_course session.
        self.finished_course = None;
        self.finished_play = None;
        self.result_exit = None;
        self.result_key5_held = false;
        self.result_key7_held = false;
        self.start_course_with_arrange(course_id, arrange_overrides);
    }

    fn result_retry_same_arrange_options(&self) -> PlayStartOptions {
        let mut options = self.play_start_options();
        if let Some(applied) = self.finished_play.as_ref().map(|finished| &finished.applied_arrange)
        {
            options.arrange = applied.arrange;
            options.arrange_seed = applied.seed;
            options.arrange_pattern = applied.pattern.clone();
        }
        options
    }

    fn result_retry_different_arrange_options(&self) -> PlayStartOptions {
        let mut options = self.play_start_options();
        if let Some(applied) = self.finished_play.as_ref().map(|finished| &finished.applied_arrange)
        {
            options.arrange = applied.arrange;
            options.arrange_seed = None;
            options.arrange_pattern = None;
        }
        options
    }

    /// Key5/Key7 の現在の押下状態を記録する。フェードアウト中も含めて
    /// 常に呼び、終了アニメーション終了時に retry arrange を決める。
    fn track_result_lane_hold(&mut self, control: &PhysicalControl, pressed: bool) {
        match self.result_lane_for_control(control) {
            Some(Lane::Key5) => self.result_key5_held = pressed,
            Some(Lane::Key7) => self.result_key7_held = pressed,
            _ => {}
        }
    }

    fn handle_result_control(
        &mut self,
        control: &PhysicalControl,
        pressed: bool,
        repeat: bool,
    ) -> bool {
        let Some(lane) = self.result_lane_for_control(control) else {
            return false;
        };
        match lane {
            // ゲージグラフ種別の切り替え。
            Lane::Key6 => {
                if pressed && !repeat && self.result_input_ready() {
                    self.cycle_result_gauge_graph_type();
                }
                true
            }
            // Key1-4 / Key5 / Key7 の押下で終了アニメーションを開始する。
            // フェードアウト終了時の Key5/Key7 押下状態で retry か選曲へ戻るかを決める。
            lane if lane_starts_result_exit(lane) => {
                if pressed && self.result_input_ready() {
                    self.begin_result_exit(ResultExitAction::HeldLanes);
                }
                true
            }
            _ => false,
        }
    }

    fn result_lane_for_control(&self, control: &PhysicalControl) -> Option<Lane> {
        let key_mode = self.finished_play.as_ref()?.summary.key_mode;
        crate::config::play::lane_binding_for_chart(&self.boot.profile_config.input, key_mode)
            .resolve(DeviceId(0), control)
    }

    fn cycle_result_gauge_graph_type(&mut self) {
        self.result_gauge_graph_type = cycle_result_gauge_graph_type(self.result_gauge_graph_type);
        tracing::info!(
            gauge_type = self.result_gauge_graph_type,
            "result gauge graph type changed"
        );
        self.play_system_sound(crate::system_sound::SoundType::OptionChange);
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
        // HeldLanes の遷移判定はフェードアウト終了時に Key5/Key7 の
        // 押下状態を読むため、ここでは held フラグをリセットしない。
        // ResultClear / ResultFail のループ風長尺音を止めて、close SE を鳴らす。
        self.stop_system_sound(crate::system_sound::SoundType::ResultClear);
        self.stop_system_sound(crate::system_sound::SoundType::ResultFail);
        self.play_system_sound(crate::system_sound::SoundType::ResultClose);
    }

    fn begin_decide_fadeout(&mut self, cancel: bool) {
        if self.pending_decide.is_none() {
            return;
        }
        self.clear_play_control_holds();
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
        let Some(fadeout_started) =
            self.pending_decide.as_ref().map(|decide| decide.fadeout_started_at.is_some())
        else {
            return;
        };
        if !fadeout_started && self.cancel_decide_if_exit_hold_elapsed() {
            return;
        }
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

        if !decide.cancel && !self.decide_play_start_ready() {
            return;
        }

        let Some(decide) = self.pending_decide.take() else {
            return;
        };
        if decide.cancel {
            self.invalidate_play_preload();
            // Decide screen cancel (Escape) returns to select.  If a course
            // was being started, drop the course session — the user opted
            // out before the first chart actually began.
            self.clear_active_course_state();
            let now = Instant::now();
            self.select_scene_started_at = now;
            self.restart_select_bar_timer_without_scroll(now);
        } else {
            self.enter_play_scene(decide.chart_id, decide.options, decide.snapshot);
        }
    }

    fn decide_play_start_ready(&self) -> bool {
        // preload (WAV ロード等) の完了は待たない。Play 画面へ先に入場し、
        // ロード完了後に poll_play_preload が active_play を install して
        // READY タイマーが始まる。
        !self.pending_play_skin
    }

    fn update_decide_cancel_control_state(&mut self, control: &str, pressed: bool) -> bool {
        let mut handled = false;
        if self.select_keys.is_start(control) {
            self.decide_e1_held = pressed;
            handled = true;
        }
        if self.select_keys.is_e2_action(control) {
            self.play_e2_held = pressed;
            handled = true;
        }
        if self.select_keys.is_e3_action(control) {
            self.play_e3_held = pressed;
            handled = true;
        }
        if !handled {
            return false;
        }
        update_play_exit_hold_started_at(
            &mut self.play_exit_hold_started_at,
            self.decide_e1_held,
            self.play_e2_held,
            Instant::now(),
        );
        if pressed && self.play_e2_held && self.play_e3_held {
            self.begin_decide_fadeout(true);
            return true;
        }
        true
    }

    fn cancel_decide_if_exit_hold_elapsed(&mut self) -> bool {
        let hold_duration =
            Duration::from_millis(self.boot.profile_config.play.play_exit_hold_ms as u64);
        if play_exit_hold_elapsed(self.play_exit_hold_started_at, Instant::now(), hold_duration) {
            self.begin_decide_fadeout(true);
            return true;
        }
        false
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
        self.result_gauge_graph_type = self
            .finished_play
            .as_ref()
            .map(|finished| finished.summary.gauge_type as i32)
            .unwrap_or(GaugeType::Normal as i32);
        self.result_key5_held = false;
        self.result_key7_held = false;
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
            // 中間リザルトは scene 時間経過で次の曲へ、それ以外は選曲へ戻る。
            let action = if self.is_course_intermediate_result() {
                ResultExitAction::AdvanceCourse
            } else {
                ResultExitAction::Leave
            };
            self.begin_result_exit(action);
        }
        let Some(exit) = self.result_exit.as_ref() else {
            return;
        };
        // 何らかの理由でリザルトを抜けていたら終了状態を破棄する。
        if self.finished_play.is_none() {
            self.result_exit = None;
            return;
        }
        let started_at = exit.started_at;
        let action = exit.action.clone();
        let fadeout = Duration::from_millis(self.renderer.result_skin_fadeout_ms().max(0) as u64);
        let elapsed = started_at.elapsed();
        // スキンの終了アニメーション時間に合わせて、プレイ残響(draining_audio)を
        // 1.0 → 0.0 へ絞る。遷移時に音量が 0 付近まで落ちているので、
        // draining_audio を破棄しても唐突な音切れにならない。
        self.fade_draining_audio_for_result_exit(elapsed, fadeout);
        if elapsed < fadeout {
            return;
        }
        self.result_exit = None;
        match action {
            ResultExitAction::Leave => self.leave_result(),
            ResultExitAction::Retry(mode) => self.retry_last_chart_with_mode(mode),
            ResultExitAction::HeldLanes => {
                match result_action_for_held_lanes(self.result_key5_held, self.result_key7_held) {
                    Some(mode) => self.retry_last_chart_with_mode(mode),
                    None => self.leave_result(),
                }
            }
            ResultExitAction::RetryCourseSameArrange => self.retry_course_same_arrange(),
            ResultExitAction::AdvanceCourse => self.advance_to_next_course_chart(),
        }
    }

    /// リザルト終了アニメ中、プレイ残響(draining_audio)のマスターゲインを
    /// 1.0 → 0.0 へランプする。毎フレーム呼ぶ。
    /// フェード時間は `RESULT_EXIT_AUDIO_FADE` を上限とし、スキンの終了アニメ時間
    /// (`fadeout`) がそれより短ければ遷移前に絞り切れるよう短い方を採用する。
    /// 見た目の遷移タイミング自体は `fadeout` のまま変えない。
    /// ResultClose SE 等のシステム音は別エンジンなので影響を受けない。
    fn fade_draining_audio_for_result_exit(&mut self, elapsed: Duration, fadeout: Duration) {
        let Some(audio) = &self.draining_audio else {
            return;
        };
        let audio_fade = fadeout.min(RESULT_EXIT_AUDIO_FADE);
        let gain = if audio_fade.is_zero() {
            0.0
        } else {
            (1.0 - elapsed.as_secs_f32() / audio_fade.as_secs_f32()).clamp(0.0, 1.0)
        };
        if let Ok(mut engine) = audio.engine.lock() {
            engine.set_master_gain(gain);
        }
    }

    fn leave_result(&mut self) {
        self.finished_play = None;
        self.clear_active_course_state();
        self.result_exit = None;
        self.result_key5_held = false;
        self.result_key7_held = false;
        self.clear_play_backbmp_state();
        // リザルト画面を抜けたら、まだ鳴っていても余韻再生を止める。
        self.draining_audio = None;
        self.last_play_snapshot = None;
        self.reload_select_items();
        let now = Instant::now();
        self.select_scene_started_at = now;
        self.restart_select_bar_timer_without_scroll(now);
    }

    fn decide_scene_duration(&self) -> Duration {
        skin_duration_ms(self.renderer.decide_skin_document().map(|d| d.scene).unwrap_or(0))
    }

    fn decide_fadeout_duration(&self) -> Duration {
        skin_duration_ms(self.renderer.decide_skin_document().map(|d| d.fadeout).unwrap_or(0))
    }

    fn decide_fadeout_scene_timing(&self) -> DecideFadeoutSceneTiming {
        decide_fadeout_scene_timing(self.renderer.decide_skin_document())
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
        let history: Vec<String> = self.search_history.iter().cloned().collect();
        let (items, resolved_mode_filter) = load_items_for_stack(
            &self.boot,
            &self.folder_stack,
            &history,
            self.select_mode_filter,
            self.select_sort,
        );
        // beatoraja 準拠の自動送りで mode filter が変わることがあるので、
        // 表示状態と永続化用 profile config を実際に適用したモードへ揃える。
        self.select_mode_filter = resolved_mode_filter;
        self.boot.profile_config.select.mode_filter = resolved_mode_filter.as_str().to_string();
        self.select_items = items;
        self.select_distribution_cache.borrow_mut().clear();
        self.select_folder_summary_cache.clear();
        if self.selected_index >= self.select_items.len() {
            self.selected_index = self.select_items.len().saturating_sub(1);
        }
    }

    fn load_songs_and_reload(&mut self) {
        let scan_roots = self.song_load_roots_from_stack();

        if !scan_roots.is_empty() {
            self.spawn_song_scan(scan_roots, false, "song-scan".to_string());
        }
    }

    fn import_external_scores(&mut self, request: ScoreImportRequest) {
        let label = request.kind.label();
        let path = request.path.display().to_string();
        match import_scores(
            &request,
            &mut self.boot.library_db,
            &mut self.boot.score_db,
            now_unix_seconds(),
        ) {
            Ok(report) => {
                let summary = report.summary();
                tracing::info!(kind = label, path, summary, "external scores imported");
                self.reload_select_items();
                if let Some(egui) = self.egui.as_mut() {
                    egui.set_score_import_status(
                        format!(
                            "{label}: {} をインポートしました ({summary})",
                            request.path.display()
                        ),
                        false,
                    );
                }
            }
            Err(error) => {
                let message = format!("{label}: インポートに失敗しました: {error}");
                tracing::error!(kind = label, path, error = %format_error_chain(&error), "external score import failed");
                if let Some(egui) = self.egui.as_mut() {
                    egui.set_score_import_status(message, true);
                }
            }
        }
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
            self.spawn_song_scan(roots, true, "F5 song reload".to_string());
            return;
        }
        tracing::debug!("F5 reload: no applicable target in select context");
    }

    fn spawn_song_scan(&mut self, roots: Vec<PathEntry>, force: bool, label: String) {
        if self.pending_song_scan.is_some() {
            tracing::debug!(%label, "song scan already in progress");
            return;
        }
        let library_db_path = self.boot.app_paths.library_db.clone();
        let scan_config = self.boot.app_config.scan.clone();
        let (tx, rx) = mpsc::channel();
        self.song_scan_progress = Some(ScanProgress::default());
        thread::Builder::new()
            .name("song-scan".to_string())
            .spawn(move || {
                let result = (|| -> Result<ScanReport> {
                    migrate_library_db(&library_db_path)?;
                    let mut library_db = LibraryDatabase::open(&library_db_path)?;
                    scan_songs_with_progress(
                        &mut library_db,
                        &roots,
                        &scan_config,
                        now_unix_seconds(),
                        force,
                        |progress| {
                            let _ = tx.send(SongScanEvent::Progress(progress));
                        },
                    )
                })();
                let _ = tx.send(SongScanEvent::Finished(result));
            })
            .expect("failed to spawn song scan thread");
        self.pending_song_scan = Some(rx);
        tracing::info!(%label, force, "started song scan");
    }

    fn poll_pending_song_scan(&mut self) {
        let Some(rx) = self.pending_song_scan.take() else {
            return;
        };
        let mut keep_pending = true;
        loop {
            match rx.try_recv() {
                Ok(SongScanEvent::Progress(progress)) => {
                    self.song_scan_progress = Some(progress);
                }
                Ok(SongScanEvent::Finished(Ok(report))) => {
                    tracing::info!(
                        imported = report.summary.imported,
                        skipped = report.summary.skipped,
                        failed = report.summary.failed,
                        "song scan complete"
                    );
                    self.song_scan_progress = None;
                    self.reload_select_items();
                    keep_pending = false;
                    break;
                }
                Ok(SongScanEvent::Finished(Err(error))) => {
                    tracing::error!(%error, "song scan failed");
                    self.song_scan_progress = None;
                    keep_pending = false;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    tracing::warn!("song scan worker disconnected");
                    self.song_scan_progress = None;
                    keep_pending = false;
                    break;
                }
            }
        }
        if keep_pending {
            self.pending_song_scan = Some(rx);
        }
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
                self.table_breadcrumb_cache.borrow_mut().clear();
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

    fn refresh_visible_select_folder_summaries(&mut self) {
        self.poll_select_folder_summary_loads();
        self.request_visible_select_folder_summaries(25);
    }

    fn poll_select_folder_summary_loads(&mut self) {
        loop {
            match self.select_folder_summary_rx.try_recv() {
                Ok(result) => {
                    let entry = match result.result {
                        Ok(summary) => SelectFolderSummaryCacheEntry::Ready(summary),
                        Err(error) => {
                            tracing::warn!(
                                key = %result.key,
                                %error,
                                "select folder lamp summary worker failed"
                            );
                            SelectFolderSummaryCacheEntry::Missing
                        }
                    };
                    self.select_folder_summary_cache.insert(result.key, entry);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }

        for item in &mut self.select_items {
            let SelectItem::Folder { path, kind, summary, .. } = item else {
                continue;
            };
            if summary.is_some() {
                continue;
            }
            let key = select_folder_summary_cache_key(path, *kind);
            if let Some(SelectFolderSummaryCacheEntry::Ready(Some(ready))) =
                self.select_folder_summary_cache.get(&key)
            {
                *summary = Some(ready.clone());
            }
        }
    }

    fn request_visible_select_folder_summaries(&mut self, visible_limit: usize) {
        let visible_indices = select_visible_item_indices(
            self.select_items.len(),
            self.selected_index,
            visible_limit,
        );
        let mut requests = Vec::new();
        for index in visible_indices {
            let Some(SelectItem::Folder { path, kind, summary, .. }) = self.select_items.get(index)
            else {
                continue;
            };
            if summary.is_some() {
                continue;
            }
            let key = select_folder_summary_cache_key(path, *kind);
            match self.select_folder_summary_cache.get(&key) {
                Some(
                    SelectFolderSummaryCacheEntry::Loading
                    | SelectFolderSummaryCacheEntry::Ready(_)
                    | SelectFolderSummaryCacheEntry::Missing,
                ) => continue,
                None => {
                    self.select_folder_summary_cache
                        .insert(key.clone(), SelectFolderSummaryCacheEntry::Loading);
                    requests.push((key, path.clone(), *kind));
                }
            }
        }

        for (key, path, kind) in requests {
            self.spawn_select_folder_summary_load(key, path, kind);
        }
    }

    fn spawn_select_folder_summary_load(
        &self,
        key: String,
        path: String,
        kind: bmz_render::scene::SelectRowKind,
    ) {
        let library_db_path = self.boot.app_paths.library_db.clone();
        let score_db_path = self.boot.profile_paths.score_db.clone();
        let ln_policy_setting = self.boot.profile_config.play.ln_mode_policy;
        let tx = self.select_folder_summary_tx.clone();
        thread::Builder::new()
            .name("select-folder-lamp".to_string())
            .spawn(move || {
                let result = (|| -> Result<Option<SelectFolderSummary>> {
                    migrate_library_db(&library_db_path)?;
                    migrate_score_db(&score_db_path)?;
                    let library_db = LibraryDatabase::open(&library_db_path)?;
                    let score_db = ScoreDatabase::open(&score_db_path)?;
                    select_folder_summary(&library_db, &score_db, &path, kind, ln_policy_setting)
                })()
                .map_err(|error| error.to_string());
                let _ = tx.send(SelectFolderSummaryResult { key, result });
            })
            .expect("failed to spawn select folder lamp worker");
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
        let mut video_sources = Vec::new();
        for source in prepared {
            let PreparedSource { source_id, path, texture, prepared, size, is_video } = source;
            self.renderer.insert_prepared_texture(TextureId(texture.0), prepared);
            if is_video {
                let gating = skin_video_source_gating(&document, &source_id);
                video_sources.push(ActiveSkinVideoSource {
                    texture,
                    path,
                    decoder: None,
                    last_pts: None,
                    loop_start_us: 0,
                    active: gating.active,
                    gating_op_sets: gating.op_sets,
                    enabled_options: document.enabled_options(),
                    result_ranktime_ms: document.ranktime,
                    failed: false,
                });
            }
            document_textures.push(SkinDocumentTexture { source_id, texture, source_size: size });
        }
        tracing::info!(
            path = %path.display(),
            kind = ?kind,
            sources = document_textures.len(),
            "beatoraja skin fully installed"
        );
        if video_sources.is_empty() {
            self.skin_video_sources.remove(&kind);
        } else {
            self.skin_video_sources.insert(kind, video_sources);
        }
        let preserve_play_dynamic_timers = kind == SkinKind::Play && self.active_play.is_some();
        set_decoded_skin_context(
            &mut self.renderer,
            kind,
            manifest,
            document,
            document_textures,
            preserve_play_dynamic_timers,
        );
        if kind == SkinKind::Select && matches!(self.view_state(), AppViewState::Select) {
            self.restart_select_scene_timers();
        }
    }

    fn restart_select_scene_timers(&mut self) {
        let now = Instant::now();
        self.select_scene_started_at = now;
        self.restart_select_bar_timer_without_scroll(now);
        self.option_panel_started_at = now;
    }

    fn advance_active_play(&mut self) {
        if self.play_ending.is_some() {
            self.update_play_ending_snapshot();
            return;
        }
        if self.pending_play_start.is_some() {
            self.update_pending_play_snapshot_timers();
        }
        if self.active_play.is_none() {
            return;
        }
        if self.stop_play_if_exit_hold_elapsed() {
            self.clear_play_control_holds();
        }
        self.maybe_start_ready_phase();
        if self.play_ready_sound_started_at.is_none() {
            self.update_pre_ready_play_state();
            self.update_pending_play_snapshot_timers();
            return;
        }
        let course_titles = self.current_course_titles();
        let course_stage = self.current_course_stage_marker();
        let play_elapsed_time = self.play_elapsed_time();
        let ready_elapsed_time = self.play_ready_sound_started_at.map(elapsed_since);
        let backbmp_background = self.play_backbmp_loaded;
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

        let advance_outcome = advance_running_play_session_until_result(
            &mut active_play.running,
            &mut self.boot.score_db,
            &self.boot.profile_paths,
            &self.boot.profile_config.replay,
            &self.boot.profile_config.ir,
            now_unix_seconds(),
        );
        match advance_outcome {
            Ok(PlayAdvanceOutcome::Playing(frame)) => {
                let mine_hits = frame.mine_hits.len();
                let mut snapshot = frame.render_snapshot;
                self.apply_profile_fast_slow_filter(&mut snapshot);
                snapshot.play_elapsed_time = play_elapsed_time;
                snapshot.ready_elapsed_time = ready_elapsed_time;
                snapshot.backbmp_background = backbmp_background;
                snapshot.course_stage = course_stage;
                snapshot.course_titles = course_titles.clone();
                self.apply_play_table_text(&mut snapshot);
                if let Some(active_play) = &self.active_play {
                    crate::screens::play_snapshot::refresh_play_skin_visuals(
                        &mut snapshot,
                        &active_play.running.session,
                    );
                }
                self.last_play_snapshot = Some(snapshot);
                self.play_landmine_se(mine_hits);
            }
            Ok(PlayAdvanceOutcome::Finished { frame, finished }) => {
                if self
                    .practice_session
                    .as_ref()
                    .is_some_and(|practice| practice.phase == PracticePhase::Playing)
                {
                    let mine_hits = frame.mine_hits.len();
                    self.play_landmine_se(mine_hits);
                    self.finish_practice_round();
                    return;
                }
                let hispeed =
                    self.active_play.as_ref().map(|active| active.running.session.hispeed);
                let mine_hits = frame.mine_hits.len();
                let mut snapshot = frame.render_snapshot;
                self.apply_profile_fast_slow_filter(&mut snapshot);
                snapshot.play_elapsed_time = play_elapsed_time;
                snapshot.ready_elapsed_time = ready_elapsed_time;
                snapshot.backbmp_background = backbmp_background;
                snapshot.course_stage = course_stage;
                snapshot.course_titles = course_titles.clone();
                self.apply_play_table_text(&mut snapshot);
                let full_combo_elapsed_at_finish_ms = snapshot.full_combo_elapsed_ms;
                if let Some(active_play) = &self.active_play {
                    crate::screens::play_snapshot::refresh_play_skin_visuals(
                        &mut snapshot,
                        &active_play.running.session,
                    );
                }
                self.last_play_snapshot = Some(snapshot);
                self.play_landmine_se(mine_hits);
                // active_play がまだ残っている内に hispeed/lane_cover/lift を profile に保存する。
                self.save_current_play_options(hispeed, "play finished");
                self.play_ending = Some(PlayEndingTransition {
                    started_at: Instant::now(),
                    fadeout_started_at: None,
                    failed: frame.state == bmz_gameplay::session::PlayState::Failed,
                    full_combo_elapsed_at_finish_ms,
                    finished,
                });
                self.update_play_ending_snapshot();
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
        let chart_zero_time = self
            .practice_chart_zero_time
            .take()
            .unwrap_or_else(|| self.play_skin_playstart_offset());
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
            if let Some(active_play) = &self.active_play {
                crate::screens::play_snapshot::refresh_play_skin_visuals(
                    snapshot,
                    &active_play.running.session,
                );
            }
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

    fn update_pre_ready_play_state(&mut self) {
        let play_elapsed_time = self.play_elapsed_time();
        let Some(active_play) = &mut self.active_play else {
            return;
        };
        bmz_gameplay::session::drain_pre_ready_visual_inputs(
            &mut active_play.running.session,
            play_elapsed_time,
        );
        let Some(snapshot) = &mut self.last_play_snapshot else {
            return;
        };
        snapshot.play_elapsed_time = play_elapsed_time;
        crate::screens::play_snapshot::refresh_play_skin_visuals(
            snapshot,
            &active_play.running.session,
        );
    }

    fn update_pre_ready_play_snapshot_options(&mut self) {
        let Some(active_play) = &self.active_play else {
            return;
        };
        update_pre_ready_play_snapshot_options_for_session(
            self.play_ready_sound_started_at,
            &mut self.last_play_snapshot,
            &active_play.running.session,
            active_play.running.applied_arrange.arrange,
        );
    }

    fn stop_active_play_like_escape(&mut self, reason: &'static str) -> bool {
        let stopped = {
            let Some(active_play) = &mut self.active_play else {
                return false;
            };
            let session = &mut active_play.running.session;
            if session.judge.is_exhausted(&session.chart)
                || matches!(
                    session.state,
                    bmz_gameplay::session::PlayState::Failed
                        | bmz_gameplay::session::PlayState::Finished
                )
            {
                return false;
            }
            tracing::info!(reason, "stopping active play");
            session.state = bmz_gameplay::session::PlayState::Failed;
            true
        };
        self.clear_play_control_holds();
        self.play_system_sound(crate::system_sound::SoundType::PlayStop);
        stopped
    }

    fn update_play_exit_hold_timer(&mut self) {
        let e1_held = self
            .active_play
            .as_ref()
            .is_some_and(|active| active.running.session.lane_cover_changing);
        update_play_exit_hold_started_at(
            &mut self.play_exit_hold_started_at,
            e1_held,
            self.play_e2_held,
            Instant::now(),
        );
    }

    fn clear_play_control_holds(&mut self) {
        self.last_play_start_press_at = None;
        self.decide_e1_held = false;
        self.play_e2_held = false;
        self.play_e3_held = false;
        self.play_exit_hold_started_at = None;
    }

    fn update_play_exit_control_state(&mut self, control: &str, pressed: bool) -> bool {
        let mut changed = false;
        if self.select_keys.is_e2_action(control) {
            self.play_e2_held = pressed;
            changed = true;
        }
        if self.select_keys.is_e3_action(control) {
            self.play_e3_held = pressed;
            changed = true;
        }
        if !changed {
            return false;
        }
        self.update_play_exit_hold_timer();
        if self.play_e2_held && self.play_e3_held {
            return self.stop_active_play_like_escape("E2+E3 pressed during play");
        }
        false
    }

    fn stop_play_if_exit_hold_elapsed(&mut self) -> bool {
        let hold_duration =
            Duration::from_millis(self.boot.profile_config.play.play_exit_hold_ms as u64);
        if play_exit_hold_elapsed(self.play_exit_hold_started_at, Instant::now(), hold_duration) {
            self.play_exit_hold_started_at = None;
            return self.stop_active_play_like_escape("E1+E2 held during play");
        }
        false
    }

    fn update_play_ending_snapshot(&mut self) {
        let Some(ending) = &self.play_ending else {
            return;
        };
        let play_elapsed_time = self.play_elapsed_time();
        let ready_elapsed_time = self.play_ready_sound_started_at.map(elapsed_since);
        let timers = PlayEndingSkinTimers {
            play_elapsed_time,
            ready_elapsed_time,
            backbmp_background: self.play_backbmp_loaded,
            failed_elapsed_ms: ending.failed.then_some(elapsed_since_ms(ending.started_at)),
            music_end_elapsed_ms: (!ending.failed).then_some(elapsed_since_ms(ending.started_at)),
            fadeout_elapsed_ms: ending.fadeout_started_at.map(elapsed_since_ms),
        };

        let Some(active_play) = &mut self.active_play else {
            let Some(snapshot) = &mut self.last_play_snapshot else {
                return;
            };
            snapshot.play_elapsed_time = timers.play_elapsed_time;
            snapshot.ready_elapsed_time = timers.ready_elapsed_time;
            snapshot.failed_elapsed_ms = timers.failed_elapsed_ms;
            snapshot.music_end_elapsed_ms = timers.music_end_elapsed_ms;
            snapshot.fadeout_elapsed_ms = timers.fadeout_elapsed_ms;
            return;
        };

        let video_update_time = compute_frame_times(&active_play.running.session).render_now;
        crate::video_bga::update_video_bga_frames(
            &mut self.renderer,
            &mut active_play.running,
            video_update_time,
        );

        let mut snapshot = refresh_play_ending_snapshot(&mut active_play.running, timers);
        self.apply_profile_fast_slow_filter(&mut snapshot);
        self.apply_course_skin_context(&mut snapshot);
        self.apply_play_table_text(&mut snapshot);
        self.last_play_snapshot = Some(snapshot);
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
        let scene_kind = self.current_scene_kind();
        let scene = match scene_kind {
            AppSceneKind::Select => "Select",
            AppSceneKind::Decide => "Decide",
            AppSceneKind::Play => "Play",
            AppSceneKind::Result => "Result",
        };
        let size = window.inner_size();
        let info = DebugInfo { scene, width: size.width, height: size.height };
        let play5_path = self.boot.profile_config.skin.play5.clone();
        let play7_path = self.boot.profile_config.skin.play7.clone();
        let play9_path = self.boot.profile_config.skin.play9.clone();
        let play10_path = self.boot.profile_config.skin.play10.clone();
        let play14_path = self.boot.profile_config.skin.play14.clone();
        let play5_defs = self.play_skin_defs_for_path(&play5_path);
        let play7_defs = self.play_skin_defs_for_path(&play7_path);
        let play9_defs = self.play_skin_defs_for_path(&play9_path);
        let play10_defs = self.play_skin_defs_for_path(&play10_path);
        let play14_defs = self.play_skin_defs_for_path(&play14_path);
        let skin_meta = SkinConfigMeta {
            select: SceneSkinDefs::from_document(self.renderer.select_skin_document()),
            decide: SceneSkinDefs::from_document(self.renderer.decide_skin_document()),
            play5: play5_defs,
            play7: play7_defs,
            play9: play9_defs,
            play10: play10_defs,
            play14: play14_defs,
            result: SceneSkinDefs::from_document(self.renderer.result_skin_document()),
        };
        // Clone the course summary so the egui closure can borrow it while
        // `self.egui` is uniquely borrowed.  CourseResultSummary is small —
        // a few strings and Vec<ResultSummary> — so the clone cost is minor.
        let course_result = self.finished_course.clone();
        // Only show the course preview when the user is on the select screen
        // and the cursor is over a course row.
        let course_preview = matches!(scene_kind, AppSceneKind::Select)
            .then(|| {
                self.select_items.get(self.selected_index).and_then(|item| match item {
                    SelectItem::Course(row) => Some(row.clone()),
                    _ => None,
                })
            })
            .flatten();
        let practice_media_ready = self.practice_media_ready();
        let mut practice_panel_ctx = None;
        if let Some(practice) = &mut self.practice_session
            && practice.phase == PracticePhase::Config
        {
            practice_panel_ctx = Some(PracticePanelContext {
                property: &mut practice.property,
                chart_title: &practice.chart_title,
                media_ready: practice_media_ready,
                max_end_time_ms: practice.max_end_time_ms,
            });
        }
        // リザルト画面に入ったら IR 送信・ランキング取得タスクを起動し、
        // 離れたら破棄する。コース最終リザルトは対象外 (チャート単位でないため)。
        if matches!(scene_kind, AppSceneKind::Result) && self.finished_course.is_none() {
            if self.result_ir.is_none()
                && let Some(finished) = &self.finished_play
            {
                self.result_ir = crate::screens::result_ir::spawn_result_ir_task(
                    self.boot.profile_paths.root_dir.clone(),
                    self.boot.profile_paths.score_db.clone(),
                    &self.boot.profile_config.ir,
                    crate::storage::common::hash_to_hex(&finished.result.chart_sha256),
                    finished.result.gauge_type.as_str().to_string(),
                    finished.ln_policy,
                );
            }
            if let Some(state) = &mut self.result_ir {
                state.poll();
            }
        } else {
            self.result_ir = None;
        }
        // 選曲画面ではカーソル譜面の IR ランキングをデバウンスつきで取得する
        // (NUMBER_IR_RANK / NUMBER_IR_TOTALPLAYER / OPTION_IR_* 用)。
        if matches!(scene_kind, AppSceneKind::Select) {
            // `selected_chart_sha256()` は &self 全体を借りるため、practice ctx の
            // &mut 借用と衝突しないようフィールド単位で参照する。
            let (selected, ln_profile) = match self.select_items.get(self.selected_index) {
                Some(SelectItem::Chart(row)) => (
                    row.score_sha256(),
                    // library 登録済みなら譜面の LN プロファイルから実プレイと
                    // 同じスコア分離キーを解決する。未登録は default 近似。
                    row.chart.as_ref().map(|chart| chart.ln_profile).unwrap_or_default(),
                ),
                _ => (None, crate::ln_policy::ChartLnProfile::default()),
            };
            let gauge =
                crate::config::play::gauge_type_from_config(self.boot.profile_config.play.gauge)
                    .as_str()
                    .to_string();
            let ln_policy = crate::ln_policy::score_ln_policy(
                self.boot.profile_config.play.ln_mode_policy,
                ln_profile,
            );
            let context = format!("{gauge}:{:?}", self.boot.profile_config.play.ln_mode_policy);
            let ir_config = self.boot.profile_config.ir.clone();
            self.select_ir.update(
                &ir_config,
                &self.boot.profile_paths.root_dir,
                &context,
                &gauge,
                ln_policy,
                selected,
            );
        }
        let result_ir_panel = self.result_ir.as_mut();
        let Some(egui) = self.egui.as_mut() else {
            return;
        };
        let output = egui.run(
            &window,
            EguiRunContext {
                info: &info,
                app_config: &mut self.boot.app_config,
                profile_config: &mut self.boot.profile_config,
                skin_meta: &skin_meta,
                skin_catalog: &self.skin_catalog,
                course_result: course_result.as_ref(),
                course_preview: course_preview.as_ref(),
                practice: practice_panel_ctx.as_mut(),
                result_ir: result_ir_panel,
                profile_root: &self.boot.profile_paths.root_dir,
            },
        );
        self.renderer.set_egui_frame(output.frame);
        self.sync_realtime_profile_settings();
        if output.practice_leave {
            self.leave_practice();
            return;
        }
        if output.practice_start {
            self.start_practice_round();
        }
        // デバッグパネルの開閉状態を profile config へ同期する。
        // 永続化は終了時 / プレイ後の save_profile_config に任せる。
        self.boot.profile_config.ui.show_fps = output.debug_panel_visible;
        // 本体設定パネルでの present mode 変更を即座に反映する。
        self.renderer.set_present_mode(config_present_mode(&self.boot.app_config.video));
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
        if output.apply_audio_output {
            self.reopen_audio_output();
        }
        if output.trigger_song_rescan {
            self.load_songs_and_reload();
        }
        if let Some(request) = output.score_import_request {
            self.import_external_scores(request);
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

    fn sync_realtime_profile_settings(&mut self) {
        self.sync_active_play_realtime_profile_settings();
        if let Some(manager) = &self.system_sound {
            let mix = self.boot.profile_config.audio_mix.clone();
            manager.refresh_volumes(|sound_type| system_sound_volume_from_mix(&mix, sound_type));
        }
        self.apply_select_preview_audio_mix();
    }

    fn sync_active_play_realtime_profile_settings(&mut self) {
        if let Some(active_play) = &mut self.active_play {
            let session = &mut active_play.running.session;
            session.audio_mix =
                crate::config::play::audio_mix_from_profile(&self.boot.profile_config);
            session.offsets =
                crate::config::play::play_offsets_from_profile(&self.boot.profile_config);
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

    fn update_current_skin_video_sources(&mut self) {
        let Some((kind, elapsed_us)) = self.current_skin_video_context() else {
            return;
        };
        // 実行時 op 条件 (例: リザルトのランク別 BG) で実際に表示されるソースだけを
        // デコードする。state を作れないシーンでは静的な `active` 判定に従う。
        let runtime_state = self.current_skin_video_draw_state(kind);
        let Some(sources) = self.skin_video_sources.get_mut(&kind) else {
            return;
        };
        for source in sources {
            if source.failed || !source.active {
                continue;
            }
            if let Some(state) = runtime_state.as_ref()
                && !skin_video_source_runtime_visible(source, state)
            {
                // 現在のシーン状態では非表示。デコード中なら止めて開放する。
                if source.decoder.is_some() {
                    source.decoder = None;
                    source.last_pts = None;
                }
                continue;
            }
            if source.decoder.is_none() {
                match VideoBgaDecoder::open(&source.path) {
                    Ok(decoder) => {
                        tracing::info!(
                            kind = ?kind,
                            texture_id = source.texture.0,
                            path = %source.path.display(),
                            "opened skin video source decoder"
                        );
                        source.decoder = Some(decoder);
                    }
                    Err(error) => {
                        tracing::warn!(
                            kind = ?kind,
                            texture_id = source.texture.0,
                            path = %source.path.display(),
                            %error,
                            "failed to open skin video source"
                        );
                        source.failed = true;
                        continue;
                    }
                }
            }

            let Some(decoder) = source.decoder.as_mut() else {
                continue;
            };
            let video_offset_us = elapsed_us.saturating_sub(source.loop_start_us);
            if let Some(frame) = decoder.poll_frame(video_offset_us)
                && source.last_pts != Some(frame.pts_us)
            {
                let pts = frame.pts_us;
                match self.renderer.upsert_rgba_texture_ref(
                    TextureId(source.texture.0),
                    frame.width,
                    frame.height,
                    &frame.rgba,
                ) {
                    Ok(()) => {
                        source.last_pts = Some(pts);
                    }
                    Err(error) => {
                        tracing::warn!(
                            kind = ?kind,
                            texture_id = source.texture.0,
                            path = %source.path.display(),
                            %error,
                            "failed to upload skin video source frame"
                        );
                    }
                }
            }
            if source.decoder.as_ref().is_some_and(VideoBgaDecoder::is_finished) {
                source.decoder = None;
                source.last_pts = None;
                source.loop_start_us = elapsed_us;
            }
        }
    }

    fn current_skin_video_context(&self) -> Option<(SkinKind, i64)> {
        match self.view_state() {
            AppViewState::Select => Some((SkinKind::Select, self.select_time().0)),
            AppViewState::Decide => self
                .pending_decide
                .as_ref()
                .map(|decide| (SkinKind::Decide, elapsed_since(decide.started_at).0)),
            AppViewState::Play => Some((SkinKind::Play, self.play_elapsed_time().0)),
            AppViewState::Result(_) => {
                Some((SkinKind::Result, elapsed_since(self.result_scene_started_at).0))
            }
        }
    }

    /// 動画ソースの実行時可視判定に使う `SkinDrawState` を、現在のシーン用に構築する。
    /// 現状はランク別 BG を持つリザルト画面だけ対応し、他シーンは静的な `active`
    /// 判定に委ねるため `None` を返す。
    fn current_skin_video_draw_state(
        &self,
        kind: SkinKind,
    ) -> Option<bmz_render::skin::SkinDrawState> {
        if kind != SkinKind::Result {
            return None;
        }
        let AppSceneSnapshot::Result(snapshot) = self.scene_snapshot() else {
            return None;
        };
        let ranktime = self
            .skin_video_sources
            .get(&SkinKind::Result)
            .and_then(|sources| sources.first())
            .map_or(0, |source| source.result_ranktime_ms);
        Some(bmz_render::plan::result_skin_draw_state(&snapshot, ranktime))
    }

    fn render_current_scene(&mut self) {
        let select_view = matches!(self.view_state(), AppViewState::Select);
        let result_view = matches!(self.view_state(), AppViewState::Result(_));
        let profiling_select = select_view
            && tracing::enabled!(target: "bmz_player::select_profile", tracing::Level::DEBUG);
        let profiling_result = result_view
            && tracing::enabled!(target: "bmz_player::result_profile", tracing::Level::DEBUG);
        if select_view {
            self.refresh_visible_select_folder_summaries();
            self.poll_select_asset_loads();
            self.sync_select_stage_texture();
            self.sync_select_backbmp_texture();
            self.sync_select_banner_texture();
            self.sync_select_preview_audio();
            self.update_select_preview_fade();
        }
        self.start_scene_timers_before_snapshot(select_view, result_view);
        let video_start = Instant::now();
        self.update_current_skin_video_sources();
        let video_us = video_start.elapsed().as_micros();
        let snapshot_start = Instant::now();
        let scene = self.scene_snapshot();
        let snapshot_us = snapshot_start.elapsed().as_micros();
        let scene_kind = scene_kind(&scene);
        self.update_window_title_for_scene(scene_kind);
        if let (Some(path), Some(exit_after_frames)) =
            (&self.smoke_screenshot_path, self.smoke_exit_after_frames)
            && self.rendered_frames.saturating_add(1) >= exit_after_frames
        {
            self.renderer.request_screenshot(path.clone());
        }
        let render_start = Instant::now();
        let render_status = self.renderer.render_scene_status(scene);
        let render_us = render_start.elapsed().as_micros();
        if profiling_select {
            self.select_frame_profiler.record(
                FrameProfileKind::Select,
                video_us,
                snapshot_us,
                render_us,
                self.renderer.last_frame_timings(),
            );
        }
        if profiling_result {
            self.result_frame_profiler.record(
                FrameProfileKind::Result,
                video_us,
                snapshot_us,
                render_us,
                self.renderer.last_frame_timings(),
            );
        }
        match render_status {
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

        if let Some(exit_after_result_frames) = self.smoke_exit_after_result_frames
            && self.finished_play.is_some()
        {
            self.rendered_result_frames = self.rendered_result_frames.saturating_add(1);
            if self.rendered_result_frames >= exit_after_result_frames {
                self.smoke_exit_after_result_frames = None;
                tracing::info!(
                    frames = self.rendered_result_frames,
                    "smoke result frame count reached; leaving event loop"
                );
                self.save_current_play_options(None, "game exit");
                event_loop.exit();
                return;
            }
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

    fn start_scene_timers_before_snapshot(&mut self, select_view: bool, result_view: bool) {
        match self.last_scene_kind {
            Some(AppSceneKind::Select) if select_view => {}
            _ if select_view => self.restart_select_scene_timers(),
            Some(AppSceneKind::Result) if result_view => {}
            _ if result_view => {
                self.result_scene_started_at = Instant::now();
            }
            _ => {}
        }
    }

    fn active_lane_state(&self) -> Option<ActiveLaneState> {
        self.active_play.as_ref().map(|active| {
            let session = &active.running.session;
            ActiveLaneState {
                lane_cover: session.lane_cover,
                lift: session.lift,
                hispeed_mode: session.hispeed_mode,
                target_green_number: session.target_green_number,
            }
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
            self.gauge_auto_shift_option,
            self.bottom_shiftable_gauge_option,
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
            AppSceneKind::Select
                if should_play_select_bgm_on_enter(self.select_preview_playing) =>
            {
                self.play_system_sound(SoundType::Select);
            }
            AppSceneKind::Select => {}
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

    /// `profile.audio_mix.system_bgm_volume` / `system_se_volume` に
    /// `master_volume` を乗算してシステム音を鳴らす。
    /// ボリュームは AudioEngine 側で 0.0..=1.0 にクランプされる。
    fn play_system_sound(&self, sound_type: crate::system_sound::SoundType) {
        if let Some(manager) = &self.system_sound {
            manager.play(
                sound_type,
                system_sound_volume_from_mix(&self.boot.profile_config.audio_mix, sound_type),
            );
            self.start_audio_output_stream();
        }
    }

    fn start_audio_output_stream(&self) {
        let Some(runtime) = &self.audio_runtime else {
            return;
        };
        if let Err(error) = runtime.play() {
            tracing::warn!(%error, "failed to start shared audio output stream");
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

fn should_play_select_bgm_on_enter(select_preview_playing: bool) -> bool {
    !select_preview_playing
}

fn select_preview_fade_factor(fade: SelectPreviewFade, now: Instant) -> f32 {
    match fade {
        SelectPreviewFade::Silent => 0.0,
        SelectPreviewFade::Playing => 1.0,
        SelectPreviewFade::FadingIn { started_at } => {
            fade_progress(started_at, now, SELECT_PREVIEW_FADE_DURATION)
        }
        SelectPreviewFade::FadingOut { started_at } => {
            1.0 - fade_progress(started_at, now, SELECT_PREVIEW_FADE_DURATION)
        }
    }
    .clamp(0.0, 1.0)
}

fn select_preview_key_after_delay(
    key: Option<String>,
    elapsed: Duration,
    delay: Duration,
) -> Option<String> {
    if elapsed >= delay { key } else { None }
}

fn fade_progress(started_at: Instant, now: Instant, duration: Duration) -> f32 {
    if duration == Duration::ZERO {
        return 1.0;
    }
    now.saturating_duration_since(started_at).as_secs_f32() / duration.as_secs_f32()
}

fn should_route_settings_key_event(
    state: ElementState,
    repeat: bool,
    settings_editing: bool,
) -> bool {
    state == ElementState::Pressed && (settings_editing || !repeat)
}

fn settings_browse_move_control(control: &str, bindings: &SettingsBindings) -> Option<SelectMove> {
    match control {
        "ArrowUp" | "DPadUp" | "ScratchUp" => Some(SelectMove::Previous),
        "ArrowDown" | "DPadDown" | "ScratchDown" => Some(SelectMove::Next),
        _ if bindings.is_increase(control) => Some(SelectMove::Next),
        _ if bindings.is_decrease(control) => Some(SelectMove::Previous),
        _ => None,
    }
}

fn system_sound_manager_from_boot(
    boot: &BootstrappedApp,
    audio: &crate::audio::SystemAudio,
) -> crate::system_sound_manager::SystemSoundManager {
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
    let selection =
        crate::system_sound::select_random_sound_set(&bgm_candidates, &se_candidates, default_dir);
    crate::system_sound_manager::SystemSoundManager::new(audio.engine(), &selection)
}

fn system_sound_volume_from_mix(
    mix: &crate::config::profile_config::AudioMixConfig,
    sound_type: crate::system_sound::SoundType,
) -> f32 {
    let unit = if sound_type.is_bgm() { mix.system_bgm_volume } else { mix.system_se_volume };
    let volume = crate::config::play::volume_unit_to_f32(mix.master_volume)
        * crate::config::play::volume_unit_to_f32(unit);
    volume.clamp(0.0, 1.0)
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
) -> (Option<SkinManifest>, HashMap<SkinKind, Vec<ActiveSkinVideoSource>>, bool, bool, bool) {
    // Decide / Result の JSON skin は Select の同期ロードより**前**に decode スレッドを起動して
    // CPU をフル活用する。Select の sync 処理 (PNG GPU upload など) と並列に decode が進む。
    let pending_select = false;
    let mut pending_decide = false;
    let mut pending_result = false;
    let mut skin_video_sources = HashMap::new();

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
            let video_sources = apply_json_skin_sync(
                renderer,
                path,
                SkinKind::Select,
                default_manifest.as_ref(),
                select_options,
                select_files,
            );
            if !video_sources.is_empty() {
                skin_video_sources.insert(SkinKind::Select, video_sources);
            }
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

    (default_manifest, skin_video_sources, pending_select, pending_decide, pending_result)
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
) -> Vec<ActiveSkinVideoSource> {
    let Some(manifest) = default_manifest else {
        tracing::warn!(
            path = %path.display(),
            kind = ?kind,
            "skipping skin install because default skin manifest is unavailable"
        );
        return Vec::new();
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
            return Vec::new();
        }
    };
    let video_sources = skin_video_sources_from_decoded(&decoded);
    if let Err(error) = install_decoded_skin(renderer, decoded, manifest.clone()) {
        tracing::warn!(
            path = %path.display(),
            kind = ?kind,
            error = %format_error_chain(&error),
            "failed to install beatoraja skin"
        );
        return Vec::new();
    }
    video_sources
}

fn skin_video_sources_from_decoded(decoded: &DecodedSkin) -> Vec<ActiveSkinVideoSource> {
    let enabled_options = decoded.document.enabled_options();
    decoded
        .sources
        .iter()
        .filter(|source| source.is_video)
        .map(|source| {
            let gating = skin_video_source_gating(&decoded.document, &source.source_id);
            ActiveSkinVideoSource {
                texture: source.texture,
                path: source.path.clone(),
                decoder: None,
                last_pts: None,
                loop_start_us: 0,
                active: gating.active,
                gating_op_sets: gating.op_sets,
                enabled_options: enabled_options.clone(),
                result_ranktime_ms: decoded.document.ranktime,
                failed: false,
            }
        })
        .collect()
}

/// 動画ソースの可視判定に必要なゲーティング情報。
struct SkinVideoSourceGating {
    /// スキン config の option による静的な有効判定。
    active: bool,
    /// このソースを参照する各 destination の op 条件。conditional destination の
    /// outer `if` 条件も合成済み。空なら参照されていない (= 常時可視)。
    op_sets: Vec<Vec<i32>>,
}

fn skin_video_source_gating(document: &SkinDocument, source_id: &str) -> SkinVideoSourceGating {
    let image_ids: HashSet<&str> = document
        .image
        .iter()
        .filter(|image| image.src == source_id)
        .map(|image| image.id.as_str())
        .collect();
    if image_ids.is_empty() {
        return SkinVideoSourceGating { active: true, op_sets: Vec::new() };
    }

    let mut render_object_ids = image_ids.clone();
    for imageset in &document.imageset {
        if imageset.images.iter().any(|id| image_ids.contains(id.as_str())) {
            render_object_ids.insert(imageset.id.as_str());
        }
    }

    let property_ops: HashSet<i32> = document
        .property
        .iter()
        .flat_map(|property| property.item.iter().filter_map(|item| item.op.checked_abs()))
        .collect();
    let enabled_options = document.enabled_options();
    let mut referenced = false;
    let mut active = false;
    let mut op_sets = Vec::new();
    for (destination, op_set) in skin_document_destination_op_sets(document) {
        if !render_object_ids.contains(destination.id.as_str()) {
            continue;
        }
        referenced = true;
        if destination_property_ops_allow(&op_set, &enabled_options, &property_ops) {
            active = true;
        }
        op_sets.push(op_set);
    }
    if !referenced {
        return SkinVideoSourceGating { active: true, op_sets: Vec::new() };
    }
    SkinVideoSourceGating { active, op_sets }
}

/// 実行時 state に対して、動画ソースが現在のシーン状態で表示されるかどうかを判定する。
/// `op_sets` が空 (= destination から参照されていない) 場合は常時可視。
fn skin_video_source_runtime_visible(
    source: &ActiveSkinVideoSource,
    state: &bmz_render::skin::SkinDrawState,
) -> bool {
    if source.gating_op_sets.is_empty() {
        return true;
    }
    source
        .gating_op_sets
        .iter()
        .any(|ops| bmz_render::skin::test_skin_ops(ops, &source.enabled_options, state))
}

fn skin_document_destination_op_sets(
    document: &SkinDocument,
) -> Vec<(&SkinDestinationDef, Vec<i32>)> {
    document
        .destination
        .iter()
        .flat_map(|entry| match entry {
            DestinationListEntry::Single(destination) => {
                vec![(destination, destination.op.clone())]
            }
            DestinationListEntry::Conditional { if_ops, destinations } => destinations
                .iter()
                .map(|destination| {
                    let mut op_set = if_ops.clone();
                    op_set.extend(destination.op.iter().copied());
                    (destination, op_set)
                })
                .collect::<Vec<_>>(),
        })
        .collect()
}

fn destination_property_ops_allow(
    ops: &[i32],
    enabled_options: &[i32],
    property_ops: &HashSet<i32>,
) -> bool {
    ops.iter().all(|op| {
        let Some(abs_op) = op.checked_abs() else {
            return true;
        };
        if !property_ops.contains(&abs_op) {
            return true;
        }
        if *op >= 0 { enabled_options.contains(op) } else { !enabled_options.contains(&abs_op) }
    })
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
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "json" | "luaskin" | "lr2skin"))
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
    } else if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lr2skin"))
    {
        bmz_skin::load_lr2_csv_skin(
            path,
            bmz_skin::SkinKind::Play,
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .ok()
        .map(|loaded| loaded.document)
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
        4 => catalog.play9.push(candidate),
        5 => catalog.select.push(candidate),
        6 => catalog.decide.push(candidate),
        7 | 15 => catalog.result.push(candidate),
        _ => {}
    }
}

fn sort_skin_catalog(catalog: &mut SkinCatalog) {
    for candidates in [
        &mut catalog.select,
        &mut catalog.decide,
        &mut catalog.play5,
        &mut catalog.play7,
        &mut catalog.play9,
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
                tracing::warn!(
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

fn config_renderer_backend(
    backend: crate::config::app_config::RendererBackend,
) -> bmz_render::WgpuBackend {
    match backend {
        crate::config::app_config::RendererBackend::Auto => bmz_render::WgpuBackend::Auto,
        crate::config::app_config::RendererBackend::Vulkan => bmz_render::WgpuBackend::Vulkan,
        crate::config::app_config::RendererBackend::Metal => bmz_render::WgpuBackend::Metal,
        crate::config::app_config::RendererBackend::Dx12 => bmz_render::WgpuBackend::Dx12,
        crate::config::app_config::RendererBackend::Gl => bmz_render::WgpuBackend::Gl,
    }
}

fn config_present_mode(
    video: &crate::config::app_config::VideoConfig,
) -> bmz_render::WgpuPresentMode {
    match video.present_mode {
        crate::config::app_config::PresentModeConfig::Auto => {
            if video.vsync {
                bmz_render::WgpuPresentMode::AutoVsync
            } else {
                bmz_render::WgpuPresentMode::AutoNoVsync
            }
        }
        crate::config::app_config::PresentModeConfig::AutoVsync => {
            bmz_render::WgpuPresentMode::AutoVsync
        }
        crate::config::app_config::PresentModeConfig::AutoNoVsync => {
            bmz_render::WgpuPresentMode::AutoNoVsync
        }
        crate::config::app_config::PresentModeConfig::Immediate => {
            bmz_render::WgpuPresentMode::Immediate
        }
        crate::config::app_config::PresentModeConfig::Mailbox => {
            bmz_render::WgpuPresentMode::Mailbox
        }
        crate::config::app_config::PresentModeConfig::Fifo => bmz_render::WgpuPresentMode::Fifo,
        crate::config::app_config::PresentModeConfig::FifoRelaxed => {
            bmz_render::WgpuPresentMode::FifoRelaxed
        }
    }
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
        let practice_overlay = self
            .practice_session
            .as_ref()
            .is_some_and(|practice| practice.phase == PracticePhase::Config);
        let egui_consumed = match (self.window.clone(), self.egui.as_mut()) {
            (Some(window), Some(egui)) => egui.on_window_event(&window, &event, practice_overlay),
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
                self.last_cursor_action_at = Instant::now();
                if !self.cursor_visible {
                    if let Some(window) = &self.window {
                        window.set_cursor_visible(true);
                    }
                    self.cursor_visible = true;
                }
                if egui_consumed {
                    return;
                }
                self.route_mouse_wheel(delta);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.last_cursor_position = Some(position);
                self.last_cursor_action_at = Instant::now();
                if !self.cursor_visible {
                    if let Some(window) = &self.window {
                        window.set_cursor_visible(true);
                    }
                    self.cursor_visible = true;
                }
                if !egui_consumed {
                    self.route_select_slider_drag();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.last_cursor_action_at = Instant::now();
                if !self.cursor_visible {
                    if let Some(window) = &self.window {
                        window.set_cursor_visible(true);
                    }
                    self.cursor_visible = true;
                }
                if egui_consumed {
                    return;
                }
                self.route_mouse_input(state, button);
            }
            WindowEvent::Ime(ime) => {
                if egui_consumed {
                    return;
                }
                self.route_ime_event(&ime);
            }
            WindowEvent::Resized(size) => {
                self.renderer
                    .resize_surface(SurfaceSize { width: size.width, height: size.height });
                // 検索モード中はリサイズに合わせて IME 候補ウィンドウ位置を再計算する。
                self.update_search_ime_cursor_area();
            }
            WindowEvent::Focused(focused) => {
                self.focused = focused;
            }
            WindowEvent::RedrawRequested => {
                if self.cursor_visible
                    && self.last_cursor_action_at.elapsed() >= Duration::from_secs(2)
                {
                    if let Some(window) = &self.window {
                        window.set_cursor_visible(false);
                    }
                    self.cursor_visible = false;
                }
                self.limit_frame_rate();
                self.poll_gamepad_events();
                self.advance_select_hold_move();
                self.advance_select_analog_scroll();
                self.drain_pending_skins();
                self.poll_play_preload();
                self.poll_pending_table_fetch();
                self.poll_pending_song_scan();
                self.advance_decide_transition();
                self.advance_play_ending();
                self.advance_result_exit();
                self.run_egui_frame();
                self.render_current_scene();
                if !self.first_frame_startup_completed {
                    self.first_frame_startup_completed = true;
                    self.ensure_audio_output();
                    self.start_deferred_boot();
                    if self.current_scene_kind() == AppSceneKind::Result {
                        self.ensure_skin_ready(SkinKind::Result);
                    }
                    self.last_scene_kind = None;
                }
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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.shutdown_requested.load(Ordering::SeqCst) {
            tracing::info!("Ctrl-C received; exiting cleanly");
            event_loop.exit();
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.save_current_play_options(self.active_hispeed(), "game exit");
        // プロセス終了前に音声出力を確実に Drop し、ASIO の停止・後処理を走らせる。
        self.draining_audio = None;
        self.active_play = None;
        self.system_audio = None;
        self.audio_runtime = None;
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

fn deferred_boot_action(boot_chart_id: Option<i64>, options: &AppOptions) -> Option<DeferredBoot> {
    if let Some(chart_id) = boot_chart_id {
        if options.boot_practice {
            return Some(DeferredBoot::Practice {
                chart_id,
                start_time_ms: options.practice_start_ms,
                end_time_ms: options.practice_end_ms,
            });
        }
        return Some(DeferredBoot::Chart { chart_id, replay_slot: options.boot_replay_slot });
    }
    if let Some(path) = options.boot_replay_file.clone() {
        return Some(DeferredBoot::ReplayFile { path });
    }
    if let Some(course_id) = options.boot_course_replay_id {
        return Some(DeferredBoot::CourseReplay { course_id });
    }
    options.boot_course_id.map(|course_id| DeferredBoot::Course { course_id })
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
    if options.boot_result_sample {
        tracing::info!(arg = BOOT_RESULT_SAMPLE_ARG, "debug result boot enabled");
    }
    if options.autoplay_on_start {
        tracing::info!(arg = AUTOPLAY_ON_START_ARG, "autoplay enabled for started charts");
    }
    if let Some(frames) = options.smoke_exit_after_frames {
        tracing::info!(arg = SMOKE_EXIT_AFTER_FRAMES_ARG, frames, "smoke auto-exit enabled");
    }
    if let Some(frames) = options.smoke_exit_after_result_frames {
        tracing::info!(
            arg = SMOKE_EXIT_AFTER_RESULT_FRAMES_ARG,
            frames,
            "smoke result-frame auto-exit enabled"
        );
    }
    if options.smoke_exit_on_result {
        tracing::info!(arg = SMOKE_EXIT_ON_RESULT_ARG, "smoke auto-exit on result enabled");
    }
    if options.boot_practice {
        tracing::info!("practice mode enabled for boot chart");
    }
    if let Some(path) = &options.smoke_screenshot_path {
        tracing::info!(arg = SMOKE_SCREENSHOT_ARG, path, "smoke screenshot enabled");
    }
}

fn initial_folder_stack(_app_config: &crate::config::app_config::AppConfig) -> Vec<String> {
    // 有効な曲フォルダが 1 つだけでも、設定フォルダ等を含む選曲ルートから始める。
    Vec::new()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectModeFilter {
    All,
    K7,
    K14,
    K9,
    K5,
    K10,
    K24,
    K24Double,
}

impl SelectModeFilter {
    const ORDER: [Self; 8] =
        [Self::All, Self::K7, Self::K14, Self::K9, Self::K5, Self::K10, Self::K24, Self::K24Double];

    fn next(self) -> Self {
        cycle_enum(Self::ORDER, self, 1)
    }

    fn previous(self) -> Self {
        cycle_enum(Self::ORDER, self, -1)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::K7 => "7K",
            Self::K14 => "14K",
            Self::K9 => "9K",
            Self::K5 => "5K",
            Self::K10 => "10K",
            Self::K24 => "24K",
            Self::K24Double => "24K_DOUBLE",
        }
    }

    fn key_mode(self) -> Option<KeyMode> {
        match self {
            Self::All | Self::K24 | Self::K24Double => None,
            Self::K7 => Some(KeyMode::K7),
            Self::K14 => Some(KeyMode::K14),
            Self::K9 => Some(KeyMode::K9),
            Self::K5 => Some(KeyMode::K5),
            Self::K10 => Some(KeyMode::K10),
        }
    }

    /// `as_str()` の逆変換。未知の値は `ALL` へフォールバックする。
    fn from_str_or_default(value: &str) -> Self {
        Self::ORDER.into_iter().find(|mode| mode.as_str() == value).unwrap_or(Self::All)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectSort {
    Title,
    Artist,
    Bpm,
    Length,
    Level,
    Clear,
    Score,
    Bp,
}

impl SelectSort {
    const ORDER: [Self; 8] = [
        Self::Title,
        Self::Artist,
        Self::Bpm,
        Self::Length,
        Self::Level,
        Self::Clear,
        Self::Score,
        Self::Bp,
    ];

    fn next(self) -> Self {
        cycle_enum(Self::ORDER, self, 1)
    }

    fn previous(self) -> Self {
        cycle_enum(Self::ORDER, self, -1)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Title => "TITLE",
            Self::Artist => "ARTIST",
            Self::Bpm => "BPM",
            Self::Length => "LENGTH",
            Self::Level => "LEVEL",
            Self::Clear => "CLEAR",
            Self::Score => "SCORE",
            Self::Bp => "BPCOUNT",
        }
    }

    /// `as_str()` の逆変換。未知の値は `TITLE` へフォールバックする。
    fn from_str_or_default(value: &str) -> Self {
        Self::ORDER.into_iter().find(|sort| sort.as_str() == value).unwrap_or(Self::Title)
    }
}

fn cycle_enum<T: Copy + PartialEq, const N: usize>(
    values: [T; N],
    current: T,
    direction: i32,
) -> T {
    let index = values.iter().position(|value| *value == current).unwrap_or(0);
    let len = values.len();
    if direction >= 0 { values[(index + 1) % len] } else { values[(index + len - 1) % len] }
}

fn enabled_root_paths(app_config: &crate::config::app_config::AppConfig) -> Vec<String> {
    app_config.songs.roots.iter().filter(|p| p.enabled).map(|p| p.path.clone()).collect()
}

/// 選曲リストを構築し、mode filter / sort を適用して返す。
///
/// mode filter は beatoraja `BarManager` 準拠で、指定モードがこの一覧の
/// チャートを「全て」消してしまう場合のみ、チャートが残るモードへ前方向に
/// 自動送りする。実際に適用したモードを items と共に返すので、呼び出し側で
/// 永続化 / 表示状態を更新できる。
fn load_items_for_stack(
    boot: &crate::bootstrap::BootstrappedApp,
    stack: &[String],
    search_history: &[String],
    mode_filter: SelectModeFilter,
    sort: SelectSort,
) -> (Vec<SelectItem>, SelectModeFilter) {
    let mut items = build_select_items_for_stack(boot, stack, search_history);
    let resolved = resolve_non_empty_mode_filter(&items, mode_filter);
    apply_select_mode_filter(&mut items, resolved);
    apply_select_sort(&mut items, sort);
    (items, resolved)
}

fn build_select_items_for_stack(
    boot: &crate::bootstrap::BootstrappedApp,
    stack: &[String],
    search_history: &[String],
) -> Vec<SelectItem> {
    match stack.last() {
        Some(path) if path.starts_with(crate::screens::settings_model::CONFIG_ROOT_PATH) => {
            load_settings_items(path)
        }
        Some(path) if path == COURSE_ROOT_PATH => {
            match load_select_items_for_courses(&boot.library_db) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(%error, "failed to load course list");
                    Vec::new()
                }
            }
        }
        Some(path) if path.starts_with(SEARCH_PATH_PREFIX) => match parse_search_query(path) {
            Some(query) => {
                match load_select_items_for_search(
                    &boot.library_db,
                    &boot.score_db,
                    query,
                    boot.profile_config.play.ln_mode_policy,
                ) {
                    Ok(items) => items,
                    Err(error) => {
                        tracing::error!(%error, query, "failed to load search results");
                        Vec::new()
                    }
                }
            }
            None => Vec::new(),
        },
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
                    boot.profile_config.play.ln_mode_policy,
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
            match load_select_items_in_folder(
                &boot.library_db,
                &boot.score_db,
                folder,
                boot.profile_config.play.ln_mode_policy,
            ) {
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
                Ok(courses) if courses.iter().any(|c| !c.source.starts_with("table:")) => {
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
            items.push(settings_root_item());
            if !search_history.is_empty() {
                items.extend(search_history_folder_items(search_history));
            }
            items
        }
    }
}

/// beatoraja `BarManager` 準拠の mode filter 自動送り。
///
/// 指定モードがこの一覧の全 bar を消す（= 残るのが mismatch のチャート行だけ）
/// 場合のみ、チャートが残るモードへ前方向に送る。フォルダ等チャート以外の行が
/// 1 つでも残る、または ALL/24K のように絞り込まないモードの場合は据え置く。
fn resolve_non_empty_mode_filter(
    items: &[SelectItem],
    start: SelectModeFilter,
) -> SelectModeFilter {
    let mut candidate = start;
    for _ in 0..SelectModeFilter::ORDER.len() {
        if !mode_filter_removes_everything(items, candidate) {
            return candidate;
        }
        candidate = candidate.next();
    }
    start
}

/// `apply_select_mode_filter` を適用すると一覧が空になるか。
fn mode_filter_removes_everything(items: &[SelectItem], filter: SelectModeFilter) -> bool {
    if items.is_empty() {
        return false;
    }
    let Some(key_mode) = filter.key_mode() else {
        // ALL / 24K 系は絞り込まないので空にはならない。
        return false;
    };
    items.iter().all(|item| match item {
        SelectItem::Chart(row) => !row
            .chart
            .as_ref()
            .and_then(|chart| KeyMode::from_str_opt(&chart.mode))
            .is_some_and(|mode| mode == key_mode),
        // フォルダ・コース等は除去対象外なので、残れば「全除去」ではない。
        _ => false,
    })
}

fn select_folder_summary_cache_key(path: &str, kind: bmz_render::scene::SelectRowKind) -> String {
    format!("{kind:?}\n{path}")
}

fn apply_select_mode_filter(items: &mut Vec<SelectItem>, filter: SelectModeFilter) {
    let Some(key_mode) = filter.key_mode() else {
        return;
    };
    items.retain(|item| match item {
        SelectItem::Chart(row) => row
            .chart
            .as_ref()
            .and_then(|chart| KeyMode::from_str_opt(&chart.mode))
            .is_some_and(|mode| mode == key_mode),
        _ => true,
    });
}

fn apply_select_sort(items: &mut [SelectItem], sort: SelectSort) {
    items.sort_by(|a, b| match (a, b) {
        (SelectItem::Chart(a), SelectItem::Chart(b)) => compare_select_chart_rows(a, b, sort),
        _ => std::cmp::Ordering::Equal,
    });
}

fn compare_select_chart_rows(
    a: &crate::screens::select_model::SelectChartRow,
    b: &crate::screens::select_model::SelectChartRow,
    sort: SelectSort,
) -> std::cmp::Ordering {
    let ordering = match sort {
        SelectSort::Title => compare_case_insensitive(a.display_title(), b.display_title()),
        SelectSort::Artist => compare_case_insensitive(a.display_artist(), b.display_artist()),
        SelectSort::Bpm => chart_initial_bpm(a).total_cmp(&chart_initial_bpm(b)),
        SelectSort::Length => chart_length_ms(a).cmp(&chart_length_ms(b)),
        SelectSort::Level => compare_play_level(a, b),
        SelectSort::Clear => clear_rank(a).cmp(&clear_rank(b)),
        SelectSort::Score => ex_score(a).cmp(&ex_score(b)),
        SelectSort::Bp => bp(a).cmp(&bp(b)),
    };
    ordering.then_with(|| compare_case_insensitive(a.display_title(), b.display_title()))
}

fn compare_case_insensitive(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

fn chart_initial_bpm(row: &crate::screens::select_model::SelectChartRow) -> f64 {
    row.chart.as_ref().map(|chart| chart.initial_bpm).unwrap_or(0.0)
}

fn chart_length_ms(row: &crate::screens::select_model::SelectChartRow) -> i64 {
    row.chart.as_ref().map(|chart| chart.length_ms).unwrap_or(0)
}

fn compare_play_level(
    a: &crate::screens::select_model::SelectChartRow,
    b: &crate::screens::select_model::SelectChartRow,
) -> std::cmp::Ordering {
    play_level_number(a)
        .total_cmp(&play_level_number(b))
        .then_with(|| compare_case_insensitive(a.display_title(), b.display_title()))
}

fn play_level_number(row: &crate::screens::select_model::SelectChartRow) -> f64 {
    row.chart.as_ref().and_then(|chart| chart.play_level.parse::<f64>().ok()).unwrap_or(0.0)
}

fn clear_rank(row: &crate::screens::select_model::SelectChartRow) -> i8 {
    if !row.in_library() {
        // 難易度表にあるがローカル未所持。NoPlay よりさらに下位へ並べる。
        return -1;
    }
    // 所持済み: NoPlay / 未記録 = 0、Failed=1 .. Max=10。
    ClearType::rank_from_label(
        row.best_score.as_ref().map(|score| score.clear_type.as_str()).unwrap_or_default(),
    ) as i8
}

fn ex_score(row: &crate::screens::select_model::SelectChartRow) -> u32 {
    row.best_score.as_ref().map(|score| score.ex_score).unwrap_or(0)
}

fn bp(row: &crate::screens::select_model::SelectChartRow) -> u32 {
    row.best_score.as_ref().map(|score| score.bp).unwrap_or(u32::MAX)
}

fn cycle_gauge_option(current: GaugeTypeConfig) -> GaugeTypeConfig {
    match current {
        GaugeTypeConfig::AssistEasy => GaugeTypeConfig::Easy,
        GaugeTypeConfig::Easy => GaugeTypeConfig::Normal,
        GaugeTypeConfig::Normal => GaugeTypeConfig::Hard,
        GaugeTypeConfig::Hard => GaugeTypeConfig::ExHard,
        GaugeTypeConfig::ExHard | GaugeTypeConfig::AutoShift => GaugeTypeConfig::Hazard,
        GaugeTypeConfig::Hazard => GaugeTypeConfig::AssistEasy,
    }
}

fn cycle_gauge_option_with_direction(current: GaugeTypeConfig, direction: i32) -> GaugeTypeConfig {
    const VALUES: [GaugeTypeConfig; 6] = [
        GaugeTypeConfig::AssistEasy,
        GaugeTypeConfig::Easy,
        GaugeTypeConfig::Normal,
        GaugeTypeConfig::Hard,
        GaugeTypeConfig::ExHard,
        GaugeTypeConfig::Hazard,
    ];
    cycle_enum(VALUES, normalize_gauge_option(current), direction)
}

fn normalize_gauge_option(current: GaugeTypeConfig) -> GaugeTypeConfig {
    match current {
        GaugeTypeConfig::AutoShift => GaugeTypeConfig::ExHard,
        _ => current,
    }
}

fn gauge_option_as_str(gauge: GaugeTypeConfig) -> &'static str {
    match gauge {
        GaugeTypeConfig::AssistEasy => "A-EASY",
        GaugeTypeConfig::Easy => "EASY",
        GaugeTypeConfig::Normal => "NORMAL",
        GaugeTypeConfig::Hard => "HARD",
        GaugeTypeConfig::ExHard => "EX-HARD",
        GaugeTypeConfig::AutoShift => "EX-HARD",
        GaugeTypeConfig::Hazard => "HAZARD",
    }
}

fn cycle_gauge_auto_shift_option(current: GaugeAutoShiftConfig) -> GaugeAutoShiftConfig {
    match current {
        GaugeAutoShiftConfig::Off => GaugeAutoShiftConfig::Continue,
        GaugeAutoShiftConfig::Continue => GaugeAutoShiftConfig::HardToGroove,
        GaugeAutoShiftConfig::HardToGroove => GaugeAutoShiftConfig::BestClear,
        GaugeAutoShiftConfig::BestClear => GaugeAutoShiftConfig::SelectToUnder,
        GaugeAutoShiftConfig::SelectToUnder => GaugeAutoShiftConfig::Off,
    }
}

fn cycle_gauge_auto_shift_option_with_direction(
    current: GaugeAutoShiftConfig,
    direction: i32,
) -> GaugeAutoShiftConfig {
    const VALUES: [GaugeAutoShiftConfig; 5] = [
        GaugeAutoShiftConfig::Off,
        GaugeAutoShiftConfig::Continue,
        GaugeAutoShiftConfig::HardToGroove,
        GaugeAutoShiftConfig::BestClear,
        GaugeAutoShiftConfig::SelectToUnder,
    ];
    cycle_enum(VALUES, current, direction)
}

fn gauge_auto_shift_as_str(mode: GaugeAutoShiftConfig) -> &'static str {
    match mode {
        GaugeAutoShiftConfig::Off => "OFF",
        GaugeAutoShiftConfig::Continue => "CONTINUE",
        GaugeAutoShiftConfig::HardToGroove => "HARD TO GROOVE",
        GaugeAutoShiftConfig::BestClear => "BEST CLEAR",
        GaugeAutoShiftConfig::SelectToUnder => "SELECT TO UNDER",
    }
}

fn cycle_bottom_shiftable_gauge_with_direction(
    current: BottomShiftableGaugeConfig,
    direction: i32,
) -> BottomShiftableGaugeConfig {
    const VALUES: [BottomShiftableGaugeConfig; 3] = [
        BottomShiftableGaugeConfig::AssistEasy,
        BottomShiftableGaugeConfig::Easy,
        BottomShiftableGaugeConfig::Normal,
    ];
    cycle_enum(VALUES, current, direction)
}

fn bottom_shiftable_gauge_as_str(gauge: BottomShiftableGaugeConfig) -> &'static str {
    match gauge {
        BottomShiftableGaugeConfig::AssistEasy => "A-EASY",
        BottomShiftableGaugeConfig::Easy => "EASY",
        BottomShiftableGaugeConfig::Normal => "NORMAL",
    }
}

fn bga_mode_as_str(bga: BgaModeConfig) -> &'static str {
    match bga {
        BgaModeConfig::On => "ON",
        BgaModeConfig::Auto => "AUTO",
        BgaModeConfig::Off => "OFF",
    }
}

fn volume_f32_to_unit(value: f32) -> u32 {
    (value.clamp(0.0, 1.0) * 100.0).round() as u32
}

fn cycle_arrange_option_with_direction(current: ArrangeOption, direction: i32) -> ArrangeOption {
    cycle_enum(ArrangeOption::VALUES, current, direction)
}

fn cycle_bga_option(current: BgaModeConfig) -> BgaModeConfig {
    match current {
        BgaModeConfig::On => BgaModeConfig::Auto,
        BgaModeConfig::Auto => BgaModeConfig::Off,
        BgaModeConfig::Off => BgaModeConfig::On,
    }
}

fn cycle_result_gauge_graph_type(current: i32) -> i32 {
    if (GaugeType::AssistEasy as i32..=GaugeType::Hazard as i32).contains(&current) {
        (current + 1).rem_euclid(6)
    } else {
        (current - 5).rem_euclid(3) + 6
    }
}

/// コース曲間の中間リザルトかどうか。active_course を保持したまま finished_play
/// だけが立ち、finished_course はまだ無い状態を指す。中間リザルトでは retry を
/// 無効化し、次の曲へ進むだけにする (beatoraja MusicResult のコース分岐相当)。
fn is_course_intermediate_result(
    active_course: bool,
    finished_course: bool,
    finished_play: bool,
) -> bool {
    active_course && finished_play && !finished_course
}

/// リザルト画面で押すと終了アニメーションを開始するレーン。
/// beatoraja の OK (Key1-4) / REPLAY_DIFFERENT (Key5) / REPLAY_SAME (Key7) に相当。
/// Key6 は CHANGE_GRAPH、scratch は無割り当てなので開始しない。
fn lane_starts_result_exit(lane: Lane) -> bool {
    matches!(lane, Lane::Key1 | Lane::Key2 | Lane::Key3 | Lane::Key4 | Lane::Key5 | Lane::Key7)
}

/// フェードアウト終了時の Key5/Key7 押下状態から遷移を決める。
/// beatoraja 準拠: Key5=別配置 (REPLAY_DIFFERENT)、Key7=同配置 (REPLAY_SAME)。
/// - Key7 押下 (両押し含む) → 同配置 (SameArrange)
/// - Key5 のみ押下 → 別配置 (DifferentArrange)
/// - どちらも非押下 → None (選曲へ戻る)
///
/// beatoraja は両押し時に index の若い Key5 (DIFFERENT) を優先するが、
/// 本実装はユーザー仕様として両押しを SameArrange とする。
fn result_action_for_held_lanes(key5_held: bool, key7_held: bool) -> Option<ResultRetryMode> {
    match (key5_held, key7_held) {
        (_, true) => Some(ResultRetryMode::SameArrange),
        (true, false) => Some(ResultRetryMode::DifferentArrange),
        (false, false) => None,
    }
}

fn cycle_bga_option_with_direction(current: BgaModeConfig, direction: i32) -> BgaModeConfig {
    const VALUES: [BgaModeConfig; 3] = [BgaModeConfig::On, BgaModeConfig::Auto, BgaModeConfig::Off];
    cycle_enum(VALUES, current, direction)
}

fn cycle_bga_expand_with_direction(current: BgaExpandConfig, direction: i32) -> BgaExpandConfig {
    const VALUES: [BgaExpandConfig; 3] =
        [BgaExpandConfig::KeepAspect, BgaExpandConfig::Full, BgaExpandConfig::Off];
    cycle_enum(VALUES, current, direction)
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
    physical_key_name(physical_key).is_some_and(|control| bindings.is_e2_action(&control))
}

fn should_toggle_select_gauge_auto_shift(
    control: &str,
    start_held: bool,
    select_held: bool,
    bindings: &SelectKeyBindings,
) -> bool {
    start_held && (select_held || bindings.is_e2_action(control)) && bindings.is_key2(control)
}

fn arrange_option_from_profile(random: RandomOptionConfig) -> ArrangeOption {
    match random {
        RandomOptionConfig::Mirror => ArrangeOption::Mirror,
        RandomOptionConfig::Random => ArrangeOption::Random,
        RandomOptionConfig::RRandom => ArrangeOption::RRandom,
        RandomOptionConfig::SRandom => ArrangeOption::SRandom,
        RandomOptionConfig::Spiral => ArrangeOption::Spiral,
        RandomOptionConfig::HRandom => ArrangeOption::HRandom,
        RandomOptionConfig::AllScratch => ArrangeOption::AllScratch,
        RandomOptionConfig::RandomEx => ArrangeOption::RandomEx,
        RandomOptionConfig::SRandomEx => ArrangeOption::SRandomEx,
        RandomOptionConfig::Off => ArrangeOption::Normal,
    }
}

fn random_config_from_arrange(arrange: ArrangeOption) -> RandomOptionConfig {
    match arrange {
        ArrangeOption::Normal => RandomOptionConfig::Off,
        ArrangeOption::Mirror => RandomOptionConfig::Mirror,
        ArrangeOption::Random => RandomOptionConfig::Random,
        ArrangeOption::RRandom => RandomOptionConfig::RRandom,
        ArrangeOption::SRandom => RandomOptionConfig::SRandom,
        ArrangeOption::Spiral => RandomOptionConfig::Spiral,
        ArrangeOption::HRandom => RandomOptionConfig::HRandom,
        ArrangeOption::AllScratch => RandomOptionConfig::AllScratch,
        ArrangeOption::RandomEx => RandomOptionConfig::RandomEx,
        ArrangeOption::SRandomEx => RandomOptionConfig::SRandomEx,
    }
}

fn target_option_from_profile(target: TargetOptionConfig) -> TargetOption {
    match target {
        TargetOptionConfig::None => TargetOption::None,
        TargetOptionConfig::RankA => TargetOption::RankA,
        TargetOptionConfig::RankAaMinus => TargetOption::RankAaMinus,
        TargetOptionConfig::RankAa => TargetOption::RankAa,
        TargetOptionConfig::RankAaaMinus => TargetOption::RankAaaMinus,
        TargetOptionConfig::RankAaa => TargetOption::RankAaa,
        TargetOptionConfig::RankMaxMinus => TargetOption::RankMaxMinus,
        TargetOptionConfig::Max => TargetOption::Max,
        TargetOptionConfig::RankNext => TargetOption::RankNext,
        TargetOptionConfig::IrTop => TargetOption::IrTop,
        TargetOptionConfig::IrNext => TargetOption::IrNext,
        TargetOptionConfig::RivalTop => TargetOption::RivalTop,
        TargetOptionConfig::RivalNext => TargetOption::RivalNext,
        TargetOptionConfig::RivalIndex(index) => TargetOption::RivalIndex(index),
    }
}

fn target_config_from_option(target: TargetOption) -> TargetOptionConfig {
    match target {
        TargetOption::None => TargetOptionConfig::None,
        TargetOption::RankA => TargetOptionConfig::RankA,
        TargetOption::RankAaMinus => TargetOptionConfig::RankAaMinus,
        TargetOption::RankAa => TargetOptionConfig::RankAa,
        TargetOption::RankAaaMinus => TargetOptionConfig::RankAaaMinus,
        TargetOption::RankAaa => TargetOptionConfig::RankAaa,
        TargetOption::RankMaxMinus => TargetOptionConfig::RankMaxMinus,
        TargetOption::Max => TargetOptionConfig::Max,
        TargetOption::RankNext => TargetOptionConfig::RankNext,
        TargetOption::IrTop => TargetOptionConfig::IrTop,
        TargetOption::IrNext => TargetOptionConfig::IrNext,
        TargetOption::RivalTop => TargetOptionConfig::RivalTop,
        TargetOption::RivalNext => TargetOptionConfig::RivalNext,
        TargetOption::RivalIndex(index) => TargetOptionConfig::RivalIndex(index),
    }
}

#[derive(Debug, Clone, Copy)]
struct ActiveLaneState {
    lane_cover: f32,
    lift: f32,
    hispeed_mode: HispeedMode,
    target_green_number: u32,
}

fn apply_current_play_options_to_profile(
    profile: &mut ProfileConfig,
    hispeed: Option<f32>,
    lane_state: Option<ActiveLaneState>,
    arrange: ArrangeOption,
    target: TargetOption,
    gauge: GaugeTypeConfig,
    gauge_auto_shift: GaugeAutoShiftConfig,
    bottom_shiftable_gauge: BottomShiftableGaugeConfig,
    assist: AssistOption,
    updated_at: i64,
) {
    if let Some(hispeed) = hispeed {
        profile.lane.hispeed = clamp_hispeed_for_profile(hispeed);
    }
    if let Some(state) = lane_state {
        profile.lane.sudden = crate::config::play::lane_f32_to_unit(state.lane_cover);
        profile.lane.lift = crate::config::play::lane_f32_to_unit(state.lift);
        profile.lane.hispeed_mode = hispeed_mode_to_config(state.hispeed_mode);
        profile.lane.target_green_number = state.target_green_number.max(1);
    }
    profile.play.random = random_config_from_arrange(arrange);
    profile.play.target = target_config_from_option(target);
    profile.play.gauge = gauge;
    profile.play.gauge_auto_shift = gauge_auto_shift;
    profile.play.bottom_shiftable_gauge = bottom_shiftable_gauge;
    profile.play.auto_play = assist == AssistOption::Autoplay;
    profile.play.assist = AssistOptionConfig::None;
    profile.updated_at = updated_at;
}

fn clamp_hispeed_for_profile(hispeed: f32) -> f32 {
    (hispeed * 4.0).round().clamp(2.0, 40.0) / 4.0
}

fn hispeed_mode_to_config(mode: HispeedMode) -> HispeedModeConfig {
    match mode {
        HispeedMode::Normal => HispeedModeConfig::Normal,
        HispeedMode::Floating => HispeedModeConfig::Floating,
    }
}

fn select_chart_distribution(
    distribution: &[ChartDistributionSecond],
) -> Vec<SelectChartDistributionSecond> {
    distribution
        .iter()
        .map(|second| SelectChartDistributionSecond {
            scratch_long_heads: second.scratch_long_heads,
            scratch_long_bodies: second.scratch_long_bodies,
            scratch_taps: second.scratch_taps,
            key_long_heads: second.key_long_heads,
            key_long_bodies: second.key_long_bodies,
            key_taps: second.key_taps,
            mines: second.mines,
        })
        .collect()
}

fn select_bpm_graph_segments(
    speed_changes: &[crate::storage::library_db::ChartSpeedChange],
    length_ms: i64,
) -> Vec<bmz_render::chart_graph::BpmGraphSegment> {
    let duration_ms = length_ms.max(1) as f32;
    let mut segments = Vec::new();
    for (index, change) in speed_changes.iter().enumerate() {
        let start_ms = change.time_ms.max(0) as f32;
        let end_ms = speed_changes
            .get(index + 1)
            .map(|next| next.time_ms.max(change.time_ms) as f32)
            .unwrap_or(duration_ms)
            .min(duration_ms);
        if end_ms <= start_ms {
            continue;
        }
        segments.push(bmz_render::chart_graph::BpmGraphSegment {
            start_ratio: (start_ms / duration_ms).clamp(0.0, 1.0),
            end_ratio: (end_ms / duration_ms).clamp(0.0, 1.0),
            bpm: change.speed.max(0.0) as f32,
            is_stop: change.speed == 0.0,
        });
    }
    segments
}

fn select_visible_item_indices(
    item_len: usize,
    selected_index: usize,
    visible_limit: usize,
) -> Vec<usize> {
    if item_len == 0 || visible_limit == 0 {
        return Vec::new();
    }

    let row_count = visible_limit;
    let selected_index = selected_index.min(item_len - 1);
    let half_window = row_count / 2;
    let start = (selected_index + item_len - (half_window % item_len)) % item_len;

    (0..row_count).map(|offset| (start + offset) % item_len).collect()
}

fn select_snapshot_rows(
    items: &[SelectItem],
    selected_index: usize,
    visible_limit: usize,
    profile: &ProfileConfig,
    key_config_edit: Option<&KeyConfigEditSession>,
    chart_distributions: &HashMap<i64, Vec<ChartDistributionSecond>>,
) -> Vec<SelectRowSnapshot> {
    let visible_indices = select_visible_item_indices(items.len(), selected_index, visible_limit);
    if visible_indices.is_empty() {
        return Vec::new();
    }

    visible_indices
        .into_iter()
        .map(|index| {
            let item = &items[index];
            match item {
                SelectItem::Folder { name, kind, summary, .. } => SelectRowSnapshot {
                    index: index as u32,
                    title: name.clone(),
                    subtitle: String::new(),
                    artist: String::new(),
                    difficulty_name: String::new(),
                    play_level: String::new(),
                    table_level: String::new(),
                    judge_rank: None,
                    total_notes: 0,
                    initial_bpm: 0.0,
                    min_bpm: 0.0,
                    max_bpm: 0.0,
                    length_ms: 0,
                    clear_type: summary
                        .as_ref()
                        .map(|summary| summary.clear_type())
                        .unwrap_or_default(),
                    ex_score: None,
                    max_combo: None,
                    gauge_value: None,
                    bp: None,
                    cb: None,
                    judge_counts: DisplayJudgeCounts::default(),
                    fast_slow_counts: None,
                    play_count: 0,
                    clear_count: 0,
                    replay_slots: [false; 4],
                    has_long_notes: false,
                    has_mines: false,
                    has_random: false,
                    chart_normal_notes: 0,
                    chart_long_notes: 0,
                    chart_scratch_notes: 0,
                    chart_long_scratch_notes: 0,
                    chart_mine_notes: 0,
                    chart_density: 0.0,
                    chart_peak_density: 0.0,
                    chart_end_density: 0.0,
                    chart_total_gauge: 0.0,
                    chart_main_bpm: 0.0,
                    chart_distribution: Vec::new(),
                    chart_bpm_graph_segments: Vec::new(),
                    folder_lamp_counts: summary
                        .as_ref()
                        .map(|summary| summary.lamp_counts)
                        .unwrap_or([0; 11]),
                    is_folder: true,
                    kind: *kind,
                    in_library: true,
                    achieved_trophy_names: Vec::new(),
                    course_titles: Default::default(),
                    course_constraints: Default::default(),
                    chart_key_mode: None,
                },
                SelectItem::Chart(row) => {
                    let play_count =
                        row.best_score.as_ref().map(|score| score.play_count).unwrap_or(0);
                    let clear_count =
                        row.best_score.as_ref().map(|score| score.clear_count).unwrap_or(0);
                    SelectRowSnapshot {
                        index: index as u32,
                        title: row.display_title().to_string(),
                        subtitle: row
                            .chart
                            .as_ref()
                            .map(|chart| chart.subtitle.clone())
                            .unwrap_or_default(),
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
                        judge_rank: row.chart.as_ref().and_then(|chart| chart.judge_rank),
                        total_notes: row.chart.as_ref().map(|chart| chart.total_notes).unwrap_or(0),
                        initial_bpm: row
                            .chart
                            .as_ref()
                            .map(|chart| chart.initial_bpm as f32)
                            .unwrap_or(0.0),
                        min_bpm: row
                            .chart
                            .as_ref()
                            .map(|chart| chart.min_bpm as f32)
                            .unwrap_or(0.0),
                        max_bpm: row
                            .chart
                            .as_ref()
                            .map(|chart| chart.max_bpm as f32)
                            .unwrap_or(0.0),
                        length_ms: row.chart.as_ref().map(|chart| chart.length_ms).unwrap_or(0),
                        clear_type: row
                            .best_score
                            .as_ref()
                            .map(|score| score.clear_type.clone())
                            .unwrap_or_default(),
                        ex_score: row.best_score.as_ref().map(|score| score.ex_score),
                        max_combo: row.best_score.as_ref().map(|score| score.max_combo),
                        gauge_value: row.best_score.as_ref().map(|score| score.gauge_value),
                        bp: row.best_score.as_ref().map(|score| score.bp),
                        cb: row.best_score.as_ref().map(|score| score.cb),
                        judge_counts: row
                            .best_score
                            .as_ref()
                            .map(|score| score.judge_counts)
                            .unwrap_or_default(),
                        fast_slow_counts: row
                            .best_score
                            .as_ref()
                            .map(|score| score.fast_slow_counts),
                        play_count,
                        clear_count,
                        replay_slots: row.replay_slots,
                        has_long_notes: row
                            .chart
                            .as_ref()
                            .is_some_and(|chart| chart.has_long_notes),
                        has_mines: row.chart.as_ref().is_some_and(|chart| chart.has_mines),
                        has_random: false,
                        chart_normal_notes: row
                            .chart_analysis
                            .as_ref()
                            .map(|analysis| analysis.normal_notes)
                            .unwrap_or_else(|| {
                                row.chart.as_ref().map(|chart| chart.total_notes).unwrap_or(0)
                            }),
                        chart_long_notes: row
                            .chart_analysis
                            .as_ref()
                            .map(|analysis| analysis.long_notes)
                            .unwrap_or(0),
                        chart_scratch_notes: row
                            .chart_analysis
                            .as_ref()
                            .map(|analysis| analysis.scratch_notes)
                            .unwrap_or(0),
                        chart_long_scratch_notes: row
                            .chart_analysis
                            .as_ref()
                            .map(|analysis| analysis.long_scratch_notes)
                            .unwrap_or(0),
                        chart_density: row
                            .chart_analysis
                            .as_ref()
                            .map(|analysis| analysis.density as f32)
                            .unwrap_or(0.0),
                        chart_peak_density: row
                            .chart_analysis
                            .as_ref()
                            .map(|analysis| analysis.peak_density as f32)
                            .unwrap_or(0.0),
                        chart_end_density: row
                            .chart_analysis
                            .as_ref()
                            .map(|analysis| analysis.end_density as f32)
                            .unwrap_or(0.0),
                        chart_total_gauge: row
                            .chart
                            .as_ref()
                            .map(|chart| chart.bms_total as f32)
                            .unwrap_or(0.0),
                        chart_main_bpm: row
                            .chart_analysis
                            .as_ref()
                            .map(|analysis| analysis.main_bpm as f32)
                            .unwrap_or_else(|| {
                                row.chart
                                    .as_ref()
                                    .map(|chart| chart.initial_bpm as f32)
                                    .unwrap_or(0.0)
                            }),
                        chart_distribution: row
                            .chart
                            .as_ref()
                            .and_then(|chart| chart_distributions.get(&chart.chart_id))
                            .map(|distribution| select_chart_distribution(distribution))
                            .unwrap_or_default(),
                        chart_mine_notes: row
                            .chart
                            .as_ref()
                            .and_then(|chart| chart_distributions.get(&chart.chart_id))
                            .map(|distribution| {
                                distribution.iter().map(|second| u32::from(second.mines)).sum()
                            })
                            .unwrap_or(0),
                        chart_bpm_graph_segments: row
                            .chart_analysis
                            .as_ref()
                            .map(|analysis| {
                                select_bpm_graph_segments(
                                    &analysis.speed_changes,
                                    row.chart.as_ref().map(|chart| chart.length_ms).unwrap_or(0),
                                )
                            })
                            .unwrap_or_default(),
                        folder_lamp_counts: [0; 11],
                        is_folder: false,
                        kind: bmz_render::scene::SelectRowKind::Song,
                        in_library: row.in_library(),
                        // Song rows have no course trophies.
                        achieved_trophy_names: Vec::new(),
                        course_titles: Default::default(),
                        course_constraints: Default::default(),
                        chart_key_mode: row
                            .chart
                            .as_ref()
                            .and_then(|chart| KeyMode::from_str_opt(&chart.mode)),
                    }
                }
                SelectItem::Course(row) => SelectRowSnapshot {
                    index: index as u32,
                    title: row.title.clone(),
                    subtitle: String::new(),
                    // Use the trophy names joined as "subtitle" so the artist
                    // slot shows e.g. "silvermedal / goldmedal".
                    artist: row.trophy_names.join(" / "),
                    // Beatoraja-style category tag (DAN / COURSE).
                    difficulty_name: row.category_label.clone(),
                    // Show "N stages" in the play_level slot.
                    play_level: format!("{} stages", row.entry_count),
                    table_level: String::new(),
                    judge_rank: None,
                    total_notes: row.total_notes,
                    initial_bpm: row.min_bpm,
                    min_bpm: row.min_bpm,
                    max_bpm: row.max_bpm,
                    length_ms: row.total_length_ms,
                    clear_type: row
                        .best_score
                        .as_ref()
                        .map(|best| best.clear_type.clone())
                        .unwrap_or_default(),
                    ex_score: row.best_score.as_ref().map(|best| best.ex_score),
                    max_combo: row.best_score.as_ref().map(|best| best.max_combo),
                    gauge_value: row.best_score.as_ref().map(|best| best.gauge_value),
                    bp: row.best_score.as_ref().map(|best| best.bp),
                    cb: None,
                    judge_counts: DisplayJudgeCounts::default(),
                    fast_slow_counts: None,
                    play_count: u32::from(row.best_score.is_some()),
                    clear_count: u32::from(row.best_score.as_ref().is_some_and(|best| {
                        !best.clear_type.is_empty() && best.clear_type != "Failed"
                    })),
                    replay_slots: row.replay_slots,
                    has_long_notes: false,
                    has_mines: false,
                    has_random: false,
                    chart_normal_notes: 0,
                    chart_long_notes: 0,
                    chart_scratch_notes: 0,
                    chart_long_scratch_notes: 0,
                    chart_mine_notes: 0,
                    chart_density: 0.0,
                    chart_peak_density: 0.0,
                    chart_end_density: 0.0,
                    chart_total_gauge: 0.0,
                    chart_main_bpm: 0.0,
                    chart_distribution: Vec::new(),
                    chart_bpm_graph_segments: Vec::new(),
                    folder_lamp_counts: [0; 11],
                    is_folder: false,
                    kind: bmz_render::scene::SelectRowKind::Course,
                    in_library: row.exists_all_songs(),
                    achieved_trophy_names: row.achieved_trophy_names.clone(),
                    course_titles: course_titles_from_entries(
                        row.entry_previews
                            .iter()
                            .map(|entry| (entry.title.as_str(), entry.resolved)),
                    ),
                    course_constraints: course_constraint_flags(&row.constraints),
                    chart_key_mode: None,
                },
                SelectItem::Config(row) => {
                    let value = row.value_text(profile);
                    SelectRowSnapshot {
                        index: index as u32,
                        title: row.label().to_string(),
                        subtitle: String::new(),
                        artist: value.clone(),
                        difficulty_name: String::new(),
                        play_level: value,
                        table_level: String::new(),
                        judge_rank: None,
                        total_notes: 0,
                        initial_bpm: 0.0,
                        min_bpm: 0.0,
                        max_bpm: 0.0,
                        length_ms: 0,
                        clear_type: String::new(),
                        ex_score: None,
                        max_combo: None,
                        gauge_value: None,
                        bp: None,
                        cb: None,
                        judge_counts: DisplayJudgeCounts::default(),
                        fast_slow_counts: None,
                        play_count: 0,
                        clear_count: 0,
                        replay_slots: [false; 4],
                        has_long_notes: false,
                        has_mines: false,
                        has_random: false,
                        chart_normal_notes: 0,
                        chart_long_notes: 0,
                        chart_scratch_notes: 0,
                        chart_long_scratch_notes: 0,
                        chart_mine_notes: 0,
                        chart_density: 0.0,
                        chart_peak_density: 0.0,
                        chart_end_density: 0.0,
                        chart_total_gauge: 0.0,
                        chart_main_bpm: 0.0,
                        chart_distribution: Vec::new(),
                        chart_bpm_graph_segments: Vec::new(),
                        folder_lamp_counts: [0; 11],
                        is_folder: false,
                        kind: bmz_render::scene::SelectRowKind::Config,
                        in_library: true,
                        achieved_trophy_names: Vec::new(),
                        course_titles: Default::default(),
                        course_constraints: Default::default(),
                        chart_key_mode: None,
                    }
                }
                SelectItem::KeyBinding(row) => {
                    let value = key_config_edit
                        .filter(|session| {
                            session.key_mode == row.key_mode && session.target == row.target
                        })
                        .map(|session| session.preview_value(profile))
                        .unwrap_or_else(|| row.value_text(profile));
                    SelectRowSnapshot {
                        index: index as u32,
                        title: row.label(),
                        subtitle: String::new(),
                        artist: value.clone(),
                        difficulty_name: String::new(),
                        play_level: value,
                        table_level: String::new(),
                        judge_rank: None,
                        total_notes: 0,
                        initial_bpm: 0.0,
                        min_bpm: 0.0,
                        max_bpm: 0.0,
                        length_ms: 0,
                        clear_type: String::new(),
                        ex_score: None,
                        max_combo: None,
                        gauge_value: None,
                        bp: None,
                        cb: None,
                        judge_counts: DisplayJudgeCounts::default(),
                        fast_slow_counts: None,
                        play_count: 0,
                        clear_count: 0,
                        replay_slots: [false; 4],
                        has_long_notes: false,
                        has_mines: false,
                        has_random: false,
                        chart_normal_notes: 0,
                        chart_long_notes: 0,
                        chart_scratch_notes: 0,
                        chart_long_scratch_notes: 0,
                        chart_mine_notes: 0,
                        chart_density: 0.0,
                        chart_peak_density: 0.0,
                        chart_end_density: 0.0,
                        chart_total_gauge: 0.0,
                        chart_main_bpm: 0.0,
                        chart_distribution: Vec::new(),
                        chart_bpm_graph_segments: Vec::new(),
                        folder_lamp_counts: [0; 11],
                        is_folder: false,
                        kind: bmz_render::scene::SelectRowKind::Config,
                        in_library: true,
                        achieved_trophy_names: Vec::new(),
                        course_titles: Default::default(),
                        course_constraints: Default::default(),
                        chart_key_mode: None,
                    }
                }
                SelectItem::Back => SelectRowSnapshot {
                    index: index as u32,
                    title: "戻る".to_string(),
                    subtitle: String::new(),
                    artist: String::new(),
                    difficulty_name: String::new(),
                    play_level: String::new(),
                    table_level: String::new(),
                    judge_rank: None,
                    total_notes: 0,
                    initial_bpm: 0.0,
                    min_bpm: 0.0,
                    max_bpm: 0.0,
                    length_ms: 0,
                    clear_type: String::new(),
                    ex_score: None,
                    max_combo: None,
                    gauge_value: None,
                    bp: None,
                    cb: None,
                    judge_counts: DisplayJudgeCounts::default(),
                    fast_slow_counts: None,
                    play_count: 0,
                    clear_count: 0,
                    replay_slots: [false; 4],
                    has_long_notes: false,
                    has_mines: false,
                    has_random: false,
                    chart_normal_notes: 0,
                    chart_long_notes: 0,
                    chart_scratch_notes: 0,
                    chart_long_scratch_notes: 0,
                    chart_mine_notes: 0,
                    chart_density: 0.0,
                    chart_peak_density: 0.0,
                    chart_end_density: 0.0,
                    chart_total_gauge: 0.0,
                    chart_main_bpm: 0.0,
                    chart_distribution: Vec::new(),
                    chart_bpm_graph_segments: Vec::new(),
                    folder_lamp_counts: [0; 11],
                    is_folder: true,
                    kind: bmz_render::scene::SelectRowKind::SearchFolder,
                    in_library: true,
                    achieved_trophy_names: Vec::new(),
                    course_titles: Default::default(),
                    course_constraints: Default::default(),
                    chart_key_mode: None,
                },
                SelectItem::AdvancedSettings => SelectRowSnapshot {
                    index: index as u32,
                    title: "詳細設定".to_string(),
                    subtitle: String::new(),
                    artist: String::new(),
                    difficulty_name: String::new(),
                    play_level: String::new(),
                    table_level: String::new(),
                    judge_rank: None,
                    total_notes: 0,
                    initial_bpm: 0.0,
                    min_bpm: 0.0,
                    max_bpm: 0.0,
                    length_ms: 0,
                    clear_type: String::new(),
                    ex_score: None,
                    max_combo: None,
                    gauge_value: None,
                    bp: None,
                    cb: None,
                    judge_counts: DisplayJudgeCounts::default(),
                    fast_slow_counts: None,
                    play_count: 0,
                    clear_count: 0,
                    replay_slots: [false; 4],
                    has_long_notes: false,
                    has_mines: false,
                    has_random: false,
                    chart_normal_notes: 0,
                    chart_long_notes: 0,
                    chart_scratch_notes: 0,
                    chart_long_scratch_notes: 0,
                    chart_mine_notes: 0,
                    chart_density: 0.0,
                    chart_peak_density: 0.0,
                    chart_end_density: 0.0,
                    chart_total_gauge: 0.0,
                    chart_main_bpm: 0.0,
                    chart_distribution: Vec::new(),
                    chart_bpm_graph_segments: Vec::new(),
                    folder_lamp_counts: [0; 11],
                    is_folder: true,
                    kind: bmz_render::scene::SelectRowKind::SettingsFolder,
                    in_library: true,
                    achieved_trophy_names: Vec::new(),
                    course_titles: Default::default(),
                    course_constraints: Default::default(),
                    chart_key_mode: None,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectRowClickAction {
    Select(usize),
    EnterOrPlay,
    ExitFolder,
}

fn select_row_click_action(
    row_index: u32,
    button: MouseButton,
    selected_index: usize,
    item_len: usize,
) -> Option<SelectRowClickAction> {
    match button {
        MouseButton::Left => {
            let next = row_index as usize;
            if next >= item_len {
                None
            } else if next == selected_index {
                Some(SelectRowClickAction::EnterOrPlay)
            } else {
                Some(SelectRowClickAction::Select(next))
            }
        }
        MouseButton::Right => Some(SelectRowClickAction::ExitFolder),
        _ => None,
    }
}

fn select_scroll_slider_index(value: f32, item_len: usize) -> Option<usize> {
    if item_len == 0 {
        return None;
    }
    if item_len == 1 {
        return Some(0);
    }
    let max_index = item_len - 1;
    Some((value.clamp(0.0, 1.0) * max_index as f32).round() as usize)
}

fn select_scroll_duration_low_ms(config: &crate::config::app_config::AppConfig) -> u32 {
    config.select.scroll_duration_low_ms.clamp(2, 1000)
}

fn select_scroll_duration_high_ms(config: &crate::config::app_config::AppConfig) -> u32 {
    config.select.scroll_duration_high_ms.clamp(1, 1000)
}

fn select_analog_scroll_duration(mov: i32) -> Duration {
    let remaining = mov.abs().clamp(1, 2);
    Duration::from_millis((120 / remaining / remaining) as u64)
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
    } else if bindings.is_scratch_up(&control) {
        if bindings.is_scratch_down(&control) {
            Some(SelectAction::Move(SelectMove::Next))
        } else {
            Some(SelectAction::Move(SelectMove::Previous))
        }
    } else if bindings.is_scratch_down(&control) {
        Some(SelectAction::Move(SelectMove::Next))
    } else {
        None
    }
}

fn select_wheel_move(delta: MouseScrollDelta) -> Option<SelectMove> {
    let y = mouse_wheel_y(delta);

    if y > 0.0 {
        Some(SelectMove::Previous)
    } else if y < 0.0 {
        Some(SelectMove::Next)
    } else {
        None
    }
}

fn lane_cover_wheel_change(delta: MouseScrollDelta) -> Option<LaneCoverChange> {
    let y = mouse_wheel_y(delta);
    if y > 0.0 {
        Some(LaneCoverChange::Up)
    } else if y < 0.0 {
        Some(LaneCoverChange::Down)
    } else {
        None
    }
}

fn mouse_wheel_y(delta: MouseScrollDelta) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => y,
        MouseScrollDelta::PixelDelta(position) => position.y as f32,
    }
}

fn update_pre_ready_play_snapshot_options_for_session(
    ready_sound_started_at: Option<Instant>,
    last_play_snapshot: &mut Option<RenderSnapshot>,
    session: &bmz_gameplay::session::GameSession,
    arrange: ArrangeOption,
) {
    if ready_sound_started_at.is_some() {
        return;
    }
    let Some(snapshot) = last_play_snapshot else {
        return;
    };
    crate::screens::play_snapshot::update_render_snapshot_play_options(
        snapshot,
        session,
        snapshot.time,
    );
    snapshot.arrange = arrange.as_str().to_string();
}

fn update_play_exit_hold_started_at(
    started_at: &mut Option<Instant>,
    e1_held: bool,
    e2_held: bool,
    now: Instant,
) {
    if e1_held && e2_held {
        started_at.get_or_insert(now);
    } else {
        *started_at = None;
    }
}

fn play_exit_hold_elapsed(started_at: Option<Instant>, now: Instant, duration: Duration) -> bool {
    started_at.is_some_and(|started_at| now.duration_since(started_at) >= duration)
}

fn select_click_event_arg(
    click_type: i32,
    button: MouseButton,
    rect: Rect,
    x: f32,
    y: f32,
) -> Option<i32> {
    let button_arg = match button {
        MouseButton::Left => 1,
        MouseButton::Right => -1,
        MouseButton::Middle => 1,
        _ => return None,
    };
    match click_type {
        0 => Some(button_arg),
        1 => Some(-button_arg),
        2 => Some(if x >= rect.x + rect.width * 0.5 { 1 } else { -1 }),
        3 => Some(if y <= rect.y + rect.height * 0.5 { 1 } else { -1 }),
        _ => None,
    }
}

fn course_titles_from_entries<'a>(
    entries: impl IntoIterator<Item = (&'a str, bool)>,
) -> [String; 10] {
    let mut titles: [String; 10] = Default::default();
    for (index, (title, resolved)) in entries.into_iter().take(10).enumerate() {
        titles[index] = if resolved {
            title.to_string()
        } else {
            format!("(no song) {}", if title.is_empty() { "----" } else { title })
        };
    }
    titles
}

fn course_constraint_flags(
    constraints: &bmz_core::course::CourseConstraints,
) -> bmz_render::scene::CourseConstraintFlags {
    use bmz_core::course::{
        CourseClassConstraint, CourseGaugeConstraint, CourseJudgeConstraint, CourseLnConstraint,
        CourseSpeedConstraint,
    };

    bmz_render::scene::CourseConstraintFlags {
        class: constraints.class == CourseClassConstraint::Grade,
        mirror: constraints.class == CourseClassConstraint::GradeMirrorAllowed,
        random: constraints.class == CourseClassConstraint::GradeRandomAllowed,
        no_speed: constraints.speed == CourseSpeedConstraint::NoSpeed,
        no_good: constraints.judge == CourseJudgeConstraint::NoGood,
        no_great: constraints.judge == CourseJudgeConstraint::NoGreat,
        gauge_lr2: constraints.gauge == CourseGaugeConstraint::Lr2,
        gauge_5k: constraints.gauge == CourseGaugeConstraint::Keys5,
        gauge_7k: constraints.gauge == CourseGaugeConstraint::Keys7,
        gauge_9k: constraints.gauge == CourseGaugeConstraint::Keys9,
        gauge_24k: constraints.gauge == CourseGaugeConstraint::Keys24,
        ln: constraints.ln == CourseLnConstraint::Ln,
        cn: constraints.ln == CourseLnConstraint::Cn,
        hcn: constraints.ln == CourseLnConstraint::Hcn,
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

fn select_move_scroll_direction(select_move: SelectMove) -> i32 {
    match select_move {
        SelectMove::Previous | SelectMove::PagePrevious => -1,
        SelectMove::Next | SelectMove::PageNext => 1,
        SelectMove::First | SelectMove::Last => 0,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlayOptionControl {
    ToggleHispeedMode,
    Hispeed(HispeedChange),
    LaneCover(LaneCoverChange),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaneCoverChange {
    Up,
    Down,
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

fn play_option_control(control: &str, bindings: &SelectKeyBindings) -> Option<PlayOptionControl> {
    if bindings.is_e2_action(control) {
        return Some(PlayOptionControl::ToggleHispeedMode);
    }
    if bindings.is_hispeed_down_key(control) {
        return Some(PlayOptionControl::Hispeed(HispeedChange::Down));
    }
    if bindings.is_hispeed_up_key(control) {
        return Some(PlayOptionControl::Hispeed(HispeedChange::Up));
    }
    if bindings.is_scratch_up(control) {
        return Some(PlayOptionControl::LaneCover(LaneCoverChange::Up));
    }
    if bindings.is_scratch_down(control) {
        return Some(PlayOptionControl::LaneCover(LaneCoverChange::Down));
    }
    None
}

fn visual_offset_delta_control(control: &str, bindings: &SelectKeyBindings) -> Option<i32> {
    if bindings.is_key5(control) {
        Some(-1)
    } else if bindings.is_key7(control) {
        Some(1)
    } else {
        None
    }
}

fn lane_cover_step(physical_key: PhysicalKey, state: ElementState, repeat: bool) -> Option<f32> {
    if state != ElementState::Pressed {
        return None;
    }
    let step = if repeat { LANE_COVER_REPEAT_STEP } else { LANE_COVER_STEP };
    match physical_key {
        // 上キー: カバー位置を上げる(下方向への余白を縮める = 値を増やす)
        PhysicalKey::Code(KeyCode::ArrowUp) => Some(step),
        PhysicalKey::Code(KeyCode::ArrowDown) => Some(-step),
        _ => None,
    }
}

fn lane_cover_change_step(change: LaneCoverChange) -> f32 {
    match change {
        LaneCoverChange::Up => LANE_COVER_STEP,
        LaneCoverChange::Down => -LANE_COVER_STEP,
    }
}

fn adjusted_hispeed(current: f32, change: HispeedChange) -> f32 {
    let delta = match change {
        HispeedChange::Down => -0.25,
        HispeedChange::Up => 0.25,
    };
    ((current + delta) * 4.0).round().clamp(2.0, 40.0) / 4.0
}

fn apply_play_option_control_to_session(
    session: &mut bmz_gameplay::session::GameSession,
    action: PlayOptionControl,
    speed_locked: bool,
) -> bool {
    match action {
        PlayOptionControl::ToggleHispeedMode => {
            match session.hispeed_mode {
                HispeedMode::Normal => {
                    let now = session.audio_clock.now();
                    session.target_green_number = current_green_number(session, now);
                    session.hispeed_mode = HispeedMode::Floating;
                }
                HispeedMode::Floating => {
                    session.hispeed = clamp_hispeed_for_profile(session.hispeed);
                    session.hispeed_mode = HispeedMode::Normal;
                }
            }
            true
        }
        PlayOptionControl::Hispeed(change) => {
            if speed_locked {
                return false;
            }
            session.hispeed = adjusted_hispeed(session.hispeed, change);
            true
        }
        PlayOptionControl::LaneCover(change) => {
            apply_lane_cover_step_to_session(session, lane_cover_change_step(change), speed_locked)
        }
    }
}

fn apply_lane_cover_step_to_session(
    session: &mut bmz_gameplay::session::GameSession,
    delta: f32,
    speed_locked: bool,
) -> bool {
    if session.lane_cover_visible {
        session.lane_cover = (session.lane_cover - delta).clamp(0.0, 1.0);
        if session.hispeed_mode == HispeedMode::Floating && !speed_locked {
            let now = session.audio_clock.now();
            session.hispeed = hispeed_for_green_number(session, session.lane_cover, now);
        }
    } else {
        session.lift = (session.lift + delta).clamp(0.0, 1.0);
    }
    true
}

fn reset_floating_hispeed_if_enabled(
    session: &mut bmz_gameplay::session::GameSession,
    speed_locked: bool,
) {
    if session.hispeed_mode == HispeedMode::Floating && !speed_locked {
        let now = session.audio_clock.now();
        let lane_cover = if session.lane_cover_visible { session.lane_cover } else { 0.0 };
        session.hispeed = hispeed_for_green_number(session, lane_cover, now);
    }
}

fn current_green_number(session: &bmz_gameplay::session::GameSession, now: TimeUs) -> u32 {
    let total = note_display_duration_ms_for_hispeed(
        session,
        session.hispeed,
        if session.lane_cover_visible { session.lane_cover } else { 0.0 },
        now,
    );
    ((total * 0.6).round().clamp(1.0, u32::MAX as f32)) as u32
}

fn note_display_duration_ms_for_hispeed(
    session: &bmz_gameplay::session::GameSession,
    hispeed: f32,
    lane_cover: f32,
    now: TimeUs,
) -> f32 {
    let visible_max = (1.0 - lane_cover).clamp(0.0, 1.0);
    let initial_bpm = session.chart.metadata.initial_bpm.max(1.0);
    let now_bpm = crate::screens::play_snapshot::current_bpm(&session.chart, now).max(1.0);
    let bpm_ratio = (initial_bpm / now_bpm) as f32;
    crate::screens::play_snapshot::DEFAULT_LOOKAHEAD_US as f32 / hispeed.max(0.01)
        * visible_max
        * bpm_ratio
        / 1_000.0
}

fn hispeed_for_green_number(
    session: &bmz_gameplay::session::GameSession,
    lane_cover: f32,
    now: TimeUs,
) -> f32 {
    let target_green = session.target_green_number.max(1) as f32;
    let visible_max = (1.0 - lane_cover).clamp(0.0, 1.0);
    let initial_bpm = session.chart.metadata.initial_bpm.max(1.0);
    let now_bpm = crate::screens::play_snapshot::current_bpm(&session.chart, now).max(1.0);
    let hispeed = hispeed_for_green_number_values(target_green, visible_max, initial_bpm, now_bpm);
    hispeed.clamp(0.5, 10.0)
}

fn hispeed_for_green_number_values(
    target_green: f32,
    visible_max: f32,
    initial_bpm: f64,
    now_bpm: f64,
) -> f32 {
    let bpm_ratio = (initial_bpm.max(1.0) / now_bpm.max(1.0)) as f32;
    crate::screens::play_snapshot::DEFAULT_LOOKAHEAD_US as f32
        * visible_max.clamp(0.0, 1.0)
        * bpm_ratio
        * 0.6
        / (target_green.max(1.0) * 1_000.0)
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

fn preloaded_matches_start(
    preloaded: &PreloadedWinitPlaySession,
    chart_id: i64,
    options: &PlayStartOptions,
) -> bool {
    preloaded.chart_id == chart_id
        && preloaded.session_options.autoplay == options.autoplay
        && preloaded.session_options.practice_mode == options.practice_mode
        && preloaded.session_options.arrange == options.arrange
        && preloaded.session_options.arrange_seed == options.arrange_seed
        && preloaded.session_options.arrange_pattern == options.arrange_pattern
}

fn skin_duration_ms(ms: i32) -> Duration {
    Duration::from_millis(ms.max(0) as u64)
}

fn decide_fadeout_scene_elapsed(
    fadeout_started_elapsed: Duration,
    fadeout_elapsed: Duration,
    scene_duration: Duration,
    fadeout_duration: Duration,
    timing: DecideFadeoutSceneTiming,
) -> Duration {
    let direct_elapsed = fadeout_started_elapsed.saturating_add(fadeout_elapsed);
    let tail_elapsed = match timing {
        DecideFadeoutSceneTiming::DirectOnly => direct_elapsed,
        DecideFadeoutSceneTiming::TailStart(tail_start) if fadeout_duration > Duration::ZERO => {
            let tail_start = tail_start.min(scene_duration);
            let tail_duration = scene_duration.saturating_sub(tail_start);
            if tail_duration > Duration::ZERO {
                let scaled = scale_duration(
                    fadeout_elapsed.min(fadeout_duration),
                    tail_duration,
                    fadeout_duration,
                );
                tail_start.saturating_add(scaled).min(scene_duration)
            } else {
                scene_duration
            }
        }
        DecideFadeoutSceneTiming::TailStart(_) => scene_duration,
        DecideFadeoutSceneTiming::DefaultTail => {
            let tail_start = scene_duration.checked_sub(fadeout_duration).unwrap_or_default();
            tail_start.saturating_add(fadeout_elapsed).min(scene_duration)
        }
    };
    direct_elapsed.max(tail_elapsed)
}

fn scale_duration(value: Duration, numerator: Duration, denominator: Duration) -> Duration {
    if denominator == Duration::ZERO {
        return Duration::ZERO;
    }
    let micros = value
        .as_micros()
        .saturating_mul(numerator.as_micros())
        .checked_div(denominator.as_micros())
        .unwrap_or(0);
    Duration::from_micros(micros.min(u64::MAX as u128) as u64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecideFadeoutSceneTiming {
    /// `timer=2` が fadeout を担う skin。scene 時刻を終端へ飛ばすと
    /// timer なしの終了演出まで同時に進み、暗転が即飽和する。
    DirectOnly,
    /// timer=2 が無い skin 向け。従来通り fadeout 中は scene 末尾へ寄せる。
    DefaultTail,
    /// m-select のように scene 末尾の黒フェードを fadeout として使う skin。
    TailStart(Duration),
}

fn decide_fadeout_scene_timing(document: Option<&SkinDocument>) -> DecideFadeoutSceneTiming {
    let Some(document) = document else {
        return DecideFadeoutSceneTiming::DefaultTail;
    };
    if document_has_fadeout_timer_black(document) {
        return DecideFadeoutSceneTiming::DirectOnly;
    }
    decide_scene_fadeout_tail_start(Some(document))
        .map(skin_duration_ms)
        .map_or(DecideFadeoutSceneTiming::DefaultTail, DecideFadeoutSceneTiming::TailStart)
}

fn decide_scene_fadeout_tail_start(document: Option<&SkinDocument>) -> Option<i32> {
    let document = document?;
    if document.scene <= 0 || document.w == 0 || document.h == 0 {
        return None;
    }
    if document_has_fadeout_timer_black(document) {
        return None;
    }
    document
        .destination
        .iter()
        .flat_map(destination_entry_values)
        .filter_map(|destination| {
            if destination.id != "-110" || destination.timer.is_some() {
                return None;
            }
            scene_black_fade_tail_start(destination.dst.iter().flat_map(dst_entry_frames), document)
        })
        .max()
}

fn document_has_fadeout_timer_black(document: &SkinDocument) -> bool {
    document.destination.iter().flat_map(destination_entry_values).any(|destination| {
        destination.id == "-110"
            && destination.timer == Some(2)
            && black_fade_start(destination.dst.iter().flat_map(dst_entry_frames), document, 0)
                .is_some()
    })
}

fn destination_entry_values(
    entry: &DestinationListEntry,
) -> &[bmz_render::skin::SkinDestinationDef] {
    match entry {
        DestinationListEntry::Single(destination) => std::slice::from_ref(destination),
        DestinationListEntry::Conditional { destinations, .. } => destinations.as_slice(),
    }
}

fn dst_entry_frames(entry: &SkinDstEntry) -> &[SkinAnimationDef] {
    match entry {
        SkinDstEntry::Frame(frame) => std::slice::from_ref(frame),
        SkinDstEntry::Conditional { frames, .. } => frames.as_slice(),
    }
}

fn scene_black_fade_tail_start<'a>(
    frames: impl Iterator<Item = &'a SkinAnimationDef>,
    document: &SkinDocument,
) -> Option<i32> {
    black_fade_start(frames, document, document.scene)
}

fn black_fade_start<'a>(
    frames: impl Iterator<Item = &'a SkinAnimationDef>,
    document: &SkinDocument,
    min_end_time: i32,
) -> Option<i32> {
    let mut resolved = ResolvedTailFrame::default();
    let mut previous: Option<ResolvedTailFrame> = None;
    let mut start = None;
    for frame in frames {
        resolved.apply(frame);
        let Some(previous_frame) = previous else {
            previous = Some(resolved);
            continue;
        };
        if resolved.time >= min_end_time
            && previous_frame.time < resolved.time
            && previous_frame.a < resolved.a
            && previous_frame.is_fullscreen(document)
        {
            start = Some(previous_frame.time);
        }
        previous = Some(resolved);
    }
    start
}

#[derive(Debug, Clone, Copy)]
struct ResolvedTailFrame {
    time: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    a: i32,
}

impl Default for ResolvedTailFrame {
    fn default() -> Self {
        Self { time: 0, x: 0, y: 0, w: 0, h: 0, a: 255 }
    }
}

impl ResolvedTailFrame {
    fn apply(&mut self, frame: &SkinAnimationDef) {
        if let Some(time) = frame.time {
            self.time = time;
        }
        if let Some(x) = frame.x {
            self.x = x;
        }
        if let Some(y) = frame.y {
            self.y = y;
        }
        if let Some(w) = frame.w {
            self.w = w;
        }
        if let Some(h) = frame.h {
            self.h = h;
        }
        if let Some(a) = frame.a {
            self.a = a;
        }
    }

    fn is_fullscreen(self, document: &SkinDocument) -> bool {
        let width = document.w as i32;
        let height = document.h as i32;
        self.x <= width / 20
            && self.y <= height / 20
            && self.w >= width * 9 / 10
            && self.h >= height * 9 / 10
    }
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
    if bindings.is_scratch_up(control) {
        Some(TargetCycle::Previous)
    } else if bindings.is_scratch_down(control) {
        Some(TargetCycle::Next)
    } else {
        None
    }
}

struct SelectKeyBindings {
    start: Vec<String>,
    e2_action_controls: Vec<String>,
    e3_action_controls: Vec<String>,
    enter: Vec<String>,
    back: Vec<String>,
    key2_controls: Vec<String>,
    key5_controls: Vec<String>,
    key7_controls: Vec<String>,
    hispeed_down_controls: Vec<String>,
    hispeed_up_controls: Vec<String>,
    scratch_up_controls: Vec<String>,
    scratch_down_controls: Vec<String>,
    cycle_arrange: Option<String>,
    cycle_gauge: Option<String>,
    cycle_assist: Option<String>,
    cycle_bga: Option<String>,
    key_hint: String,
    option_hint: String,
}

impl SelectKeyBindings {
    fn from_profile(input: &ProfileInputConfig) -> Self {
        use crate::config::play_input::resolve_play_bindings;

        let play_7k = resolve_play_bindings(input, bmz_core::lane::KeyMode::K7).unwrap_or_default();
        let kb: Vec<_> = input.ui.bindings.iter().filter(|e| e.device == "keyboard").collect();
        let play_kb: Vec<_> = play_7k.iter().filter(|e| e.device == "keyboard").collect();
        let all_input: Vec<_> = input
            .ui
            .bindings
            .iter()
            .filter(|e| e.device == "keyboard" || e.device == "gamepad")
            .collect();
        let play_all: Vec<_> =
            play_7k.iter().filter(|e| e.device == "keyboard" || e.device == "gamepad").collect();

        // キーボード専用（ヒント文字列表示用）
        let kb_keys_for = |lane: LaneConfig| -> Vec<String> {
            play_kb.iter().filter(|e| e.lane == Some(lane)).map(|e| e.control.clone()).collect()
        };

        // キーボード + ゲームパッド（is_enter / is_back ルックアップ用）
        let keys_for = |lane: LaneConfig| -> Vec<String> {
            play_all.iter().filter(|e| e.lane == Some(lane)).map(|e| e.control.clone()).collect()
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
        let lane_back: Vec<String> = [LaneConfig::Key2, LaneConfig::Key4, LaneConfig::Key6]
            .iter()
            .flat_map(|&l| keys_for(l))
            .collect();
        let enter = merge_select_controls(actions_for(InputActionConfig::SelectEnter), lane_enter);
        let back = merge_select_controls(actions_for(InputActionConfig::E2), lane_back);
        let e2_action_controls = actions_for(InputActionConfig::E2);
        let e3_action_controls = actions_for(InputActionConfig::E3);
        let key2_controls = keys_for(LaneConfig::Key2);
        let key5_controls = keys_for(LaneConfig::Key5);
        let key7_controls = keys_for(LaneConfig::Key7);
        let hispeed_down_controls: Vec<String> =
            [LaneConfig::Key1, LaneConfig::Key3, LaneConfig::Key5, LaneConfig::Key7]
                .iter()
                .flat_map(|&l| keys_for(l))
                .collect();
        let hispeed_up_controls: Vec<String> =
            [LaneConfig::Key2, LaneConfig::Key4, LaneConfig::Key6]
                .iter()
                .flat_map(|&l| keys_for(l))
                .collect();
        let mut scratch_up_controls = Vec::new();
        let mut scratch_down_controls = Vec::new();
        for entry in play_all.iter().filter(|e| e.lane == Some(LaneConfig::Scratch)) {
            let control = entry.control.clone();
            // 明示の direction タグを最優先し、無ければコントロール名から推測する。
            match entry.scratch {
                Some(ScratchDirectionConfig::Up) => scratch_up_controls.push(control),
                Some(ScratchDirectionConfig::Down) => scratch_down_controls.push(control),
                None => {
                    if is_scratch_up_control(&control) {
                        scratch_up_controls.push(control);
                    } else if is_scratch_down_control(&control) {
                        scratch_down_controls.push(control);
                    } else {
                        scratch_up_controls.push(control.clone());
                        scratch_down_controls.push(control);
                    }
                }
            }
        }
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
        let kb_lane_back: Vec<String> = [LaneConfig::Key2, LaneConfig::Key4, LaneConfig::Key6]
            .iter()
            .flat_map(|&l| kb_keys_for(l))
            .collect();
        let kb_enter =
            merge_select_controls(kb_actions_for(InputActionConfig::SelectEnter), kb_lane_enter);
        let enter_str =
            if kb_enter.is_empty() { String::new() } else { format!("/{}", kb_enter.join("/")) };
        let back_str = if kb_lane_back.is_empty() {
            kb_actions_for(InputActionConfig::E2)
                .first()
                .map(|k| format!("/{k}"))
                .unwrap_or_default()
        } else {
            format!("/{}", kb_lane_back.join("/"))
        };
        let key2_str =
            kb_keys_for(LaneConfig::Key2).into_iter().next().unwrap_or_else(|| "Key2".to_string());
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
             {start_str}+BACK+{key2_str}:GAS  {start_str}+UP/DOWN:TARGET  {start_str}+{bga_str}:BGA  {start_str}+1..4:REPLAY"
        );

        Self {
            start,
            e2_action_controls,
            e3_action_controls,
            enter,
            back,
            key2_controls,
            key5_controls,
            key7_controls,
            hispeed_down_controls,
            hispeed_up_controls,
            scratch_up_controls,
            scratch_down_controls,
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

    fn is_key2(&self, control: &str) -> bool {
        self.key2_controls.iter().any(|k| k == control)
    }

    fn is_key5(&self, control: &str) -> bool {
        self.key5_controls.iter().any(|k| k == control)
    }

    fn is_key7(&self, control: &str) -> bool {
        self.key7_controls.iter().any(|k| k == control)
    }

    fn is_e2_action(&self, control: &str) -> bool {
        self.e2_action_controls.iter().any(|k| k == control)
    }

    fn is_e3_action(&self, control: &str) -> bool {
        self.e3_action_controls.iter().any(|k| k == control)
    }

    fn is_hispeed_down_key(&self, control: &str) -> bool {
        self.hispeed_down_controls.iter().any(|k| k == control)
    }

    fn is_hispeed_up_key(&self, control: &str) -> bool {
        self.hispeed_up_controls.iter().any(|k| k == control)
    }

    fn is_scratch_up(&self, control: &str) -> bool {
        self.scratch_up_controls.iter().any(|k| k == control)
    }

    fn is_scratch_down(&self, control: &str) -> bool {
        self.scratch_down_controls.iter().any(|k| k == control)
    }
}

/// アナログ tick の選曲スクロール寄与を返す。Next 方向を正とする。
/// scratch up/down にバインドされていない軸は `None`。
fn select_analog_scroll_delta(axis: &str, ticks: i32, bindings: &SelectKeyBindings) -> Option<i32> {
    if ticks == 0 {
        return None;
    }
    let control = format!("{}{}", axis, if ticks > 0 { "+" } else { "-" });
    if bindings.is_scratch_down(&control) {
        Some(ticks.abs())
    } else if bindings.is_scratch_up(&control) {
        Some(-ticks.abs())
    } else {
        None
    }
}

/// アナログスクロールバッファへ delta を蓄積する。
/// suppress 中は idle (200ms 以上の tick 途切れ) を観測するまで delta を捨てる。
/// idle 後の最初の delta から通常蓄積に戻る。
fn update_analog_scroll_buffer(buffer: &mut i32, suppress: &mut bool, idle: bool, delta: i32) {
    if *suppress {
        if !idle {
            *buffer = 0;
            return;
        }
        *suppress = false;
    }
    if idle {
        *buffer = 0;
    }
    *buffer += delta;
}

/// バッファから ticks_per_scroll ごとの移動数を取り出す。端数はバッファに残す。
fn take_analog_scroll_steps(buffer: &mut i32, ticks_per_scroll: i32) -> i32 {
    let steps = *buffer / ticks_per_scroll;
    *buffer %= ticks_per_scroll;
    steps
}

fn merge_select_controls(configured: Vec<String>, lane_controls: Vec<String>) -> Vec<String> {
    let mut merged = configured;
    for control in lane_controls {
        if !merged.iter().any(|existing| existing == &control) {
            merged.push(control);
        }
    }
    merged
}

fn select_control_with_lane_fallback(
    configured: Vec<String>,
    lane_fallback: Vec<String>,
) -> Option<String> {
    configured.into_iter().next().or_else(|| lane_fallback.into_iter().next())
}

#[cfg(test)]
mod tests {
    use bmz_render::scene::SelectRowKind;
    use bmz_render::skin::SkinManifest;

    use crate::config::app_config::{AppConfig, PathEntry, PresentModeConfig};
    use crate::config::profile_config::ProfileConfig;
    use crate::screens::select_model::{SelectChartRow, SelectCourseRow};
    use crate::skin_loader::default_skin_root;
    use crate::storage::library_db::ChartListItem;
    use crate::storage::score_db::BestScoreSummary;

    use super::*;

    #[test]
    fn initial_folder_stack_starts_at_select_root_even_with_single_enabled_root() {
        let mut config = AppConfig::default();
        config.songs.roots =
            vec![PathEntry { path: "/music/bms".to_string(), enabled: true, recursive: true }];
        assert!(initial_folder_stack(&config).is_empty());
    }

    #[test]
    fn config_present_mode_auto_follows_vsync() {
        let mut config = AppConfig::default().video;
        config.present_mode = PresentModeConfig::Auto;

        config.vsync = true;
        assert_eq!(config_present_mode(&config), bmz_render::WgpuPresentMode::AutoVsync);

        config.vsync = false;
        assert_eq!(config_present_mode(&config), bmz_render::WgpuPresentMode::AutoNoVsync);
    }

    #[test]
    fn config_present_mode_explicit_overrides_vsync() {
        let mut config = AppConfig::default().video;
        config.vsync = true;
        config.present_mode = PresentModeConfig::Immediate;

        assert_eq!(config_present_mode(&config), bmz_render::WgpuPresentMode::Immediate);
    }

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
    fn debug_boot_result_summary_has_stat_graph_data() {
        let finished = debug_boot_finished_play_session();
        let summary = &finished.summary;

        assert_eq!(summary.title, "Debug Result Boot [ANOTHER]");
        assert_eq!(summary.key_mode, KeyMode::K7);
        assert!(summary.ex_score > 0);
        assert!(!summary.graph.gauge_points.is_empty());
        assert!(!summary.graph.judge_graph_buckets.is_empty());
        assert!(!summary.graph.early_late_graph_buckets.is_empty());
        assert!(!summary.graph.timing_points.is_empty());
        assert!(summary.graph.timing_distribution.total() > 0);
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
        assert!(is_skin_candidate_file(Path::new("data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin")));
        assert!(!is_skin_candidate_file(Path::new("data/skins/ECFN/play/play_parts.lua")));
    }

    #[test]
    fn lr2skin_header_document_exposes_skin_config_defs_when_available() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../data/skins/WMII_FHD/play/FHDPLAY_AC.lr2skin");
        if !path.is_file() {
            return;
        }

        let document = load_skin_header_document(&path).expect("load lr2 skin header");

        assert!(document.property.iter().any(|property| property.name == "Displayjudge"));
        assert!(document.filepath.iter().any(|filepath| filepath.name == "GAUGE COLOR"));
        assert!(document.offset.iter().any(|offset| offset.id == 1));
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
        push_skin_candidate(
            &mut catalog,
            4,
            SkinCandidate {
                name: "Nine".to_string(),
                path: "data/skins/example/play9.luaskin".to_string(),
            },
        );
        push_skin_candidate(
            &mut catalog,
            15,
            SkinCandidate {
                name: "Course Result".to_string(),
                path: "data/skins/example/course-result.luaskin".to_string(),
            },
        );

        assert_eq!(catalog.play5.len(), 1);
        assert_eq!(catalog.play7.len(), 1);
        assert_eq!(catalog.play9.len(), 1);
        assert_eq!(catalog.play10.len(), 1);
        assert_eq!(catalog.play14.len(), 1);
        assert_eq!(catalog.result.len(), 1);
        assert_eq!(catalog.play5[0].path, "data/skins/example/play5.luaskin");
        assert_eq!(catalog.play7[0].path, "data/skins/example/play7.luaskin");
        assert_eq!(catalog.play9[0].path, "data/skins/example/play9.luaskin");
        assert_eq!(catalog.play10[0].path, "data/skins/example/play10.luaskin");
        assert_eq!(catalog.play14[0].path, "data/skins/example/play14.luaskin");
        assert_eq!(catalog.result[0].path, "data/skins/example/course-result.luaskin");
    }

    #[test]
    fn course_result_summary_for_skin_uses_aggregate_course_values() {
        fn entry_summary(
            ex_score: u32,
            notes: u32,
            max_combo: u32,
            duration_ms: i32,
        ) -> ResultSummary {
            ResultSummary {
                clear_type: ClearType::Normal,
                arrange: "NORMAL".to_string(),
                lane_shuffle_pattern: Vec::new(),
                ex_score,
                max_combo,
                bp: 0,
                cb: 0,
                gauge_value: 80.0,
                gauge_type: GaugeType::Normal,
                total_notes: notes,
                duration_ms,
                initial_bpm: 128.0,
                min_bpm: 128.0,
                max_bpm: 128.0,
                main_bpm: 128.0,
                total_gauge: 260.0,
                judge_rank: Some(2),
                key_mode: KeyMode::K7,
                judge_counts: crate::screens::result_model::ResultJudgeCounts {
                    pgreat: ex_score / 2,
                    ..Default::default()
                },
                fast_slow_counts: ResultFastSlowJudgeCounts {
                    fast_pgreat: ex_score / 2,
                    ..Default::default()
                },
                replay_path: String::new(),
                replay_slots: [false; 4],
                saved_replay_slots: [false; 4],
                score_history_id: 0,
                best_ex_score: None,
                best_clear_type: None,
                best_max_combo: None,
                best_bp: None,
                previous_best_ex_score: None,
                previous_best_clear_type: None,
                previous_best_max_combo: None,
                previous_best_bp: None,
                target_ex_score: None,
                target_max_combo: None,
                target_bp: None,
                target_clear_type: None,
                ir_queued_jobs: 0,
                ir_last_error: None,
                title: String::new(),
                subtitle: String::new(),
                artist: String::new(),
                subartist: String::new(),
                genre: String::new(),
                difficulty_name: String::new(),
                play_level: String::new(),
                graph: bmz_render::snapshot::ResultGraphSnapshot {
                    gauge_points: vec![bmz_render::snapshot::ResultGaugeGraphPoint {
                        time_ms: duration_ms,
                        value: 80.0,
                        border: 20.0,
                        gauge_type: GaugeType::Normal as i32,
                    }],
                    timing_points: vec![bmz_render::snapshot::ResultTimingPoint {
                        time_ms: duration_ms,
                        delta_us: i64::from(duration_ms),
                        judge: bmz_core::judge::Judge::PGreat,
                    }],
                    judge_graph_density: vec![notes as u8],
                    bpm_graph_segments: vec![bmz_render::snapshot::BpmGraphSegment {
                        start_ratio: 0.0,
                        end_ratio: 1.0,
                        bpm: 120.0 + duration_ms as f32,
                        is_stop: false,
                    }],
                    ..Default::default()
                },
            }
        }

        let course = CourseResultSummary {
            course_id: 1,
            title: "Course Title".to_string(),
            kind: bmz_core::course::CourseKind::Dan,
            entry_summaries: vec![
                entry_summary(120, 100, 80, 1_000),
                entry_summary(200, 120, 90, 2_000),
            ],
            entry_arranges: Vec::new(),
            total_ex_score: 320,
            max_ex_score: 440,
            total_notes: 220,
            judge_counts: crate::screens::result_model::ResultJudgeCounts {
                pgreat: 160,
                bad: 2,
                ..Default::default()
            },
            trophy_results: Vec::new(),
            course_clear: true,
            course_failed: false,
            total_entries: 2,
            played_entries: 2,
            replay_slots: [true, false, true, false],
            saved_replay_slots: [false, false, true, false],
            best_score: None,
        };

        let summary = course_result_summary_for_skin(&course);
        assert_eq!(summary.title, "Course Title");
        assert_eq!(summary.genre, "DAN");
        assert_eq!(summary.ex_score, 320);
        assert_eq!(summary.total_notes, 220);
        assert_eq!(summary.max_combo, 90);
        assert_eq!(summary.replay_slots, [true, false, true, false]);
        assert_eq!(summary.saved_replay_slots, [false, false, true, false]);
        assert_eq!(summary.judge_counts.pgreat, 160);
        assert_eq!(summary.fast_slow_counts.fast_pgreat, 160);
        assert_eq!(
            summary.graph.gauge_points.iter().map(|point| point.time_ms).collect::<Vec<_>>(),
            vec![1_000, 3_000]
        );
        assert_eq!(
            summary.graph.timing_points.iter().map(|point| point.time_ms).collect::<Vec<_>>(),
            vec![1_000, 3_000]
        );
        assert_eq!(summary.graph.judge_graph_density, vec![100, 120]);
        assert_eq!(summary.graph.bpm_graph_segments[0].start_ratio, 0.0);
        assert!((summary.graph.bpm_graph_segments[0].end_ratio - 1.0 / 3.0).abs() < 0.001);
        assert!((summary.graph.bpm_graph_segments[1].start_ratio - 1.0 / 3.0).abs() < 0.001);
        assert_eq!(summary.graph.bpm_graph_segments[1].end_ratio, 1.0);
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
        SelectKeyBindings::from_profile(&crate::config::play_input::default_profile_input())
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
    fn select_row_click_enters_only_when_row_is_already_selected() {
        assert_eq!(
            select_row_click_action(2, MouseButton::Left, 0, 4),
            Some(SelectRowClickAction::Select(2))
        );
        assert_eq!(
            select_row_click_action(2, MouseButton::Left, 2, 4),
            Some(SelectRowClickAction::EnterOrPlay)
        );
        assert_eq!(select_row_click_action(4, MouseButton::Left, 2, 4), None);
        assert_eq!(
            select_row_click_action(2, MouseButton::Right, 2, 4),
            Some(SelectRowClickAction::ExitFolder)
        );
        assert_eq!(select_row_click_action(2, MouseButton::Middle, 2, 4), None);
    }

    #[test]
    fn select_scroll_slider_value_maps_to_nearest_row() {
        assert_eq!(select_scroll_slider_index(0.0, 0), None);
        assert_eq!(select_scroll_slider_index(0.5, 1), Some(0));
        assert_eq!(select_scroll_slider_index(-1.0, 10), Some(0));
        assert_eq!(select_scroll_slider_index(0.0, 10), Some(0));
        assert_eq!(select_scroll_slider_index(0.49, 10), Some(4));
        assert_eq!(select_scroll_slider_index(0.50, 10), Some(5));
        assert_eq!(select_scroll_slider_index(1.0, 10), Some(9));
        assert_eq!(select_scroll_slider_index(2.0, 10), Some(9));
    }

    #[test]
    fn skin_video_source_respects_static_property_ops() {
        let mut document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 5,
                "property": [
                    {
                        "name": "動画を使用する",
                        "def": "ON",
                        "item": [
                            { "name": "ON", "op": 920 },
                            { "name": "OFF", "op": 921 }
                        ]
                    }
                ],
                "source": [{ "id": "mv", "path": "mv/default.mp4" }],
                "image": [{ "id": "mv", "src": "mv", "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [{ "id": "mv", "op": [920], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] }]
            }
            "#,
        )
        .unwrap();

        assert!(skin_video_source_gating(&document, "mv").active);

        document.user_selected_options = Some(vec![921]);
        assert!(!skin_video_source_gating(&document, "mv").active);
        assert!(skin_video_source_gating(&document, "unknown-source").active);
    }

    #[test]
    fn skin_video_source_runtime_visibility_follows_result_rank_op() {
        use bmz_render::skin::SkinDrawState;

        // ランク別 BG を op で出し分けるリザルトスキン構成 (Starseeker 相当)。
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 7,
                "source": [
                    { "id": "BG_A", "path": "BG/A/a.mp4" },
                    { "id": "BG_AAA", "path": "BG/AAA/aaa.mp4" }
                ],
                "image": [
                    { "id": "BG_A", "src": "BG_A", "x": 0, "y": 0, "w": 10, "h": 10 },
                    { "id": "BG_AAA", "src": "BG_AAA", "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "destination": [
                    { "id": "BG_A", "op": [90, 302], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "BG_AAA", "op": [90, 300], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();

        let make_source = |source_id: &str| {
            let gating = skin_video_source_gating(&document, source_id);
            ActiveSkinVideoSource {
                texture: SkinTextureId(0),
                path: PathBuf::new(),
                decoder: None,
                last_pts: None,
                loop_start_us: 0,
                active: gating.active,
                gating_op_sets: gating.op_sets,
                enabled_options: document.enabled_options(),
                result_ranktime_ms: document.ranktime,
                failed: false,
            }
        };
        let bg_a = make_source("BG_A");
        let bg_aaa = make_source("BG_AAA");

        // ex_score / total_notes でランクが決まる。9/9 = AAA, 6/9 = A 付近。
        let aaa_state = SkinDrawState {
            result_failed: Some(false),
            ex_score: 18,
            total_notes: 9,
            ..SkinDrawState::default()
        };
        assert!(skin_video_source_runtime_visible(&bg_aaa, &aaa_state));
        assert!(!skin_video_source_runtime_visible(&bg_a, &aaa_state));

        // 13/18 = 72.2% は rank index 2 (= A), op 302 に対応する。
        let a_state = SkinDrawState {
            result_failed: Some(false),
            ex_score: 13,
            total_notes: 9,
            ..SkinDrawState::default()
        };
        assert!(skin_video_source_runtime_visible(&bg_a, &a_state));
        assert!(!skin_video_source_runtime_visible(&bg_aaa, &a_state));
    }

    #[test]
    fn skin_video_source_gating_respects_conditional_destination_if_ops() {
        use bmz_render::skin::SkinDrawState;

        let mut document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 7,
                "property": [
                    {
                        "name": "動画を使用する",
                        "def": "ON",
                        "item": [
                            { "name": "ON", "op": 920 },
                            { "name": "OFF", "op": 921 }
                        ]
                    }
                ],
                "source": [{ "id": "BG_AAA", "path": "BG/AAA/aaa.mp4" }],
                "image": [{ "id": "BG_AAA", "src": "BG_AAA", "x": 0, "y": 0, "w": 10, "h": 10 }],
                "destination": [
                    {
                        "if": [920],
                        "values": [
                            { "id": "BG_AAA", "op": [90, 300], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] }
                        ]
                    }
                ]
            }
            "#,
        )
        .unwrap();

        let gating = skin_video_source_gating(&document, "BG_AAA");
        assert!(gating.active);
        assert_eq!(gating.op_sets, vec![vec![920, 90, 300]]);
        let aaa_state = SkinDrawState {
            result_failed: Some(false),
            ex_score: 18,
            total_notes: 9,
            ..SkinDrawState::default()
        };
        let source = ActiveSkinVideoSource {
            texture: SkinTextureId(0),
            path: PathBuf::new(),
            decoder: None,
            last_pts: None,
            loop_start_us: 0,
            active: gating.active,
            gating_op_sets: gating.op_sets,
            enabled_options: document.enabled_options(),
            result_ranktime_ms: document.ranktime,
            failed: false,
        };
        assert!(skin_video_source_runtime_visible(&source, &aaa_state));

        document.user_selected_options = Some(vec![921]);
        let gating = skin_video_source_gating(&document, "BG_AAA");
        assert!(!gating.active);
        let disabled_source = ActiveSkinVideoSource {
            texture: SkinTextureId(0),
            path: PathBuf::new(),
            decoder: None,
            last_pts: None,
            loop_start_us: 0,
            active: gating.active,
            gating_op_sets: gating.op_sets,
            enabled_options: document.enabled_options(),
            result_ranktime_ms: document.ranktime,
            failed: false,
        };
        assert!(!skin_video_source_runtime_visible(&disabled_source, &aaa_state));
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
        // Key2(S), Key4(D), Key6(F) → ExitFolder
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::KeyS), ElementState::Pressed, false, &keys),
            Some(SelectAction::ExitFolder)
        );
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::KeyD), ElementState::Pressed, false, &keys),
            Some(SelectAction::ExitFolder)
        );
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::KeyF), ElementState::Pressed, false, &keys),
            Some(SelectAction::ExitFolder)
        );
        // E2(W) is also mapped to ExitFolder for direct lookup paths.
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::KeyW), ElementState::Pressed, false, &keys),
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
    fn settings_key_repeat_is_accepted_only_while_editing_value() {
        assert!(should_route_settings_key_event(ElementState::Pressed, false, false));
        assert!(!should_route_settings_key_event(ElementState::Pressed, true, false));
        assert!(should_route_settings_key_event(ElementState::Pressed, true, true));
        assert!(!should_route_settings_key_event(ElementState::Released, true, true));
    }

    #[test]
    fn settings_browse_keeps_cursor_navigation_direction() {
        let profile = ProfileConfig::new_default("default", "Default", 0);
        let bindings = SettingsBindings::from_profile(&profile.input);

        assert_eq!(settings_browse_move_control("ArrowUp", &bindings), Some(SelectMove::Previous));
        assert_eq!(settings_browse_move_control("ArrowDown", &bindings), Some(SelectMove::Next));
        assert_eq!(settings_browse_move_control("DPadUp", &bindings), Some(SelectMove::Previous));
        assert_eq!(settings_browse_move_control("DPadDown", &bindings), Some(SelectMove::Next));
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
    fn lane_cover_wheel_change_maps_vertical_scroll() {
        assert_eq!(
            lane_cover_wheel_change(MouseScrollDelta::LineDelta(0.0, 1.0)),
            Some(LaneCoverChange::Up)
        );
        assert_eq!(
            lane_cover_wheel_change(MouseScrollDelta::LineDelta(0.0, -1.0)),
            Some(LaneCoverChange::Down)
        );
        assert_eq!(lane_cover_wheel_change(MouseScrollDelta::LineDelta(1.0, 0.0)), None);
    }

    #[test]
    fn select_click_event_arg_matches_beatoraja_click_types() {
        let rect = Rect { x: 0.2, y: 0.3, width: 0.4, height: 0.2 };
        assert_eq!(select_click_event_arg(0, MouseButton::Left, rect, 0.3, 0.4), Some(1));
        assert_eq!(select_click_event_arg(0, MouseButton::Right, rect, 0.3, 0.4), Some(-1));
        assert_eq!(select_click_event_arg(1, MouseButton::Right, rect, 0.3, 0.4), Some(1));
        assert_eq!(select_click_event_arg(2, MouseButton::Left, rect, 0.39, 0.4), Some(-1));
        assert_eq!(select_click_event_arg(2, MouseButton::Left, rect, 0.41, 0.4), Some(1));
        assert_eq!(select_click_event_arg(3, MouseButton::Left, rect, 0.3, 0.39), Some(1));
        assert_eq!(select_click_event_arg(3, MouseButton::Left, rect, 0.3, 0.41), Some(-1));
        assert_eq!(select_click_event_arg(4, MouseButton::Left, rect, 0.3, 0.4), None);
    }

    #[test]
    fn select_key_bindings_builds_correct_hints() {
        let keys = default_select_keys();
        assert!(keys.key_hint.contains("Z/X/C/V"), "enter keys in hint: {}", keys.key_hint);
        assert!(keys.key_hint.contains("/S/D/F:BACK"), "back keys in hint: {}", keys.key_hint);
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
    fn select_analog_scroll_delta_maps_scratch_bindings() {
        let gamepad_keys = SelectKeyBindings::from_profile(
            &ProfileConfig::new_default("default", "Default", 1).input,
        );
        // AxisLeftX- = scratch up (Previous = 負), AxisLeftX+ = scratch down (Next = 正)
        assert_eq!(select_analog_scroll_delta("AxisLeftX", -4, &gamepad_keys), Some(-4));
        assert_eq!(select_analog_scroll_delta("AxisLeftX", 4, &gamepad_keys), Some(4));
        assert_eq!(select_analog_scroll_delta("AxisLeftX", 0, &gamepad_keys), None);
        assert_eq!(select_analog_scroll_delta("AxisRightY", 4, &gamepad_keys), None);
    }

    #[test]
    fn update_analog_scroll_buffer_suppresses_until_idle() {
        let mut buffer = 0;
        let mut suppress = true;
        // 回転継続中 (idle=false) は捨て続ける
        update_analog_scroll_buffer(&mut buffer, &mut suppress, false, 5);
        assert_eq!(buffer, 0);
        assert!(suppress);
        // 一度止まった後の tick から蓄積再開
        update_analog_scroll_buffer(&mut buffer, &mut suppress, true, 2);
        assert_eq!(buffer, 2);
        assert!(!suppress);
        update_analog_scroll_buffer(&mut buffer, &mut suppress, false, 3);
        assert_eq!(buffer, 5);
        // 通常時も idle で端数を破棄
        update_analog_scroll_buffer(&mut buffer, &mut suppress, true, 1);
        assert_eq!(buffer, 1);
    }

    #[test]
    fn take_analog_scroll_steps_keeps_remainder() {
        let mut buffer = 7;
        assert_eq!(take_analog_scroll_steps(&mut buffer, 3), 2);
        assert_eq!(buffer, 1);

        let mut buffer = -7;
        assert_eq!(take_analog_scroll_steps(&mut buffer, 3), -2);
        assert_eq!(buffer, -1);

        let mut buffer = 2;
        assert_eq!(take_analog_scroll_steps(&mut buffer, 3), 0);
        assert_eq!(buffer, 2);
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
        assert!(is_select_modifier_key(PhysicalKey::Code(KeyCode::KeyW), &keys));
        assert!(!is_select_modifier_key(PhysicalKey::Code(KeyCode::KeyS), &keys));
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
            select_action(PhysicalKey::Code(KeyCode::KeyW), ElementState::Pressed, false, &keys),
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
        assert!(!is_select_start_key(PhysicalKey::Code(KeyCode::KeyW), &keys));
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
    fn select_key_bindings_include_e3_action() {
        let keys = default_select_keys();

        assert!(keys.is_e3_action("E"));
    }

    #[test]
    fn select_key_bindings_expose_key2_for_gas_toggle() {
        let keys = default_select_keys();

        assert!(keys.is_start("Q"));
        assert!(keys.is_back("W"));
        assert!(keys.is_back("S"));
        assert!(keys.is_back("D"));
        assert!(keys.is_back("F"));
        assert!(keys.is_key2("S"));
    }

    #[test]
    fn select_gauge_auto_shift_toggle_requires_start_then_key2() {
        let keys = default_select_keys();

        assert!(should_toggle_select_gauge_auto_shift("S", true, true, &keys));
        assert!(!should_toggle_select_gauge_auto_shift("Q", false, true, &keys));
        assert!(!should_toggle_select_gauge_auto_shift("Q", true, true, &keys));
        assert!(!should_toggle_select_gauge_auto_shift("W", true, false, &keys));
    }

    #[test]
    fn play_exit_hold_timer_uses_beatoraja_default_duration() {
        let default_hold = Duration::from_millis(1_000);
        let start = Instant::now();
        let mut held_since = None;

        update_play_exit_hold_started_at(&mut held_since, true, false, start);
        assert!(held_since.is_none());

        update_play_exit_hold_started_at(&mut held_since, true, true, start);
        assert_eq!(held_since, Some(start));
        assert!(!play_exit_hold_elapsed(held_since, start + default_hold / 2, default_hold));
        assert!(play_exit_hold_elapsed(held_since, start + default_hold, default_hold));

        update_play_exit_hold_started_at(&mut held_since, false, true, start + default_hold);
        assert!(held_since.is_none());
    }

    #[test]
    fn decide_fadeout_scene_elapsed_enters_scene_tail_on_early_skip() {
        let elapsed = decide_fadeout_scene_elapsed(
            Duration::from_millis(100),
            Duration::from_millis(250),
            Duration::from_millis(2500),
            Duration::from_millis(1000),
            DecideFadeoutSceneTiming::DefaultTail,
        );

        assert_eq!(elapsed, Duration::from_millis(1750));
    }

    #[test]
    fn decide_fadeout_scene_elapsed_stretches_detected_tail_fadeout() {
        let elapsed = decide_fadeout_scene_elapsed(
            Duration::from_millis(100),
            Duration::from_millis(500),
            Duration::from_millis(2500),
            Duration::from_millis(1000),
            DecideFadeoutSceneTiming::TailStart(Duration::from_millis(2300)),
        );

        assert_eq!(elapsed, Duration::from_millis(2400));
    }

    #[test]
    fn decide_fadeout_scene_elapsed_stays_direct_when_timer_fadeout_exists() {
        let elapsed = decide_fadeout_scene_elapsed(
            Duration::from_millis(100),
            Duration::from_millis(0),
            Duration::from_millis(2500),
            Duration::from_millis(500),
            DecideFadeoutSceneTiming::DirectOnly,
        );

        assert_eq!(elapsed, Duration::from_millis(100));
    }

    #[test]
    fn decide_fadeout_scene_elapsed_does_not_rewind_auto_fadeout() {
        let elapsed = decide_fadeout_scene_elapsed(
            Duration::from_millis(2500),
            Duration::from_millis(250),
            Duration::from_millis(2500),
            Duration::from_millis(1000),
            DecideFadeoutSceneTiming::DefaultTail,
        );

        assert_eq!(elapsed, Duration::from_millis(2750));
    }

    #[test]
    fn decide_scene_fadeout_tail_start_detects_scene_end_black_fade() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 6,
                "w": 1920,
                "h": 1080,
                "scene": 2500,
                "fadeout": 1000,
                "destination": [
                    { "id": -110, "loop": 800, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 1920, "h": 1080, "a": 255 },
                        { "time": 800, "a": 0 }
                    ] },
                    { "id": -110, "loop": 2500, "dst": [
                        { "time": 2300, "x": 0, "y": 0, "w": 1920, "h": 1080, "a": 0 },
                        { "time": 2500, "a": 255 }
                    ] }
                ]
            }
            "#,
        )
        .unwrap();

        assert_eq!(decide_scene_fadeout_tail_start(Some(&document)), Some(2300));
    }

    #[test]
    fn decide_scene_fadeout_tail_start_ignores_scene_tail_when_timer_fadeout_exists() {
        let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 6,
                "w": 1920,
                "h": 1080,
                "scene": 2500,
                "fadeout": 500,
                "destination": [
                    { "id": -110, "loop": 2000, "dst": [
                        { "time": 1500, "x": 0, "y": 0, "w": 1920, "h": 1080, "a": 0 },
                        { "time": 2000, "a": 255 }
                    ] },
                    { "id": -110, "loop": 500, "timer": 2, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 1920, "h": 1080, "a": 0 },
                        { "time": 500, "a": 255 }
                    ] }
                ]
            }
            "#,
        )
        .unwrap();

        assert!(document_has_fadeout_timer_black(&document));
        assert_eq!(
            decide_fadeout_scene_timing(Some(&document)),
            DecideFadeoutSceneTiming::DirectOnly
        );
        assert_eq!(decide_scene_fadeout_tail_start(Some(&document)), None);
    }

    #[test]
    fn bga_option_cycles_on_auto_off() {
        assert!(matches!(cycle_bga_option(BgaModeConfig::On), BgaModeConfig::Auto));
        assert!(matches!(cycle_bga_option(BgaModeConfig::Auto), BgaModeConfig::Off));
        assert!(matches!(cycle_bga_option(BgaModeConfig::Off), BgaModeConfig::On));
    }

    #[test]
    fn volume_f32_to_unit_clamps_and_rounds() {
        assert_eq!(volume_f32_to_unit(-0.5), 0);
        assert_eq!(volume_f32_to_unit(0.345), 35);
        assert_eq!(volume_f32_to_unit(1.5), 100);
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
    fn result_exit_lanes_match_requested_mapping() {
        // beatoraja の OK (Key1-4) / REPLAY (Key5, Key7) が開始する。
        for lane in [Lane::Key1, Lane::Key2, Lane::Key3, Lane::Key4, Lane::Key5, Lane::Key7] {
            assert!(lane_starts_result_exit(lane), "{lane:?} should start result exit");
        }
        // Key6 は CHANGE_GRAPH、scratch は無割り当て。
        for lane in [Lane::Scratch, Lane::Key6] {
            assert!(!lane_starts_result_exit(lane), "{lane:?} should not start result exit");
        }
    }

    #[test]
    fn course_intermediate_result_only_with_active_course_and_no_course_result() {
        // active_course 保持 + finished_play あり + finished_course 無し → 中間リザルト。
        assert!(is_course_intermediate_result(true, false, true));
        // コース最終結果 (finished_course あり) は中間リザルトではない。
        assert!(!is_course_intermediate_result(true, true, true));
        // 単曲 (非コース) リザルトは中間リザルトではない。
        assert!(!is_course_intermediate_result(false, false, true));
        // 結果未表示なら中間リザルトではない。
        assert!(!is_course_intermediate_result(true, false, false));
    }

    #[test]
    fn result_action_resolves_from_held_lanes() {
        // beatoraja 準拠: Key5 のみ → 別配置 (REPLAY_DIFFERENT)。
        assert_eq!(
            result_action_for_held_lanes(true, false),
            Some(ResultRetryMode::DifferentArrange)
        );
        // Key7 のみ → 同配置 (REPLAY_SAME)。
        assert_eq!(result_action_for_held_lanes(false, true), Some(ResultRetryMode::SameArrange));
        // 両押し → 同配置 (ユーザー仕様)。
        assert_eq!(result_action_for_held_lanes(true, true), Some(ResultRetryMode::SameArrange));
        // どちらも非押下 → 選曲へ戻る。
        assert_eq!(result_action_for_held_lanes(false, false), None);
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
    fn lane_cover_step_moves_one_profile_unit() {
        assert!((LANE_COVER_STEP - 0.001).abs() < f32::EPSILON);
    }

    #[test]
    fn lane_cover_step_accelerates_on_key_repeat() {
        assert_eq!(
            lane_cover_step(PhysicalKey::Code(KeyCode::ArrowUp), ElementState::Pressed, false),
            Some(0.001)
        );
        assert_eq!(
            lane_cover_step(PhysicalKey::Code(KeyCode::ArrowUp), ElementState::Pressed, true),
            Some(0.01)
        );
        assert_eq!(
            lane_cover_step(PhysicalKey::Code(KeyCode::ArrowDown), ElementState::Pressed, true),
            Some(-0.01)
        );
    }

    #[test]
    fn play_option_control_maps_e1_combo_targets() {
        let keys = default_select_keys();

        assert_eq!(play_option_control("W", &keys), Some(PlayOptionControl::ToggleHispeedMode));
        assert_eq!(
            play_option_control("Z", &keys),
            Some(PlayOptionControl::Hispeed(HispeedChange::Down))
        );
        assert_eq!(
            play_option_control("V", &keys),
            Some(PlayOptionControl::Hispeed(HispeedChange::Down))
        );
        assert_eq!(
            play_option_control("S", &keys),
            Some(PlayOptionControl::Hispeed(HispeedChange::Up))
        );
        assert_eq!(
            play_option_control("F", &keys),
            Some(PlayOptionControl::Hispeed(HispeedChange::Up))
        );
        assert_eq!(
            play_option_control("AxisLeftX-", &keys),
            Some(PlayOptionControl::LaneCover(LaneCoverChange::Up))
        );
        assert_eq!(
            play_option_control("AxisLeftX+", &keys),
            Some(PlayOptionControl::LaneCover(LaneCoverChange::Down))
        );
    }

    #[test]
    fn detail_option_control_maps_key5_and_key7_to_visual_offset() {
        let keys = default_select_keys();

        assert_eq!(visual_offset_delta_control("C", &keys), Some(-1));
        assert_eq!(visual_offset_delta_control("V", &keys), Some(1));
        assert_eq!(visual_offset_delta_control("Z", &keys), None);
    }

    #[test]
    fn floating_hispeed_formula_uses_green_number_and_lane_cover() {
        assert_eq!(hispeed_for_green_number_values(300.0, 1.0, 120.0, 120.0), 4.0);
        assert_eq!(hispeed_for_green_number_values(300.0, 0.5, 120.0, 120.0), 2.0);
        assert_eq!(hispeed_for_green_number_values(300.0, 1.0, 120.0, 240.0), 2.0);
        assert!(
            (hispeed_for_green_number_values(295.0, 0.93, 120.0, 120.0) - 3.783_051).abs()
                < 0.000_01
        );
    }

    #[test]
    fn normal_hispeed_rounding_restores_quarter_steps() {
        assert_eq!(clamp_hispeed_for_profile(3.783_051), 3.75);
    }

    #[test]
    fn gauge_option_cycle_includes_auto_shift() {
        assert_eq!(cycle_gauge_option(GaugeTypeConfig::ExHard), GaugeTypeConfig::Hazard);
        assert_eq!(
            cycle_gauge_auto_shift_option(GaugeAutoShiftConfig::Off),
            GaugeAutoShiftConfig::Continue
        );
        assert_eq!(gauge_auto_shift_as_str(GaugeAutoShiftConfig::BestClear), "BEST CLEAR");
        assert_eq!(
            cycle_bottom_shiftable_gauge_with_direction(BottomShiftableGaugeConfig::Normal, 1),
            BottomShiftableGaugeConfig::AssistEasy
        );
        assert_eq!(bottom_shiftable_gauge_as_str(BottomShiftableGaugeConfig::Easy), "EASY");
        assert_eq!(cycle_gauge_option(GaugeTypeConfig::AutoShift), GaugeTypeConfig::Hazard);
    }

    #[test]
    fn apply_current_play_options_updates_profile_defaults() {
        let mut profile = ProfileConfig::new_default("default", "Default", 1);

        apply_current_play_options_to_profile(
            &mut profile,
            Some(3.37),
            Some(ActiveLaneState {
                lane_cover: 0.42,
                lift: 0.1,
                hispeed_mode: HispeedMode::Floating,
                target_green_number: 280,
            }),
            ArrangeOption::Mirror,
            TargetOption::RankAaa,
            GaugeTypeConfig::Hard,
            GaugeAutoShiftConfig::BestClear,
            BottomShiftableGaugeConfig::Normal,
            AssistOption::Autoplay,
            42,
        );

        assert_eq!(profile.lane.hispeed, 3.25);
        assert_eq!(profile.lane.sudden, 420);
        assert_eq!(profile.lane.lift, 100);
        assert_eq!(profile.lane.hispeed_mode, HispeedModeConfig::Floating);
        assert_eq!(profile.lane.target_green_number, 280);
        assert!(matches!(profile.play.random, RandomOptionConfig::Mirror));
        assert!(matches!(profile.play.target, TargetOptionConfig::RankAaa));
        assert!(matches!(profile.play.gauge, GaugeTypeConfig::Hard));
        assert!(matches!(profile.play.gauge_auto_shift, GaugeAutoShiftConfig::BestClear));
        assert!(matches!(profile.play.bottom_shiftable_gauge, BottomShiftableGaugeConfig::Normal));
        assert!(profile.play.auto_play);
        assert!(matches!(profile.play.assist, AssistOptionConfig::None));
        assert_eq!(profile.updated_at, 42);
    }

    #[test]
    fn arrange_option_maps_profile_random_defaults() {
        assert_eq!(arrange_option_from_profile(RandomOptionConfig::Off), ArrangeOption::Normal);
        assert_eq!(arrange_option_from_profile(RandomOptionConfig::Mirror), ArrangeOption::Mirror);
        assert_eq!(arrange_option_from_profile(RandomOptionConfig::Random), ArrangeOption::Random);
        assert_eq!(
            arrange_option_from_profile(RandomOptionConfig::RRandom),
            ArrangeOption::RRandom
        );
        assert_eq!(
            arrange_option_from_profile(RandomOptionConfig::SRandom),
            ArrangeOption::SRandom
        );
        assert_eq!(arrange_option_from_profile(RandomOptionConfig::Spiral), ArrangeOption::Spiral);
        assert_eq!(
            arrange_option_from_profile(RandomOptionConfig::HRandom),
            ArrangeOption::HRandom
        );
        assert_eq!(
            arrange_option_from_profile(RandomOptionConfig::AllScratch),
            ArrangeOption::AllScratch
        );
        assert_eq!(
            arrange_option_from_profile(RandomOptionConfig::RandomEx),
            ArrangeOption::RandomEx
        );
        assert_eq!(
            arrange_option_from_profile(RandomOptionConfig::SRandomEx),
            ArrangeOption::SRandomEx
        );
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
        assert!(matches!(
            random_config_from_arrange(ArrangeOption::RRandom),
            RandomOptionConfig::RRandom
        ));
        assert!(matches!(
            random_config_from_arrange(ArrangeOption::SRandom),
            RandomOptionConfig::SRandom
        ));
        assert!(matches!(
            random_config_from_arrange(ArrangeOption::Spiral),
            RandomOptionConfig::Spiral
        ));
        assert!(matches!(
            random_config_from_arrange(ArrangeOption::HRandom),
            RandomOptionConfig::HRandom
        ));
        assert!(matches!(
            random_config_from_arrange(ArrangeOption::AllScratch),
            RandomOptionConfig::AllScratch
        ));
        assert!(matches!(
            random_config_from_arrange(ArrangeOption::RandomEx),
            RandomOptionConfig::RandomEx
        ));
        assert!(matches!(
            random_config_from_arrange(ArrangeOption::SRandomEx),
            RandomOptionConfig::SRandomEx
        ));
    }

    #[test]
    fn window_title_uses_scene_name() {
        assert_eq!(window_title_for_scene(AppSceneKind::Select), "bmz-player - Select");
        assert_eq!(window_title_for_scene(AppSceneKind::Play), "bmz-player - Play");
        assert_eq!(window_title_for_scene(AppSceneKind::Result), "bmz-player - Result");
    }

    #[test]
    fn deferred_boot_action_keeps_practice_boot_after_window_init() {
        let mut options = AppOptions {
            boot_practice: true,
            practice_start_ms: Some(5_000),
            practice_end_ms: Some(120_000),
            ..AppOptions::default()
        };

        assert_eq!(
            deferred_boot_action(Some(42), &options),
            Some(DeferredBoot::Practice {
                chart_id: 42,
                start_time_ms: Some(5_000),
                end_time_ms: Some(120_000),
            })
        );

        options.boot_practice = false;
        assert_eq!(
            deferred_boot_action(Some(42), &options),
            Some(DeferredBoot::Chart { chart_id: 42, replay_slot: None })
        );
    }

    #[test]
    fn select_bgm_is_skipped_when_preview_is_already_playing() {
        assert!(should_play_select_bgm_on_enter(false));
        assert!(!should_play_select_bgm_on_enter(true));
    }

    #[test]
    fn select_preview_fade_factor_ramps_in_and_out() {
        let started_at = Instant::now();
        let half = started_at + SELECT_PREVIEW_FADE_DURATION / 2;
        let done = started_at + SELECT_PREVIEW_FADE_DURATION;

        assert_eq!(
            select_preview_fade_factor(SelectPreviewFade::FadingIn { started_at }, started_at),
            0.0
        );
        assert!(
            (select_preview_fade_factor(SelectPreviewFade::FadingIn { started_at }, half) - 0.5)
                .abs()
                < 0.001
        );
        assert_eq!(
            select_preview_fade_factor(SelectPreviewFade::FadingIn { started_at }, done),
            1.0
        );
        assert!(
            (select_preview_fade_factor(SelectPreviewFade::FadingOut { started_at }, half) - 0.5)
                .abs()
                < 0.001
        );
        assert_eq!(
            select_preview_fade_factor(SelectPreviewFade::FadingOut { started_at }, done),
            0.0
        );
    }

    #[test]
    fn select_preview_key_waits_for_beatoraja_start_delay() {
        let key = Some("folder|preview.ogg".to_string());

        assert_eq!(
            select_preview_key_after_delay(
                key.clone(),
                SELECT_PREVIEW_START_DELAY - Duration::from_millis(1),
                SELECT_PREVIEW_START_DELAY,
            ),
            None
        );
        assert_eq!(
            select_preview_key_after_delay(
                key.clone(),
                SELECT_PREVIEW_START_DELAY,
                SELECT_PREVIEW_START_DELAY,
            ),
            key
        );
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
                    if let Some(analysis) = &mut row.chart_analysis {
                        analysis.speed_changes = vec![
                            crate::storage::library_db::ChartSpeedChange {
                                speed: 100.0,
                                time_ms: 0,
                            },
                            crate::storage::library_db::ChartSpeedChange {
                                speed: 200.0,
                                time_ms: 45_000,
                            },
                        ];
                    }
                    let mut best_score = best_score_with_replay(1234, "replay/test.toml");
                    best_score.bp = 12;
                    best_score.cb = 8;
                    best_score.max_combo = 345;
                    row.best_score = Some(best_score);
                    row.replay_slots = [true, false, false, false];
                }
                SelectItem::Chart(row)
            })
            .collect();

        let profile = ProfileConfig::new_default("default", "Default", 0);
        let mut chart_distributions = HashMap::new();
        chart_distributions.insert(
            5,
            vec![crate::storage::library_db::ChartDistributionSecond {
                key_taps: 2,
                key_long_heads: 1,
                ..Default::default()
            }],
        );
        let snapshot_rows = select_snapshot_rows(&rows, 5, 7, &profile, None, &chart_distributions);

        assert_eq!(snapshot_rows.len(), 7);
        assert_eq!(snapshot_rows[0].index, 2);
        assert_eq!(snapshot_rows[3].index, 5);
        assert_eq!(snapshot_rows[3].title, "Title 5");
        assert_eq!(snapshot_rows[3].clear_type, "Normal");
        assert_eq!(snapshot_rows[3].ex_score, Some(1234));
        assert_eq!(snapshot_rows[3].bp, Some(12));
        assert_eq!(snapshot_rows[3].cb, Some(8));
        assert_eq!(snapshot_rows[3].max_combo, Some(345));
        assert_eq!(snapshot_rows[3].judge_rank, Some(1));
        assert_eq!(snapshot_rows[3].play_count, 42);
        assert_eq!(snapshot_rows[3].clear_count, 31);
        assert_eq!(snapshot_rows[3].replay_slots, [true, false, false, false]);
        assert_eq!(snapshot_rows[3].chart_normal_notes, 45);
        assert_eq!(snapshot_rows[3].chart_long_notes, 6);
        assert_eq!(snapshot_rows[3].chart_peak_density, 12.5);
        assert_eq!(snapshot_rows[3].chart_distribution.len(), 1);
        assert_eq!(snapshot_rows[3].chart_distribution[0].key_taps, 2);
        assert_eq!(snapshot_rows[3].chart_bpm_graph_segments.len(), 2);
        assert_eq!(snapshot_rows[3].chart_bpm_graph_segments[0].start_ratio, 0.0);
        assert_eq!(snapshot_rows[3].chart_bpm_graph_segments[0].end_ratio, 0.5);
        assert_eq!(snapshot_rows[3].chart_bpm_graph_segments[1].start_ratio, 0.5);
        assert_eq!(snapshot_rows[3].chart_bpm_graph_segments[1].end_ratio, 1.0);
    }

    #[test]
    fn select_snapshot_rows_wraps_near_edges() {
        let rows: Vec<SelectItem> =
            (0..4).map(|i| SelectItem::Chart(select_chart_row(i))).collect();

        let profile = ProfileConfig::new_default("default", "Default", 0);
        let snapshot_rows = select_snapshot_rows(&rows, 0, 7, &profile, None, &HashMap::new());

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

        let profile = ProfileConfig::new_default("default", "Default", 0);
        let snapshot_rows = select_snapshot_rows(&rows, 2, 25, &profile, None, &HashMap::new());

        assert_eq!(snapshot_rows.len(), 25);
        assert_eq!(snapshot_rows[0].index, 20);
        assert_eq!(snapshot_rows[12].index, 2);
        assert_eq!(snapshot_rows[24].index, 14);
    }

    #[test]
    fn course_rows_are_playable_only_when_all_entries_resolve() {
        let rows = vec![
            SelectItem::Course(select_course_row(4, 4)),
            SelectItem::Course(select_course_row(3, 4)),
        ];

        let profile = ProfileConfig::new_default("default", "Default", 0);
        let snapshot_rows = select_snapshot_rows(&rows, 0, 2, &profile, None, &HashMap::new());

        assert!(snapshot_rows.iter().any(|row| row.title == "Course 4/4" && row.in_library));
        assert!(snapshot_rows.iter().any(|row| row.title == "Course 3/4" && !row.in_library));
        let partial = snapshot_rows.iter().find(|row| row.title == "Course 3/4").unwrap();
        assert_eq!(partial.course_titles[0], "Stage 1");
        assert_eq!(partial.course_titles[3], "(no song) Stage 4");
    }

    #[test]
    fn course_constraint_flags_match_beatoraja_gradebar_ops() {
        let constraints = bmz_core::course::CourseConstraints {
            class: bmz_core::course::CourseClassConstraint::GradeRandomAllowed,
            speed: bmz_core::course::CourseSpeedConstraint::NoSpeed,
            judge: bmz_core::course::CourseJudgeConstraint::NoGood,
            gauge: bmz_core::course::CourseGaugeConstraint::Keys24,
            ln: bmz_core::course::CourseLnConstraint::Cn,
            source_constraints: Vec::new(),
        };

        let flags = course_constraint_flags(&constraints);

        assert!(!flags.class);
        assert!(!flags.mirror);
        assert!(flags.random);
        assert!(flags.no_speed);
        assert!(flags.no_good);
        assert!(!flags.no_great);
        assert!(flags.gauge_24k);
        assert!(!flags.gauge_7k);
        assert!(flags.cn);
        assert!(!flags.hcn);
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

    #[test]
    fn select_scroll_duration_config_uses_beatoraja_bounds() {
        let mut config = AppConfig::default();
        config.select.scroll_duration_low_ms = 0;
        config.select.scroll_duration_high_ms = 0;
        assert_eq!(select_scroll_duration_low_ms(&config), 2);
        assert_eq!(select_scroll_duration_high_ms(&config), 1);

        config.select.scroll_duration_low_ms = 5_000;
        config.select.scroll_duration_high_ms = 5_000;
        assert_eq!(select_scroll_duration_low_ms(&config), 1000);
        assert_eq!(select_scroll_duration_high_ms(&config), 1000);
    }

    #[test]
    fn select_move_scroll_direction_matches_row_movement() {
        assert_eq!(select_move_scroll_direction(SelectMove::Previous), -1);
        assert_eq!(select_move_scroll_direction(SelectMove::Next), 1);
        assert_eq!(select_move_scroll_direction(SelectMove::PagePrevious), -1);
        assert_eq!(select_move_scroll_direction(SelectMove::PageNext), 1);
        assert_eq!(select_move_scroll_direction(SelectMove::First), 0);
        assert_eq!(select_move_scroll_direction(SelectMove::Last), 0);
    }

    #[test]
    fn select_skin_event_state_cycles_like_beatoraja_defaults() {
        assert_eq!(SelectModeFilter::All.next(), SelectModeFilter::K7);
        assert_eq!(SelectModeFilter::All.previous(), SelectModeFilter::K24Double);
        assert_eq!(SelectSort::Title.next(), SelectSort::Artist);
        assert_eq!(SelectSort::Title.previous(), SelectSort::Bp);
        assert_eq!(
            crate::ln_policy::LnPolicySetting::AutoLn.next(),
            crate::ln_policy::LnPolicySetting::AutoCn
        );
        assert_eq!(
            crate::ln_policy::LnPolicySetting::AutoLn.previous(),
            crate::ln_policy::LnPolicySetting::ForceHcn
        );
        assert_eq!(crate::ln_policy::LnPolicySetting::ForceHcn.display_label(), "FORCE(HCN)");
        assert_eq!(
            cycle_gauge_option_with_direction(GaugeTypeConfig::Normal, 1),
            GaugeTypeConfig::Hard
        );
        assert_eq!(
            cycle_gauge_option_with_direction(GaugeTypeConfig::Normal, -1),
            GaugeTypeConfig::Easy
        );
        assert_eq!(
            cycle_arrange_option_with_direction(ArrangeOption::Normal, -1),
            ArrangeOption::SRandomEx
        );
        assert_eq!(cycle_bga_option_with_direction(BgaModeConfig::On, -1), BgaModeConfig::Off);
        assert_eq!(
            cycle_bga_expand_with_direction(BgaExpandConfig::KeepAspect, 1),
            BgaExpandConfig::Full
        );
        assert_eq!(
            cycle_gauge_auto_shift_option_with_direction(GaugeAutoShiftConfig::Off, -1),
            GaugeAutoShiftConfig::SelectToUnder
        );
    }

    #[test]
    fn select_mode_filter_keeps_matching_chart_rows() {
        let mut k7 = select_chart_row(1);
        k7.chart.as_mut().unwrap().mode = "7K".to_string();
        let mut k14 = select_chart_row(2);
        k14.chart.as_mut().unwrap().mode = "14K".to_string();
        let mut items = vec![
            SelectItem::Folder {
                path: "folder".to_string(),
                name: "folder".to_string(),
                kind: SelectRowKind::Folder,
                summary: None,
            },
            SelectItem::Chart(k7),
            SelectItem::Chart(k14),
        ];

        apply_select_mode_filter(&mut items, SelectModeFilter::K14);

        assert_eq!(items.len(), 2);
        assert!(matches!(items[0], SelectItem::Folder { .. }));
        assert_eq!(items[1].display_name(), "Title 2");
    }

    fn chart_row_with_mode(index: usize, mode: &str) -> SelectItem {
        let mut row = select_chart_row(index);
        row.chart.as_mut().unwrap().mode = mode.to_string();
        SelectItem::Chart(row)
    }

    #[test]
    fn clear_rank_separates_unowned_from_noplay() {
        // 所持済み・スコア無し → NoPlay = 0。
        let noplay = select_chart_row(1);
        assert!(noplay.in_library());
        assert_eq!(clear_rank(&noplay), 0);

        // 難易度表エントリだがローカル未所持 → NoPlay より下位の -1。
        let mut unowned = select_chart_row(2);
        unowned.chart = None;
        unowned.entry_sha256 = Some([2u8; 32]);
        assert!(!unowned.in_library());
        assert_eq!(clear_rank(&unowned), -1);

        assert!(clear_rank(&unowned) < clear_rank(&noplay));
    }

    #[test]
    fn resolve_mode_filter_keeps_mode_with_matching_charts() {
        let items = vec![chart_row_with_mode(1, "7K"), chart_row_with_mode(2, "5K")];
        // 7K のチャートがあるので据え置く。
        assert_eq!(
            resolve_non_empty_mode_filter(&items, SelectModeFilter::K7),
            SelectModeFilter::K7
        );
    }

    #[test]
    fn resolve_mode_filter_advances_when_all_charts_mismatch() {
        // 5K しか無いフォルダで 7K フィルターを掛けると全消えになるため、
        // beatoraja 同様に前方向 (K7 -> K14 -> K9 -> K5) へ送って K5 で止まる。
        let items = vec![chart_row_with_mode(1, "5K"), chart_row_with_mode(2, "5K")];
        assert_eq!(
            resolve_non_empty_mode_filter(&items, SelectModeFilter::K7),
            SelectModeFilter::K5
        );
    }

    #[test]
    fn resolve_mode_filter_does_not_advance_when_folder_remains() {
        // フォルダ行が残るなら全消えにはならないので据え置く（beatoraja 準拠）。
        let items = vec![
            SelectItem::Folder {
                path: "folder".to_string(),
                name: "folder".to_string(),
                kind: SelectRowKind::Folder,
                summary: None,
            },
            chart_row_with_mode(1, "5K"),
        ];
        assert_eq!(
            resolve_non_empty_mode_filter(&items, SelectModeFilter::K7),
            SelectModeFilter::K7
        );
    }

    #[test]
    fn resolve_mode_filter_keeps_all_filter() {
        let items = vec![chart_row_with_mode(1, "5K")];
        assert_eq!(
            resolve_non_empty_mode_filter(&items, SelectModeFilter::All),
            SelectModeFilter::All
        );
    }

    #[test]
    fn select_mode_filter_roundtrips_through_str() {
        for mode in SelectModeFilter::ORDER {
            assert_eq!(SelectModeFilter::from_str_or_default(mode.as_str()), mode);
        }
        assert_eq!(SelectModeFilter::from_str_or_default("unknown"), SelectModeFilter::All);
    }

    #[test]
    fn select_sort_roundtrips_through_str() {
        for sort in SelectSort::ORDER {
            assert_eq!(SelectSort::from_str_or_default(sort.as_str()), sort);
        }
        assert_eq!(SelectSort::from_str_or_default("unknown"), SelectSort::Title);
    }

    #[test]
    fn select_sort_orders_chart_rows_without_moving_folders() {
        let mut slow = select_chart_row(1);
        slow.chart.as_mut().unwrap().title = "Slow".to_string();
        slow.chart.as_mut().unwrap().initial_bpm = 100.0;
        let mut fast = select_chart_row(2);
        fast.chart.as_mut().unwrap().title = "Fast".to_string();
        fast.chart.as_mut().unwrap().initial_bpm = 200.0;
        let mut items = vec![
            SelectItem::Folder {
                path: "folder".to_string(),
                name: "folder".to_string(),
                kind: SelectRowKind::Folder,
                summary: None,
            },
            SelectItem::Chart(fast),
            SelectItem::Chart(slow),
        ];

        apply_select_sort(&mut items, SelectSort::Bpm);

        assert!(matches!(items[0], SelectItem::Folder { .. }));
        assert_eq!(items[1].display_name(), "Slow");
        assert_eq!(items[2].display_name(), "Fast");
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
                subartist: String::new(),
                genre: String::new(),
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
                has_long_notes: false,
                has_mines: false,
                judge_rank: Some(1),
                bms_total: 200.0,
                ln_profile: Default::default(),
            }),
            chart_analysis: Some(crate::storage::library_db::ChartAnalysisSummary {
                normal_notes: 40 + index as u32,
                long_notes: 1 + index as u32,
                scratch_notes: 3,
                long_scratch_notes: 1,
                density: 4.5,
                peak_density: 12.5,
                end_density: 8.25,
                total_gauge: 260.0,
                main_bpm: 128.0,
                speed_changes: Vec::new(),
            }),
            fallback_title: String::new(),
            fallback_artist: String::new(),
            entry_sha256: None,
            best_score: None,
            replay_slots: [false; 4],
            table_level: String::new(),
        }
    }

    fn select_course_row(resolved_count: usize, entry_count: usize) -> SelectCourseRow {
        let entry_previews = (0..entry_count)
            .map(|index| crate::screens::select_model::CourseEntryPreview {
                title: format!("Stage {}", index + 1),
                artist: String::new(),
                play_level: String::new(),
                difficulty_name: String::new(),
                total_notes: 0,
                resolved: index < resolved_count,
            })
            .collect();
        SelectCourseRow {
            course_id: resolved_count as i64,
            title: format!("Course {resolved_count}/{entry_count}"),
            kind: bmz_core::course::CourseKind::Dan,
            constraints: bmz_core::course::CourseConstraints::default(),
            entry_count,
            resolved_count,
            total_notes: 100,
            total_length_ms: 90_000,
            min_bpm: 128.0,
            max_bpm: 128.0,
            category_label: "DAN".to_string(),
            trophy_names: Vec::new(),
            entry_previews,
            best_score: None,
            replay_slots: [false; 4],
            achieved_trophy_names: Vec::new(),
        }
    }

    fn best_score_with_replay(ex_score: u32, replay_path: &str) -> BestScoreSummary {
        BestScoreSummary {
            chart_sha256: [0; 32],
            ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
            clear_type: "Normal".to_string(),
            gauge_type: "Normal".to_string(),
            gauge_value: 80.0,
            ex_score,
            bp: 0,
            cb: 0,
            max_combo: 100,
            judge_counts: DisplayJudgeCounts::default(),
            fast_slow_counts: FastSlowJudgeCounts::default(),
            play_count: 42,
            clear_count: 31,
            device_type: bmz_core::input::InputDeviceKind::Keyboard,
            played_at: 1,
            replay_path: replay_path.to_string(),
        }
    }
}
