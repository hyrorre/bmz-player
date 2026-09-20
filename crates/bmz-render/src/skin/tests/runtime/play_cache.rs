use super::*;

#[derive(Debug, Default)]
struct OrderedRuntime(Mutex<Vec<usize>>);

impl SkinLuaDrawRuntime for OrderedRuntime {
    fn evaluate_draw(
        &self,
        id: usize,
        state: &SkinDrawState,
        _: &[i32],
        _: &BTreeMap<i32, String>,
    ) -> bool {
        self.0.lock().unwrap().push(id);
        id == 0 || state.elapsed_ms % 2 == 0
    }
    fn evaluate_number(
        &self,
        id: usize,
        state: &SkinDrawState,
        _: &[i32],
        _: &BTreeMap<i32, String>,
    ) -> Option<f64> {
        self.0.lock().unwrap().push(id);
        Some(state.elapsed_ms as f64)
    }
}

#[test]
fn play_cache_preserves_stateful_callback_order_and_dynamic_items() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
        "w": 100, "h": 100,
        "image": [
            {"id": "fixed", "src": "src", "w": 10, "h": 10},
            {"id": "changing", "src": "src", "w": 10, "h": 10}
        ],
        "value": [{"id": "number", "src": "src", "w": 100, "h": 10, "divx": 10,
            "digit": 2, "value_expr": "bmz:lua_value_callback:2"}],
        "destination": [
            {"id": "fixed", "dst": [{"x": 0, "y": 0, "w": 10, "h": 10}]},
            {"id": "changing", "draw": "bmz:lua_draw_callback:0", "offset": 30,
                "dst": [{"x": 10, "y": 0, "w": 10, "h": 10}]},
            {"id": "notes"},
            {"id": "changing", "draw": "bmz:lua_draw_callback:1", "loop": 100,
                "dst": [{"time": 0, "x": 20, "y": 0, "w": 10, "h": 10}, {"time": 100, "x": 80}]},
            {"id": "number", "dst": [{"x": 0, "y": 20, "w": 10, "h": 10}]}
        ]
    }"#,
    )
    .unwrap();
    let sources = [SkinDocumentTexture {
        source_id: "src".into(),
        texture: SkinTextureId(1),
        source_size: SkinImageSize { width: 100.0, height: 10.0 },
    }];
    let mut skin = SkinContext::from_manifest_and_document(
        default_skin_manifest(),
        document.clone(),
        sources.clone(),
    );
    let runtime = Arc::new(OrderedRuntime::default());
    skin.set_lua_draw_runtime(Some(runtime.clone()));
    let sources = sources.into_iter().map(|source| (source.source_id.clone(), source)).collect();
    let reference_runtime = Arc::new(OrderedRuntime::default());
    for elapsed in [0, 1, 50, 51, 100, 101] {
        let mut state = SkinDrawState { elapsed_ms: elapsed, ..Default::default() };
        state.skin_offsets.set(30, SkinOffsetValue { x: elapsed, ..Default::default() });
        let text = SkinTextState::default();
        let actual =
            skin.static_document_play_items_split_for_state_and_text(&state, &text, &[], &[]);
        let mut reference_state = state.clone();
        reference_state.lua_runtime = Some(SkinLuaRuntimeContext {
            runtime: reference_runtime.clone(),
            enabled_options: Arc::from(document.enabled_options()),
            text_values: Arc::new(BTreeMap::new()),
        });
        let expected = document.static_render_items_split(&sources, &reference_state, &text);
        assert_eq!(actual, expected);
        assert_eq!(*runtime.0.lock().unwrap(), *reference_runtime.0.lock().unwrap());
        assert_eq!(runtime.0.lock().unwrap().as_slice(), &[0, 1, 2]);
        runtime.0.lock().unwrap().clear();
        reference_runtime.0.lock().unwrap().clear();
    }
}

#[test]
fn animation_metadata_matches_inherited_frame_evaluation() {
    for seed in 0..128 {
        let animations: Vec<_> = (0..4)
            .map(|i| SkinAnimationDef {
                acc: (seed & (1 << i) != 0).then_some(i - 1),
                r: (seed & (1 << (i + 1)) != 0).then_some(40),
                a: (seed & (1 << (i + 2)) != 0).then_some(128),
                ..serde_json::from_str("{}").unwrap()
            })
            .collect();
        let mut frame = ResolvedSkinFrame::default();
        let mut first_acc = 0;
        let mut colors = Vec::new();
        for animation in &animations {
            apply_skin_animation(&mut frame, animation, &SkinDrawState::default());
            if first_acc == 0 {
                first_acc = frame.acc;
            }
            colors.push((frame.r, frame.g, frame.b, frame.a));
        }
        assert_eq!(destination_interpolation_acc_from_frames(&animations), first_acc);
        assert_eq!(
            destination_frames_have_fixed_color(&animations),
            colors.windows(2).all(|pair| pair[0] == pair[1])
        );
    }
}

#[test]
fn cached_object_indices_preserve_duplicate_ids_and_live_sources_and_values() {
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "w": 100, "h": 100,
        "image": [
            {"id": "duplicate", "src": "first", "w": 10, "h": 10},
            {"id": "duplicate", "src": "last", "w": 20, "h": 10}
        ],
        "value": [
            {"id": "number", "src": "first", "w": 100, "h": 10, "divx": 10, "digit": 3, "ref": 107},
            {"id": "number", "src": "last", "w": 100, "h": 10, "divx": 10, "digit": 3, "ref": 110}
        ],
        "destination": [
            {"id": "duplicate", "offset": 30, "dst": [{"x": 0, "y": 0, "w": 10, "h": 10}]},
            {"id": "number", "dst": [{"x": 0, "y": 20, "w": 5, "h": 10}]}
        ]
    }))
    .unwrap();
    let mut cache = ResultRenderCache::default();
    let mut previous = None;
    for (texture_id, width, score) in
        [(1, 100.0, 10), (1, 100.0, 123), (3, 200.0, 123), (5, 80.0, 4)]
    {
        let sources = HashMap::from([
            (
                "first".into(),
                SkinDocumentTexture {
                    source_id: "first".into(),
                    texture: SkinTextureId(texture_id),
                    source_size: SkinImageSize { width, height: 20.0 },
                },
            ),
            (
                "last".into(),
                SkinDocumentTexture {
                    source_id: "last".into(),
                    texture: SkinTextureId(texture_id + 1),
                    source_size: SkinImageSize { width, height: 20.0 },
                },
            ),
        ]);
        let mut state = SkinDrawState { gauge: 90.0, ..Default::default() };
        state.judge_counts.pgreat = score;
        let text = SkinTextState::default();
        let actual = document.static_render_items_split_with_graphs(
            &sources,
            &state,
            &text,
            SkinRuntimeGraphs::from_document(&document),
            Some(&mut cache),
        );
        let expected = document.static_render_items_split(&sources, &state, &text);
        assert_eq!(actual, expected);
        assert!(
            matches!(actual.0.first(), Some(SkinRenderItem::Image { texture, .. }) if *texture == SkinTextureId(texture_id + 1)),
            "image maps keep the last duplicate"
        );
        assert!(
            matches!(actual.0.get(1), Some(SkinRenderItem::Image { texture, .. }) if *texture == SkinTextureId(texture_id)),
            "number sprites keep the first duplicate while value resolution uses the last"
        );
        if let Some(previous) = &previous {
            assert_ne!(&actual, previous);
        }
        previous = Some(actual);
    }
}
