// 通常のアナログカバー操作は1 tick = 0.001。全カバー無効時は
// EndlessDreamと同じ1 tick = HS 0.01へ変換する。
const ANALOG_HISPEED_PER_LANE_COVER: f32 = 10.0;

pub(in crate::app) fn apply_pending_play_lane_action_to_state(
    lane: &mut PendingPlayLaneState,
    action: PlayLaneAction,
    profile: &ProfileConfig,
    now_bpm: f32,
    speed_locked: bool,
) -> bool {
    if speed_locked {
        return false;
    }
    match action {
        PlayLaneAction::ToggleHispeedMode => {
            if lane.lift_enabled && lane.hidden_enabled {
                lane.lane_target = lane.lane_target.toggled_lift_hidden();
            } else {
                if lane.floating_policy != FloatingPolicy::Toggle {
                    return false;
                }
                match lane.hispeed_mode {
                    HispeedMode::Normal | HispeedMode::Classic => {
                        lane.target_green_number = lane.current_green_number(now_bpm);
                        lane.hispeed_mode = HispeedMode::Floating;
                    }
                    HispeedMode::Floating => {
                        lane.hispeed = clamp_hispeed(lane.hispeed);
                        lane.hispeed_mode = lane.base_hispeed_mode;
                        if lane.hispeed_mode == HispeedMode::Normal {
                            lane.normal_hispeed_level =
                                crate::config::play::normal_hispeed_level_for_green_number(
                                    lane.current_full_lane_green_number(now_bpm),
                                );
                            lane.refresh_normal_hispeed(now_bpm, false);
                        }
                    }
                }
            }
        }
        PlayLaneAction::Hispeed(change) => {
            if lane.hispeed_mode == HispeedMode::Normal {
                lane.normal_hispeed_level =
                    adjusted_normal_hispeed_level(lane.normal_hispeed_level, change);
                lane.refresh_normal_hispeed(now_bpm, false);
            } else {
                let step = hispeed_step_for_profile(profile, lane.hispeed_mode);
                lane.hispeed = adjusted_hispeed(lane.hispeed, change, step);
            }
        }
        PlayLaneAction::LaneCoverDelta(delta) => {
            apply_pending_lane_cover_delta(lane, profile, now_bpm, delta, false)
        }
        PlayLaneAction::AnalogLaneCoverDelta(delta) => {
            apply_pending_lane_cover_delta(lane, profile, now_bpm, delta, true)
        }
        PlayLaneAction::GreenNumberDelta(delta) => {
            if lane.floating_policy == FloatingPolicy::Disabled {
                return false;
            }
            let current = match lane.hispeed_mode {
                HispeedMode::Normal | HispeedMode::Classic => lane.current_green_number(now_bpm),
                HispeedMode::Floating => lane.target_green_number,
            };
            lane.target_green_number = adjusted_green_number(current, delta);
            lane.hispeed_mode = HispeedMode::Floating;
            lane.refresh_floating_hispeed(now_bpm, false);
        }
        PlayLaneAction::ToggleLaneCoverVisibility => {
            if !lane.sudden_enabled {
                return false;
            }
            let was_visible = lane.lane_cover_visible;
            lane.lane_cover_visible = !lane.lane_cover_visible;
            if !was_visible && lane.lane_cover_visible {
                lane.refresh_cover_hispeed(now_bpm, false);
            }
        }
        PlayLaneAction::VisualOffsetDelta(_) | PlayLaneAction::ToggleVisualOffsetAutoAdjust => {
            return false;
        }
    }
    true
}

fn apply_pending_lane_cover_delta(
    lane: &mut PendingPlayLaneState,
    profile: &ProfileConfig,
    now_bpm: f32,
    delta: f32,
    analog: bool,
) {
    let Some(target) = resolved_play_lane_target(
        lane.sudden_enabled,
        lane.lane_cover_visible,
        lane.lift_enabled,
        lane.hidden_enabled,
        lane.lane_target,
    ) else {
        if lane.hispeed_auto_adjust {
            lane.refresh_floating_hispeed_without_lane_effects(now_bpm, false);
        } else if analog {
            lane.hispeed = clamp_hispeed(lane.hispeed + delta * ANALOG_HISPEED_PER_LANE_COVER);
        } else {
            let change = if delta >= 0.0 { HispeedChange::Up } else { HispeedChange::Down };
            lane.hispeed = adjusted_hispeed(
                lane.hispeed,
                change,
                hispeed_step_for_profile(profile, lane.hispeed_mode),
            );
        }
        return;
    };
    match target {
        PlayLaneTarget::Sudden => {
            lane.lane_cover = (lane.lane_cover - delta)
                .clamp(0.0, crate::config::play::lane_cover_max_for_lift(lane.lift));
            lane.refresh_cover_hispeed(now_bpm, false);
        }
        PlayLaneTarget::Lift => {
            lane.lift = (lane.lift + delta).clamp(0.0, (1.0 - lane.lane_cover).clamp(0.0, 1.0));
            if lane.hispeed_auto_adjust {
                lane.refresh_floating_hispeed(now_bpm, false);
            }
        }
        PlayLaneTarget::Hidden => {
            lane.hidden_cover = (lane.hidden_cover + delta).clamp(0.0, 1.0);
        }
    }
}

