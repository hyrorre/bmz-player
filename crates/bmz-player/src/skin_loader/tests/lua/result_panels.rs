use super::*;

/// Use the original skin's decoded draw/op predicates and live callbacks, but
/// replace the visual assets with text markers so GPU/font/image availability
/// cannot hide a bad predicate. The production renderer evaluates both paths.
fn marker_context(loaded: LoadedSkinDocumentWithDependencies, ids: &[&str]) -> SkinContext {
    let mut text = Vec::new();
    let mut destinations = Vec::new();
    for (index, dst) in
        loaded.document.all_destinations(&loaded.document.enabled_options()).iter().enumerate()
    {
        if !ids.contains(&dst.id.as_str()) {
            continue;
        }
        let marker = format!("marker{index}");
        text.push(serde_json::json!({"id":marker, "size":20, "constantText":dst.id}));
        destinations.push(serde_json::json!({
            "id":marker, "draw":dst.draw, "op":dst.op,
            "dst":[{"x":0,"y":0,"w":300,"h":20}]
        }));
    }
    assert!(!destinations.is_empty());
    let mut document: SkinDocument = serde_json::from_value(serde_json::json!({
        "type":7, "w":1920, "h":1080, "text":text, "destination":destinations
    }))
    .unwrap();
    document.user_selected_options = Some(loaded.document.enabled_options());
    let mut context = SkinContext::from_manifest_and_document(
        bmz_render::skin::default_skin_manifest(),
        document,
        [],
    );
    context.set_lua_draw_runtime(loaded.lua_runtime.map(|runtime| {
        Arc::new(LuaSkinDrawRuntimeAdapter::new(runtime))
            as Arc<dyn bmz_render::skin::SkinLuaDrawRuntime>
    }));
    context
}

#[test]
fn result_panels_luxe_flat_and_wmii_switch_all_parts_in_auto_and_compat() {
    let skins = [
        (
            "Luxez-Flat/result/result.luaskin",
            vec![
                "flame_right",
                "gauge",
                "notes_graph",
                "result_modeselect_graph_data_on",
                "judge_graph",
                "timing_graph",
            ],
            vec![
                "flame_right_ir",
                "result_modeselect_ir_ranking_on",
                "ir_score1",
                "ir_name1",
                "rank1",
            ],
        ),
        (
            "WMII_FHD/result/result.luaskin",
            vec!["graphDataFrame", "notesGraph", "judgeGraph", "timingGraph"],
            vec!["irDataFrame", "irName", "ir_rank1"],
        ),
    ];
    for (relative, graph_ids, ir_ids) in skins {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins").join(relative);
        if !path.is_file() {
            continue;
        }
        for mode in [bmz_skin::LuaSkinRuntimeMode::Auto, bmz_skin::LuaSkinRuntimeMode::Compat] {
            for layout in ["左", "右"] {
                let load_state = LuaLoadRuntimeState {
                    runtime_mode: mode,
                    option_values: BTreeMap::from([(50, false), (51, true), (160, true)]),
                    ..Default::default()
                };
                let options = BTreeMap::from([("リザルトの配置".into(), layout.into())]);
                let loaded = load_skin_document_uncached(
                    &path,
                    SkinKind::Result,
                    &options,
                    &BTreeMap::new(),
                    &load_state,
                )
                .unwrap();
                assert!(loaded.lua_runtime.is_some());
                let ids = graph_ids.iter().chain(&ir_ids).copied().collect::<Vec<_>>();
                let context = marker_context(loaded, &ids);
                let mut state = SkinDrawState { elapsed_ms: 5000, ..Default::default() };
                state.ir_ranking.entries[0].rank = Some(1);
                // Reuse the same VM/context across transitions and repeated frames.
                for panel in [2, 1, 1, 2, 0, 1, 2] {
                    state.result_panel = Some(panel);
                    context.begin_frame();
                    let mut visible = context
                        .static_document_items_for_state(&state)
                        .into_iter()
                        .filter_map(|item| match item {
                            SkinRenderItem::Text { text, .. } => Some(text),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    visible.sort();
                    let mut expected = match panel {
                        1 => ir_ids.clone(),
                        2 => graph_ids.clone(),
                        _ => vec![],
                    };
                    expected.sort();
                    assert_eq!(visible, expected, "{relative} {mode:?} {layout} panel={panel}");
                }
            }
        }
    }
}
