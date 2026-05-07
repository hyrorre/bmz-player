use std::env;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use bmz_render::renderer::{RenderSurfaceStatus, Renderer, SurfaceSize};
use bmz_render::sample::{sample_play_scene, sample_result_scene, sample_select_scene};
use bmz_render::scene::{AppSceneSnapshot, ResultSnapshot, SelectRowSnapshot, SelectSnapshot};
use bmz_render::snapshot::RenderSnapshot;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::bootstrap::{self, BootstrappedApp};
use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::play_loop::{PlayAdvanceOutcome, advance_running_play_session_until_result};
use crate::screens::play_start::{PlayStartOptions, StartedWinitPlaySession};
use crate::screens::result_model::ResultSummary;
use crate::screens::select_model::{SelectChartRow, load_select_chart_rows};

const SMOKE_EXIT_AFTER_FRAMES_ENV: &str = "BMZ_SMOKE_EXIT_AFTER_FRAMES";

pub fn run() -> Result<()> {
    let boot = bootstrap::bootstrap()?;
    let event_loop = EventLoop::new().context("failed to create event loop")?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = WinitApp::new(boot)?;
    tracing::info!("starting winit event loop");
    event_loop.run_app(&mut app).context("winit event loop failed")
}

struct WinitApp {
    boot: BootstrappedApp,
    window: Option<Arc<Window>>,
    active_play: Option<StartedWinitPlaySession>,
    finished_play: Option<FinishedPlaySession>,
    last_play_snapshot: Option<RenderSnapshot>,
    select_rows: Vec<SelectChartRow>,
    selected_index: usize,
    renderer: Renderer,
    dev_scene: Option<AppSceneSnapshot>,
    last_scene_kind: Option<AppSceneKind>,
    smoke_exit_after_frames: Option<u32>,
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
    fn new(boot: BootstrappedApp) -> Result<Self> {
        let select_rows = load_select_chart_rows(&boot.library_db, &boot.score_db, 100, 0)
            .context("failed to load initial select chart rows")?;

        Ok(Self {
            boot,
            window: None,
            active_play: None,
            finished_play: None,
            last_play_snapshot: None,
            select_rows,
            selected_index: 0,
            renderer: Renderer::default(),
            dev_scene: None,
            last_scene_kind: None,
            smoke_exit_after_frames: smoke_exit_after_frames_from_env(),
            rendered_frames: 0,
        })
    }

