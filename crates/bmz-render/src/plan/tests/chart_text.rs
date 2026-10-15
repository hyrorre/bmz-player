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
