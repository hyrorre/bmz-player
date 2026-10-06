use super::*;
use crate::snapshot::RenderSnapshot;
use serde_json::json;

fn clip_frame() -> serde_json::Value {
    json!({"time":0,"x":0,"y":0,"w":80,"h":60,"clip_x":10,"clip_y":20,"clip_w":30,"clip_h":40})
}

fn clipped_contents(items: &[SkinRenderItem]) -> &[SkinRenderItem] {
    let [SkinRenderItem::PushClip { rect }, inner @ .., SkinRenderItem::PopClip] = items else {
        panic!("one whole-object clip expected: {items:?}");
    };
    assert!(approx_eq(rect.x, 0.1));
    assert!(approx_eq(rect.y, 0.4));
    assert!(approx_eq(rect.width, 0.3));
    assert!(approx_eq(rect.height, 0.4));
    assert!(!inner.is_empty());
    assert!(
        !inner
            .iter()
            .any(|item| matches!(item, SkinRenderItem::PushClip { .. } | SkinRenderItem::PopClip))
    );
    inner
}

#[test]
fn destination_clip_wraps_complete_objects_without_changing_geometry() {
    for id in ["image", "rotated", "panel", "number", "text", "graph", "set", "-111"] {
        let mut frame = clip_frame();
        if id == "rotated" {
            frame["angle"] = json!(30);
        }
        let value = json!({
            "w":100,"h":100,
            "image":[{"id":"image","src":"src","w":100,"h":10},
                     {"id":"rotated","src":"src","w":100,"h":10}],
            "panel":[{"id":"panel","color":"#123456","borderWidth":2}],
            "value":[{"id":"number","src":"src","w":100,"h":10,"divx":10,"digit":3,"ref":100}],
            "text":[{"id":"text","constantText":"clipped text","size":10}],
            "graph":[{"id":"graph","src":"src","w":100,"h":10,"type":110}],
            "imageset":[{"id":"set","ref":40,"images":["image","image","image","image","image","image","image","image","image"]}],
            "destination":[{"id":id,"dst":[frame]}]
        });
        let document: SkinDocument = serde_json::from_value(value.clone()).unwrap();
        let mut unclipped_value = value;
        for field in ["clip_x", "clip_y", "clip_w", "clip_h"] {
            unclipped_value["destination"][0]["dst"][0].as_object_mut().unwrap().remove(field);
        }
        let unclipped: SkinDocument = serde_json::from_value(unclipped_value).unwrap();
        let state = SkinDrawState { ex_score: 123, total_notes: 100, ..Default::default() };
        let sources = mock_source("src", 100.0, 10.0);
        let actual = document.static_render_items(&sources, &state, &SkinTextState::default());
        let expected = unclipped.static_render_items(&sources, &state, &SkinTextState::default());
        assert!(!expected.is_empty(), "fixture for {id} must render");
        assert_eq!(clipped_contents(&actual), expected, "{id}");
    }
}

#[test]
fn destination_clip_surrounds_ambient_without_clipping_its_source_layers() {
    let document: SkinDocument = serde_json::from_value(json!({
        "w":100,"h":100,"bga":{"id":"bga"},
        "destination":[{"id":"bga","ambient":true,"dst":[clip_frame()]}]
    }))
    .unwrap();
    let source =
        SkinBgaFrame::opaque(SkinTextureId(42), SkinImageSize { width: 100.0, height: 100.0 });
    let state = SkinDrawState {
        has_bga: true,
        bga_base: Some(source),
        bga_layer: Some(source),
        ..Default::default()
    };
    let items = document.static_render_items(&HashMap::new(), &state, &SkinTextState::default());
    let [SkinRenderItem::Ambient { layers, .. }] = clipped_contents(&items) else {
        panic!("ambient")
    };
    assert_eq!(layers.len(), 2);
    assert!(layers.iter().all(|item| matches!(item, SkinRenderItem::Image { .. })));
}

#[derive(Debug, Default)]
struct ClipLuaRuntime(std::sync::Mutex<Vec<usize>>);

impl SkinLuaDrawRuntime for ClipLuaRuntime {
    fn evaluate_draw(
        &self,
        id: usize,
        _: &SkinDrawState,
        _: &[i32],
        text: &std::collections::BTreeMap<i32, String>,
    ) -> bool {
        self.0.lock().unwrap().push(id);
        if id == 7 {
            assert_eq!(text.get(&10).map(String::as_str), Some("Test"));
        }
        true
    }

