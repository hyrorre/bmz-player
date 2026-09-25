#![cfg(target_os = "macos")]

use anyhow::{Context, Result, ensure};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::WindowId;

#[derive(Debug)]
enum TestEvent {
    InputDuringHandoff,
}

struct Harness {
    proxy: EventLoopProxy<TestEvent>,
    staged: bool,
    input_received: bool,
}

impl ApplicationHandler<TestEvent> for Harness {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::Init) && !self.staged {
            self.staged = true;
            bmz_player::update::sparkle::stage_test_install_handler();
            self.proxy
                .send_event(TestEvent::InputDuringHandoff)
                .expect("test event loop is active");
        }
    }

    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: TestEvent) {
        match event {
            TestEvent::InputDuringHandoff => self.input_received = true,
        }
        while let Some(event) = bmz_player::update::sparkle::poll() {
            if matches!(event, bmz_player::update::sparkle::Event::Shutdown) {
                assert!(self.input_received, "winit event was not delivered during handoff");
                assert!(bmz_player::update::sparkle::resume_install());
                assert!(!bmz_player::update::sparkle::resume_install());
                assert!(bmz_player::update::sparkle::test_install_handler_invoked());
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }
}

fn main() -> Result<()> {
    ensure!(
        bmz_player::update::sparkle::available() || cfg!(bmz_sparkle),
        "build with BMZ_SPARKLE_DIR"
    );
    let event_loop = EventLoop::<TestEvent>::with_user_event()
        .build()
        .context("failed to create winit event loop")?;
    let proxy = event_loop.create_proxy();
    event_loop
        .run_app(&mut Harness { proxy, staged: false, input_received: false })
        .context("winit handoff harness failed")?;
    println!("Sparkle/winit handoff regression test passed");
    Ok(())
}
