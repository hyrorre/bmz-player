use super::*;
use bmz_gameplay::hit_error::HIT_ERROR_EMPTY;
use bmz_gameplay::judge::model::JudgeWindow;
use serde_json::json;

fn document(definition: serde_json::Value) -> SkinDocument {
    serde_json::from_value(json!({
        "type": 0, "w": 1920, "h": 1080,
        "hiterrorvisualizer": [definition],
        "destination": [{"id": "hev", "dst": [{"x": 126, "y": 180, "w": 472, "h": 12}]}]
    }))
    .unwrap()
}

fn state() -> SkinDrawState {
    SkinDrawState {
        note_judge_window: Some(JudgeWindow::symmetric(
            20_000, 40_000, 60_000, 100_000, 150_000, 150_000, 0,
        )),
        ..Default::default()
    }
}

fn sample(state: &mut SkinDrawState, delta_ms: i64) {
    state.hit_error_ring_index = (state.hit_error_ring_index + 1) % state.hit_error_ring.len();
    state.hit_error_ring[state.hit_error_ring_index] = delta_ms;
    state.elapsed_ms += 1;
}

fn rects(document: &SkinDocument, state: &SkinDrawState) -> Vec<(Rect, Color)> {
    document
        .static_image_render_items(&HashMap::new(), state)
        .into_iter()
        .map(|item| match item {
            SkinRenderItem::Rect { rect, color, .. } => (rect, color),
            other => panic!("unexpected hit-error item {other:?}"),
        })
        .collect()
}

#[test]
fn hit_error_default_lines_scale_inside_simple_play_destination() {
    let document = document(json!({"id": "hev"}));
    let mut state = state();
    sample(&mut state, -20);
    DynamicTimerRuntime::default().advance(&document, &mut state, 0);
    let items = rects(&document, &state);
    assert_eq!(items.len(), 3, "hit, centre and default EMA");
    let (hit, _) = items[0];
    let (center, _) = items[1];
    let (ema, _) = items[2];
    assert!(approx_eq(hit.x, (126.0 + 130.0 * 472.0 / 301.0) / 1920.0));
    assert!(approx_eq(hit.width, 472.0 / 301.0 / 1920.0));
    assert!(hit.x < ema.x && ema.x < center.x);
    for (rect, _) in items {
        assert!(rect.x >= 126.0 / 1920.0 && rect.x + rect.width <= 598.0 / 1920.0);
        assert!(approx_eq(rect.y, 888.0 / 1080.0));
        assert!(approx_eq(rect.height, 12.0 / 1080.0));
    }
}

#[test]
fn hit_error_ema_only_and_center_only_modes_remain_visible() {
    let mut document = document(json!({"id": "hev", "hiterrorMode": 0, "emaMode": 1}));
    let mut state = state();
    sample(&mut state, 40);
    DynamicTimerRuntime::default().advance(&document, &mut state, 0);
    let items = rects(&document, &state);
    assert_eq!(items.len(), 2, "ModernChic's EMA-only mode must not disappear");
    assert!(items[1].0.x > items[0].0.x);
    document.hiterror_visualizer[0].ema_mode = 0;
    assert_eq!(rects(&document, &state).len(), 1, "centre is independent of both modes");
}

#[test]
fn hit_error_history_wraps_decays_symmetrically_and_keeps_line_alpha() {
    let document = document(json!({"id": "hev", "windowLength": 4, "emaMode": 0,
        "lineColor": "12345680"}));
    let mut state = state();
    state.hit_error_ring_index = 0;
    state.hit_error_ring[0] = -20;
    state.hit_error_ring[97] = 20;
    let items = rects(&document, &state);
    assert_eq!(items.len(), 3);
    assert!(items[0].0.x < items[2].0.x && items[2].0.x < items[1].0.x);
    assert!(approx_eq(items[1].0.height, 3.0 / 1080.0));
    assert!(approx_eq(items[1].0.y, 892.5 / 1080.0));
    assert!(approx_eq(items[1].1.a, 128.0 / 255.0 / 2.0));
}