pub(in crate::app) fn sync_active_play_visual_offset_to_profile(
    profile: &mut ProfileConfig,
    visual_offset_us: i64,
    auto_adjust_active: bool,
) {
    if !auto_adjust_active || profile.judge.visual_offset_us == visual_offset_us {
        return;
    }
    profile.judge.visual_offset_us = visual_offset_us;
    profile.updated_at = now_unix_seconds();
}

pub(in crate::app) fn apply_hispeed_change_to_session(
    session: &mut bmz_gameplay::session::GameSession,
    change: HispeedChange,
    step: f32,
) {
    session.hispeed = adjusted_hispeed(session.hispeed, change, step);
}

pub(in crate::app) fn apply_play_lane_action_to_session(
    session: &mut bmz_gameplay::session::GameSession,
    lane_target: &mut PlayLaneTarget,
    action: PlayLaneAction,
    speed_locked: bool,
    hispeed_step: f32,
) -> bool {
    if speed_locked {
        return false;
    }
    match action {
        PlayLaneAction::ToggleHispeedMode => {
            if session.lift_enabled && session.hidden_enabled {
                *lane_target = lane_target.toggled_lift_hidden();
                return true;
            }
            if session.floating_policy != FloatingPolicy::Toggle {
                return false;
            }
            match session.hispeed_mode {
                HispeedMode::Normal | HispeedMode::Classic => {
                    let now = session.audio_clock.now();
                    session.target_green_number = current_green_number(session, now);
                    session.hispeed_mode = HispeedMode::Floating;
                }
                HispeedMode::Floating => {
                    session.hispeed = clamp_hispeed(session.hispeed);
                    session.hispeed_mode = session.base_hispeed_mode;
                    if session.hispeed_mode == HispeedMode::Normal {
                        let now = session.audio_clock.now();
                        session.normal_hispeed_level =
                            crate::config::play::normal_hispeed_level_for_green_number(
                                current_full_lane_green_number(session, now),
                            );
                        session.hispeed =
                            hispeed_for_normal_level(session, session.normal_hispeed_level, now);
                    }
                }
            }
            true
        }
        PlayLaneAction::Hispeed(change) => {
            if session.hispeed_mode == HispeedMode::Normal {
                session.normal_hispeed_level =
                    adjusted_normal_hispeed_level(session.normal_hispeed_level, change);
                let now = session.audio_clock.now();
                session.hispeed =
                    hispeed_for_normal_level(session, session.normal_hispeed_level, now);
            } else {
                apply_hispeed_change_to_session(session, change, hispeed_step);
            }
            true
        }
        PlayLaneAction::LaneCoverDelta(delta) => {
            apply_lane_cover_action_to_session(session, lane_target, delta, false, hispeed_step)
        }
        PlayLaneAction::AnalogLaneCoverDelta(delta) => {
            apply_lane_cover_action_to_session(session, lane_target, delta, true, hispeed_step)
        }
        PlayLaneAction::GreenNumberDelta(delta) => {
            apply_green_number_step_to_session(session, delta, false)
        }
        PlayLaneAction::ToggleLaneCoverVisibility => toggle_lane_cover_visibility(session, false),
        PlayLaneAction::VisualOffsetDelta(_) | PlayLaneAction::ToggleVisualOffsetAutoAdjust => {
            false
        }
    }
}

fn apply_lane_cover_action_to_session(
    session: &mut bmz_gameplay::session::GameSession,
    lane_target: &mut PlayLaneTarget,
    delta: f32,
    analog: bool,
    hispeed_step: f32,
) -> bool {
    if resolved_play_lane_target(
        session.lanecover_enabled,
        session.lane_cover_visible,
        session.lift_enabled,
        session.hidden_enabled,
        *lane_target,
    )
    .is_some()
    {
        return apply_lane_cover_step_to_session(session, lane_target, delta, false);
    }

    if session.hispeed_auto_adjust {
        if session.hispeed_mode == HispeedMode::Floating {
            let now = session.audio_clock.now();
            let now_bpm = session.timing_map.bpm_at_time(now);
            session.hispeed =
                hispeed_for_green_number_without_lane_effects_at_bpm(session, now, now_bpm);
        }
    } else if analog {
        session.hispeed = clamp_hispeed(session.hispeed + delta * ANALOG_HISPEED_PER_LANE_COVER);
    } else {
        let change = if delta >= 0.0 { HispeedChange::Up } else { HispeedChange::Down };
        apply_hispeed_change_to_session(session, change, hispeed_step);
    }
    true
}

#[cfg(test)]
pub(in crate::app) fn apply_play_option_control_to_session(
    session: &mut bmz_gameplay::session::GameSession,
    action: PlayOptionControl,
    speed_locked: bool,
    hispeed_step: f32,
) -> bool {
    let mut lane_target = PlayLaneTarget::Lift;
    apply_play_lane_action_to_session(
        session,
        &mut lane_target,
        lane_action_from_option(action, false).expect("button option always maps to a lane action"),
        speed_locked,
        hispeed_step,
    )
}