    fn evaluate_number(
        &self,
        id: usize,
        _: &SkinDrawState,
        _: &[i32],
        _: &std::collections::BTreeMap<i32, String>,
    ) -> Option<f64> {
        self.0.lock().unwrap().push(id);
        Some(123.0)
    }
}

#[test]
fn destination_clip_keeps_lua_callback_count_on_number_cache_hits() {
    let document: SkinDocument = serde_json::from_value(json!({
        "w":100,"h":100,
        "value":[{"id":"number","src":"src","w":100,"h":10,"divx":10,"digit":3,
                  "value_expr":"bmz:lua_value_callback:2"}],
        "destination":[{"id":"number","draw":"bmz:lua_draw_callback:1","dst":[clip_frame()]}]
    }))
    .unwrap();
    let mut context = SkinContext::from_manifest_and_document(
        default_skin_manifest(),
        document,
        mock_source("src", 100.0, 10.0).into_values(),
    );
    let runtime = Arc::new(ClipLuaRuntime::default());
    context.set_lua_draw_runtime(Some(runtime.clone()));
    for scene in ["play", "result", "select"] {
        for _ in 0..2 {
            context.begin_frame();
            let state = SkinDrawState::default();
            let items = match scene {
                "play" => context.static_document_items_for_state(&state),
                "result" => context.static_document_items_for_result_state_and_text(
                    &Arc::default(),
                    &state,
                    &SkinTextState::default(),
                ),
                _ => context.select_document_items(&SelectSnapshot::default()),
            };
            assert_eq!(clipped_contents(&items).len(), 3);
            assert_eq!(runtime.0.lock().unwrap().as_slice(), &[2, 1], "{scene}");
            runtime.0.lock().unwrap().clear();
        }
    }
}

#[test]
fn destination_clip_keeps_songlist_and_judge_children_under_only_the_outer_clip() {
    let document: SkinDocument = serde_json::from_value(json!({
        "w":100,"h":100,
        "image":[{"id":"row-image","src":"src","w":100,"h":10}],
        "imageset":[{"id":"row","images":["row-image"]}],
        "text":[{"id":"label","constantText":"row","size":10}],
        "songlist":{"id":"list","center":0,
            "liston":[{"id":"row","dst":[clip_frame()]}],
            "text":[{"id":"label","dst":[clip_frame()]}]},
        "destination":[{"id":"list","dst":[clip_frame()]}]
    }))
    .unwrap();
    let snapshot = SelectSnapshot {
        rows: vec![SelectRowSnapshot {
            index: 0,
            kind: SelectRowKind::Song,
            in_library: true,
            ..Default::default()
        }],
        ..Default::default()
    };
    let sources = mock_source("src", 100.0, 10.0);
    let items = document.select_render_items(&sources, &snapshot);
    assert!(matches!(
        clipped_contents(&items),
        [SkinRenderItem::Image { .. }, SkinRenderItem::Text { .. }]
    ));

    let document: SkinDocument = serde_json::from_value(json!({
        "w":100,"h":100,
        "image":[{"id":"judge-image","src":"src","w":100,"h":10}],
        "value":[{"id":"combo","src":"src","w":100,"h":10,"divx":10,"digit":3}],
        "judge":[{"id":"judge","images":[{"id":"judge-image","dst":[clip_frame()]}],
                  "numbers":[{"id":"combo","dst":[clip_frame()]}]}],
        "destination":[{"id":"judge","dst":[clip_frame()]}]
    }))
    .unwrap();
    let mut state = SkinDrawState::default();
    state.judge_ms[0] = Some(0);
    state.judge_index[0] = Some(0);
    state.judge_combo[0] = 123;
    let items = document.static_render_items(&sources, &state, &SkinTextState::default());
    assert!(clipped_contents(&items).len() >= 2);
}

#[test]
fn destination_clip_applies_to_search_placeholder_but_not_the_input_overlay() {
    let document: SkinDocument = serde_json::from_value(json!({
        "type":5,"w":100,"h":100,"text":[{"id":"search","ref":30,"size":10}],
        "destination":[{"id":"search","dst":[clip_frame()]}]
    }))
    .unwrap();
    for active in [false, true] {
        let items = document.select_render_items(
            &HashMap::new(),
            &SelectSnapshot {
                search_word: "query".into(),
                search_word_alpha: 1.0,
                search_input_active: active,
                ..Default::default()
            },
        );
        if active {
            assert!(matches!(items.as_slice(), [SkinRenderItem::Text { .. }]));
        } else {
            assert!(matches!(clipped_contents(&items), [SkinRenderItem::Text { .. }]));
        }
    }
}

