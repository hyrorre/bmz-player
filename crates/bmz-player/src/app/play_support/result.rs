#[cfg(test)]
pub(in crate::app) fn result_action(
    physical_key: PhysicalKey,
    state: ElementState,
    repeat: bool,
) -> Option<ResultAction> {
    scene_result_action(&ControlInputEvent::keyboard_parts(physical_key, state, repeat))
}

pub(in crate::app) fn result_exit_skip_key(
    physical_key: PhysicalKey,
    state: ElementState,
    repeat: bool,
) -> bool {
    if state != ElementState::Pressed || repeat {
        return false;
    }
    matches!(physical_key, PhysicalKey::Code(KeyCode::Enter | KeyCode::Escape))
}

pub(in crate::app) fn result_exit_transition_ready(
    elapsed: Duration,
    fadeout: Duration,
    animation_duration: Duration,
    skip_requested: bool,
    skip_final_frame_held: bool,
) -> bool {
    let required_duration = if skip_requested { animation_duration } else { fadeout };
    elapsed >= required_duration && (!skip_requested || skip_final_frame_held)
}

#[cfg(test)]
pub(in crate::app) fn decide_control_action(
    control: &str,
    bindings: &SelectKeyBindings,
) -> Option<DecideAction> {
    scene_decide_action(&ControlInputEvent::gamepad(DeviceId(1), control, true), bindings)
}

pub(in crate::app) fn decide_cancel_chord_pressed(
    e1_held: bool,
    e2_held: bool,
    e3_held: bool,
) -> bool {
    e2_held && (e1_held || e3_held)
}

pub(in crate::app) fn elapsed_since(started_at: Instant) -> TimeUs {
    TimeUs(started_at.elapsed().as_micros().min(i64::MAX as u128) as i64)
}

pub(in crate::app) fn elapsed_since_ms(started_at: Instant) -> i32 {
    (started_at.elapsed().as_millis().min(i32::MAX as u128)) as i32
}

pub(in crate::app) fn apply_operating_time_ms_to_scene(
    scene: &mut AppSceneSnapshot,
    operating_time_ms: i32,
) {
    match scene {
        AppSceneSnapshot::Select(snapshot) => {
            snapshot.operating_time_ms = operating_time_ms;
        }
        AppSceneSnapshot::Decide(snapshot) | AppSceneSnapshot::Play(snapshot) => {
            snapshot.operating_time_ms = operating_time_ms;
        }
        AppSceneSnapshot::Result(snapshot) => {
            snapshot.operating_time_ms = operating_time_ms;
        }
    }
}

pub(in crate::app) fn apply_skin_runtime_info_to_scene(
    scene: &mut AppSceneSnapshot,
    player_name: &str,
    current_fps: u32,
) {
    match scene {
        AppSceneSnapshot::Select(snapshot) => {
            snapshot.player_name = player_name.to_string();
            snapshot.current_fps = current_fps;
        }
        AppSceneSnapshot::Decide(snapshot) | AppSceneSnapshot::Play(snapshot) => {
            snapshot.player_name = player_name.to_string();
            snapshot.current_fps = current_fps;
        }
        AppSceneSnapshot::Result(snapshot) => {
            snapshot.player_name = player_name.to_string();
            snapshot.current_fps = current_fps;
        }
    }
}

pub(in crate::app) fn preloaded_matches_start(
    preloaded: &PreloadedInputPlaySession,
    chart_id: i64,
    options: &PlayStartOptions,
) -> bool {
    preloaded.chart_id == chart_id
        && preloaded.session_options.session_mode == options.session_mode
        && preloaded.session_options.autoplay == options.autoplay
        && preloaded.session_options.key_mode_conversion == options.key_mode_conversion
        && preloaded.session_options.seven_to_nine_pattern == options.seven_to_nine_pattern
        && preloaded.session_options.seven_to_nine_type == options.seven_to_nine_type
        && preloaded.session_options.seven_to_nine_rule_mode == options.seven_to_nine_rule_mode
        && preloaded.session_options.score_save_disabled == options.score_save_disabled
        && preloaded.session_options.arrange == options.arrange
        && preloaded.session_options.arrange_2p == options.arrange_2p
        && preloaded.session_options.double_option == options.double_option
        && preloaded.session_options.hs_fix == options.hs_fix
        && preloaded.session_options.arrange_seed == options.arrange_seed
        && preloaded.session_options.arrange_seed_2p == options.arrange_seed_2p
        && preloaded.session_options.random_trainer_seed == options.random_trainer_seed
        && preloaded.session_options.legacy_arrange_seed == options.legacy_arrange_seed
        && preloaded.session_options.s_random_scheme == options.s_random_scheme
        && preloaded.session_options.s_random_scheme_2p == options.s_random_scheme_2p
        && preloaded.session_options.bms_random_seed == options.bms_random_seed
        && preloaded.session_options.bms_random_choices == options.bms_random_choices
        && preloaded.session_options.bms_switch_choices == options.bms_switch_choices
        && preloaded.session_options.arrange_pattern == options.arrange_pattern
        && preloaded.session_options.initial_gauge_value == options.initial_gauge_value
        && preloaded.session_options.initial_gauge_values == options.initial_gauge_values
        && preloaded.session_options.initial_course_combo == options.initial_course_combo
}
use super::*;
