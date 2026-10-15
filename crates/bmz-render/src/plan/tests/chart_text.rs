use super::*;
use crate::skin::{SKIN_OPTION_BMZ_CHART_TEXT_VISIBLE, test_skin_ops};

fn chart_text_skin() -> SkinContext {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "text": [
                    { "id": "by_value_id", "size": 12, "value": 20100 },
                    { "id": "by_value_name", "size": 12, "value": "bmz_chart_text" },
                    { "id": "by_ref", "size": 12, "ref": 20100 },
                    { "id": "marker", "size": 12, "constantText": "VISIBLE" }
                ],
                "destination": [
                    { "id": "by_value_id", "dst": [{ "x": 0, "y": 0, "w": 60, "h": 12 }] },
                    { "id": "by_value_name", "dst": [{ "x": 0, "y": 20, "w": 60, "h": 12 }] },
                    { "id": "by_ref", "dst": [{ "x": 0, "y": 40, "w": 60, "h": 12 }] },
                    { "id": "marker", "op": [20101], "dst": [{ "x": 0, "y": 60, "w": 60, "h": 12 }] }
                ]
            }
        "#,
    )
    .unwrap();
    SkinContext::from_manifest_and_document(SkinManifest::default(), document, [])
}

fn plan_texts(skin: &SkinContext, snapshot: RenderSnapshot) -> Vec<String> {
    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(snapshot),
        skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );
    plan.commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn json_text_reads_chart_text_by_id_name_and_ref() {
    let skin = chart_text_skin();
    let texts = plan_texts(
        &skin,
        RenderSnapshot { chart_text: "Hello World".into(), ..Default::default() },
    );
    assert_eq!(texts, ["Hello World", "Hello World", "Hello World", "VISIBLE"]);

    // The next frame follows the current chart text; nothing is cached from load time.
    let texts = plan_texts(&skin, RenderSnapshot { chart_text: "次".into(), ..Default::default() });
    assert_eq!(texts, ["次", "次", "次", "VISIBLE"]);
}

#[test]
fn chart_text_visible_option_is_false_before_the_first_event_and_for_empty_text() {
    let skin = chart_text_skin();
    assert!(plan_texts(&skin, RenderSnapshot::default()).is_empty());

    let empty = build_play_skin_state(&RenderSnapshot::default(), &skin, 0);
    assert!(!test_skin_ops(&[SKIN_OPTION_BMZ_CHART_TEXT_VISIBLE], &[], &empty));
    let shown = build_play_skin_state(
        &RenderSnapshot { chart_text: " ".into(), ..Default::default() },
        &skin,
        0,
    );
    assert!(test_skin_ops(&[SKIN_OPTION_BMZ_CHART_TEXT_VISIBLE], &[], &shown));
    assert!(!test_skin_ops(&[-SKIN_OPTION_BMZ_CHART_TEXT_VISIBLE], &[], &shown));
}

fn fade_skin() -> SkinContext {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0,
                "w": 100,
                "h": 100,
                "text": [{ "id": "message", "size": 12, "value": "bmz_chart_text" }],
                "destination": [
                    { "id": "message", "timer": 20102, "loop": 1000, "dst": [
                        { "time": 0, "x": 0, "y": 0, "w": 60, "h": 12, "a": 0 },
                        { "time": 1000, "x": 0, "y": 0, "w": 60, "h": 12, "a": 255 }
                    ] }
                ]
            }
        "#,
    )
    .unwrap();
    SkinContext::from_manifest_and_document(SkinManifest::default(), document, [])
}

fn message_alpha(skin: &SkinContext, elapsed: Option<i32>) -> Option<f32> {
    let plan = DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Play(RenderSnapshot {
            chart_text: "FADE".into(),
            chart_text_event_elapsed_ms: elapsed,
            ..Default::default()
        }),
        skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    );
    plan.commands.iter().find_map(|command| match command {
        DrawCommand::Text { text, style, .. } if text == "FADE" => Some(style.color.a),
        _ => None,
    })
}

#[test]
fn chart_text_event_timer_drives_destination_animation() {
    let skin = fade_skin();
    // OFF before the first event: the timer destination is hidden.
    assert_eq!(message_alpha(&skin, None), None);
    let start = message_alpha(&skin, Some(0)).unwrap();
    let middle = message_alpha(&skin, Some(500)).unwrap();
    let end = message_alpha(&skin, Some(1_000)).unwrap();
    assert!(start < 0.01, "{start}");
    assert!((middle - 0.5).abs() < 0.02, "{middle}");
    assert!(end > 0.99, "{end}");
    assert!(message_alpha(&skin, Some(60_000)).unwrap() > 0.99);

    let state = build_play_skin_state(
        &RenderSnapshot { chart_text_event_elapsed_ms: Some(250), ..Default::default() },
        &skin,
        0,
    );
    assert_eq!(
        crate::skin::lua_main_state_timer(crate::skin::SKIN_TIMER_BMZ_CHART_TEXT_EVENT, &state),
        Some(250)
    );
    let off = build_play_skin_state(&RenderSnapshot::default(), &skin, 0);
    assert_eq!(
        crate::skin::lua_main_state_timer(crate::skin::SKIN_TIMER_BMZ_CHART_TEXT_EVENT, &off),
        None
    );
}

#[test]
fn miss_text_refs_option_and_timer_are_exposed() {
    let document: SkinDocument = serde_json::from_str(
        r#"
            {
                "type": 0, "w": 100, "h": 100,
                "text": [
                    { "id": "shown", "size": 12, "value": "bmz_chart_text" },
                    { "id": "definition", "size": 12, "ref": 20103 },
                    { "id": "flash", "size": 12, "constantText": "FLASH" }
                ],
                "destination": [
                    { "id": "shown", "dst": [{ "x": 0, "y": 0, "w": 60, "h": 12 }] },
                    { "id": "definition", "op": [-20104],
                      "dst": [{ "x": 0, "y": 20, "w": 60, "h": 12 }] },
                    { "id": "flash", "timer": 20105, "op": ["bmz_chart_miss_text_active"],
                      "dst": [{ "x": 0, "y": 40, "w": 60, "h": 12 }] }
                ]
            }
        "#,
    )
    .unwrap();
    let skin = SkinContext::from_manifest_and_document(SkinManifest::default(), document, []);
    let normal = RenderSnapshot {
        chart_text: "Normal".into(),
        chart_miss_text: "MISS!!".into(),
        ..Default::default()
    };
    assert_eq!(plan_texts(&skin, normal.clone()), ["Normal", "MISS!!"]);
    let missed = RenderSnapshot {
        chart_text: "MISS!!".into(),
        chart_miss_text_elapsed_ms: Some(10),
        ..normal
    };
    assert_eq!(plan_texts(&skin, missed.clone()), ["MISS!!", "FLASH"]);
    let state = build_play_skin_state(&missed, &skin, 0);
    assert!(test_skin_ops(&[crate::skin::SKIN_OPTION_BMZ_CHART_MISS_TEXT_ACTIVE], &[], &state));
    assert_eq!(
        crate::skin::lua_main_state_timer(crate::skin::SKIN_TIMER_BMZ_CHART_MISS_TEXT, &state),
        Some(10)
    );
    // Without #TEXT00 nothing extra is drawn.
    assert_eq!(
        plan_texts(&skin, RenderSnapshot { chart_text: "Only".into(), ..Default::default() }),
        ["Only"]
    );
}
