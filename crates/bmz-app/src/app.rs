use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use bmz_render::scene::{AppSceneSnapshot, ResultSnapshot, SelectSnapshot};
use bmz_render::snapshot::RenderSnapshot;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::bootstrap::{self, BootstrappedApp};
use crate::screens::play_finish::FinishedPlaySession;
use crate::screens::play_loop::{PlayAdvanceOutcome, advance_running_play_session_until_result};
use crate::screens::play_start::{PlayStartOptions, StartedWinitPlaySession};
use crate::screens::result_model::ResultSummary;
use crate::screens::select_model::{SelectChartRow, load_select_chart_rows};

pub fn run() -> Result<()> {
    let boot = bootstrap::bootstrap()?;
    let event_loop = EventLoop::new().context("failed to create event loop")?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = WinitApp::new(boot)?;
    event_loop.run_app(&mut app).context("winit event loop failed")
}

struct WinitApp {
    boot: BootstrappedApp,
    window: Option<Window>,
    active_play: Option<StartedWinitPlaySession>,
    finished_play: Option<FinishedPlaySession>,
    last_play_snapshot: Option<RenderSnapshot>,
    select_rows: Vec<SelectChartRow>,
}

#[derive(Debug, Clone, PartialEq)]
enum AppViewState {
    Select,
    Play,
    Result(ResultSummary),
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
        })
    }

    fn ensure_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = WindowAttributes::default().with_title("bmz-player");
        match event_loop.create_window(attributes) {
            Ok(window) => {
                self.window = Some(window);
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
        let selected = self.select_rows.first();
        SelectSnapshot {
            chart_count: self.select_rows.len() as u32,
            selected_index: 0,
            selected_chart_id: selected.map(|row| row.chart.chart_id),
            selected_title: selected.map(|row| row.chart.title.clone()).unwrap_or_default(),
        }
    }

    fn route_keyboard_input(&mut self, event: &winit::event::KeyEvent) {
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

        if should_start_play_from_select(event.physical_key, event.state, event.repeat) {
            self.start_first_select_chart();
        }
    }

    fn start_first_select_chart(&mut self) {
        if self.select_rows.is_empty() {
            self.refresh_select_rows();
        }

        let Some(row) = self.select_rows.first() else {
            tracing::warn!("no chart is available to start");
            return;
        };
        let chart_id = row.chart.chart_id;

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
}

impl ApplicationHandler for WinitApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.ensure_window(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.window.as_ref().map(Window::id) != Some(window_id) {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. } => self.route_keyboard_input(&event),
            WindowEvent::RedrawRequested => {
                let _scene = self.scene_snapshot();
                self.advance_active_play();
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

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn should_start_play_from_select(
    physical_key: PhysicalKey,
    state: ElementState,
    repeat: bool,
) -> bool {
    state == ElementState::Pressed
        && !repeat
        && matches!(physical_key, PhysicalKey::Code(KeyCode::Enter | KeyCode::Space))
}

fn should_leave_result(physical_key: PhysicalKey, state: ElementState, repeat: bool) -> bool {
    state == ElementState::Pressed
        && !repeat
        && matches!(physical_key, PhysicalKey::Code(KeyCode::Enter | KeyCode::Escape))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_start_key_accepts_enter_and_space_press() {
        assert!(should_start_play_from_select(
            PhysicalKey::Code(KeyCode::Enter),
            ElementState::Pressed,
            false
        ));
        assert!(should_start_play_from_select(
            PhysicalKey::Code(KeyCode::Space),
            ElementState::Pressed,
            false
        ));
    }

    #[test]
    fn select_start_key_rejects_releases_repeats_and_other_keys() {
        assert!(!should_start_play_from_select(
            PhysicalKey::Code(KeyCode::Enter),
            ElementState::Released,
            false
        ));
        assert!(!should_start_play_from_select(
            PhysicalKey::Code(KeyCode::Enter),
            ElementState::Pressed,
            true
        ));
        assert!(!should_start_play_from_select(
            PhysicalKey::Code(KeyCode::KeyZ),
            ElementState::Pressed,
            false
        ));
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
}
