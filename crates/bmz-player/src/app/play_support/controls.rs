pub(in crate::app) fn cycle_bga_option_with_direction(
    current: BgaModeConfig,
    direction: i32,
) -> BgaModeConfig {
    const VALUES: [BgaModeConfig; 3] = [BgaModeConfig::On, BgaModeConfig::Auto, BgaModeConfig::Off];
    cycle_enum(VALUES, current, direction)
}

pub(in crate::app) fn cycle_bga_expand_with_direction(
    current: BgaExpandConfig,
    direction: i32,
) -> BgaExpandConfig {
    const VALUES: [BgaExpandConfig; 3] =
        [BgaExpandConfig::KeepAspect, BgaExpandConfig::Full, BgaExpandConfig::Off];
    cycle_enum(VALUES, current, direction)
}

pub(in crate::app) fn select_option_panel_for_holds(start_held: bool, select_held: bool) -> u8 {
    match (start_held, select_held) {
        (true, true) => 3,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 0,
    }
}

pub(in crate::app) fn select_option_panel_sound_for_transition(
    current_panel: u8,
    next_panel: u8,
) -> Option<crate::system_sound::SoundType> {
    use crate::system_sound::SoundType;

    match (current_panel == 0, next_panel == 0) {
        (true, false) => Some(SoundType::OptionOpen),
        (false, true) => Some(SoundType::OptionClose),
        _ => None,
    }
}

pub(in crate::app) fn select_option_panel_sound_for_scene_transition(
    scene_kind: AppSceneKind,
    current_panel: u8,
    next_panel: u8,
) -> Option<crate::system_sound::SoundType> {
    if scene_kind == AppSceneKind::Select {
        select_option_panel_sound_for_transition(current_panel, next_panel)
    } else {
        None
    }
}

pub(in crate::app) fn transition_select_option_panel(
    current_panel: &mut u8,
    on_started_at: &mut Instant,
    off_started_at: &mut [Option<Instant>; 6],
    next_panel: u8,
    now: Instant,
) -> bool {
    if *current_panel == next_panel {
        return false;
    }
    if let Some(index) = current_panel.checked_sub(1).filter(|index| *index < 6) {
        off_started_at[index as usize] = Some(now);
    }
    if let Some(index) = next_panel.checked_sub(1).filter(|index| *index < 6) {
        off_started_at[index as usize] = None;
    }
    *current_panel = next_panel;
    *on_started_at = now;
    true
}

pub(in crate::app) fn select_hold_state_from_pressed_controls(
    pressed_controls: &HashSet<String>,
    bindings: &SelectKeyBindings,
) -> (bool, bool, HashSet<InputActionConfig>) {
    let start_held = pressed_controls.iter().any(|control| bindings.is_start(control));
    let select_held = pressed_controls
        .iter()
        .any(|control| control == "Select" || bindings.is_e2_action(control));
    let e_action_holds = pressed_controls
        .iter()
        .filter_map(|control| bindings.e_action_for_control(control))
        .collect();
    (start_held, select_held, e_action_holds)
}

pub(in crate::app) fn skin_logical_input_snapshot_from_pressed_controls(
    pressed_controls: &HashSet<String>,
    bindings: &SelectKeyBindings,
) -> SkinLogicalInputSnapshot {
    let mut held = [false; bmz_render::skin::SKIN_BMZ_INPUT_COUNT];
    for control in pressed_controls {
        held[0] |= bindings.is_start(control);
        held[1] |= control == "Select" || bindings.is_e2_action(control);
        match bindings.e_action_for_control(control) {
            Some(InputActionConfig::E1) => held[0] = true,
            Some(InputActionConfig::E2) => held[1] = true,
            Some(InputActionConfig::E3) => held[2] = true,
            Some(InputActionConfig::E4) => held[3] = true,
            _ => {}
        }
        match control.as_str() {
            "ArrowLeft" | "DPadLeft" => held[4] = true,
            "ArrowRight" | "DPadRight" => held[5] = true,
            "ArrowUp" | "DPadUp" => held[6] = true,
            "ArrowDown" | "DPadDown" => held[7] = true,
            _ => {}
        }
    }
    SkinLogicalInputSnapshot { held }
}

pub(in crate::app) fn apply_skin_logical_input_to_scene(
    scene: &mut AppSceneSnapshot,
    skin_input: SkinLogicalInputSnapshot,
) {
    match scene {
        AppSceneSnapshot::Select(snapshot) => snapshot.skin_input = skin_input,
        AppSceneSnapshot::Decide(snapshot) | AppSceneSnapshot::Play(snapshot) => {
            snapshot.skin_input = skin_input;
        }
        AppSceneSnapshot::Result(snapshot) => snapshot.skin_input = skin_input,
    }
}

pub(in crate::app) fn play_control_hold_state_from_pressed_inputs(
    pressed_inputs: &HashSet<(DeviceId, PhysicalControl)>,
    input: &PlayOptionInput,
) -> (bool, bool, bool) {
    let action_held = |action| {
        pressed_inputs.iter().any(|(device, control)| {
            !input.resolves_lane(*device, control) && input.is_action(*device, control, action)
        })
    };
    let e1_held = action_held(InputActionConfig::E1);
    let e2_held = action_held(InputActionConfig::E2);
    let e3_held = action_held(InputActionConfig::E3);
    (e1_held, e2_held, e3_held)
}

