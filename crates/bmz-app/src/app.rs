use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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
use winit::window::{Window, WindowAttributes, WindowId};

use crate::bootstrap::{self, BootstrappedApp};
use crate::cli::{
    AUTOPLAY_ON_START_ARG, AppOptions, BOOT_PLAY_SAMPLE_ARG, SMOKE_EXIT_AFTER_FRAMES_ARG,
    SMOKE_EXIT_ON_RESULT_ARG,
};
use crate::config::app_config::PathEntry;
use crate::config::profile_config::{
    AssistOptionConfig, BgaModeConfig, GaugeTypeConfig, LaneConfig, ProfileConfig,
    ProfileInputConfig, RandomOptionConfig,
};
use crate::config::save::save_profile_config;
use crate::input::winit::physical_key_to_control;
use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::play_loop::{PlayAdvanceOutcome, advance_running_play_session_until_result};
use crate::screens::play_snapshot::{BgaFrameCatalog, bga_texture_id, display_bga_frame};
use crate::screens::play_start::{PlayStartOptions, StartedWinitPlaySession};
use crate::screens::result_model::ResultSummary;
use crate::screens::select_model::{
    SelectItem, TABLE_ROOT_PATH, load_select_items_in_folder, load_select_items_in_table,
    root_folder_items, table_folder_items, table_folder_root_item,
};
use crate::select_options::{ArrangeOption, AssistOption};
use crate::skin_loader::{
    DecodedFont, DecodedSkin, DecodedSource, SkinKind, apply_skin_from_config,
    decode_beatoraja_json_skin, install_decoded_font, install_decoded_skin, install_decoded_source,
    load_default_skin_into_renderer, set_decoded_skin_context,
};
use crate::storage::replay::load_replay_for_chart;
use crate::storage::scan::scan_song_roots;
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
}

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
/// PNG アップロードは GPU テクスチャ作成を伴い 1 件あたり数 ms 〜数十 ms かかるため、
/// 体感の引っかかりを避けるよう 1 件ずつに抑える (フォントはコストが低いので一括処理)。
const SKIN_INSTALL_SOURCES_PER_FRAME: usize = 1;

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

        let mut app = Self {
            boot,
            window: None,
            active_play: None,
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

        let attributes = window_attributes_from_config(&self.boot.app_config.video);
        match event_loop.create_window(attributes) {
            Ok(window) => {
                let window = Arc::new(window);
                window.set_visible(true);
                let size = surface_size_for_window(&window);
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
                title: summary.title.clone(),
                subtitle: summary.subtitle.clone(),
                artist: summary.artist.clone(),
                subartist: summary.subartist.clone(),
                genre: summary.genre.clone(),
            }),
        }
    }

    fn select_snapshot(&self) -> SelectSnapshot {
        let selected = self.select_items.get(self.selected_index);
        let current_folder = match self.folder_stack.last() {
            None => String::new(),
            Some(path) if path == TABLE_ROOT_PATH => "難易度表".to_string(),
            Some(path) if path.starts_with(TABLE_ROOT_PATH) => {
                // Show "[symbol] name" if the table is known, else the URL's filename segment.
                let url = &path[TABLE_ROOT_PATH.len()..];
                self.boot
                    .library_db
                    .list_difficulty_tables()
                    .ok()
                    .and_then(|ts| ts.into_iter().find(|t| t.source_url == url))
                    .map(|t| format!("[{}] {}", t.symbol, t.name))
                    .unwrap_or_else(|| {
                        std::path::Path::new(url)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(url)
                            .to_string()
                    })
            }
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
            rows: select_snapshot_rows(&self.select_items, self.selected_index, 7),
            arrange: self.arrange_option.as_str().to_string(),
            gauge: gauge_option_as_str(self.gauge_option).to_string(),
            assist: self.assist_option.as_str().to_string(),
            bga: bga_mode_as_str(self.boot.profile_config.play.bga).to_string(),
            current_folder,
            key_hint: self.select_keys.key_hint.clone(),
            option_hint: self.select_keys.option_hint.clone(),
        }
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
            active_play.input.handle_key_event(event);
            return;
        }

        if self.finished_play.is_some() {
            if let Some(action) = result_action(event.physical_key, event.state, event.repeat) {
                match action {
                    ResultAction::Retry => self.retry_last_chart(),
                    ResultAction::Leave => self.leave_result(),
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
            match button {
                "Button1" | "Start" => self.retry_last_chart(),
                "Button2" | "Select" => self.leave_result(),
                _ => {}
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

    fn leave_result(&mut self) {
        self.finished_play = None;
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
    /// フォントは安価なので 1 フレームで全部、PNG は重いので `SKIN_INSTALL_SOURCES_PER_FRAME` 個まで。
    fn step_skin_installs(&mut self) {
        if self.pending_skin_installs.is_empty() {
            return;
        }
        // 先頭の install を 1 フレーム分進める。
        let install = &mut self.pending_skin_installs[0];
        // フォントは全部一括で取り込む (FontArc::try_from_vec はミリ秒オーダー)。
        while let Some(font) = install.fonts.pop_front() {
            install_decoded_font(&mut self.renderer, font);
        }
        // PNG ソースはフレームあたり SKIN_INSTALL_SOURCES_PER_FRAME 個まで。
        let mut budget = SKIN_INSTALL_SOURCES_PER_FRAME;
        while budget > 0
            && let Some(source) = install.sources.pop_front()
        {
            if let Some(texture) = install_decoded_source(&mut self.renderer, source) {
                install.document_textures.push(texture);
            }
            budget -= 1;
        }
        // 全て install 終わったら scene context をセットして dequeue する。
        if install.fonts.is_empty() && install.sources.is_empty() {
            let install = self.pending_skin_installs.remove(0);
            self.finalize_skin_install(install);
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
                self.active_play = None;
                self.finished_play = Some(finished);
                self.save_current_play_options(Some(hispeed), "play finished");
                self.ensure_skin_ready(SkinKind::Result);
            }
            Err(error) => {
                tracing::error!(%error, "failed to advance play session");
                self.active_play = None;
                self.last_play_snapshot = None;
            }
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

    fn save_current_play_options(&mut self, hispeed: Option<f32>, reason: &'static str) {
        apply_current_play_options_to_profile(
            &mut self.boot.profile_config,
            hispeed,
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
        if is_json_skin_path(path) {
            apply_json_skin_sync(renderer, path, SkinKind::Select, default_manifest.as_ref());
        } else {
            tracing::warn!(
                path = %path.display(),
                "select skin path is not a .json file; ignoring"
            );
        }
    }

    // Play / Result skin はバックグラウンドで Phase A を実行する。
    let (tx, rx) = mpsc::channel::<PendingSkinResult>();
    let mut pending_play = false;
    let mut pending_result = false;

    let play_trimmed = play_skin_path.trim().to_string();
    if !play_trimmed.is_empty() {
        let play_path = Path::new(&play_trimmed);
        if is_json_skin_path(play_path) {
            spawn_skin_decode(tx.clone(), play_path.to_path_buf(), SkinKind::Play);
            pending_play = true;
        } else {
            // toml or bmz-style directory skin。デフォルトスキンと比べて軽いので同期でロードする。
            if let Err(error) = apply_skin_from_config(renderer, &play_trimmed) {
                tracing::warn!(
                    error = %format_error_chain(&error),
                    "failed to apply play skin; using fallback textures"
                );
            }
        }
    }

    let result_trimmed = result_skin_path.trim();
    if !result_trimmed.is_empty() {
        let result_path = Path::new(result_trimmed);
        if is_json_skin_path(result_path) {
            spawn_skin_decode(tx.clone(), result_path.to_path_buf(), SkinKind::Result);
            pending_result = true;
        } else {
            tracing::warn!(
                path = %result_path.display(),
                "result skin path is not a .json file; ignoring"
            );
        }
    }

    drop(tx);
    let rx = if pending_play || pending_result { Some(rx) } else { None };
    (default_manifest, rx, pending_play, pending_result)
}

fn is_json_skin_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
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
    let decoded = match decode_beatoraja_json_skin(path, kind) {
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
            let result = decode_beatoraja_json_skin(&path, kind);
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

        match event {
            WindowEvent::CloseRequested => {
                self.save_current_play_options(self.active_hispeed(), "game exit");
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event, .. } => self.route_keyboard_input(&event),
            WindowEvent::Resized(size) => {
                self.renderer
                    .resize_surface(SurfaceSize { width: size.width, height: size.height });
            }
            WindowEvent::RedrawRequested => {
                self.drain_pending_skins();
                self.render_current_scene();
                self.advance_active_play();
                self.handle_smoke_exit_after_redraw(event_loop);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.poll_gamepad_events();
        if let Some(window) = &self.window {
            window.request_redraw();
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
        Some(path) if path == TABLE_ROOT_PATH => match table_folder_items(&boot.library_db) {
            Ok(items) => items,
            Err(error) => {
                tracing::error!(%error, "failed to load difficulty table list");
                Vec::new()
            }
        },
        Some(path) if path.starts_with(TABLE_ROOT_PATH) => {
            let source_url = &path[TABLE_ROOT_PATH.len()..];
            match load_select_items_in_table(&boot.library_db, &boot.score_db, source_url) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(%error, "failed to load difficulty table charts");
                    Vec::new()
                }
            }
        }
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
            let mut items = root_folder_items(&enabled_root_paths(&boot.app_config));
            items.push(table_folder_root_item());
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

fn apply_current_play_options_to_profile(
    profile: &mut ProfileConfig,
    hispeed: Option<f32>,
    arrange: ArrangeOption,
    gauge: GaugeTypeConfig,
    assist: AssistOption,
    updated_at: i64,
) {
    if let Some(hispeed) = hispeed {
        profile.lane.hispeed = clamp_hispeed_for_profile(hispeed);
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

    let selected_index = selected_index.min(items.len() - 1);
    let half_window = visible_limit / 2;
    let max_start = items.len().saturating_sub(visible_limit);
    let start = selected_index.saturating_sub(half_window).min(max_start);
    let end = (start + visible_limit).min(items.len());

    items[start..end]
        .iter()
        .enumerate()
        .map(|(offset, item)| {
            let index = start + offset;
            match item {
                SelectItem::Folder { name, .. } => SelectRowSnapshot {
                    index: index as u32,
                    title: name.clone(),
                    artist: String::new(),
                    play_level: String::new(),
                    table_level: String::new(),
                    total_notes: 0,
                    initial_bpm: 0.0,
                    min_bpm: 0.0,
                    max_bpm: 0.0,
                    length_ms: 0,
                    clear_type: String::new(),
                    ex_score: None,
                    replay_slots: [false; 4],
                    is_folder: true,
                },
                SelectItem::Chart(row) => SelectRowSnapshot {
                    index: index as u32,
                    title: row.chart.title.clone(),
                    artist: row.chart.artist.clone(),
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
                    replay_slots: row.replay_slots,
                    is_folder: false,
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

    let max_index = row_count - 1;
    match select_move {
        SelectMove::Previous => current_index.saturating_sub(1),
        SelectMove::Next => current_index.saturating_add(1).min(max_index),
        SelectMove::PagePrevious => current_index.saturating_sub(7),
        SelectMove::PageNext => current_index.saturating_add(7).min(max_index),
        SelectMove::First => 0,
        SelectMove::Last => max_index,
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
            ArrangeOption::Mirror,
            GaugeTypeConfig::Hard,
            AssistOption::Autoplay,
            42,
        );

        assert_eq!(profile.lane.hispeed, 3.25);
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
    fn select_snapshot_rows_clamps_near_edges() {
        let rows: Vec<SelectItem> =
            (0..4).map(|i| SelectItem::Chart(select_chart_row(i))).collect();

        let snapshot_rows = select_snapshot_rows(&rows, 99, 7);

        assert_eq!(snapshot_rows.len(), 4);
        assert_eq!(snapshot_rows[0].index, 0);
        assert_eq!(snapshot_rows[3].index, 3);
    }

    #[test]
    fn moved_select_index_moves_by_single_page_and_edges() {
        assert_eq!(moved_select_index(4, 10, SelectMove::Previous), 3);
        assert_eq!(moved_select_index(4, 10, SelectMove::Next), 5);
        assert_eq!(moved_select_index(8, 10, SelectMove::Next), 9);
        assert_eq!(moved_select_index(8, 10, SelectMove::PagePrevious), 1);
        assert_eq!(moved_select_index(4, 10, SelectMove::PagePrevious), 0);
        assert_eq!(moved_select_index(2, 10, SelectMove::PageNext), 9);
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
