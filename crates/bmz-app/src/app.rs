use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::bootstrap::{self, BootstrappedApp};
use crate::screens::play_loop::{PlayAdvanceOutcome, advance_running_play_session_until_result};
use crate::screens::play_start::StartedWinitPlaySession;

pub fn run() -> Result<()> {
    let boot = bootstrap::bootstrap()?;
    let event_loop = EventLoop::new().context("failed to create event loop")?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = WinitApp::new(boot);
    event_loop.run_app(&mut app).context("winit event loop failed")
}

struct WinitApp {
    boot: BootstrappedApp,
    window: Option<Window>,
    active_play: Option<StartedWinitPlaySession>,
}

impl WinitApp {
    fn new(boot: BootstrappedApp) -> Self {
        Self { boot, window: None, active_play: None }
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

    fn route_keyboard_input(&mut self, event: &winit::event::KeyEvent) {
        let Some(active_play) = &self.active_play else {
            return;
        };

        active_play.input.handle_key_event(event);
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
            Ok(PlayAdvanceOutcome::Playing(_)) => {}
            Ok(PlayAdvanceOutcome::Finished { .. }) => {
                self.active_play = None;
            }
            Err(error) => {
                tracing::error!(%error, "failed to advance play session");
                self.active_play = None;
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
            WindowEvent::RedrawRequested => self.advance_active_play(),
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
