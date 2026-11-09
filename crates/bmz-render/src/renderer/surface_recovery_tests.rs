use super::*;

fn renderer() -> Renderer {
    let mut renderer = Renderer::default();
    if let Ok(backend) = std::env::var("BMZ_TEST_SURFACE_BACKEND") {
        renderer.set_backend(match backend.as_str() {
            "vulkan" => WgpuBackend::Vulkan,
            "dx12" => WgpuBackend::Dx12,
            _ => panic!("unsupported surface test backend: {backend}"),
        });
    }
    renderer.attach_offscreen(SurfaceSize { width: 64, height: 64 }).unwrap();
    renderer.last_plan = Some(DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands: vec![] });
    let gpu = renderer.gpu.as_mut().unwrap();
    gpu.surface_test = Some(SurfaceTest { failures_remaining: 1, ..Default::default() });
    gpu.surface_recovery = Some(SurfaceRecovery::new(&gpu.config));
    gpu.present_modes = vec![wgpu::PresentMode::Fifo, wgpu::PresentMode::Immediate];
    renderer
}

fn counters(renderer: &Renderer) -> (u32, u32) {
    let test = renderer.gpu.as_ref().unwrap().surface_test.as_ref().unwrap();
    (test.configure_calls, test.acquire_calls)
}

#[test]
#[ignore = "requires a GPU; deterministic configure failure in the production render path"]
fn failed_reconfigure_blocks_next_frame_acquire() {
    let mut renderer = renderer();
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Rendered);
    renderer.gpu.as_mut().unwrap().surface_test.as_mut().unwrap().acquire_calls = 0;
    // A smaller viewport still fits the headless test target.
    renderer.resize_surface(SurfaceSize { width: 32, height: 32 }).unwrap();
    assert_eq!(renderer.render_target_size(), [64, 64]);
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Recovering);
    assert_eq!(
        renderer.gpu.as_ref().unwrap().surface_test.as_ref().unwrap().acquire_calls,
        0,
        "an unconfigured surface must never reach the acquisition boundary"
    );
    // Returning to the old numeric settings cannot restore wgpu presentation.
    renderer.resize_surface(SurfaceSize { width: 64, height: 64 }).unwrap();
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Recovering);
    assert_eq!(counters(&renderer), (1, 0));
    renderer.resize_surface(SurfaceSize { width: 32, height: 32 }).unwrap();
    let deadline = renderer.surface_retry_deadline().unwrap();
    assert!(renderer.retry_surface_configuration(deadline));
    assert_eq!(renderer.render_target_size(), [32, 32]);
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Rendered);
    assert_eq!(renderer.gpu.as_ref().unwrap().surface_test.as_ref().unwrap().acquire_calls, 1);
}

#[test]
#[ignore = "requires a GPU; exercises real renderer setters and acquisition branches"]
fn all_reconfiguration_entries_share_recovery() {
    for reason in [
        SurfaceConfigureReason::Resize,
        SurfaceConfigureReason::Outdated,
        SurfaceConfigureReason::PresentMode,
        SurfaceConfigureReason::FrameLatency,
    ] {
        let mut renderer = renderer();
        match reason {
            SurfaceConfigureReason::Resize => {
                renderer.resize_surface(SurfaceSize { width: 32, height: 32 }).unwrap()
            }
            SurfaceConfigureReason::PresentMode => {
                renderer.set_present_mode(WgpuPresentMode::Immediate).unwrap()
            }
            SurfaceConfigureReason::FrameLatency => {
                renderer.set_frame_latency_mode(WgpuFrameLatencyMode::Stable).unwrap()
            }
            SurfaceConfigureReason::Outdated => {
                renderer.gpu.as_mut().unwrap().surface_test.as_mut().unwrap().acquire_result =
                    Some(wgpu::CurrentSurfaceTexture::Outdated);
                assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Recovering);
                assert_eq!(counters(&renderer).0, 0);
                renderer.update_surface_physical_size(SurfaceSize { width: 32, height: 32 });
                assert!(!renderer.retry_surface_configuration(Instant::now()));
            }
            SurfaceConfigureReason::Lost | SurfaceConfigureReason::DeviceLost => unreachable!(),
        }
        assert_eq!(counters(&renderer).0, 1, "{reason:?}");
        let acquire_before = counters(&renderer).1;
        assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Recovering);
        assert_eq!(counters(&renderer).1, acquire_before, "{reason:?}");
        assert!(renderer.retry_surface_configuration(renderer.surface_retry_deadline().unwrap()));
        assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Rendered);
    }
}