#[test]
fn hit_error_judge_colors_use_effective_windows_and_poor_transparency() {
    let mut document = document(json!({"id": "hev", "colorMode": 1, "emaMode": 0,
        "PGColor": "FF000080", "GRColor": "00FF0080", "GDColor": "0000FF80",
        "BDColor": "FFFF0080", "PRColor": "FF00FF80"}));
    for (delta, expected) in [
        (0, (1.0, 0.0, 0.0)),
        (20, (0.0, 1.0, 0.0)),
        (40, (0.0, 0.0, 1.0)),
        (60, (1.0, 1.0, 0.0)),
        (100, (1.0, 0.0, 1.0)),
    ] {
        let mut state = state();
        sample(&mut state, delta);
        let color = rects(&document, &state)[0].1;
        assert_eq!((color.r, color.g, color.b), expected, "delta={delta}");
        assert!(approx_eq(color.a, 128.0 / 255.0));
    }
    document.hiterror_visualizer[0].transparent = 1;
    let mut state = state();
    sample(&mut state, 100);
    assert_eq!(rects(&document, &state).len(), 1);
}

#[test]
fn hit_error_ema_updates_once_per_index_and_resets_per_scene_and_rewind() {
    let mut document = document(json!({"id": "hev", "alpha": 0.5}));
    let mut second = document.hiterror_visualizer[0].clone();
    second.id = "second".into();
    second.alpha = 0.25;
    document.hiterror_visualizer.push(second);
    let mut state = state();
    let mut runtime = DynamicTimerRuntime::default();
    sample(&mut state, 40);
    for _ in 0..3 {
        runtime.advance(&document, &mut state, 0);
        assert_eq!(state.hit_error_ema["hev"], 20);
        assert_eq!(state.hit_error_ema["second"], 10);
    }
    sample(&mut state, 40);
    runtime.advance(&document, &mut state, 0);
    assert_eq!(state.hit_error_ema["hev"], 30);
    sample(&mut state, 100);
    runtime.advance(&document, &mut state, 0);
    assert_eq!(state.hit_error_ema["hev"], 30, "ignore BAD boundary and misses");
    sample(&mut state, HIT_ERROR_EMPTY);
    runtime.advance(&document, &mut state, 0);
    assert_eq!(state.hit_error_ema["hev"], 30);
    state.hit_error_ring_index = 99;
    state.hit_error_ring[99] = -40;
    runtime.advance(&document, &mut state, 0);
    assert_eq!(state.hit_error_ema["hev"], -5);
    sample(&mut state, 40);
    runtime.advance(&document, &mut state, 0);
    assert_eq!(state.hit_error_ema["hev"], 17, "wrap from slot 99 to 0");
    runtime.reset_for_document(Some(&document));
    runtime.advance(&document, &mut state, 0);
    assert_eq!(state.hit_error_ema["hev"], 20);
    state.elapsed_ms = 0;
    state.hit_error_ring[0] = -40;
    runtime.advance(&document, &mut state, 0);
    assert_eq!(state.hit_error_ema["hev"], -20);
}

#[test]
fn hit_error_triangle_modes_tint_clip_and_mirror_only_their_destination() {
    let mut document = document(json!({"id": "hev", "hiterrorMode": 0, "emaMode": 2}));
    let mut state = state();
    state.hit_error_ema.insert("hev".into(), -150);
    let normal = rects(&document, &state);
    assert!(normal.len() > 2);
    for (rect, _) in &normal {
        assert!(
            rect.x >= 126.0 / 1920.0 - 0.000001 && rect.x + rect.width <= 598.0 / 1920.0 + 0.000001
        );
        assert!(
            rect.y >= 888.0 / 1080.0 - 0.000001
                && rect.y + rect.height <= 900.0 / 1080.0 + 0.000001
        );
    }
    let DestinationListEntry::Single(destination) = &mut document.destination[0] else { panic!() };
    let SkinDstEntry::Frame(frame) = &mut destination.dst[0] else { panic!() };
    frame.x = Some(598);
    frame.w = Some(-472);
    frame.y = Some(192);
    frame.h = Some(-12);
    frame.r = Some(128);
    frame.a = Some(128);
    let mirrored = rects(&document, &state);
    assert_eq!(normal.len(), mirrored.len());
    for ((before, color), (after, tinted)) in normal.iter().zip(&mirrored) {
        assert!(approx_eq(after.x, 724.0 / 1920.0 - before.x - before.width));
        assert!(approx_eq(after.y, 1788.0 / 1080.0 - before.y - before.height));
        assert!(approx_eq(tinted.r, color.r * 128.0 / 255.0));
        assert!(approx_eq(tinted.a, color.a * 128.0 / 255.0));
    }
    document.hiterror_visualizer[0].ema_mode = 3;
    assert_eq!(rects(&document, &state).len(), mirrored.len() + 1);
}

