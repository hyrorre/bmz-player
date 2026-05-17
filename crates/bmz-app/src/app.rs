use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use bmz_chart::model::PlayableChart;
use bmz_render::assets::load_static_rgba_image;
use bmz_render::plan::TextureId;
use bmz_render::renderer::{RenderSurfaceStatus, Renderer, SurfaceSize};
use bmz_render::sample::{sample_play_scene, sample_result_scene, sample_select_scene};
use bmz_render::scene::{AppSceneSnapshot, ResultSnapshot, SelectRowSnapshot, SelectSnapshot};
use bmz_render::snapshot::{DisplayJudgeCounts, RenderSnapshot};
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
    AssistOptionConfig, GaugeTypeConfig, LaneConfig, ProfileConfig, ProfileInputConfig,
    RandomOptionConfig,
};
use crate::config::save::save_profile_config;
use crate::input::winit::physical_key_to_control;
use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::play_loop::{PlayAdvanceOutcome, advance_running_play_session_until_result};
use crate::screens::play_snapshot::{BgaFrameCatalog, bga_texture_id, display_bga_frame};
use crate::screens::play_start::{PlayStartOptions, StartedWinitPlaySession};
use crate::screens::result_model::ResultSummary;
use crate::screens::select_model::{SelectItem, load_select_items_in_folder, root_folder_items};
use crate::select_options::{ArrangeOption, AssistOption};
use crate::skin_loader::{apply_beatoraja_select_json_skin, apply_skin_from_config};
use crate::storage::scan::scan_song_roots;

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
}

#[derive(Debug, Clone, PartialEq)]
enum AppViewState {
    Select,
    Play,
    Result(ResultSummary),
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

        let mut renderer = Renderer::default();
        load_skin_textures(
            &mut renderer,
            &boot.profile_config.skin.select,
            &boot.profile_config.skin.play,
        );

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
        };
        if let Some(chart_id) = boot_sample_chart_id {
            tracing::info!(
                arg = BOOT_PLAY_SAMPLE_ARG,
                chart_id,
                "booting directly into bundled sample chart"
            );
            app.start_chart(chart_id);
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
            return AppViewState::Result(finished.summary.clone());
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
                total_notes: summary.total_notes,
                judge_counts: DisplayJudgeCounts {
                    pgreat: summary.judge_counts.pgreat,
                    great: summary.judge_counts.great,
                    good: summary.judge_counts.good,
                    bad: summary.judge_counts.bad,
                    poor: summary.judge_counts.poor,
                    empty_poor: summary.judge_counts.empty_poor,
                },
                score_history_id: summary.score_history_id,
                replay_saved: !summary.replay_path.is_empty(),
            }),
        }
    }

    fn select_snapshot(&self) -> SelectSnapshot {
        let selected = self.select_items.get(self.selected_index);
        let current_folder = self
            .folder_stack
            .last()
            .and_then(|p| std::path::Path::new(p).file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        SelectSnapshot {
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
            current_folder,
            key_hint: self.select_keys.key_hint.clone(),
            option_hint: self.select_keys.option_hint.clone(),
        }
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
            self.start_held = event.state == ElementState::Pressed;
            return;
        }

        if self.start_held {
            if event.state == ElementState::Pressed
                && !event.repeat
                && let Some(control) = physical_key_name(event.physical_key)
            {
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
                    tracing::info!(assist = self.assist_option.as_str(), "assist option changed");
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

    fn move_selection(&mut self, select_move: SelectMove) {
        if self.select_items.is_empty() {
            self.reload_select_items();
        }
        if self.select_items.is_empty() {
            return;
        }
        self.selected_index =
            moved_select_index(self.selected_index, self.select_items.len(), select_move);
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
            tracing::info!(depth = self.folder_stack.len(), "exited folder");
        }
    }

    fn start_chart(&mut self, chart_id: i64) {
        match self.boot.start_play_for_chart_with_winit_input(chart_id, self.play_start_options()) {
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
        PlayStartOptions {
            autoplay: self.assist_option == AssistOption::Autoplay,
            gauge: Some(self.gauge_option),
            arrange: self.arrange_option,
            ..Default::default()
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

fn load_skin_textures(renderer: &mut Renderer, select_skin_path: &str, play_skin_path: &str) {
    if !select_skin_path.trim().is_empty()
        && let Err(error) =
            apply_beatoraja_select_json_skin(renderer, Path::new(select_skin_path.trim()))
    {
        tracing::warn!(
            error = %format_error_chain(&error),
            "failed to apply select skin; using fallback select drawing"
        );
    }

    if let Err(error) = apply_skin_from_config(renderer, play_skin_path) {
        tracing::warn!(
            error = %format_error_chain(&error),
            "failed to apply play skin; using fallback textures"
        );
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
                self.render_current_scene();
                self.advance_active_play();
                self.handle_smoke_exit_after_redraw(event_loop);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
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
        Some(folder) => {
            match load_select_items_in_folder(&boot.library_db, &boot.score_db, folder) {
                Ok(items) => items,
                Err(error) => {
                    tracing::error!(%error, "failed to load select items");
                    Vec::new()
                }
            }
        }
        None => root_folder_items(&enabled_root_paths(&boot.app_config)),
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
                    clear_type: String::new(),
                    ex_score: None,
                    is_folder: true,
                },
                SelectItem::Chart(row) => SelectRowSnapshot {
                    index: index as u32,
                    title: row.chart.title.clone(),
                    artist: row.chart.artist.clone(),
                    play_level: row.chart.play_level.clone(),
                    table_level: row.table_level.clone(),
                    clear_type: row
                        .best_score
                        .as_ref()
                        .map(|score| score.clear_type.clone())
                        .unwrap_or_default(),
                    ex_score: row.best_score.as_ref().map(|score| score.ex_score),
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

        let keys_for = |lane: LaneConfig| -> Vec<String> {
            kb.iter().filter(|e| e.lane == lane).map(|e| e.control.clone()).collect()
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

        let enter_str =
            if enter.is_empty() { String::new() } else { format!("/{}", enter.join("/")) };
        let back_str = back.first().map(|k| format!("/{k}")).unwrap_or_default();
        let key_hint =
            format!("UP DOWN  RIGHT{enter_str}:ENTER  LEFT{back_str}:BACK  ENTER {start}");

        let arrange_str = cycle_arrange.as_deref().unwrap_or("?");
        let gauge_str = cycle_gauge.as_deref().unwrap_or("?");
        let assist_str = cycle_assist.as_deref().unwrap_or("?");
        let option_hint = format!(
            "F1 SELECT  F2 RELOAD  F3 RESULT  F4 PLAY   \
             {start}+{arrange_str}:ARRANGE  {start}+{gauge_str}:GAUGE  {start}+{assist_str}:ASSIST"
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
                    row.best_score = Some(best_score(1234));
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
                folder_path: String::new(),
            },
            best_score: None,
            table_level: String::new(),
        }
    }

    fn best_score(ex_score: u32) -> BestScoreSummary {
        BestScoreSummary {
            chart_sha256: [0; 32],
            clear_type: "Normal".to_string(),
            gauge_type: "Normal".to_string(),
            gauge_value: 80.0,
            ex_score,
            max_combo: 100,
            played_at: 1,
            replay_path: String::new(),
        }
    }
}