pub(in crate::app) fn autoplay_replay_playback_rate_from_pressed_inputs(
    pressed_inputs: &HashSet<(DeviceId, PhysicalControl)>,
    play_input: Option<&PlayOptionInput>,
) -> u16 {
    const SPEED_KEYS: [(&str, u16); 8] = [
        ("1", 25),
        ("Numpad1", 25),
        ("2", 50),
        ("Numpad2", 50),
        ("3", 200),
        ("Numpad3", 200),
        ("4", 300),
        ("Numpad4", 300),
    ];
    SPEED_KEYS
        .into_iter()
        .find_map(|(control, rate)| {
            let control = PhysicalControl::KeyboardKey(control.to_string());
            (pressed_inputs.contains(&(W_KEYBOARD_DEVICE_ID, control.clone()))
                && !play_input.is_some_and(|input| play_input_claims_control(input, &control)))
            .then_some(rate)
        })
        .unwrap_or(100)
}

fn play_input_claims_control(input: &PlayOptionInput, control: &PhysicalControl) -> bool {
    input.resolves_lane(W_KEYBOARD_DEVICE_ID, control)
        || [
            InputActionConfig::E1,
            InputActionConfig::E2,
            InputActionConfig::E3,
            InputActionConfig::E4,
        ]
        .into_iter()
        .any(|action| input.is_action(W_KEYBOARD_DEVICE_ID, control, action))
}

pub(in crate::app) fn is_unassigned_autoplay_replay_playback_rate_key(
    physical_key: PhysicalKey,
    play_input: Option<&PlayOptionInput>,
) -> bool {
    if !is_autoplay_replay_playback_rate_key(physical_key) {
        return false;
    }
    let Some(control) = physical_key_name(physical_key).map(PhysicalControl::KeyboardKey) else {
        return false;
    };
    !play_input.is_some_and(|input| play_input_claims_control(input, &control))
}

pub(in crate::app) fn is_autoplay_replay_playback_rate_key(physical_key: PhysicalKey) -> bool {
    matches!(
        physical_key,
        PhysicalKey::Code(
            KeyCode::Digit1
                | KeyCode::Digit2
                | KeyCode::Digit3
                | KeyCode::Digit4
                | KeyCode::Numpad1
                | KeyCode::Numpad2
                | KeyCode::Numpad3
                | KeyCode::Numpad4
        )
    )
}

pub(in crate::app) fn play_ready_blocked_by_control_holds(e1_held: bool, e2_held: bool) -> bool {
    e1_held || e2_held
}

pub(in crate::app) fn play_ready_blocked_by_recent_control_hold(
    last_control_hold_at: Option<Instant>,
    now: Instant,
) -> bool {
    last_control_hold_at.is_some_and(|last_control_hold_at| {
        now.saturating_duration_since(last_control_hold_at) <= Duration::from_secs(1)
    })
}

pub(in crate::app) fn should_route_quick_retry_input(
    pressed: bool,
    repeat: bool,
    play_ending_active: bool,
) -> bool {
    pressed && !repeat && play_ending_active
}

pub(in crate::app) fn play_exit_should_leave_practice(
    practice_phase: Option<PracticePhase>,
) -> bool {
    practice_phase == Some(PracticePhase::Config)
}

pub(in crate::app) const fn play_exit_chord_pressed(e2_held: bool, e3_held: bool) -> bool {
    e2_held && e3_held
}

pub(in crate::app) fn should_begin_play_fadeout_after_final_notes(
    control: &str,
    bindings: &SelectKeyBindings,
    ready_started: bool,
    play_ending_active: bool,
    play_state: bmz_gameplay::session::PlayState,
    final_notes_processed: bool,
) -> bool {
    ready_started
        && !play_ending_active
        && play_state == bmz_gameplay::session::PlayState::Playing
        && final_notes_processed
        && play_fadeout_after_final_notes_control(control, bindings)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum FinalNotesControlAction {
    ReturnToPractice,
    BeginResultFadeout,
}

pub(in crate::app) fn final_notes_control_action(
    should_begin: bool,
    practice_playing: bool,
) -> Option<FinalNotesControlAction> {
    should_begin.then_some(if practice_playing {
        FinalNotesControlAction::ReturnToPractice
    } else {
        FinalNotesControlAction::BeginResultFadeout
    })
}

#[cfg(test)]
pub(in crate::app) fn should_play_retire_sound_for_failed_transition(
    previous: bmz_gameplay::session::PlayState,
    current: bmz_gameplay::session::PlayState,
) -> bool {
    previous == bmz_gameplay::session::PlayState::Playing
        && current == bmz_gameplay::session::PlayState::Failed
}

pub(in crate::app) fn play_fadeout_after_final_notes_control(
    control: &str,
    bindings: &SelectKeyBindings,
) -> bool {
    bindings.is_start(control) || bindings.is_e2_action(control) || control == "Escape"
}

pub(in crate::app) fn is_select_start_key(
    physical_key: PhysicalKey,
    bindings: &SelectKeyBindings,
) -> bool {
    physical_key_name(physical_key).is_some_and(|control| bindings.is_start(&control))
}

pub(in crate::app) fn is_select_modifier_key(
    physical_key: PhysicalKey,
    bindings: &SelectKeyBindings,
) -> bool {
    physical_key_name(physical_key).is_some_and(|control| bindings.is_e2_action(&control))
}

pub(in crate::app) fn should_toggle_select_gauge_auto_shift(
    control: &str,
    start_held: bool,
    select_held: bool,
    bindings: &SelectKeyBindings,
) -> bool {
    start_held && (select_held || bindings.is_e2_action(control)) && bindings.is_ui_key2(control)
}

pub(in crate::app) fn should_toggle_select_judge_auto_adjust(
    control: &str,
    start_held: bool,
    select_held: bool,
    bindings: &SelectKeyBindings,
) -> bool {
    start_held && (select_held || bindings.is_e2_action(control)) && bindings.is_ui_key3(control)
}
use super::*;
