use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use bmz_chart::model::PlayableChart;
use bmz_core::time::TimeUs;
use bmz_render::assets::load_static_rgba_image;
use bmz_render::plan::TextureId;
use bmz_render::renderer::{RenderSurfaceStatus, Renderer, SurfaceSize};
use bmz_render::sample::{sample_play_scene, sample_result_scene, sample_select_scene};
use bmz_render::scene::{AppSceneSnapshot, ResultSnapshot, SelectRowSnapshot, SelectSnapshot};
use bmz_render::snapshot::{DisplayJudgeCounts, FastSlowJudgeCounts, RenderSnapshot};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::monitor::{MonitorHandle, VideoModeHandle};
use winit::window::{Fullscreen, Window, WindowAttributes, WindowId};

use crate::audio::AppAudioOutput;
use crate::bootstrap::{self, BootstrappedApp};
use crate::cli::{
    AUTOPLAY_ON_START_ARG, AppOptions, BOOT_PLAY_SAMPLE_ARG, SMOKE_EXIT_AFTER_FRAMES_ARG,
    SMOKE_EXIT_ON_RESULT_ARG,
};
use crate::config::app_config::{PathEntry, WindowMode};
use crate::config::profile_config::{
    AssistOptionConfig, BgaModeConfig, GaugeTypeConfig, LaneConfig, ProfileConfig,
    ProfileInputConfig, RandomOptionConfig,
};
use crate::config::save::{save_app_config, save_profile_config};
use crate::input::winit::physical_key_to_control;
use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::play_loop::{PlayAdvanceOutcome, advance_running_play_session_until_result};
use crate::screens::play_snapshot::{BgaFrameCatalog, bga_texture_id, display_bga_frame};
use crate::screens::play_start::{PlayStartOptions, StartedWinitPlaySession};
use crate::screens::result_model::ResultSummary;
use crate::screens::select_model::{
    SelectItem, TABLE_ROOT_PATH, TablePath, load_select_items_in_folder,
    load_select_items_in_table_level, parse_table_path, root_folder_items, table_folder_items,
    table_level_folder_items,
};
use crate::select_options::{ArrangeOption, AssistOption};
use crate::skin_loader::{
    DecodedFont, DecodedSkin, DecodedSource, SkinKind, apply_skin_from_config,
    decode_beatoraja_skin, install_decoded_font, install_decoded_skin, install_decoded_source,
    is_decodable_skin_path, load_default_skin_into_renderer, set_decoded_skin_context,
};
use crate::storage::replay::load_replay_for_chart;
use crate::storage::scan::scan_song_roots;
use crate::ui::{DebugInfo, EguiLayer};
use bmz_render::skin::{SkinDocument, SkinDocumentTexture, SkinManifest};
use std::collections::VecDeque;

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

    let mut app = WinitApp::new(boot, options)?;
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
                    "difficulty table fetched"
                );
                if let Err(e) = boot.library_db.upsert_difficulty_table(&table) {
                    tracing::warn!(%url, error = %e, "failed to store difficulty table");
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
    /// プレイ終了でリザルトへ移った後、曲の余韻を鳴らし切るために保持する音声出力。
    /// ドレインが完了するか、選曲復帰・次プレイ開始で解放される。
    draining_audio: Option<AppAudioOutput>,
    finished_play: Option<FinishedPlaySession>,
    last_play_snapshot: Option<RenderSnapshot>,
    last_started_chart_id: Option<i64>,
    select_items: Vec<SelectItem>,
    folder_stack: Vec<String>,
    selected_index: usize,
    renderer: Renderer,
    dev_scene: Option<AppSceneSnapshot>,
    last_scene_kind: Option<AppSceneKind>,
    start_held: bool,
    arrange_option: ArrangeOption,
    gauge_option: GaugeTypeConfig,
    assist_option: AssistOption,
    select_keys: SelectKeyBindings,
    smoke_exit_after_frames: Option<u32>,
    smoke_exit_on_result: bool,
    rendered_frames: u32,
    select_scene_started_at: Instant,
    select_bar_started_at: Instant,
    result_scene_started_at: Instant,
    option_panel_started_at: Instant,
    select_option_panel: u8,
    gilrs: Option<crate::input::gilrs::GilrsBackend>,
    default_skin_manifest: Option<SkinManifest>,
    pending_skin_rx: Option<Receiver<PendingSkinResult>>,
    pending_play_skin: bool,
    pending_result_skin: bool,
    pending_skin_installs: Vec<PendingSkinInstall>,
    /// 選曲画面でESCを長押し中の開始時刻。離されたり画面を抜けると None になる。
    select_exit_hold_started_at: Option<Instant>,
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
    /// リザルト画面終了アニメーションの進行状態。
    /// Some のあいだは終了フェードアウト中で、入力は受け付けない。
    result_exit: Option<ResultExit>,
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

struct PendingSkinResult {
    path: PathBuf,
    kind: SkinKind,
    result: Result<DecodedSkin>,
}

/// Phase B (install) を 1 フレームあたり数件ずつに分散させるためのキュー。
/// 受信した DecodedSkin をここに積み、毎フレーム少しずつ消化する。
struct PendingSkinInstall {
    kind: SkinKind,
    path: PathBuf,
    document: SkinDocument,
    default_manifest: SkinManifest,
    fonts: VecDeque<DecodedFont>,
    sources: VecDeque<DecodedSource>,
    document_textures: Vec<SkinDocumentTexture>,
}

/// 1 フレームに install する PNG ソースの最大個数。
/// 細かく分散させると debug build や低 fps 環境で完了までの総時間が伸び、
/// 結局 Select→Play 遷移で install 待ちが発生してしまうため、
/// decode が完了して main thread が一段落しているこのタイミングで一気に流し込む。
/// (release build では 1 フレームの stutter は ~数百 ms 程度に収まる)
const SKIN_INSTALL_SOURCES_PER_FRAME: usize = usize::MAX;

#[derive(Debug, Clone, PartialEq)]
enum AppViewState {
    Select,
    Play,
    Result(Box<ResultSummary>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppSceneKind {
    Select,
    Play,
    Result,
}

impl WinitApp {
    fn new(boot: BootstrappedApp, options: AppOptions) -> Result<Self> {
        let folder_stack = initial_folder_stack(&boot.app_config);
        let select_items = load_items_for_stack(&boot, &folder_stack);
        let boot_sample_chart_id = options
            .boot_play_sample
            .then(|| boot.library_db.chart_id_by_title(SAMPLE_PLAYABLE_TITLE).ok().flatten())
            .flatten();
        log_startup_options(&options);

        let assist_option = if options.autoplay_on_start || boot.profile_config.play.auto_play {
            AssistOption::Autoplay
        } else {
            AssistOption::Normal
        };
        let gauge_option = boot.profile_config.play.gauge;
        let arrange_option = arrange_option_from_profile(boot.profile_config.play.random);
        let select_keys = SelectKeyBindings::from_profile(&boot.profile_config.input);
        let now = Instant::now();

        let mut renderer = Renderer::default();
        let (default_skin_manifest, pending_skin_rx, pending_play_skin, pending_result_skin) =
            load_skin_textures(
                &mut renderer,
                &boot.profile_config.skin.select,
                &boot.profile_config.skin.play,
                &boot.profile_config.skin.result,
            );

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

        let mut app = Self {
            boot,
            window: None,
            active_play: None,
            draining_audio: None,
            finished_play: None,
            last_play_snapshot: None,
            last_started_chart_id: None,
            select_items,
            folder_stack,
            selected_index: 0,
            renderer,
            dev_scene: None,
            last_scene_kind: None,
            start_held: false,
            arrange_option,
            gauge_option,
            assist_option,
            select_keys,
            smoke_exit_after_frames: options.smoke_exit_after_frames,
            smoke_exit_on_result: options.smoke_exit_on_result,
            rendered_frames: 0,
            select_scene_started_at: now,
            select_bar_started_at: now,
            result_scene_started_at: now,
            option_panel_started_at: now,
            select_option_panel: 0,
            gilrs,
            default_skin_manifest,
            pending_skin_rx,
            pending_play_skin,
            pending_result_skin,
            pending_skin_installs: Vec::new(),
            select_exit_hold_started_at: None,
            last_play_start_press_at: None,
            egui: None,
            applied_window_mode: initial_window_mode,
            focused: true,
            last_frame_at: None,
            result_exit: None,
        };
        if let Some(chart_id) = boot_sample_chart_id {
            tracing::info!(
                arg = BOOT_PLAY_SAMPLE_ARG,
                chart_id,
                "booting directly into bundled sample chart"
            );
            if let Some(slot) = options.boot_replay_slot {
                if !app.try_start_replay_for_chart(chart_id, slot) {
                    tracing::warn!(
                        slot,
                        "--boot-replay: slot empty for sample chart, falling back to normal boot"
                    );
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
                // サーフェス生成前に VSync 設定を反映させておく。
                self.renderer.set_vsync(self.boot.app_config.video.vsync);
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
        if self.active_play.is_some() {
            return AppViewState::Play;
        }

        if let Some(finished) = &self.finished_play {
            return AppViewState::Result(Box::new(finished.summary.clone()));
        }

        AppViewState::Select
    }

    fn scene_snapshot(&self) -> AppSceneSnapshot {
        if let Some(scene) = &self.dev_scene {
            return scene.clone();
        }

        match self.view_state() {
            AppViewState::Select => AppSceneSnapshot::Select(self.select_snapshot()),
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
            }),
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
                Some(SelectItem::Chart(row)) => Some(row.chart.chart_id),
                _ => None,
            },
            selected_title: selected.map(|i| i.display_name().to_string()).unwrap_or_default(),
            rows: select_snapshot_rows(&self.select_items, self.selected_index, 25),
            arrange: self.arrange_option.as_str().to_string(),
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
        }
    }

    fn should_exit_via_select_hold(&mut self) -> bool {
        if !matches!(self.view_state(), AppViewState::Select) || self.dev_scene.is_some() {
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

    fn option_panel_time(&self) -> TimeUs {
        let micros =
            self.option_panel_started_at.elapsed().as_micros().min(i64::MAX as u128) as i64;
        TimeUs(micros)
    }

    fn route_keyboard_input(&mut self, event: &winit::event::KeyEvent) {
        if self.route_dev_scene_key(event.physical_key, event.state, event.repeat) {
            return;
        }

        if let Some(active_play) = &mut self.active_play {
            if let Some(control) = physical_key_name(event.physical_key)
                && control == self.select_keys.start
                && !event.repeat
            {
                active_play.running.session.lane_cover_changing =
                    event.state == ElementState::Pressed;
            }
            if let Some(change) = hispeed_action(event.physical_key, event.state, event.repeat) {
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
                && control == self.select_keys.start
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

        if self.finished_play.is_some() {
            // 終了アニメーション中 (result_exit=Some) は追加入力を受け付けない。
            if self.result_exit.is_none()
                && let Some(action) = result_action(event.physical_key, event.state, event.repeat)
            {
                match action {
                    ResultAction::Retry => self.begin_result_exit(ResultExitAction::Retry),
                    ResultAction::Leave => self.begin_result_exit(ResultExitAction::Leave),
                }
            }
            return;
        }

        if self.dev_scene.is_none()
            && event.physical_key == PhysicalKey::Code(KeyCode::F2)
            && event.state == ElementState::Pressed
            && !event.repeat
        {
            self.rescan_and_reload();
            return;
        }

        // Select 画面で ESC 長押し → アプリ終了 (実際の exit は redraw 時にチェック)。
        if self.dev_scene.is_none() && event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
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

        // Track start button held state for option cycling
        if let Some(control) = physical_key_name(event.physical_key)
            && control == self.select_keys.start
        {
            let pressed = event.state == ElementState::Pressed;
            if self.start_held != pressed {
                self.option_panel_started_at = Instant::now();
            }
            self.start_held = pressed;
            self.select_option_panel = if pressed { 1 } else { 0 };
            return;
        }

        if self.start_held {
            if event.state == ElementState::Pressed && !event.repeat {
                if let Some(slot) = digit_to_replay_slot(event.physical_key) {
                    if !self.start_replay_for_selected(slot) {
                        tracing::info!(slot, "Start+digit pressed but no replay available");
                    }
                    return;
                }
                if let Some(control) = physical_key_name(event.physical_key) {
                    if self.select_keys.cycle_arrange.as_deref() == Some(&control) {
                        self.arrange_option = self.arrange_option.cycle();
                        tracing::info!(
                            arrange = self.arrange_option.as_str(),
                            "arrange option changed"
                        );
                    } else if self.select_keys.cycle_gauge.as_deref() == Some(&control) {
                        self.gauge_option = cycle_gauge_option(self.gauge_option);
                        tracing::info!(gauge = ?self.gauge_option, "gauge option changed");
                    } else if self.select_keys.cycle_assist.as_deref() == Some(&control) {
                        self.assist_option = self.assist_option.cycle();
                        tracing::info!(
                            assist = self.assist_option.as_str(),
                            "assist option changed"
                        );
                    }
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
            && button == "Start"
        {
            active_play.running.session.lane_cover_changing = pressed;
        }
        if !pressed {
            if button == "Start" {
                self.start_held = false;
                self.select_option_panel = 0;
                self.option_panel_started_at = Instant::now();
            }
            return;
        }

        // プレイ中: プレイ入力は push_shared_event で処理済み
        if self.active_play.is_some() {
            return;
        }

        // リザルト画面
        if self.finished_play.is_some() {
            // 終了アニメーション中 (result_exit=Some) は追加入力を受け付けない。
            if self.result_exit.is_none() {
                match button {
                    "Button1" | "Start" => self.begin_result_exit(ResultExitAction::Retry),
                    "Button2" | "Select" => self.begin_result_exit(ResultExitAction::Leave),
                    _ => {}
                }
            }
            return;
        }

        // Start ボタン押下
        if button == "Start" {
            if !self.start_held {
                self.option_panel_started_at = Instant::now();
            }
            self.start_held = true;
            self.select_option_panel = 1;
            return;
        }

        // Start 押しながら: オプション切替
        if self.start_held {
            if self.select_keys.cycle_arrange.as_deref() == Some(button) {
                self.arrange_option = self.arrange_option.cycle();
                tracing::info!(arrange = self.arrange_option.as_str(), "arrange option changed");
            } else if self.select_keys.cycle_gauge.as_deref() == Some(button) {
                self.gauge_option = cycle_gauge_option(self.gauge_option);
                tracing::info!(gauge = ?self.gauge_option, "gauge option changed");
            } else if self.select_keys.cycle_assist.as_deref() == Some(button) {
                self.assist_option = self.assist_option.cycle();
                tracing::info!(assist = self.assist_option.as_str(), "assist option changed");
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
        }
    }

    fn enter_or_play_selected(&mut self) {
        if self.select_items.is_empty() {
            self.reload_select_items();
        }
        match self.select_items.get(self.selected_index).cloned() {
            Some(SelectItem::Folder { path, .. }) => {
                self.folder_stack.push(path);
                self.reload_select_items();
                self.selected_index = 0;
                self.select_bar_started_at = Instant::now();
                tracing::info!(folder = ?self.folder_stack.last(), "entered folder");
            }
            Some(SelectItem::Chart(row)) => {
                self.start_chart(row.chart.chart_id);
            }
            None => {
                tracing::warn!("no item is available to select");
            }
        }
    }

    fn exit_folder(&mut self) {
        if self.folder_stack.pop().is_some() {
            self.reload_select_items();
            self.selected_index = 0;
            self.select_bar_started_at = Instant::now();
            tracing::info!(depth = self.folder_stack.len(), "exited folder");
        }
    }

    fn start_chart(&mut self, chart_id: i64) {
        let options = self.play_start_options();
        self.start_chart_with_options(chart_id, options);
    }

    fn start_chart_with_options(&mut self, chart_id: i64, options: PlayStartOptions) {
        self.ensure_skin_ready(SkinKind::Play);
        // 新しいプレイの音声出力を開く前に、前曲の余韻再生を止めて出力を解放する。
        self.draining_audio = None;
        match self.boot.start_play_for_chart_with_winit_input(chart_id, options) {
            Ok(mut active_play) => {
                active_play.running.bga_frames =
                    load_chart_bga_textures(&mut self.renderer, &active_play.running.session.chart);
                self.active_play = Some(active_play);
                self.finished_play = None;
                self.last_play_snapshot = None;
                self.last_started_chart_id = Some(chart_id);
            }
            Err(error) => {
                tracing::error!(chart_id, %error, "failed to start play");
            }
        }
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
            arrange_seed: replay_file.arrange_seed,
            arrange_pattern: replay_file.lane_shuffle_pattern.clone(),
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
            SelectItem::Chart(row) => Some(row.chart.chart_id),
            SelectItem::Folder { .. } => None,
        }
    }

    fn retry_last_chart(&mut self) {
        let Some(chart_id) = self.last_started_chart_id else {
            tracing::warn!("no previous chart is available to retry");
            return;
        };
        self.start_chart(chart_id);
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
    }

    /// 終了フェードアウトの経過を監視し、スキンのフェードアウト時間を過ぎたら
    /// 保留していた遷移を実行する。毎フレーム描画前に呼ぶ。
    fn advance_result_exit(&mut self) {
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
        self.result_exit = None;
        // リザルト画面を抜けたら、まだ鳴っていても余韻再生を止める。
        self.draining_audio = None;
        self.last_play_snapshot = None;
        self.reload_select_items();
        let now = Instant::now();
        self.select_scene_started_at = now;
        self.select_bar_started_at = now;
    }

    fn reload_select_items(&mut self) {
        let items = load_items_for_stack(&self.boot, &self.folder_stack);
        self.select_items = items;
        if self.selected_index >= self.select_items.len() {
            self.selected_index = self.select_items.len().saturating_sub(1);
        }
    }

    fn rescan_and_reload(&mut self) {
        let scan_roots: Vec<PathEntry> = if let Some(folder) = self.folder_stack.last() {
            vec![PathEntry { path: folder.clone(), enabled: true, recursive: true }]
        } else {
            self.boot.app_config.songs.roots.iter().filter(|p| p.enabled).cloned().collect()
        };

        if !scan_roots.is_empty() {
            match scan_song_roots(
                &mut self.boot.library_db,
                &scan_roots,
                &self.boot.app_config.scan,
                now_unix_seconds(),
            ) {
                Ok(report) => tracing::info!(
                    imported = report.summary.imported,
                    skipped = report.summary.skipped,
                    failed = report.summary.failed,
                    "F2 rescan complete"
                ),
                Err(error) => tracing::error!(%error, "F2 rescan failed"),
            }
        }

        self.reload_select_items();
    }

    /// バックグラウンドで decode 中のスキンを非ブロッキングで取り込み、install キューに積む。
    /// 毎フレーム呼ぶ。実際の Renderer への install 反映は `step_skin_installs` で分散実行する。
    fn drain_pending_skins(&mut self) {
        while let Some(rx) = self.pending_skin_rx.as_ref() {
            match rx.try_recv() {
                Ok(result) => self.enqueue_pending_skin_install(result),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_skin_rx = None;
                    break;
                }
            }
        }
        self.step_skin_installs();
    }

    /// 指定された kind のスキンが install されるまで強制的に完了させる。
    /// scene 遷移直前に呼ぶ用途。decode 完了まちのチャネル受信もブロックして待つ。
    fn ensure_skin_ready(&mut self, kind: SkinKind) {
        // まだ decode 結果が届いていなければブロックして受信する。
        while self.is_kind_pending_decode(kind) {
            let Some(rx) = self.pending_skin_rx.as_ref() else {
                break;
            };
            match rx.recv() {
                Ok(result) => self.enqueue_pending_skin_install(result),
                Err(_) => {
                    self.pending_skin_rx = None;
                    break;
                }
            }
        }
        // install キューに残っている該当 kind を一気に流す。
        self.flush_skin_installs_for(kind);
    }

    fn is_kind_pending_decode(&self, kind: SkinKind) -> bool {
        match kind {
            SkinKind::Play => self.pending_play_skin,
            SkinKind::Result => self.pending_result_skin,
            SkinKind::Select => false,
        }
    }

    fn enqueue_pending_skin_install(&mut self, pending: PendingSkinResult) {
        let PendingSkinResult { path, kind, result } = pending;
        match kind {
            SkinKind::Play => self.pending_play_skin = false,
            SkinKind::Result => self.pending_result_skin = false,
            SkinKind::Select => {}
        }
        let decoded = match result {
            Ok(decoded) => decoded,
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    kind = ?kind,
                    error = %format_error_chain(&error),
                    "failed to decode beatoraja skin in background"
                );
                return;
            }
        };
        let Some(manifest) = self.default_skin_manifest.clone() else {
            tracing::warn!(
                path = %path.display(),
                kind = ?kind,
                "skipping pending skin install because default skin manifest is unavailable"
            );
            return;
        };
        let DecodedSkin { kind, document, fonts, sources } = decoded;
        self.pending_skin_installs.push(PendingSkinInstall {
            kind,
            path,
            document,
            default_manifest: manifest,
            fonts: fonts.into_iter().collect(),
            sources: sources.into_iter().collect(),
            document_textures: Vec::new(),
        });
    }

    /// install キューを 1 フレーム分だけ進める。
    /// フォントは安価なので一括処理、PNG は `SKIN_INSTALL_SOURCES_PER_FRAME` 個までを上限に
    /// 各 install を順番に進めていく。budget を使い切ったらそのフレームは打ち切る。
    fn step_skin_installs(&mut self) {
        let mut budget = SKIN_INSTALL_SOURCES_PER_FRAME;
        while !self.pending_skin_installs.is_empty() {
            let install = &mut self.pending_skin_installs[0];
            while let Some(font) = install.fonts.pop_front() {
                install_decoded_font(&mut self.renderer, font);
            }
            while budget > 0
                && let Some(source) = install.sources.pop_front()
            {
                if let Some(texture) = install_decoded_source(&mut self.renderer, source) {
                    install.document_textures.push(texture);
                }
                budget = budget.saturating_sub(1);
            }
            if install.fonts.is_empty() && install.sources.is_empty() {
                let install = self.pending_skin_installs.remove(0);
                self.finalize_skin_install(install);
            } else {
                // budget を使い切った: 続きは次フレームで。
                break;
            }
        }
    }

    fn flush_skin_installs_for(&mut self, kind: SkinKind) {
        let mut remaining = std::mem::take(&mut self.pending_skin_installs);
        let (match_kind, others): (Vec<_>, Vec<_>) =
            remaining.drain(..).partition(|install| install.kind == kind);
        self.pending_skin_installs = others;
        for mut install in match_kind {
            while let Some(font) = install.fonts.pop_front() {
                install_decoded_font(&mut self.renderer, font);
            }
            while let Some(source) = install.sources.pop_front() {
                if let Some(texture) = install_decoded_source(&mut self.renderer, source) {
                    install.document_textures.push(texture);
                }
            }
            self.finalize_skin_install(install);
        }
    }

    fn finalize_skin_install(&mut self, install: PendingSkinInstall) {
        let PendingSkinInstall {
            kind, path, document, default_manifest, document_textures, ..
        } = install;
        tracing::info!(
            path = %path.display(),
            kind = ?kind,
            sources = document_textures.len(),
            "beatoraja skin fully installed"
        );
        set_decoded_skin_context(
            &mut self.renderer,
            kind,
            default_manifest,
            document,
            document_textures,
        );
    }

    fn advance_active_play(&mut self) {
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
                self.last_play_snapshot = Some(frame.render_snapshot);
            }
            Ok(PlayAdvanceOutcome::Finished { frame, finished }) => {
                let hispeed = active_play.running.session.hispeed;
                self.last_play_snapshot = Some(frame.render_snapshot);
                // active_play がまだ残っている内に hispeed/lane_cover/lift を profile に保存する。
                self.save_current_play_options(Some(hispeed), "play finished");
                // リザルト画面へ移っても曲の最後まで鳴らすため、音声出力だけは
                // 取り出して保持する。スケジュール済みの BGM/キー音はオーディオ
                // スレッドで鳴り切るまで再生され、advance_draining_audio が
                // ドレイン完了後に解放する。
                if let Some(started) = self.active_play.take() {
                    self.draining_audio = Some(started.running.audio);
                }
                self.finished_play = Some(finished);
                self.ensure_skin_ready(SkinKind::Result);
            }
            Err(error) => {
                tracing::error!(%error, "failed to advance play session");
                self.active_play = None;
                self.last_play_snapshot = None;
            }
        }
    }

    /// `target_fps` (フォアグラウンド) / `frame_limit_in_background`
    /// (非フォーカス時) に従ってフレーム開始間隔を一定に保つ。
    ///
    /// 各 `RedrawRequested` の先頭で呼び、前フレーム開始からの経過が
    /// フレーム予算に満たなければ残りをスリープする。FPS 値が 0 の場合は
    /// 無制限としてスリープしない。
    fn limit_frame_rate(&mut self) {
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
        self.last_frame_at = Some(Instant::now());
    }

    /// egui の 1 フレームを構築し、renderer へ描画データを渡す。
    /// `render_current_scene` の前に呼ぶこと。
    fn run_egui_frame(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let scene = match scene_kind(&self.scene_snapshot()) {
            AppSceneKind::Select => "Select",
            AppSceneKind::Play => "Play",
            AppSceneKind::Result => "Result",
        };
        let size = window.inner_size();
        let info = DebugInfo { scene, width: size.width, height: size.height };
        let Some(egui) = self.egui.as_mut() else {
            return;
        };
        let output =
            egui.run(&window, &info, &mut self.boot.app_config, &mut self.boot.profile_config);
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
        if output.save_profile_config {
            match save_profile_config(
                &self.boot.profile_paths.profile_toml,
                &self.boot.profile_config,
            ) {
                Ok(()) => tracing::info!("profile config saved from egui skin panel"),
                Err(error) => tracing::error!(%error, "failed to save profile config"),
            }
        }
        if output.reload_skins {
            self.reload_skins();
        }
    }

    /// 現在の profile config のスキンパスを renderer へ再適用する。
    ///
    /// 起動時と同じ `load_skin_textures` 経路を使い、JSON スキンは
    /// バックグラウンド decode + 段階 install パイプラインへ流す。
    fn reload_skins(&mut self) {
        let select = self.boot.profile_config.skin.select.clone();
        let play = self.boot.profile_config.skin.play.clone();
        let result = self.boot.profile_config.skin.result.clone();
        let (manifest, rx, pending_play, pending_result) =
            load_skin_textures(&mut self.renderer, &select, &play, &result);
        self.default_skin_manifest = manifest;
        self.pending_skin_rx = rx;
        self.pending_play_skin = pending_play;
        self.pending_result_skin = pending_result;
        // 前回リロードの未完了 install は破棄する。
        self.pending_skin_installs.clear();
        tracing::info!("skins reloaded from egui skin panel");
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

    fn route_dev_scene_key(
        &mut self,
        physical_key: PhysicalKey,
        state: ElementState,
        repeat: bool,
    ) -> bool {
        match dev_scene_action(physical_key, state, repeat) {
            Some(DevSceneAction::SampleSelect) => {
                self.dev_scene = Some(sample_select_scene());
                tracing::info!("showing sample select scene");
                true
            }
            Some(DevSceneAction::SamplePlay) => {
                self.dev_scene = Some(sample_play_scene());
                tracing::info!("showing sample play scene");
                true
            }
            Some(DevSceneAction::SampleResult) => {
                self.dev_scene = Some(sample_result_scene());
                tracing::info!("showing sample result scene");
                true
            }
            Some(DevSceneAction::Clear) if self.dev_scene.is_some() => {
                self.dev_scene = None;
                tracing::info!("leaving sample scene");
                true
            }
            _ => false,
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

        self.last_scene_kind = Some(scene_kind);
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
/// - play / result の JSON skin はバックグラウンドスレッドで Phase A (decode) を実行。
///   完了したものは main thread の `try_recv` で順次 Phase B (install) する。
/// - select/play/result の各パスが JSON 以外 (空文字 or .toml ディレクトリ) の場合は
///   従来通り同期処理 (短時間で完了する想定)。
fn load_skin_textures(
    renderer: &mut Renderer,
    select_skin_path: &str,
    play_skin_path: &str,
    result_skin_path: &str,
) -> (Option<SkinManifest>, Option<Receiver<PendingSkinResult>>, bool, bool) {
    // Play / Result の JSON skin は Select の同期ロードより**前**に decode スレッドを起動して
    // CPU をフル活用する。Select の sync 処理 (PNG GPU upload など) と並列に decode が進む。
    let (tx, rx) = mpsc::channel::<PendingSkinResult>();
    let mut pending_play = false;
    let mut pending_result = false;

    let play_trimmed = play_skin_path.trim().to_string();
    let result_trimmed = result_skin_path.trim().to_string();

    if !play_trimmed.is_empty() {
        let play_path = Path::new(&play_trimmed);
        if is_decodable_skin_path(play_path) {
            spawn_skin_decode(tx.clone(), play_path.to_path_buf(), SkinKind::Play);
            pending_play = true;
        }
    }
    if !result_trimmed.is_empty() {
        let result_path = Path::new(&result_trimmed);
        if is_decodable_skin_path(result_path) {
            spawn_skin_decode(tx.clone(), result_path.to_path_buf(), SkinKind::Result);
            pending_result = true;
        }
    }
    drop(tx);

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
            apply_json_skin_sync(renderer, path, SkinKind::Select, default_manifest.as_ref());
        } else {
            tracing::warn!(
                path = %path.display(),
                "select skin path is not a supported beatoraja skin file; ignoring"
            );
        }
    }

    // beatoraja 形式以外の play skin (toml/bmz-style) は同期ロード。
    if !play_trimmed.is_empty()
        && !is_decodable_skin_path(Path::new(&play_trimmed))
        && let Err(error) = apply_skin_from_config(renderer, &play_trimmed)
    {
        tracing::warn!(
            error = %format_error_chain(&error),
            "failed to apply play skin; using fallback textures"
        );
    }
    if !result_trimmed.is_empty() && !is_decodable_skin_path(Path::new(&result_trimmed)) {
        tracing::warn!(
            path = %result_trimmed,
            "result skin path is not a supported beatoraja skin file; ignoring"
        );
    }

    let rx = if pending_play || pending_result { Some(rx) } else { None };
    (default_manifest, rx, pending_play, pending_result)
}

fn apply_json_skin_sync(
    renderer: &mut Renderer,
    path: &Path,
    kind: SkinKind,
    default_manifest: Option<&SkinManifest>,
) {
    let Some(manifest) = default_manifest else {
        tracing::warn!(
            path = %path.display(),
            kind = ?kind,
            "skipping skin install because default skin manifest is unavailable"
        );
        return;
    };
    let decoded = match decode_beatoraja_skin(path, kind) {
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

fn spawn_skin_decode(tx: mpsc::Sender<PendingSkinResult>, path: PathBuf, kind: SkinKind) {
    let send_path = path.clone();
    thread::Builder::new()
        .name(format!("skin-decode-{:?}", kind))
        .spawn(move || {
            let result = decode_beatoraja_skin(&path, kind);
            let _ = tx.send(PendingSkinResult { path: send_path, kind, result });
        })
        .expect("failed to spawn skin decode thread");
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
                // F5 で egui メニューを開閉する。
                if event.physical_key == PhysicalKey::Code(KeyCode::F5)
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

fn log_startup_options(options: &AppOptions) {
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
            // ルートには曲フォルダに続けて、各難易度表フォルダ（発狂BMS / Stella 等）を並べる。
            let mut items = root_folder_items(&enabled_root_paths(&boot.app_config));
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
                },
                SelectItem::Chart(row) => SelectRowSnapshot {
                    index: index as u32,
                    title: row.chart.title.clone(),
                    artist: row.chart.artist.clone(),
                    difficulty_name: row.chart.difficulty_name.clone(),
                    play_level: row.chart.play_level.clone(),
                    table_level: row.table_level.clone(),
                    total_notes: row.chart.total_notes,
                    initial_bpm: row.chart.initial_bpm as f32,
                    min_bpm: row.chart.min_bpm as f32,
                    max_bpm: row.chart.max_bpm as f32,
                    length_ms: row.chart.length_ms,
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

fn scene_kind(scene: &AppSceneSnapshot) -> AppSceneKind {
    match scene {
        AppSceneSnapshot::Select(_) => AppSceneKind::Select,
        AppSceneSnapshot::Play(_) => AppSceneKind::Play,
        AppSceneSnapshot::Result(_) => AppSceneKind::Result,
    }
}

fn window_title_for_scene(scene_kind: AppSceneKind) -> &'static str {
    match scene_kind {
        AppSceneKind::Select => "bmz-player - Select",
        AppSceneKind::Play => "bmz-player - Play",
        AppSceneKind::Result => "bmz-player - Result",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DevSceneAction {
    SampleSelect,
    SamplePlay,
    SampleResult,
    Clear,
}

fn dev_scene_action(
    physical_key: PhysicalKey,
    state: ElementState,
    repeat: bool,
) -> Option<DevSceneAction> {
    if state != ElementState::Pressed || repeat {
        return None;
    }

    match physical_key {
        PhysicalKey::Code(KeyCode::F1) => Some(DevSceneAction::SampleSelect),
        PhysicalKey::Code(KeyCode::F3) => Some(DevSceneAction::SampleResult),
        PhysicalKey::Code(KeyCode::F4) => Some(DevSceneAction::SamplePlay),
        PhysicalKey::Code(KeyCode::Escape) => Some(DevSceneAction::Clear),
        _ => None,
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

struct SelectKeyBindings {
    start: String,
    enter: Vec<String>,
    back: Vec<String>,
    cycle_arrange: Option<String>,
    cycle_gauge: Option<String>,
    cycle_assist: Option<String>,
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
            kb.iter().filter(|e| e.lane == lane).map(|e| e.control.clone()).collect()
        };

        // キーボード + ゲームパッド（is_enter / is_back ルックアップ用）
        let keys_for = |lane: LaneConfig| -> Vec<String> {
            all_input.iter().filter(|e| e.lane == lane).map(|e| e.control.clone()).collect()
        };

        let enter: Vec<String> =
            [LaneConfig::Key1, LaneConfig::Key3, LaneConfig::Key5, LaneConfig::Key7]
                .iter()
                .flat_map(|&l| keys_for(l))
                .collect();
        let back = keys_for(LaneConfig::Key2);
        let cycle_arrange = keys_for(LaneConfig::Key1).into_iter().next();
        let cycle_gauge = keys_for(LaneConfig::Key3).into_iter().next();
        let cycle_assist = keys_for(LaneConfig::Key5).into_iter().next();
        let start = input.start_key.clone();

        // ヒント文字列はキーボードバインドのみ使用
        let kb_enter: Vec<String> =
            [LaneConfig::Key1, LaneConfig::Key3, LaneConfig::Key5, LaneConfig::Key7]
                .iter()
                .flat_map(|&l| kb_keys_for(l))
                .collect();
        let kb_back = kb_keys_for(LaneConfig::Key2);
        let enter_str =
            if kb_enter.is_empty() { String::new() } else { format!("/{}", kb_enter.join("/")) };
        let back_str = kb_back.first().map(|k| format!("/{k}")).unwrap_or_default();
        let key_hint =
            format!("UP DOWN  RIGHT{enter_str}:ENTER  LEFT{back_str}:BACK  ENTER {start}");

        let kb_arrange_str = kb_keys_for(LaneConfig::Key1).into_iter().next();
        let kb_gauge_str = kb_keys_for(LaneConfig::Key3).into_iter().next();
        let kb_assist_str = kb_keys_for(LaneConfig::Key5).into_iter().next();
        let arrange_str = kb_arrange_str.as_deref().unwrap_or("?");
        let gauge_str = kb_gauge_str.as_deref().unwrap_or("?");
        let assist_str = kb_assist_str.as_deref().unwrap_or("?");
        let option_hint = format!(
            "F1 SELECT  F2 RELOAD  F3 RESULT  F4 PLAY   \
             {start}+{arrange_str}:ARRANGE  {start}+{gauge_str}:GAUGE  {start}+{assist_str}:ASSIST  \
             {start}+1..4:REPLAY"
        );

        Self { start, enter, back, cycle_arrange, cycle_gauge, cycle_assist, key_hint, option_hint }
    }

    fn is_enter(&self, control: &str) -> bool {
        self.enter.iter().any(|k| k == control)
    }

    fn is_back(&self, control: &str) -> bool {
        self.back.iter().any(|k| k == control)
    }
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

    fn default_select_keys() -> SelectKeyBindings {
        use crate::config::profile_config::default_keyboard_bindings;
        SelectKeyBindings::from_profile(&ProfileInputConfig {
            scratch_mode: crate::config::profile_config::ScratchInputMode::Normal,
            start_key: "Q".to_string(),
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
    fn select_key_bindings_builds_correct_hints() {
        let keys = default_select_keys();
        assert!(keys.key_hint.contains("Z/X/C/V"), "enter keys in hint: {}", keys.key_hint);
        assert!(keys.key_hint.contains("/S:BACK"), "back key in hint: {}", keys.key_hint);
        assert!(keys.key_hint.contains(" Q"), "start key in hint: {}", keys.key_hint);
        assert!(keys.option_hint.contains("F2 RELOAD"), "reload in hint: {}", keys.option_hint);
        assert!(keys.option_hint.contains("F4 PLAY"), "play in hint: {}", keys.option_hint);
        assert!(keys.option_hint.contains("Q+Z:ARRANGE"), "arrange in hint: {}", keys.option_hint);
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
            GaugeTypeConfig::Hard,
            AssistOption::Autoplay,
            42,
        );

        assert_eq!(profile.lane.hispeed, 3.25);
        assert!((profile.lane.lane_cover - 0.42).abs() < 1e-6);
        assert!((profile.lane.lift - 0.1).abs() < 1e-6);
        assert!(matches!(profile.play.random, RandomOptionConfig::Mirror));
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
    fn dev_scene_keys_map_to_sample_scenes() {
        assert_eq!(
            dev_scene_action(PhysicalKey::Code(KeyCode::F1), ElementState::Pressed, false),
            Some(DevSceneAction::SampleSelect)
        );
        assert_eq!(
            dev_scene_action(PhysicalKey::Code(KeyCode::F2), ElementState::Pressed, false),
            None
        );
        assert_eq!(
            dev_scene_action(PhysicalKey::Code(KeyCode::F3), ElementState::Pressed, false),
            Some(DevSceneAction::SampleResult)
        );
        assert_eq!(
            dev_scene_action(PhysicalKey::Code(KeyCode::F4), ElementState::Pressed, false),
            Some(DevSceneAction::SamplePlay)
        );
        assert_eq!(
            dev_scene_action(PhysicalKey::Code(KeyCode::Escape), ElementState::Pressed, false),
            Some(DevSceneAction::Clear)
        );
    }

    #[test]
    fn dev_scene_keys_ignore_releases_repeats_and_other_keys() {
        assert_eq!(
            dev_scene_action(PhysicalKey::Code(KeyCode::F1), ElementState::Released, false),
            None
        );
        assert_eq!(
            dev_scene_action(PhysicalKey::Code(KeyCode::F1), ElementState::Pressed, true),
            None
        );
        assert_eq!(
            dev_scene_action(PhysicalKey::Code(KeyCode::KeyZ), ElementState::Pressed, false),
            None
        );
    }

    #[test]
    fn scene_kind_maps_scene_variants() {
        assert_eq!(scene_kind(&sample_select_scene()), AppSceneKind::Select);
        assert_eq!(scene_kind(&sample_play_scene()), AppSceneKind::Play);
        assert_eq!(scene_kind(&sample_result_scene()), AppSceneKind::Result);
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
            chart: ChartListItem {
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
            },
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