    fn ensure_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = WindowAttributes::default().with_title("bmz-player");
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
            }),
        }
    }

    fn select_snapshot(&self) -> SelectSnapshot {
        let selected = self.select_rows.get(self.selected_index);
        SelectSnapshot {
            chart_count: self.select_rows.len() as u32,
            selected_index: self.selected_index as u32,
            selected_chart_id: selected.map(|row| row.chart.chart_id),
            selected_title: selected.map(|row| row.chart.title.clone()).unwrap_or_default(),
            rows: select_snapshot_rows(&self.select_rows, self.selected_index, 7),
        }
    }

    fn route_keyboard_input(&mut self, event: &winit::event::KeyEvent) {
        if self.route_dev_scene_key(event.physical_key, event.state, event.repeat) {
            return;
        }

        if let Some(active_play) = &self.active_play {
            active_play.input.handle_key_event(event);
            return;
        }

        if self.finished_play.is_some() {
            if should_leave_result(event.physical_key, event.state, event.repeat) {
                self.finished_play = None;
                self.last_play_snapshot = None;
                self.refresh_select_rows();
            }
            return;
        }

        if let Some(action) = select_action(event.physical_key, event.state, event.repeat) {
            match action {
                SelectAction::Start => self.start_selected_chart(),
                SelectAction::MovePrevious => self.move_selection(-1),
                SelectAction::MoveNext => self.move_selection(1),
            }
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.select_rows.is_empty() {
            self.refresh_select_rows();
        }
        if self.select_rows.is_empty() {
            return;
        }

        let max_index = self.select_rows.len() - 1;
        self.selected_index = self.selected_index.saturating_add_signed(delta).clamp(0, max_index);
    }

    fn start_selected_chart(&mut self) {
        if self.select_rows.is_empty() {
            self.refresh_select_rows();
        }
        if self.selected_index >= self.select_rows.len() {
            self.selected_index = self.select_rows.len().saturating_sub(1);
        }

        let Some(row) = self.select_rows.get(self.selected_index) else {
            tracing::warn!("no chart is available to start");
            return;
        };
        self.start_chart(row.chart.chart_id);
    }

    fn start_chart(&mut self, chart_id: i64) {
        match self.boot.start_play_for_chart_with_winit_input(chart_id, PlayStartOptions::default())
        {
            Ok(active_play) => {
                self.active_play = Some(active_play);
                self.finished_play = None;
                self.last_play_snapshot = None;
            }
            Err(error) => {
                tracing::error!(chart_id, %error, "failed to start play");
            }
        }
    }

    fn refresh_select_rows(&mut self) {
        match load_select_chart_rows(&self.boot.library_db, &self.boot.score_db, 100, 0) {
            Ok(rows) => {
                self.select_rows = rows;
                if self.selected_index >= self.select_rows.len() {
                    self.selected_index = self.select_rows.len().saturating_sub(1);
                }
            }
            Err(error) => {
                tracing::error!(%error, "failed to refresh select chart rows");
            }
        }
    }

    fn advance_active_play(&mut self) {
        let Some(active_play) = &mut self.active_play else {
            return;
        };

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
                self.last_play_snapshot = Some(frame.render_snapshot);
                self.active_play = None;
                self.finished_play = Some(finished);
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
        let Some(exit_after_frames) = self.smoke_exit_after_frames else {
            return;
        };

        self.rendered_frames = self.rendered_frames.saturating_add(1);
        if self.rendered_frames >= exit_after_frames {
            tracing::info!(
                frames = self.rendered_frames,
                "smoke exit frame count reached; leaving event loop"
            );
            event_loop.exit();
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
            WindowEvent::CloseRequested => event_loop.exit(),
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

fn smoke_exit_after_frames_from_env() -> Option<u32> {
    let value = env::var(SMOKE_EXIT_AFTER_FRAMES_ENV).ok();
    let frames = parse_smoke_exit_after_frames(value.as_deref());
    if let Some(frames) = frames {
        tracing::info!(env = SMOKE_EXIT_AFTER_FRAMES_ENV, frames, "smoke auto-exit enabled");
    }
    frames
}

fn parse_smoke_exit_after_frames(value: Option<&str>) -> Option<u32> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }

    value.parse::<u32>().ok().map(|frames| frames.max(1))
}

fn select_snapshot_rows(
    rows: &[SelectChartRow],
    selected_index: usize,
    visible_limit: usize,
) -> Vec<SelectRowSnapshot> {
    if rows.is_empty() || visible_limit == 0 {
        return Vec::new();
    }

    let selected_index = selected_index.min(rows.len() - 1);
    let half_window = visible_limit / 2;
    let max_start = rows.len().saturating_sub(visible_limit);
    let start = selected_index.saturating_sub(half_window).min(max_start);
    let end = (start + visible_limit).min(rows.len());

    rows[start..end]
        .iter()
        .enumerate()
        .map(|(offset, row)| {
            let index = start + offset;
            SelectRowSnapshot {
                index: index as u32,
                title: row.chart.title.clone(),
                artist: row.chart.artist.clone(),
                play_level: row.chart.play_level.clone(),
                clear_type: row
                    .best_score
                    .as_ref()
                    .map(|score| score.clear_type.clone())
                    .unwrap_or_default(),
                ex_score: row.best_score.as_ref().map(|score| score.ex_score),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectAction {
    Start,
    MovePrevious,
    MoveNext,
}

fn select_action(
    physical_key: PhysicalKey,
    state: ElementState,
    repeat: bool,
) -> Option<SelectAction> {
    if state != ElementState::Pressed || repeat {
        return None;
    }

    match physical_key {
        PhysicalKey::Code(KeyCode::Enter | KeyCode::Space) => Some(SelectAction::Start),
        PhysicalKey::Code(KeyCode::ArrowUp) => Some(SelectAction::MovePrevious),
        PhysicalKey::Code(KeyCode::ArrowDown) => Some(SelectAction::MoveNext),
        _ => None,
    }
}

fn should_leave_result(physical_key: PhysicalKey, state: ElementState, repeat: bool) -> bool {
    state == ElementState::Pressed
        && !repeat
        && matches!(physical_key, PhysicalKey::Code(KeyCode::Enter | KeyCode::Escape))
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
        PhysicalKey::Code(KeyCode::F2) => Some(DevSceneAction::SamplePlay),
        PhysicalKey::Code(KeyCode::F3) => Some(DevSceneAction::SampleResult),
        PhysicalKey::Code(KeyCode::Escape) => Some(DevSceneAction::Clear),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::library_db::ChartListItem;
    use crate::storage::score_db::BestScoreSummary;

    use super::*;

    #[test]
    fn select_action_maps_start_and_vertical_movement() {
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::Enter), ElementState::Pressed, false),
            Some(SelectAction::Start)
        );
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::ArrowUp), ElementState::Pressed, false),
            Some(SelectAction::MovePrevious)
        );
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::ArrowDown), ElementState::Pressed, false),
            Some(SelectAction::MoveNext)
        );
    }

    #[test]
    fn select_action_rejects_releases_repeats_and_other_keys() {
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::ArrowDown), ElementState::Released, false),
            None
        );
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::ArrowDown), ElementState::Pressed, true),
            None
        );
        assert_eq!(
            select_action(PhysicalKey::Code(KeyCode::KeyZ), ElementState::Pressed, false),
            None
        );
    }

    #[test]
    fn result_leave_key_accepts_enter_and_escape_press() {
        assert!(should_leave_result(
            PhysicalKey::Code(KeyCode::Enter),
            ElementState::Pressed,
            false
        ));
        assert!(should_leave_result(
            PhysicalKey::Code(KeyCode::Escape),
            ElementState::Pressed,
            false
        ));
    }

    #[test]
    fn result_leave_key_rejects_releases_repeats_and_other_keys() {
        assert!(!should_leave_result(
            PhysicalKey::Code(KeyCode::Enter),
            ElementState::Released,
            false
        ));
        assert!(!should_leave_result(
            PhysicalKey::Code(KeyCode::Escape),
            ElementState::Pressed,
            true
        ));
        assert!(!should_leave_result(
            PhysicalKey::Code(KeyCode::Space),
            ElementState::Pressed,
            false
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
            Some(DevSceneAction::SamplePlay)
        );
        assert_eq!(
            dev_scene_action(PhysicalKey::Code(KeyCode::F3), ElementState::Pressed, false),
            Some(DevSceneAction::SampleResult)
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
    fn smoke_exit_after_frames_parses_positive_counts() {
        assert_eq!(parse_smoke_exit_after_frames(Some("3")), Some(3));
        assert_eq!(parse_smoke_exit_after_frames(Some(" 12 ")), Some(12));
    }

    #[test]
    fn smoke_exit_after_frames_ignores_empty_and_invalid_values() {
        assert_eq!(parse_smoke_exit_after_frames(None), None);
        assert_eq!(parse_smoke_exit_after_frames(Some("")), None);
        assert_eq!(parse_smoke_exit_after_frames(Some("abc")), None);
    }

    #[test]
    fn smoke_exit_after_frames_clamps_zero_to_one_redraw() {
        assert_eq!(parse_smoke_exit_after_frames(Some("0")), Some(1));
    }

    #[test]
    fn select_snapshot_rows_centers_selection_and_copies_score_summary() {
        let rows: Vec<SelectChartRow> = (0..10)
            .map(|index| {
                let mut row = select_chart_row(index);
                if index == 5 {
                    row.best_score = Some(best_score(1234));
                }
                row
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
        let rows: Vec<SelectChartRow> = (0..4).map(select_chart_row).collect();

        let snapshot_rows = select_snapshot_rows(&rows, 99, 7);

        assert_eq!(snapshot_rows.len(), 4);
        assert_eq!(snapshot_rows[0].index, 0);
        assert_eq!(snapshot_rows[3].index, 3);
    }

    fn select_chart_row(index: usize) -> SelectChartRow {
        SelectChartRow {
            chart: ChartListItem {
                chart_id: index as i64,
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
