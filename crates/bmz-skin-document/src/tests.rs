//! serde decode と include 展開の純 document テスト。
//! 描画評価を含むテストは `bmz-render/src/skin.rs` 側に残している。

use super::*;

#[test]
fn lua_numeric_normalization_preserves_singleton_offset_definitions() {
    for count in 0..=2 {
        let offsets = (0..count)
            .map(|id| serde_json::json!({"id":id,"name":"Position","x":true}))
            .collect::<Vec<_>>();
        let value = normalize_lua_json_skin_integer_numbers(serde_json::json!({
            "offset": offsets,
            "value": [{"id":"number","offset":[{"ref":[42.0]}]}],
            "note": {"offset":[3.0]},
            "destination": [{"id":"image","dst":[{"x":[10.5]}]}]
        }));
        assert!(value["offset"].is_array(), "count: {count}, {value}");
        assert!(value["value"][0]["offset"].is_array());
        assert_eq!(value["note"]["offset"], 3);
        assert_eq!(value["destination"][0]["dst"][0]["x"], 11);
        let document: SkinDocument = serde_json::from_value(value).unwrap();
        assert_eq!(document.offset.len(), count);
        assert_eq!(document.value[0].offset[0].ref_id, 42);
    }
}

#[test]
fn destination_clip_fields_decode_through_json_and_lua_numeric_normalization() {
    let frame = serde_json::json!({"clip_x":1.5,"clip_y":2.0,"clip_w":30.0,"clip_h":40.0});
    for value in [
        normalize_json_skin_integer_numbers(frame),
        normalize_lua_json_skin_integer_numbers(serde_json::json!({
            "clip_x":[1.5],"clip_y":[2.0],"clip_w":[30.0],"clip_h":[40.0]
        })),
    ] {
        let frame: SkinAnimationDef = serde_json::from_value(value).unwrap();
        assert_eq!(
            [frame.clip_x, frame.clip_y, frame.clip_w, frame.clip_h],
            [Some(2), Some(2), Some(30), Some(40)]
        );
    }
}

#[test]
fn skin_document_normalizes_numeric_and_string_ids() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "type": 0,
            "source": [
                { "id": 100, "path": "a.png" },
                { "id": "100", "path": "b.png" }
            ],
            "image": [
                { "id": 200, "src": 100, "x": 0, "y": 0, "w": 8, "h": 8 },
                { "id": "300", "src": "100", "x": 8, "y": 0, "w": 8, "h": 8 }
            ],
            "imageset": [
                { "id": "set", "images": [200, "300"] }
            ],
            "destination": [
                { "id": 200, "dst": [{ "x": 0, "y": 0, "w": 8, "h": 8 }] }
            ]
        }
        "#,
    )
    .unwrap();

    assert_eq!(document.source[0].id, "100");
    assert_eq!(document.source[1].id, "100");
    assert_eq!(document.image[0].id, "200");
    assert_eq!(document.image[0].src, "100");
    assert_eq!(document.image[1].src, "100");
    assert_eq!(document.imageset[0].images, ["200", "300"]);
    let DestinationListEntry::Single(dst0) = &document.destination[0] else {
        panic!("expected Single destination");
    };
    assert_eq!(dst0.id, "200");
}

#[test]
fn skin_document_accepts_lua_integer_flags_for_is_ref_num() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "slider": [
                { "id": "integer-true", "isRefNum": 1 },
                { "id": "integer-false", "isRefNum": 0 },
                { "id": "boolean-true", "isRefNum": true }
            ],
            "graph": [
                { "id": "integer-true", "isRefNum": 1 },
                { "id": "integer-false", "isRefNum": 0 },
                { "id": "boolean-false", "isRefNum": false }
            ]
        }
        "#,
    )
    .unwrap();

    assert!(document.slider[0].is_ref_num);
    assert!(!document.slider[1].is_ref_num);
    assert!(document.slider[2].is_ref_num);
    assert!(document.graph[0].is_ref_num);
    assert!(!document.graph[1].is_ref_num);
    assert!(!document.graph[2].is_ref_num);
}

