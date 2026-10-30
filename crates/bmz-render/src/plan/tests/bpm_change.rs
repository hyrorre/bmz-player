use super::*;
use crate::skin::{
    SKIN_OPTION_BMZ_BPM_CHANGE_AVAILABLE, SKIN_OPTION_BMZ_BPM_CHANGE_DECREASE,
    SKIN_OPTION_BMZ_BPM_CHANGE_INCREASE, SKIN_OPTION_BMZ_BPM_CHANGE_PREVIEW_ACTIVE,
    SKIN_REF_BMZ_BPM_CHANGE_NEXT_BPM, SKIN_REF_BMZ_BPM_CHANGE_PREVIOUS_BPM,
    SKIN_REF_BMZ_BPM_CHANGE_RATIO_PERCENT, SKIN_REF_BMZ_BPM_CHANGE_REMAINING_MS,
    SKIN_TIMER_BMZ_BPM_CHANGE_PREVIEW, SKIN_TIMER_BMZ_BPM_CHANGED, lua_main_state_number,
    lua_main_state_option, lua_main_state_timer, test_skin_ops,
};
use crate::snapshot::{BpmChangePreviewSnapshot, UpcomingBpmChange};

fn skin(json: &str) -> SkinContext {
    SkinContext::from_manifest_and_document(
        SkinManifest::default(),
        serde_json::from_str(json).unwrap(),
        [],
    )
}

fn snapshot(remaining_ms: i32, from_bpm: f32, to_bpm: f32) -> RenderSnapshot {
    RenderSnapshot {
        bpm_change_preview: BpmChangePreviewSnapshot {
            next: Some(UpcomingBpmChange { remaining_ms, from_bpm, to_bpm }),
            since_last_change_ms: Some(1_234),
        },
        ..Default::default()
    }
}

fn op(id: i32, state: &crate::skin::SkinDrawState) -> bool {
    test_skin_ops(&[id], &[], state)
}