#[test]
#[ignore = "requires a GPU; tests physical-size refresh at the application/renderer boundary"]
fn outdated_retries_use_current_physical_size_and_pause_at_zero() {
    let mut renderer = renderer();
    renderer.gpu.as_mut().unwrap().surface_test.as_mut().unwrap().acquire_result =
        Some(wgpu::CurrentSurfaceTexture::Outdated);
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Recovering);
    assert_eq!(counters(&renderer), (0, 1));
    renderer.update_surface_physical_size(SurfaceSize { width: 48, height: 32 });
    assert!(!renderer.retry_surface_configuration(Instant::now()));
    assert_eq!(renderer.render_target_size(), [64, 64]);
    assert_eq!(renderer.gpu.as_ref().unwrap().surface_recovery.as_ref().unwrap().desired.width, 48);
    let deadline = renderer.surface_retry_deadline().unwrap();
    renderer.update_surface_physical_size(SurfaceSize { width: 0, height: 32 });
    assert!(!renderer.retry_surface_configuration(deadline));
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::SkippedZeroSize);
    assert_eq!(counters(&renderer), (1, 1));
    renderer.update_surface_physical_size(SurfaceSize { width: 24, height: 16 });
    renderer.set_frame_latency_mode(WgpuFrameLatencyMode::Stable).unwrap();
    assert!(renderer.retry_surface_configuration(deadline));
    assert_eq!(renderer.render_target_size(), [24, 16]);
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Rendered);
}

#[test]
#[ignore = "requires a GPU; tests SurfaceLost termination at the actual acquire branch"]
fn surface_lost_stops_without_reconfiguring_the_lost_surface() {
    let mut renderer = renderer();
    renderer.gpu.as_mut().unwrap().surface_test.as_mut().unwrap().acquire_result =
        Some(wgpu::CurrentSurfaceTexture::Lost);
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Recovering);
    for _ in 0..5 {
        renderer.resize_surface(SurfaceSize { width: 32, height: 32 }).unwrap();
        assert!(
            !renderer
                .retry_surface_configuration(Instant::now() + std::time::Duration::from_secs(60))
        );
        assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Recovering);
    }
    assert_eq!(counters(&renderer), (0, 1));
    assert!(renderer.take_surface_failure().unwrap().contains("new Surface is required"));
    assert!(renderer.take_surface_failure().is_none());
}

#[test]
#[ignore = "requires a GPU; callback notification during otherwise successful configure"]
fn device_lost_with_empty_error_scopes_never_commits_configuration_or_acquires() {
    let mut renderer = renderer();
    let test = renderer.gpu.as_mut().unwrap().surface_test.as_mut().unwrap();
    test.failures_remaining = 0;
    test.lose_device_during_configure = true;
    renderer.resize_surface(SurfaceSize { width: 32, height: 32 }).unwrap();
    assert_eq!(renderer.render_target_size(), [64, 64]);
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Recovering);
    assert!(
        !renderer.retry_surface_configuration(Instant::now() + std::time::Duration::from_secs(60))
    );
    assert_eq!(counters(&renderer), (1, 0));
    assert!(renderer.take_surface_failure().unwrap().contains("DeviceLost"));
    assert!(renderer.take_surface_failure().is_none());
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Recovering);
}

#[test]
#[ignore = "requires a GPU; callback notification outside configure"]
fn device_lost_between_frames_stops_acquisition() {
    let mut renderer = renderer();
    renderer.render_last_plan().unwrap();
    renderer
        .gpu
        .as_ref()
        .unwrap()
        .device_loss
        .notify(wgpu::DeviceLostReason::Unknown, "injected async device failure".into());
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Recovering);
    assert_eq!(counters(&renderer), (0, 1));
    assert!(renderer.take_surface_failure().unwrap().contains("injected async device failure"));
    assert!(renderer.take_surface_failure().is_none());
}

#[test]
#[ignore = "requires a GPU; validates applied renderer size and presentation status"]
fn pending_requests_keep_applied_settings_and_recover_latest_values() {
    let mut renderer = renderer();
    renderer.resize_surface(SurfaceSize { width: 48, height: 48 }).unwrap();
    let deadline = renderer.surface_retry_deadline().unwrap();
    for _ in 0..10 {
        renderer.resize_surface(SurfaceSize { width: 32, height: 24 }).unwrap();
        renderer.set_present_mode(WgpuPresentMode::Immediate).unwrap();
        renderer.set_frame_latency_mode(WgpuFrameLatencyMode::Stable).unwrap();
    }
    assert_eq!(counters(&renderer), (1, 0));
    assert_eq!(renderer.surface_retry_deadline(), Some(deadline));
    assert_eq!(renderer.render_target_size(), [64, 64]);
    let status = renderer.surface_presentation_status().unwrap();
    assert_eq!(status.requested_mode, WgpuPresentMode::Immediate);
    assert_eq!(status.effective_mode, "Fifo");
    assert_eq!(status.maximum_frame_latency, LOW_LATENCY_MAXIMUM_FRAME_LATENCY);
    assert!(renderer.retry_surface_configuration(deadline));
    assert_eq!(renderer.render_target_size(), [32, 24]);
    let status = renderer.surface_presentation_status().unwrap();
    assert_eq!(status.effective_mode, "Immediate");
    assert_eq!(status.maximum_frame_latency, STABLE_MAXIMUM_FRAME_LATENCY);
}

