use super::*;

#[test]
fn cached_object_categories_preserve_first_text_and_fall_through_when_empty() {
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "w":100,"h":100,
        "text":[{"id":"shared","ref":10}, {"id":"shared","ref":11}],
        "hiddenCover":[{"id":"shared","src":"src","w":10,"h":10}],
        "destination":[{"id":"shared","dst":[{"x":10,"y":20,"w":30,"h":40}]}]
    }))
    .unwrap();
    let sources = HashMap::from([(
        "src".into(),
        SkinDocumentTexture {
            source_id: "src".into(),
            texture: SkinTextureId(1),
            source_size: SkinImageSize { width: 10.0, height: 10.0 },
        },
    )]);
    let mut cache = ResultRenderCache::default();
    for title in ["", "first", "", "changed"] {
        let state = SkinDrawState::default();
        let text = SkinTextState { title, subtitle: "second must not win", ..Default::default() };
        let actual = document.static_render_items_split_with_graphs(
            &sources,
            &state,
            &text,
            SkinRuntimeGraphs::from_document(&document),
            Some(&mut cache),
        );
        assert_eq!(actual, document.static_render_items_split(&sources, &state, &text));
        if title.is_empty() {
            assert!(matches!(actual.0.as_slice(), [SkinRenderItem::Image { .. }]));
        } else {
            assert!(
                matches!(actual.0.as_slice(), [SkinRenderItem::Text { text, .. }] if text == title)
            );
        }
    }
}

#[test]
fn cached_select_search_uses_first_search_definition_and_live_panel_and_edit_state() {
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "type":5,"w":100,"h":100,
        "text":[{"id":"search","ref":10}, {"id":"search","ref":30},
            {"id":"search","ref":30,"align":2}],
        "destination":[{"id":"search","op":[21],"loop":100,
            "dst":[{"time":0,"x":10,"y":20,"w":70,"h":10}, {"time":100,"x":20}]}]
    }))
    .unwrap();
    let context =
        SkinContext::from_manifest_and_document(default_skin_manifest(), document.clone(), []);
    for (panel, editing, elapsed) in
        [(0, false, 0), (1, false, 50), (1, true, 100), (0, true, 150), (1, false, 200)]
    {
        let snapshot = SelectSnapshot {
            option_panel: panel,
            search_input_active: editing,
            search_word: "BMZ".into(),
            search_caret_byte_index: editing.then_some(1),
            time: TimeUs(elapsed * 1000),
            ..Default::default()
        };
        let actual = context.select_document_items(&snapshot);
        assert_eq!(actual, document.select_render_items(&HashMap::new(), &snapshot));
        assert_eq!(actual.len(), usize::from(panel == 1));
        if panel == 1 {
            assert!(matches!(&actual[0], SkinRenderItem::Text { text, style, .. }
                if text == "BMZ" && style.align == TextAlign::Left));
        }
    }
}