#[test]
fn bpm_change_preview_refs_follow_skin_lead_time() {
    let skin = skin(r#"{"bmzBpmChangePreview":{"leadMs":1500}}"#);

    let before = build_play_skin_state(&snapshot(1_501, 155.4, 149.0), &skin, 0);
    assert_eq!(before.bpm_change_preview_lead_ms, Some(1_500));
    assert!(!op(SKIN_OPTION_BMZ_BPM_CHANGE_PREVIEW_ACTIVE, &before));
    assert_eq!(lua_main_state_timer(SKIN_TIMER_BMZ_BPM_CHANGE_PREVIEW, &before), None);
    assert!(op(SKIN_OPTION_BMZ_BPM_CHANGE_AVAILABLE, &before));
    assert!(op(SKIN_OPTION_BMZ_BPM_CHANGE_DECREASE, &before));
    assert!(!op(SKIN_OPTION_BMZ_BPM_CHANGE_INCREASE, &before));
    for (ref_id, expected) in [
        (SKIN_REF_BMZ_BPM_CHANGE_REMAINING_MS, 1_501),
        (SKIN_REF_BMZ_BPM_CHANGE_NEXT_BPM, 149),
        (SKIN_REF_BMZ_BPM_CHANGE_PREVIOUS_BPM, 155),
        (SKIN_REF_BMZ_BPM_CHANGE_RATIO_PERCENT, 96),
    ] {
        assert_eq!(lua_main_state_number(ref_id, &before), expected, "ref {ref_id}");
    }

    let start = build_play_skin_state(&snapshot(1_500, 153.0, 156.0), &skin, 0);
    assert!(op(SKIN_OPTION_BMZ_BPM_CHANGE_PREVIEW_ACTIVE, &start));
    assert!(lua_main_state_option(SKIN_OPTION_BMZ_BPM_CHANGE_INCREASE, &[], &start));
    assert_eq!(lua_main_state_timer(SKIN_TIMER_BMZ_BPM_CHANGE_PREVIEW, &start), Some(0));

    let late = build_play_skin_state(&snapshot(250, 153.0, 156.0), &skin, 0);
    assert_eq!(lua_main_state_timer(SKIN_TIMER_BMZ_BPM_CHANGE_PREVIEW, &late), Some(1_250));
    assert_eq!(lua_main_state_timer(SKIN_TIMER_BMZ_BPM_CHANGED, &late), Some(1_234));
}

#[test]
fn bpm_change_preview_requires_declaration_and_clamps_lead() {
    let undeclared = build_play_skin_state(&snapshot(10, 120.0, 240.0), &skin("{}"), 0);
    assert_eq!(undeclared.bpm_change_preview_lead_ms, None);
    assert!(!op(SKIN_OPTION_BMZ_BPM_CHANGE_PREVIEW_ACTIVE, &undeclared));
    assert_eq!(lua_main_state_timer(SKIN_TIMER_BMZ_BPM_CHANGE_PREVIEW, &undeclared), None);
    // Raw facts remain available to skins without the preview declaration.
    assert_eq!(lua_main_state_number(SKIN_REF_BMZ_BPM_CHANGE_NEXT_BPM, &undeclared), 240);

    let default_lead = skin(r#"{"bmzBpmChangePreview":{}}"#);
    let state = build_play_skin_state(&snapshot(10, 120.0, 240.0), &default_lead, 0);
    assert_eq!(state.bpm_change_preview_lead_ms, Some(2_000));

    let clamped = skin(r#"{"bmzBpmChangePreview":{"leadMs":60000}}"#);
    let state = build_play_skin_state(&snapshot(10, 120.0, 240.0), &clamped, 0);
    assert_eq!(state.bpm_change_preview_lead_ms, Some(10_000));
}

#[test]
fn bpm_change_preview_without_next_change_hides_values() {
    let skin = skin(r#"{"bmzBpmChangePreview":{"leadMs":2000}}"#);
    let state = build_play_skin_state(&RenderSnapshot::default(), &skin, 0);
    for option in [
        SKIN_OPTION_BMZ_BPM_CHANGE_PREVIEW_ACTIVE,
        SKIN_OPTION_BMZ_BPM_CHANGE_INCREASE,
        SKIN_OPTION_BMZ_BPM_CHANGE_DECREASE,
        SKIN_OPTION_BMZ_BPM_CHANGE_AVAILABLE,
    ] {
        assert!(!op(option, &state), "option {option}");
    }
    // Number images hide the missing value; Lua receives 0 and checks option 19067.
    assert_eq!(lua_main_state_number(SKIN_REF_BMZ_BPM_CHANGE_NEXT_BPM, &state), 0);
    assert_eq!(lua_main_state_timer(SKIN_TIMER_BMZ_BPM_CHANGE_PREVIEW, &state), None);
    assert_eq!(lua_main_state_timer(SKIN_TIMER_BMZ_BPM_CHANGED, &state), None);
}

#[test]
fn effective_bpm_preview_keeps_signed_values_and_numeric_color_direction() {
    let skin = skin(r#"{"bmzBpmChangePreview":{"leadMs":2000}}"#);
    for (from, to, increase, ratio) in [
        (150.0, 0.0, false, 0),
        (0.0, 150.0, true, 0),
        (0.0, -150.0, false, 0),
        (-150.0, 0.0, true, 0),
        (-150.0, -300.0, false, 200),
        (-300.0, -150.0, true, 50),
        (-150.0, 150.0, true, -100),
    ] {
        let state = build_play_skin_state(&snapshot(500, from, to), &skin, 0);
        assert!(op(SKIN_OPTION_BMZ_BPM_CHANGE_PREVIEW_ACTIVE, &state));
        assert_eq!(
            lua_main_state_number(SKIN_REF_BMZ_BPM_CHANGE_PREVIOUS_BPM, &state),
            from as i64
        );
        assert_eq!(lua_main_state_number(SKIN_REF_BMZ_BPM_CHANGE_NEXT_BPM, &state), to as i64);
        assert_eq!(op(SKIN_OPTION_BMZ_BPM_CHANGE_INCREASE, &state), increase);
        assert_eq!(op(SKIN_OPTION_BMZ_BPM_CHANGE_DECREASE, &state), !increase);
        assert_eq!(lua_main_state_number(SKIN_REF_BMZ_BPM_CHANGE_RATIO_PERCENT, &state), ratio);
    }
}