#[test]
fn panel_and_destination_action_deserialize() {
    let document: SkinDocument = serde_json::from_str(
        r##"
        {
            "panel": [{
                "id": "option-panel",
                "color": "#10203080",
                "borderColor": "A0B0C0",
                "borderWidth": 2.5
            }],
            "destination": [{
                "id": "option-panel",
                "act": 42,
                "click": 2,
                "clickable": true,
                "mouseRect": { "x": 1, "y": 2, "w": 3, "h": 4 },
                "dst": [{ "x": 10, "y": 20, "w": 30, "h": 40 }]
            }]
        }
        "##,
    )
    .unwrap();

    assert_eq!(document.panel[0].id, "option-panel");
    assert_eq!(document.panel[0].color, "#10203080");
    assert_eq!(document.panel[0].border_color, "A0B0C0");
    assert_eq!(document.panel[0].border_width, 2.5);
    let DestinationListEntry::Single(destination) = &document.destination[0] else {
        panic!("expected Single destination");
    };
    assert_eq!(destination.act, Some(42));
    assert_eq!(destination.click, 2);
    assert_eq!(destination.clickable, Some(true));
    assert_eq!(destination.mouse_rect.unwrap(), SkinRectDef { x: 1, y: 2, w: 3, h: 4 });
}

#[test]
fn skin_document_decodes_lift_cover_with_beatoraja_link_default() {
    let document: SkinDocument = serde_json::from_str(
        r#"
        {
            "type": 0,
            "liftCover": [
                { "id": "lift", "src": 13, "x": 0, "y": 0, "w": 432, "h": 723, "disapearLine": 357 },
                { "id": "linked-lift", "src": 14, "isDisapearLineLinkLift": true }
            ]
        }
        "#,
    )
    .unwrap();

    assert_eq!(document.lift_cover.len(), 2);
    assert_eq!(document.lift_cover[0].id, "lift");
    assert_eq!(document.lift_cover[0].src, "13");
    assert!(!document.lift_cover[0].is_disappear_line_link_lift);
    assert!(document.lift_cover[1].is_disappear_line_link_lift);
}

#[test]
fn skin_document_expands_conditions_and_includes() {
    let dir = tempfile::Builder::new().prefix("bmz-skin-document-json-").tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("included.json"),
        r#"
        [
            { "id": "included", "src": "1", "x": 0, "y": 0, "w": 8, "h": 8, },
            { "if": -901, "value": { "id": "disabled", "src": "1" } }
        ]
        "#,
    )
    .unwrap();
    std::fs::write(
        root.join("skin.json"),
        r#"
        {
            "type": 0,
            "property": [
                { "name": "Graph", "def": "On", "item": [
                    { "name": "Off", "op": 900 },
                    { "name": "On", "op": 901 }
                ]}
            ],
            "source": [{ "id": 1, "path": "system.png" }],
            "image": { "include": "included.json" },
            "destination": [
                { "if": 901, "value": { "id": "included", "dst": [{ "x": 1, "y": 2, "w": 3, "h": 4 }] } },
                { "if": -901, "value": { "id": "disabled", "dst": [{ "x": 0, "y": 0, "w": 1, "h": 1 }] } }
            ],
        }
        "#,
    )
    .unwrap();

    let document = SkinDocument::load_beatoraja_json(&root.join("skin.json")).unwrap();

    assert_eq!(document.source[0].id, "1");
    assert_eq!(document.image.len(), 1);
    assert_eq!(document.image[0].id, "included");
    assert_eq!(document.destination.len(), 1);
    let DestinationListEntry::Single(dst0) = &document.destination[0] else {
        panic!("expected Single destination");
    };
    assert_eq!(dst0.id, "included");
    let SkinDstEntry::Frame(frame) = &dst0.dst[0] else {
        panic!("expected Frame entry");
    };
    assert_eq!(frame.x, Some(1));
}

#[test]
fn runtime_flags_and_events_deserialize() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
            "runtimeFlag": [{ "id": -20001, "initial": true }],
            "runtimeEvent": [{ "id": -20002, "toggleFlags": [-20001, -20003] }]
        }"#,
    )
    .unwrap();

    assert_eq!(document.runtime_flags.len(), 1);
    assert_eq!(document.runtime_flags[0].id, -20_001);
    assert!(document.runtime_flags[0].initial);
    assert_eq!(document.runtime_events[0].id, -20_002);
    assert_eq!(document.runtime_events[0].toggle_flags, [-20_001, -20_003]);
}

#[test]
fn result_ir_scope_extensions_deserialize_with_global_defaults() {
    let defaults: SkinDocument = serde_json::from_str("{}").unwrap();
    assert_eq!(defaults.result_ir_scope_binding, IrScopeBinding::Global);
    assert_eq!(defaults.result_ir_scope_toggle, ResultIrScopeToggle::None);
    assert_eq!(defaults.select_ir_scope_binding, IrScopeBinding::Global);
    assert_eq!(defaults.select_ir_scope_toggle, SelectIrScopeToggle::None);

    let document: SkinDocument = serde_json::from_str(
        r#"{
            "resultIrScopeBinding": "active",
            "resultIrScopeToggle": "e1_press",
            "selectIrScopeBinding": "active",
            "selectIrScopeToggle": "e3_press"
        }"#,
    )
    .unwrap();
    assert_eq!(document.result_ir_scope_binding, IrScopeBinding::Active);
    assert_eq!(document.result_ir_scope_toggle, ResultIrScopeToggle::E1Press);
    assert_eq!(document.select_ir_scope_binding, IrScopeBinding::Active);
    assert_eq!(document.select_ir_scope_toggle, SelectIrScopeToggle::E3Press);
}