#[test]
fn destination_clip_animation_reuses_completed_gauge_graph_geometry() {
    use crate::snapshot::{ResultGaugeGraphPoint, ResultGraphSnapshot};
    let mut first_frame = clip_frame();
    first_frame["time"] = json!(0);
    let document: SkinDocument = serde_json::from_value(json!({
        "w":100,"h":100,"gaugegraph":[{"id":"graph"}],
        "destination":[{"id":"graph","loop":10000,"dst":[first_frame,{"time":10000,"clip_x":50}]}]
    }))
    .unwrap();
    let context = SkinContext::from_manifest_and_document(default_skin_manifest(), document, []);
    let graph = Arc::new(ResultGraphSnapshot {
        gauge_points: [20.0, 40.0, 90.0]
            .into_iter()
            .map(|value| ResultGaugeGraphPoint {
                value,
                max: 100.0,
                border: 80.0,
                gauge_type: 2,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    });
    let render = |elapsed_ms| {
        context.static_document_items_for_result_state_and_text(
            &graph,
            &SkinDrawState {
                elapsed_ms,
                result_failed: Some(false),
                result_gauge_graph_type: Some(2),
                ..Default::default()
            },
            &SkinTextState::default(),
        )
    };
    let first = render(2000);
    let next = render(3000);
    let [
        SkinRenderItem::PushClip { rect: first_clip },
        SkinRenderItem::RectBatch { rects: first_rects, cache: first_cache },
        SkinRenderItem::PopClip,
    ] = first.as_slice()
    else {
        panic!("first graph")
    };
    let [
        SkinRenderItem::PushClip { rect: next_clip },
        SkinRenderItem::RectBatch { rects: next_rects, cache: next_cache },
        SkinRenderItem::PopClip,
    ] = next.as_slice()
    else {
        panic!("next graph")
    };
    assert_ne!(first_clip, next_clip);
    assert!(first_cache.is_some());
    assert_eq!(first_cache, next_cache);
    assert!(Arc::ptr_eq(first_rects, next_rects));
}

#[test]
fn destination_clip_keeps_fixed_image_and_special_pie_cache_paths_live() {
    for id in ["image", "judge_graph"] {
        let document: SkinDocument = serde_json::from_value(json!({
            "w":100,"h":100,"image":[{"id":id,"src":"src","w":140,"h":8}],
            "destination":[{"id":id,"dst":[{
                "w":140,"h":8,"angle":120,"clip_x":10,"clip_y":20,"clip_w":30,"clip_h":40
            }]}]
        }))
        .unwrap();
        let sources = mock_source("src", 800.0, 800.0);
        let mut cache = ResultRenderCache::default();
        let mut state = SkinDrawState { result_failed: Some(false), ..Default::default() };
        state.judge_counts.pgreat = 80;
        state.judge_counts.great = 20;
        for _ in 0..2 {
            let items = document.static_render_items_with_graphs_cached(
                &sources,
                &state,
                &SkinTextState::default(),
                SkinRuntimeGraphs::from_document(&document),
                Some(&mut cache),
            );
            assert!(matches!(clipped_contents(&items), [SkinRenderItem::RotatedImage { .. }]));
        }
    }
}

#[test]
fn destination_clip_wraps_the_whole_playfield_and_preserves_line_child_bypass() {
    use crate::plan::{DrawCommand, DrawPlan};
    use crate::scene::AppSceneSnapshot;
    use crate::snapshot::{
        NoteVisualKind, VisibleBarLine, VisibleLongNote, VisibleMine, VisibleNote,
    };
    let sprite_names = ["tap", "lnbody", "lnhead", "lntail", "mine", "bar", "bpm", "stop", "time"];
    let images: Vec<_> =
        sprite_names.iter().map(|id| json!({"id":id,"src":id,"w":10,"h":2})).collect();
    let line = |id| json!({"id":id,"dst":[clip_frame()]});
    let document: SkinDocument = serde_json::from_value(json!({
        "type":0,"w":100,"h":100,
        "image":images,
        "note":{"id":"notes","note":["tap"],"lnstart":["lnhead"],"lnend":["lntail"],"lnbody":["lnbody"],"mine":["mine"],
                "dst":[{"x":10,"y":20,"w":40,"h":60}],"group":[line("bar")],"bpm":[line("bpm")],"stop":[line("stop")],"time":[line("time")]},
        "destination":[{"id":"notes","draw":"bmz:lua_draw_callback:7","dst":[clip_frame()]}]
    })).unwrap();
    let mut context = SkinContext::from_manifest_and_document(
        default_skin_manifest(),
        document,
        sprite_names.iter().enumerate().map(|(index, id)| SkinDocumentTexture {
            source_id: (*id).into(),
            texture: SkinTextureId(100 + index as u32),
            source_size: SkinImageSize { width: 10.0, height: 2.0 },
        }),
    );
    let runtime = Arc::new(ClipLuaRuntime::default());
    context.set_lua_draw_runtime(Some(runtime.clone()));
    let mut snapshot = RenderSnapshot {
        key_mode: KeyMode::K7,
        title: "Test".into(),
        practice_preview: true,
        show_ln_tail_cap: true,
        ..Default::default()
    };
    let line = VisibleBarLine { time: TimeUs(0), y: 0.5, alpha: 1.0, label: "guide".into() };
    snapshot.bar_lines.push(line.clone());
    snapshot.bpm_lines.push(line.clone());
    snapshot.stop_lines.push(line.clone());
    snapshot.time_lines.push(line);
    snapshot.visible_notes[Lane::Key1.index()].push(VisibleNote {
        lane: Lane::Key1,
        time: TimeUs(0),
        y: 0.5,
        alpha: 1.0,
        kind: NoteVisualKind::Tap,
        processed_judge: None,
    });
    snapshot.visible_mines[Lane::Key1.index()].push(VisibleMine {
        lane: Lane::Key1,
        time: TimeUs(0),
        y: 0.5,
        alpha: 1.0,
        damage: 1.0,
    });
    snapshot.visible_long_notes.push(VisibleLongNote {
        lane: Lane::Key1,
        mode: LongNoteMode::Ln,
        head_y: 0.2,
        tail_y: 0.7,
        alpha: 1.0,
        body_state: LongBodyState::Inactive,
    });
    snapshot.skin_offsets.set(OFFSET_ALL, SkinOffsetValue { x: 10, y: 5, ..Default::default() });
    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        &context,
        &mut DynamicTimerRuntime::default(),
    );
    assert_eq!(runtime.0.lock().unwrap().as_slice(), &[7]);
    let begin =
        plan.commands.iter().position(|item| matches!(item, DrawCommand::PushClip { .. })).unwrap();
    let end = plan.commands.iter().position(|item| matches!(item, DrawCommand::PopClip)).unwrap();
    assert_eq!(
        plan.commands.iter().filter(|item| matches!(item, DrawCommand::PushClip { .. })).count(),
        1
    );
    let DrawCommand::PushClip { rect } = plan.commands[begin] else { unreachable!() };
    assert!(approx_eq(rect.x, 0.2));
    assert!(approx_eq(rect.y, 0.35));
    let playfield = &plan.commands[begin + 1..end];
    // Tap, LN body/head/tail, mine and all four guide kinds must each be present.
    // Distinct textures prove no missing kind is masked by another image count.
    for (index, name) in sprite_names.iter().enumerate() {
        assert_eq!(
            playfield
                .iter()
                .filter(|item| matches!(item,
                    DrawCommand::Image { texture,.. } if texture.0 == 100 + index as u32
                ))
                .count(),
            1,
            "missing/duplicate {name}: {playfield:?}"
        );
    }
    assert_eq!(
        playfield.iter().filter(|item| matches!(item, DrawCommand::Image { .. })).count(),
        9
    );
    assert!(
        playfield
            .iter()
            .any(|item| matches!(item,DrawCommand::Text { text,.. } if text == "guide"))
    );
    assert!(
        context
            .document_bar_line_items(0.5, KeyMode::K7, &SkinDrawState::default())
            .iter()
            .all(|item| !matches!(item, SkinRenderItem::PushClip { .. }))
    );
}

#[test]
fn destination_clip_hides_inactive_notes_timer_without_treating_it_as_no_clip() {
    let document: SkinDocument = serde_json::from_value(json!({
        "w":100,"h":100,"destination":[{"id":"notes","timer":10000,"dst":[clip_frame()]}]
    }))
    .unwrap();
    let context = SkinContext::from_manifest_and_document(default_skin_manifest(), document, []);
    let rect = context
        .document_playfield_clip(&SkinDrawState::default(), &SkinTextState::default())
        .unwrap();
    assert_eq!(rect.width, 0.0);
    assert_eq!(rect.height, 0.0);
}
