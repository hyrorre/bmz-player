use super::*;
use crate::snapshot::OverlaySnapshot;

#[test]
fn select_plan_appends_three_output_overlays_after_scene_text() {
    let plan = select_overlay_plan();
    let (scene, overlay) = plan.commands.split_at(plan.commands.len() - 3);
    assert!(scene.iter().any(
        |command| matches!(command, DrawCommand::Text { style, .. } if style.layer == TextLayer::Ui)
    ));
    for (command, (expected_text, expected_x, expected_y, expected_align)) in overlay.iter().zip([
        ("Screenshot saved", 0.015, 0.015, TextAlign::Left),
        ("FPS 144", -0.015, 0.015, TextAlign::Right),
        ("bmz-player", -0.015, 0.975, TextAlign::Right),
    ]) {
        let DrawCommand::Text { origin, text, style, .. } = command else {
            panic!("expected overlay text");
        };
        assert_eq!(text, expected_text);
        assert_eq!(*origin, Point { x: expected_x, y: expected_y });
        assert_eq!(style.align, expected_align);
        assert_eq!(style.layer, TextLayer::Overlay);
    }
}

fn select_overlay_plan() -> DrawPlan {
    DrawPlan::from_scene(&AppSceneSnapshot::Select(SelectSnapshot {
        overlay: OverlaySnapshot {
            left_text: "Screenshot saved".into(),
            fps_text: "FPS 144".into(),
            text: "bmz-player".into(),
        },
        ..Default::default()
    }))
}

#[test]
#[ignore = "requires a GPU; run explicitly for output overlay changes"]
fn output_overlay_gpu_stays_at_surface_edges_across_aspect_ratios() {
    for (width, height, skin_width, skin_height) in [
        (640, 480, 336, 144), // 4:3 output, 21:9 skin
        (640, 400, 320, 180), // 16:10 output, 16:9 skin
        (640, 360, 240, 180), // 16:9 output, 4:3 skin
        (840, 360, 320, 200), // 21:9 output, 16:10 skin
    ] {
        let surface = SurfaceSize { width, height };
        let mut renderer = Renderer::default();
        renderer.attach_offscreen(surface).unwrap();
        eprintln!("overlay GPU adapter: {:?}", renderer.gpu.as_ref().unwrap().adapter_info);
        let mut plan = select_overlay_plan();
        plan.commands.drain(..plan.commands.len() - 3);
        plan.clear = Color::rgb(0.0, 0.0, 0.0);
        renderer.last_plan = Some(plan.clone());
        assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Rendered);
        let reference = renderer.read_offscreen_rgba().unwrap();
        let red_pixels =
            reference.as_chunks::<4>().0.iter().enumerate().filter(|(_, pixel)| pixel[0] > 0);
        let (mut top_left, mut top_right, mut bottom_right) = (false, false, false);
        for (index, _) in red_pixels {
            let x = index as u32 % width;
            let y = index as u32 / width;
            top_left |= x < width / 8 && y < height / 20;
            top_right |= x > width * 7 / 8 && y < height / 20;
            bottom_right |= x > width * 7 / 8 && y > height * 19 / 20;
        }
        assert!(top_left && top_right && bottom_right, "{width}x{height}");

        // The blue canvas leaves the red channel available to compare the text
        // pixel for pixel, including text that lands on the canvas.
        let full = Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 };
        let blue = DrawCommand::Rect { rect: full, color: Color::rgb(0.0, 0.0, 1.0) };
        let clip =
            DrawCommand::PushClip { rect: Rect { x: 0.25, y: 0.25, width: 0.5, height: 0.5 } };
        plan.commands.splice(..0, [blue, clip, sample_text()]);
        let policy = CanvasRenderPolicy {
            fit_mode: CanvasFitMode::Contain,
            canvas_size: Some(CanvasSize { width: skin_width, height: skin_height }),
        };
        assert!(policy.internal_render_size(surface, InternalResolutionMode::Skin).is_some());
        renderer.last_plan_canvas_policy = policy;
        renderer.last_plan = Some(plan);
        for mode in [InternalResolutionMode::Native, InternalResolutionMode::Skin] {
            renderer.set_internal_resolution_mode(mode);
            assert_eq!(renderer.render_last_plan().unwrap(), RenderSurfaceStatus::Rendered);
            let actual = renderer.read_offscreen_rgba().unwrap();
            assert!(
                actual.iter().step_by(4).eq(reference.iter().step_by(4)),
                "overlay pixels changed at {width}x{height}, {mode:?}"
            );
            let blue_at = |x, y| actual[((y * width + x) * 4 + 2) as usize];
            assert_eq!(blue_at(width / 2, height / 2), 255);
            let viewport = CanvasViewport::from_policy(surface, policy);
            if viewport.rect.x > 0.0 {
                assert_eq!(blue_at(0, height / 2), 0);
            } else {
                assert_eq!(blue_at(width / 2, 0), 0);
            }
        }
    }
}