#[test]
fn scene_audio_and_custom_event_deserialize() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
            "sceneAudio": [
                { "action": "loop", "path": "result/bgm.ogg", "volume": 0.75 }
            ],
            "customEvents": [
                {
                    "id": 1001,
                    "timer": 2,
                    "once": true,
                    "audioActions": [
                        { "action": "stop", "path": "result/bgm.ogg" },
                        { "action": "play", "path": "result/close.ogg", "volume": 0.5 }
                    ]
                }
            ]
        }"#,
    )
    .unwrap();

    assert_eq!(document.scene_audio[0].action, SkinAudioActionKind::Loop);
    assert_eq!(document.scene_audio[0].volume, 0.75);
    assert_eq!(document.custom_events[0].timer, 2);
    assert!(document.custom_events[0].once);
    assert_eq!(document.custom_events[0].audio_actions.len(), 2);
    assert_eq!(document.custom_events[0].audio_actions[0].volume, 1.0);
}

#[test]
fn practice_destination_position_uses_beatoraja_practice_object() {
    let document: SkinDocument = serde_json::from_str(
        r#"{
            "w": 1280,
            "h": 720,
            "practice": { "id": "practice", "visibleItems": 12 },
            "destination": [
                { "id": "practice", "dst": [{ "x": 128, "y": 72, "w": 500, "h": 360 }] }
            ]
        }"#,
    )
    .unwrap();

    assert_eq!(document.practice.as_ref().unwrap().visible_items, 12);
    assert_eq!(document.practice_destination_position(), Some((0.1, 0.4)));
}

#[test]
fn chart_text_reference_is_detected_from_text_ref_and_value() {
    for (json, expected) in [
        (r#"{"text":[{"id":"a","ref":20100}]}"#, true),
        (r#"{"text":[{"id":"a","value":20100}]}"#, true),
        (r#"{"text":[{"id":"a","value":"bmz_chart_text"}]}"#, true),
        (r#"{"text":[{"id":"a","ref":20103}]}"#, true),
        (r#"{"text":[{"id":"a","value":"bmz_chart_miss_text"}]}"#, true),
        (r#"{"text":[{"id":"a","ref":10}]}"#, false),
        (r#"{"text":[{"id":"a","constantText":"20100"}]}"#, false),
        (r#"{}"#, false),
    ] {
        let document: SkinDocument = serde_json::from_str(json).unwrap();
        assert_eq!(document.references_chart_text(), expected, "{json}");
    }
}

#[test]
fn bmz_exit_hold_declaration_defaults_to_the_builtin_indicator() {
    let decode = |json: &str| serde_json::from_str::<SkinDocument>(json).unwrap().bmz_exit_hold;
    assert_eq!(decode("{}"), None);
    assert_eq!(decode(r#"{"bmzExitHold":{}}"#), Some(SkinExitHoldDef { builtin: true }));
    assert_eq!(
        decode(r#"{"bmzExitHold":{"builtin":false}}"#),
        Some(SkinExitHoldDef { builtin: false })
    );
}

#[test]
fn font_fallback_accepts_beatoraja_string_and_object_forms() {
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "font": [
            {"id":"main", "path":"font/main.ttf", "fallback":[
                {"path":"font/symbols.ttf", "type":0},
                "font/emoji.ttf",
                {"value":"font/legacy.fnt", "type":1},
                {"type":2},
                null
            ]},
            {"id":"single", "path":"font/a.ttf", "fallback":"font/b.ttf"},
            {"id":"lua-empty", "path":"font/a.ttf", "fallback":{}},
            {"id":"none", "path":"font/a.ttf"}
        ]
    }))
    .unwrap();
    let fallback = |index: usize| document.font[index].fallback.clone();
    assert_eq!(
        fallback(0),
        vec![
            SkinFontFallbackDef { path: "font/symbols.ttf".into(), font_type: 0 },
            SkinFontFallbackDef { path: "font/emoji.ttf".into(), font_type: 0 },
            SkinFontFallbackDef { path: "font/legacy.fnt".into(), font_type: 1 },
        ]
    );
    assert_eq!(fallback(1), vec![SkinFontFallbackDef { path: "font/b.ttf".into(), font_type: 0 }]);
    assert!(fallback(2).is_empty());
    assert!(fallback(3).is_empty());
}