pub(in crate::app) fn replay_pending_play_lane_actions(
    session: &mut bmz_gameplay::session::GameSession,
    lane_target: &mut PlayLaneTarget,
    actions: &[PlayLaneAction],
    profile: &ProfileConfig,
    speed_locked: bool,
) {
    for &action in actions {
        let step = hispeed_step_for_profile(profile, session.hispeed_mode);
        let _ = apply_play_lane_action_to_session(session, lane_target, action, speed_locked, step);
    }
}

pub(in crate::app) fn handoff_pending_play_visual_input(
    session: &mut bmz_gameplay::session::GameSession,
    input: &SharedInputBackend,
    visual_input: &PendingPlayVisualInput,
) {
    let mut input = input.clone();
    let _ = input.drain_events();
    visual_input.clone().apply_to_session(session);
}

pub(in crate::app) fn apply_green_number_step_to_session(
    session: &mut bmz_gameplay::session::GameSession,
    delta: i32,
    speed_locked: bool,
) -> bool {
    if speed_locked {
        return false;
    }
    if session.floating_policy == FloatingPolicy::Disabled {
        return false;
    }
    let current = match session.hispeed_mode {
        HispeedMode::Normal | HispeedMode::Classic => {
            current_green_number(session, session.audio_clock.now())
        }
        HispeedMode::Floating => session.target_green_number,
    };
    session.target_green_number = adjusted_green_number(current, delta);
    session.hispeed_mode = HispeedMode::Floating;
    let now = session.audio_clock.now();
    session.hispeed =
        hispeed_for_green_number(session, active_lane_cover_for_hispeed(session), now);
    true
}

pub(in crate::app) fn apply_lane_cover_step_to_session(
    session: &mut bmz_gameplay::session::GameSession,
    lane_target: &mut PlayLaneTarget,
    delta: f32,
    speed_locked: bool,
) -> bool {
    if speed_locked {
        return false;
    }
    let Some(target) = resolved_play_lane_target(
        session.lanecover_enabled,
        session.lane_cover_visible,
        session.lift_enabled,
        session.hidden_enabled,
        *lane_target,
    ) else {
        return false;
    };
    match target {
        PlayLaneTarget::Sudden => {
            session.lane_cover = (session.lane_cover - delta)
                .clamp(0.0, crate::config::play::lane_cover_max_for_lift(session.lift));
            if session.hispeed_mode == HispeedMode::Floating {
                let now = session.audio_clock.now();
                session.hispeed = if session.hispeed_auto_adjust {
                    hispeed_for_green_number(session, session.lane_cover, now)
                } else {
                    hispeed_for_green_number_at_hsfix_bpm(session, session.lane_cover, now)
                };
            }
        }
        PlayLaneTarget::Lift => {
            session.lift =
                (session.lift + delta).clamp(0.0, (1.0 - session.lane_cover).clamp(0.0, 1.0));
            if session.hispeed_auto_adjust && session.hispeed_mode == HispeedMode::Floating {
                let now = session.audio_clock.now();
                session.hispeed = hispeed_for_green_number(session, 0.0, now);
            }
        }
        PlayLaneTarget::Hidden => {
            session.hidden_cover = (session.hidden_cover + delta).clamp(0.0, 1.0);
        }
    }
    true
}

pub(in crate::app) fn reset_floating_hispeed_if_enabled(
    session: &mut bmz_gameplay::session::GameSession,
    speed_locked: bool,
) {
    if session.hispeed_mode == HispeedMode::Floating && !speed_locked {
        let now = session.audio_clock.now();
        let lane_cover = active_lane_cover_for_hispeed(session);
        session.hispeed = if session.hispeed_auto_adjust {
            hispeed_for_green_number(session, lane_cover, now)
        } else {
            hispeed_for_green_number_at_hsfix_bpm(session, lane_cover, now)
        };
    }
}

/// Start / E1 の連続押し間隔を判定する。2回目なら true を返しタイムスタンプをクリアする。
pub(in crate::app) fn register_play_start_double_press(
    last_press_at: &mut Option<Instant>,
    now: Instant,
) -> bool {
    let is_double = last_press_at
        .is_some_and(|prev| now.duration_since(prev) <= PLAY_START_DOUBLE_PRESS_WINDOW);
    if is_double {
        *last_press_at = None;
        true
    } else {
        *last_press_at = Some(now);
        false
    }
}

pub(in crate::app) fn toggle_lane_cover_visibility(
    session: &mut bmz_gameplay::session::GameSession,
    speed_locked: bool,
) -> bool {
    if speed_locked || !session.lanecover_enabled {
        return false;
    }
    let was_visible = session.lane_cover_visible;
    session.lane_cover_visible = !session.lane_cover_visible;
    if !was_visible && session.lane_cover_visible {
        reset_floating_hispeed_if_enabled(session, speed_locked);
    }
    true
}
use super::*;
