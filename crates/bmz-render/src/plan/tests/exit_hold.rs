use super::*;
use crate::skin::{
    SKIN_OPTION_BMZ_EXIT_HOLD_ACTIVE, SKIN_REF_BMZ_EXIT_HOLD_DURATION_MS,
    SKIN_REF_BMZ_EXIT_HOLD_PERCENT, SKIN_REF_BMZ_EXIT_HOLD_REMAINING_MS, SKIN_TIMER_BMZ_EXIT_HOLD,
    SkinDrawState, lua_main_state_number, lua_main_state_timer, test_skin_ops,
};
use crate::snapshot::ExitHoldSnapshot;

const LABEL: &str = "HOLD ESC TO EXIT";

fn held(elapsed_ms: i32) -> ExitHoldSnapshot {
    ExitHoldSnapshot { elapsed_ms: Some(elapsed_ms), duration_ms: 1_200 }
}

fn select_skin(json: &str) -> SkinContext {
    SkinContext::from_manifest_and_document(
        SkinManifest::default(),
        serde_json::from_str(json).unwrap(),
        [],
    )
}

fn plan_texts(skin: &SkinContext, exit_hold: ExitHoldSnapshot) -> Vec<String> {
    let snapshot = crate::scene::SelectSnapshot { exit_hold, ..Default::default() };
    DrawPlan::from_scene_with_skin(
        &AppSceneSnapshot::Select(snapshot),
        skin,
        &mut crate::skin::DynamicTimerRuntime::default(),
    )
    .commands
    .iter()
    .filter_map(|command| match command {
        DrawCommand::Text { text, .. } => Some(text.clone()),
        _ => None,
    })
    .collect()
}

#[test]
fn builtin_indicator_draws_a_panel_and_fill_proportional_to_progress() {
    let mut commands = Vec::new();
    push_exit_hold_indicator(&mut commands, 0.0);
    assert!(commands.is_empty());

    push_exit_hold_indicator(&mut commands, 0.5);
    assert!(commands.iter().any(|command| matches!(command,
        DrawCommand::Text { text, .. } if text == LABEL)));
    let rects: Vec<_> = commands
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    // panel, accent, track, fill
    assert_eq!(rects.len(), 4);
    let (track, fill) = (rects[2], rects[3]);
    assert!((fill.width - track.width * 0.5).abs() < 1e-6);
    assert_eq!((fill.x, fill.y, fill.height), (track.x, track.y, track.height));
}

#[test]
fn select_skins_keep_the_builtin_indicator_unless_they_opt_out() {
    for json in [r#"{"type":5}"#, r#"{"type":5,"bmzExitHold":{}}"#] {
        let skin = select_skin(json);
        assert!(plan_texts(&skin, held(600)).iter().any(|text| text == LABEL), "{json}");
        assert!(!plan_texts(&skin, ExitHoldSnapshot::default()).iter().any(|text| text == LABEL));
    }
    // The built-in select screen without a skin document always shows it.
    let texts = plan_texts(&SkinContext::default(), held(600));
    assert!(texts.iter().any(|text| text == LABEL));
}

#[test]
fn opted_out_select_skin_draws_its_own_indicator_from_the_exit_hold_option() {
    let skin = select_skin(
        r#"
            {
                "type": 5,
                "w": 100,
                "h": 100,
                "bmzExitHold": { "builtin": false },
                "text": [{ "id": "exit", "size": 12, "constantText": "BYE" }],
                "destination": [
                    { "id": "exit", "op": [19073], "dst": [{ "x": 0, "y": 0, "w": 60, "h": 12 }] }
                ]
            }
        "#,
    );
    assert_eq!(plan_texts(&skin, held(600)), ["BYE"]);
    assert!(plan_texts(&skin, ExitHoldSnapshot::default()).is_empty());
}

#[test]
fn exit_hold_refs_report_progress_remaining_duration_and_timer() {
    let idle = SkinDrawState {
        exit_hold: ExitHoldSnapshot { elapsed_ms: None, duration_ms: 1_200 },
        ..Default::default()
    };
    assert!(!test_skin_ops(&[SKIN_OPTION_BMZ_EXIT_HOLD_ACTIVE], &[], &idle));
    assert_eq!(lua_main_state_timer(SKIN_TIMER_BMZ_EXIT_HOLD, &idle), None);
    assert_eq!(lua_main_state_number(SKIN_REF_BMZ_EXIT_HOLD_PERCENT, &idle), 0);
    assert_eq!(lua_main_state_number(SKIN_REF_BMZ_EXIT_HOLD_REMAINING_MS, &idle), 0);
    assert_eq!(lua_main_state_number(SKIN_REF_BMZ_EXIT_HOLD_DURATION_MS, &idle), 1_200);

    let holding = SkinDrawState { exit_hold: held(900), ..Default::default() };
    assert!(test_skin_ops(&[SKIN_OPTION_BMZ_EXIT_HOLD_ACTIVE], &[], &holding));
    assert_eq!(lua_main_state_timer(SKIN_TIMER_BMZ_EXIT_HOLD, &holding), Some(900));
    assert_eq!(lua_main_state_number(SKIN_REF_BMZ_EXIT_HOLD_PERCENT, &holding), 75);
    assert_eq!(lua_main_state_number(SKIN_REF_BMZ_EXIT_HOLD_REMAINING_MS, &holding), 300);

    let overdue = SkinDrawState { exit_hold: held(1_500), ..Default::default() };
    assert_eq!(lua_main_state_number(SKIN_REF_BMZ_EXIT_HOLD_PERCENT, &overdue), 100);
    assert_eq!(lua_main_state_number(SKIN_REF_BMZ_EXIT_HOLD_REMAINING_MS, &overdue), 0);
}
