use super::*;

#[test]
fn screenshot_uses_application_clipboard_and_keeps_png_when_copy_fails() {
    #[derive(Debug)]
    struct ApplicationClipboard;
    impl ScreenshotClipboard for ApplicationClipboard {
        fn copy_image(&self, width: u32, height: u32, rgba: &[u8], png: &[u8]) -> Result<()> {
            assert_eq!((width, height), (1, 1));
            let decoded = image::load_from_memory(png)?.into_rgba8();
            assert_eq!(decoded.into_raw(), rgba);
            Err(anyhow!("clipboard unavailable for test"))
        }
    }
    let unique =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let path =
        std::env::temp_dir().join(format!("bmz-screenshot-{}-{unique}.png", std::process::id()));
    let request = ScreenshotRequest {
        path: path.clone(),
        copy_to_clipboard: true,
        clipboard: Some(std::sync::Arc::new(ApplicationClipboard)),
    };
    let result = spawn_screenshot_save_job(request, 1, 1, vec![1, 2, 3, 255])
        .unwrap()
        .handle
        .join()
        .unwrap()
        .unwrap();
    assert_eq!(
        result.clipboard_result.unwrap().unwrap_err().to_string(),
        "clipboard unavailable for test"
    );
    let png = std::fs::read(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(image::load_from_memory(&png).unwrap().into_rgba8().into_raw(), [1, 2, 3, 255]);
}

#[test]
fn screenshot_png_is_encoded_once_for_file_and_clipboard_use() {
    let png = encode_screenshot_png(1, 1, &[0x12, 0x34, 0x56, 0x78]).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let decoded =
        image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap().into_rgba8();
    assert_eq!(decoded.into_raw(), [0x12, 0x34, 0x56, 0x78]);
}

#[test]
fn screenshot_dibv5_has_bottom_up_bgra_pixels() {
    let rgba = [
        1, 2, 3, 4, 5, 6, 7, 8, // top row
        9, 10, 11, 12, 13, 14, 15, 16, // bottom row
    ];
    let dib = screenshot_dibv5(2, 2, &rgba).unwrap();

    assert_eq!(dib.len(), 124 + rgba.len());
    assert_eq!(u32::from_le_bytes(dib[0..4].try_into().unwrap()), 124);
    assert_eq!(i32::from_le_bytes(dib[4..8].try_into().unwrap()), 2);
    assert_eq!(i32::from_le_bytes(dib[8..12].try_into().unwrap()), 2);
    assert_eq!(u16::from_le_bytes(dib[14..16].try_into().unwrap()), 32);
    assert_eq!(u32::from_le_bytes(dib[16..20].try_into().unwrap()), 3);
    assert_eq!(&dib[124..], &[11, 10, 9, 12, 15, 14, 13, 16, 3, 2, 1, 4, 7, 6, 5, 8]);
}

#[test]
fn screenshot_dibv5_rejects_mismatched_rgba_length() {
    let error = screenshot_dibv5(2, 2, &[0; 15]).unwrap_err();
    assert!(error.to_string().contains("expected 16, got 15"));
}

#[test]
fn text_align_offset_anchors_zero_width_text_like_beatoraja() {
    assert_eq!(text_align_offset_px(TextAlign::Left, 0.0, 80.0), 0.0);
    assert_eq!(text_align_offset_px(TextAlign::Center, 0.0, 80.0), -40.0);
    assert_eq!(text_align_offset_px(TextAlign::Right, 0.0, 80.0), -80.0);
}

#[test]
fn text_align_offset_uses_box_width_when_present() {
    assert_eq!(text_align_offset_px(TextAlign::Center, 120.0, 80.0), 20.0);
    assert_eq!(text_align_offset_px(TextAlign::Right, 120.0, 80.0), 40.0);
}

#[cfg(target_os = "linux")]
#[test]
fn auto_renderer_backend_prefers_vulkan_on_linux() {
    assert_eq!(auto_wgpu_backends(), wgpu::Backends::VULKAN);
    assert_eq!(fallback_wgpu_backends(WgpuBackend::Auto), &[WgpuBackend::Vulkan, WgpuBackend::Gl]);
}

#[cfg(target_os = "windows")]
#[test]
fn auto_renderer_backend_prefers_dx12_on_windows() {
    assert_eq!(auto_wgpu_backends(), wgpu::Backends::DX12);
    assert_eq!(
        fallback_wgpu_backends(WgpuBackend::Auto),
        &[WgpuBackend::Dx12, WgpuBackend::Vulkan, WgpuBackend::Gl]
    );
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
#[test]
fn auto_renderer_backend_keeps_default_candidates_on_other_platforms() {
    assert_eq!(auto_wgpu_backends(), wgpu::Backends::all());
    assert_eq!(fallback_wgpu_backends(WgpuBackend::Auto), &[WgpuBackend::Auto]);
}

#[test]
fn present_mode_fallbacks_follow_vsync_mode_semantics() {
    use wgpu::PresentMode::{Fifo, FifoRelaxed, Mailbox};

    assert_eq!(resolve_wgpu_present_mode(WgpuPresentMode::Fifo, &[Fifo]), Fifo);
    assert_eq!(resolve_wgpu_present_mode(WgpuPresentMode::FifoRelaxed, &[Fifo]), Fifo);
    assert_eq!(resolve_wgpu_present_mode(WgpuPresentMode::Immediate, &[Mailbox, Fifo]), Mailbox);
    assert_eq!(
        resolve_wgpu_present_mode(WgpuPresentMode::Immediate, &[FifoRelaxed, Fifo]),
        FifoRelaxed
    );
    assert_eq!(
        resolve_wgpu_present_mode(WgpuPresentMode::Mailbox, &[FifoRelaxed, Fifo]),
        FifoRelaxed
    );
    assert_eq!(resolve_wgpu_present_mode(WgpuPresentMode::Mailbox, &[Fifo]), Fifo);
}

#[test]
fn surface_settings_apply_frame_latency_mode_and_preserve_capture_usage() {
    let mut config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: 1,
        height: 1,
        desired_maximum_frame_latency: 2,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        view_formats: vec![],
    };

    configure_surface_settings(
        &mut config,
        WgpuPresentMode::Mailbox,
        WgpuFrameLatencyMode::Auto,
        &[wgpu::PresentMode::Fifo],
    );

    assert_eq!(config.desired_maximum_frame_latency, 1);
    assert_eq!(config.present_mode, wgpu::PresentMode::Fifo);
    assert!(config.usage.contains(wgpu::TextureUsages::COPY_SRC));

    configure_surface_settings(
        &mut config,
        WgpuPresentMode::Mailbox,
        WgpuFrameLatencyMode::Stable,
        &[wgpu::PresentMode::Mailbox, wgpu::PresentMode::Fifo],
    );
    assert_eq!(config.present_mode, wgpu::PresentMode::Mailbox);
    assert_eq!(config.desired_maximum_frame_latency, 2);
}

#[test]
fn automatic_frame_latency_is_stable_only_for_macos_immediate() {
    use wgpu::PresentMode::{Fifo, Immediate, Mailbox};

    assert_eq!(resolve_maximum_frame_latency(WgpuFrameLatencyMode::Auto, Immediate, true), 2);
    assert_eq!(resolve_maximum_frame_latency(WgpuFrameLatencyMode::Auto, Fifo, true), 1);
    assert_eq!(resolve_maximum_frame_latency(WgpuFrameLatencyMode::Auto, Mailbox, true), 1);
    assert_eq!(resolve_maximum_frame_latency(WgpuFrameLatencyMode::Auto, Immediate, false), 1);
    assert_eq!(resolve_maximum_frame_latency(WgpuFrameLatencyMode::Auto, Mailbox, false), 1);
}

#[test]
fn explicit_frame_latency_modes_ignore_platform_and_present_mode() {
    use wgpu::PresentMode::{Fifo, Immediate};

    assert_eq!(resolve_maximum_frame_latency(WgpuFrameLatencyMode::LowLatency, Immediate, true), 1);
    assert_eq!(resolve_maximum_frame_latency(WgpuFrameLatencyMode::Stable, Fifo, false), 2);
}

#[test]
fn screenshot_unpack_removes_row_padding() {
    let mapped = [1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 9, 10, 11, 12, 13, 14, 15, 16, 0, 0, 0, 0];

    let rgba = unpack_screenshot_rgba(&mapped, 2, 2, 12, wgpu::TextureFormat::Rgba8Unorm);

    assert_eq!(rgba, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
}

#[test]
fn screenshot_unpack_converts_bgra_to_rgba() {
    let mapped = [10, 20, 30, 40];

    let rgba = unpack_screenshot_rgba(&mapped, 1, 1, 4, wgpu::TextureFormat::Bgra8Unorm);

    assert_eq!(rgba, vec![30, 20, 10, 40]);
}

#[test]
fn renderer_records_last_scene() {
    let mut renderer = Renderer::default();
    let scene = AppSceneSnapshot::Select(SelectSnapshot {
        chart_count: 1,
        selected_index: 0,
        selected_chart_id: Some(7),
        selected_title: "test".to_string(),
        rows: Vec::new(),
        ..Default::default()
    });

    renderer.render_scene(scene.clone()).unwrap();

    assert_eq!(renderer.last_scene(), Some(&scene));
    assert!(renderer.last_plan().is_some());
}

#[test]
fn select_skin_context_update_does_not_reset_play_dynamic_timers() {
    use crate::skin::{
        SKIN_DYNAMIC_TIMER_BASE, SkinDocument, SkinDrawState, SkinDynamicTimerDef, SkinManifest,
    };

    let mut renderer = Renderer::default();
    let mut document: SkinDocument =
        serde_json::from_str(r#"{ "type": 0, "w": 100, "h": 100 }"#).unwrap();
    document.dynamic_timers.push(SkinDynamicTimerDef {
        id: SKIN_DYNAMIC_TIMER_BASE,
        observe: "number(0) >= 0".to_string(),
    });
    let manifest: SkinManifest = SkinManifest::default();
    let context = SkinContext::from_manifest_and_document(manifest, document.clone(), Vec::new());
    renderer.set_play_skin_context(context, false);

    let mut state = SkinDrawState::default();
    renderer.play_dynamic_timer_runtime.advance(&document, &mut state, 5_000);
    assert_eq!(state.dynamic_timer_ms[0], Some(0));

    renderer.play_dynamic_timer_runtime.advance(&document, &mut state, 8_000);
    assert_eq!(state.dynamic_timer_ms[0], Some(3_000));

    renderer.set_select_skin_context(SkinContext::default());

    renderer.play_dynamic_timer_runtime.advance(&document, &mut state, 9_000);
    assert_eq!(state.dynamic_timer_ms[0], Some(4_000));
}

#[test]
fn result_skin_fadeout_ms_reads_document_or_defaults_to_zero() {
    use crate::skin::{SkinContext, SkinDocument, SkinManifest};

    let mut renderer = Renderer::default();
    // ドキュメントスキン未設定なら 0 (フェードアウトなし)。
    assert_eq!(renderer.result_skin_fadeout_ms(), 0);

    let document: SkinDocument =
        serde_json::from_str(r#"{ "type": 7, "w": 100, "h": 100, "fadeout": 300 }"#).unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    renderer.set_result_skin_context(SkinContext::from_manifest_and_document(
        manifest,
        document,
        [],
    ));

    assert_eq!(renderer.result_skin_fadeout_ms(), 300);
}

#[test]
fn result_skin_timer_animation_duration_reads_document_or_defaults_to_zero() {
    use crate::skin::{SkinContext, SkinDocument, SkinManifest};

    let mut renderer = Renderer::default();
    assert_eq!(renderer.result_skin_timer_animation_duration_ms(2), 0);

    let document: SkinDocument = serde_json::from_str(
        r#"{
                "type": 7,
                "w": 100,
                "h": 100,
                "destination": [{
                    "id": "fadeout",
                    "timer": 2,
                    "dst": [{ "time": 0 }, { "time": 500 }]
                }]
            }"#,
    )
    .unwrap();
    let manifest: SkinManifest = SkinManifest::default();
    renderer.set_result_skin_context(SkinContext::from_manifest_and_document(
        manifest,
        document,
        [],
    ));

    assert_eq!(renderer.result_skin_timer_animation_duration_ms(2), 500);
}