#[test]
fn hit_error_sanitizes_geometry_and_clips_extreme_samples() {
    let mut document = document(json!({"id": "hev", "width": 0, "judgeWidthMillis": 0,
        "windowLength": 0, "lineWidth": 9, "emaMode": 0, "drawDecay": 0}));
    let mut state = state();
    sample(&mut state, i64::MAX);
    for (rect, _) in rects(&document, &state) {
        assert!(rect.x.is_finite() && rect.width > 0.0);
        assert!(
            rect.x >= 126.0 / 1920.0 - 0.000001 && rect.x + rect.width <= 598.0 / 1920.0 + 0.000001
        );
    }
    let DestinationListEntry::Single(destination) = &mut document.destination[0] else { panic!() };
    let SkinDstEntry::Frame(frame) = &mut destination.dst[0] else { panic!() };
    frame.w = Some(0);
    assert!(rects(&document, &state).is_empty());
}

#[test]
fn hit_error_does_not_change_neighboring_images_values_judges_or_graphs() {
    let mut document: SkinDocument = serde_json::from_value(json!({
        "w": 1920, "h": 1080,
        "image": [{"id": "panel", "src": "src", "w": 100, "h": 100}],
        "value": [{"id": "combo", "src": "src", "w": 100, "h": 10, "divx": 10, "digit": 4, "ref": 75}],
        "judge": [{"id": "judge", "shift": false,
            "images": [{"id": "panel", "dst": [{"time": 0, "x": 500, "y": 300, "w": 180, "h": 50}, {"time": 500}]}],
            "numbers": [{"id": "combo", "dst": [{"time": 0, "x": 90, "y": -90, "w": 36, "h": 50}, {"time": 500}]}]}],
        "graph": [{"id": "graph", "src": "src", "w": 100, "h": 100, "type": 110}],
        "destination": [
            {"id": "panel", "dst": [{"x": 0, "y": 0, "w": 100, "h": 100}]},
            {"id": "combo", "dst": [{"x": 0, "y": 100, "w": 10, "h": 10}]},
            {"id": "judge"},
            {"id": "graph", "dst": [{"x": 1000, "y": 100, "w": 500, "h": 200}]}
        ]
    }))
    .unwrap();
    let mut state = state();
    state.judge_ms[0] = Some(100);
    state.judge_index[0] = Some(0);
    state.judge_combo[0] = 123;
    state.total_notes = 100;
    state.ex_score = 100;
    sample(&mut state, 20);
    let sources = mock_source("src", 100.0, 100.0);
    let baseline = document.static_image_render_items(&sources, &state);
    assert!(baseline.len() >= 6, "include the two-row judge, combo digits and score graph");
    document.hiterror_visualizer = self::document(json!({"id": "hev"})).hiterror_visualizer;
    let extra = self::document(json!({"id": "hev"})).destination;
    document.destination.extend(extra);
    DynamicTimerRuntime::default().advance(&document, &mut state, 0);
    let with_hit_error = document.static_image_render_items(&sources, &state);
    assert_eq!(&with_hit_error[..baseline.len()], &baseline);
    assert_eq!(with_hit_error.len(), baseline.len() + 3);
}