#[test]
#[ignore = "requires a GPU; validates zero-size handling in the render path"]
fn zero_size_preserves_budget_and_last_applied_configuration() {
    let mut renderer = renderer();
    renderer.resize_surface(SurfaceSize { width: 32, height: 32 }).unwrap();
    let deadline = renderer.surface_retry_deadline().unwrap();
    for size in [SurfaceSize { width: 0, height: 48 }, SurfaceSize { width: 48, height: 0 }] {
        renderer.resize_surface(size).unwrap();
        assert!(
            !renderer.retry_surface_configuration(deadline + std::time::Duration::from_secs(10))
        );
        assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::SkippedZeroSize);
        assert_eq!(renderer.render_target_size(), [64, 64]);
        assert_eq!(renderer.surface_retry_deadline(), None);
        assert_eq!(counters(&renderer), (1, 0));
    }
    renderer.resize_surface(SurfaceSize { width: 24, height: 32 }).unwrap();
    assert!(renderer.retry_surface_configuration(deadline));
    assert_eq!(renderer.render_target_size(), [24, 32]);
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Rendered);
}

#[test]
#[ignore = "requires a GPU; no real-time sleep, checks the terminal renderer API"]
fn retries_are_spaced_bounded_and_terminal_failure_is_reported_once() {
    let mut renderer = renderer();
    renderer.gpu.as_mut().unwrap().surface_test.as_mut().unwrap().failures_remaining = 100;
    renderer.resize_surface(SurfaceSize { width: 32, height: 32 }).unwrap();
    for attempt in 2..=5 {
        let deadline = renderer.surface_retry_deadline().unwrap();
        for _ in 0..10 {
            assert!(
                !renderer
                    .retry_surface_configuration(deadline - std::time::Duration::from_millis(1))
            );
            renderer.resize_surface(SurfaceSize { width: 32, height: 32 }).unwrap();
            assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Recovering);
        }
        assert_eq!(counters(&renderer), (attempt - 1, 0));
        assert!(!renderer.retry_surface_configuration(deadline));
    }
    assert_eq!(counters(&renderer), (5, 0));
    assert!(
        renderer.take_surface_failure().unwrap().contains("injected surface configure failure")
    );
    assert!(renderer.take_surface_failure().is_none());
    renderer.resize_surface(SurfaceSize { width: 24, height: 24 }).unwrap();
    assert!(
        !renderer.retry_surface_configuration(Instant::now() + std::time::Duration::from_secs(60))
    );
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Recovering);
    assert_eq!(counters(&renderer), (5, 0));
    assert!(renderer.take_surface_failure().is_none());
}

#[test]
#[ignore = "requires a GPU; checks actual egui texture contents across skipped frames"]
fn egui_full_partial_and_free_survive_suspended_frames() {
    let mut renderer = renderer();
    renderer.resize_surface(SurfaceSize { width: 32, height: 32 }).unwrap();
    let id = egui::TextureId::Managed(7);
    let full = egui::epaint::ImageDelta::full(
        egui::ColorImage::filled([2, 2], egui::Color32::RED),
        egui::TextureOptions::NEAREST,
    );
    let partial = egui::epaint::ImageDelta::partial(
        [1, 0],
        egui::ColorImage::filled([1, 1], egui::Color32::GREEN),
        egui::TextureOptions::NEAREST,
    );
    for delta in [full, partial] {
        renderer.set_egui_frame(EguiFrame {
            primitives: vec![],
            pixels_per_point: 1.0,
            textures_delta: egui::TexturesDelta { set: vec![(id, delta)], free: vec![] },
        });
        assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Recovering);
    }
    assert_eq!(counters(&renderer), (1, 0));
    assert!(renderer.retry_surface_configuration(renderer.surface_retry_deadline().unwrap()));
    // Render the updated texture to the existing readable offscreen target.
    let rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(32.0, 32.0));
    let mut mesh = egui::Mesh::with_texture(id);
    mesh.add_rect_with_uv(
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    renderer.set_egui_frame(EguiFrame {
        primitives: vec![egui::ClippedPrimitive {
            clip_rect: rect,
            primitive: egui::epaint::Primitive::Mesh(mesh),
        }],
        pixels_per_point: 1.0,
        textures_delta: egui::TexturesDelta::default(),
    });
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Rendered);
    let pixels = renderer.read_offscreen_rgba().unwrap();
    let pixel = |x: usize, y: usize| &pixels[(y * 32 + x) * 4..(y * 32 + x + 1) * 4];
    assert_eq!(pixel(4, 4), &[255, 0, 0, 255]);
    assert_eq!(pixel(24, 4), &[0, 255, 0, 255]);
    assert_eq!(pixel(24, 24), &[255, 0, 0, 255]);
    renderer.resize_surface(SurfaceSize { width: 0, height: 0 }).unwrap();
    renderer.set_egui_frame(EguiFrame {
        primitives: vec![],
        pixels_per_point: 1.0,
        textures_delta: egui::TexturesDelta { set: vec![], free: vec![id] },
    });
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::SkippedZeroSize);
    assert!(renderer.gpu.as_ref().unwrap().egui.texture_for_test(id).is_none());
    assert_eq!(counters(&renderer), (2, 1));
    renderer.resize_surface(SurfaceSize { width: 32, height: 32 }).unwrap();
    assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Rendered);
}
