use super::*;

#[test]
fn double_battle_uses_dp_settings_and_independent_keyboard_and_controller_lanes() {
    use crate::config::play_input::{gamepad_play_binding_for_device, play_binding};
    use crate::config::profile_config::{LaneConfig, PlayModeInputConfig};

    for (sp, dp) in [(KeyMode::K5, KeyMode::K10), (KeyMode::K7, KeyMode::K14)] {
        for double in [DoubleOption::Battle, DoubleOption::BattleAutoScratch] {
            let mut profile = ProfileConfig::new_default("default", "Default", 1);
            profile.normalize_play_mode_configs();
            profile.activate_play_mode(sp);
            profile.lane.hispeed = 2.75;
            profile.lane.target_green_number = 285;
            profile.sync_active_play_mode();
            profile.activate_play_mode(dp);
            profile.lane.hispeed = 4.5;
            profile.lane.target_green_number = 220;
            profile.play.lane_effect = LaneEffectConfig::Sudden;
            profile.lane.sudden = 250;
            profile.lane.lift_enabled = true;
            profile.lane.lift = 100;
            profile.judge.visual_offset_us = -12_000;
            profile.sync_active_play_mode();
            for mode in [sp, dp] {
                profile.input.play.insert(
                    mode.play_map_key().to_string(),
                    PlayModeInputConfig {
                        inherit: None,
                        bindings: vec![
                            play_binding("Z", LaneConfig::Key1),
                            play_binding(
                                "M",
                                if mode == sp { LaneConfig::Key1 } else { LaneConfig::Key8 },
                            ),
                            gamepad_play_binding_for_device(
                                "gamepad1",
                                "Button1",
                                LaneConfig::Key1,
                            ),
                            gamepad_play_binding_for_device(
                                "gamepad2",
                                "Button1",
                                if mode == sp { LaneConfig::Key1 } else { LaneConfig::Key8 },
                            ),
                        ],
                        ..Default::default()
                    },
                );
            }
            let mut backend = BufferedInputBackend::default();
            for (device, control) in [
                (DeviceId(1), PhysicalControl::KeyboardKey("Z".into())),
                (DeviceId(1), PhysicalControl::KeyboardKey("M".into())),
                (DeviceId(11), PhysicalControl::GamepadButton("Button1".into())),
                (DeviceId(22), PhysicalControl::GamepadButton("Button1".into())),
            ] {
                backend.push(DeviceInputEvent {
                    device,
                    control,
                    kind: InputKind::Press,
                    timestamp: DeviceTimestamp::Unknown,
                    bounce_policy: Default::default(),
                });
            }
            let mut source = chart();
            source.metadata.key_mode = sp;
            source.total_notes = 1;
            source.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
            apply_double_option(&mut source, double);
            let options = PlaySessionOptions {
                double_option: double,
                gamepad_slots: GamepadSlotMap::from_device_ids([
                    Some(DeviceId(11)),
                    Some(DeviceId(22)),
                ]),
                ..Default::default()
            };
            let mut placeholder = bmz_render::snapshot::RenderSnapshot::default();
            apply_placeholder_session_visuals(&mut placeholder, &profile, dp, &options);
            let mut session = build_game_session_with_input_backend(
                Arc::new(source),
                &profile,
                options,
                Box::new(backend),
            );
            assert_eq!(session.play_config_key_mode, dp);
            assert_eq!(session.primary_key_mode, dp);
            assert_eq!(session.hispeed, 4.5);
            assert_eq!(session.target_green_number, 220);
            assert_eq!(session.offsets.visual_offset_us, -12_000);
            assert_eq!(placeholder.hispeed, session.hispeed);
            assert_eq!(placeholder.lane_cover, session.lane_cover);
            assert_eq!(placeholder.lift, session.lift);
            assert_eq!(session.lane_cover, 0.25);
            assert_eq!(session.lift, 0.1);
            assert_eq!(session.scored_total_notes, 2);
            assert!(session.display_only_lane_mask.iter().all(|v| !v));
            let ctx = InputTimingContext {
                audio_clock: &session.audio_clock,
                offsets: session.offsets,
                timestamp_anchor: None,
            };
            let inputs = session.input_system.collect_game_inputs(&ctx);
            assert_eq!(
                inputs.iter().map(|i| i.lane).collect::<Vec<_>>(),
                vec![Lane::Key1, Lane::Key8, Lane::Key1, Lane::Key8]
            );
        }
    }
}

#[test]
fn double_battle_as_replay_leaves_both_scratches_unscored_on_the_replay_clock() {
    use bmz_core::replay::ReplayEvent;
    for sp in [KeyMode::K5, KeyMode::K7] {
        for viewing_rate in [25, 100, 300] {
            let mut source = chart();
            source.metadata.key_mode = sp;
            source.total_notes = 2;
            source.end_time = TimeUs(1_500_000);
            source.lane_notes[Lane::Key1.index()].push(note(1, Lane::Key1, 1_000_000));
            source.lane_notes[Lane::Scratch.index()].push(note(2, Lane::Scratch, 1_100_000));
            apply_double_option(&mut source, DoubleOption::BattleAutoScratch);
            let events = [Lane::Key1, Lane::Key8].map(|lane| ReplayEvent {
                applied_after: None,
                lane,
                kind: InputKind::Press,
                time: TimeUs(1_000_000),
                device_kind: bmz_core::input::InputDeviceKind::Keyboard,
                scratch_direction: None,
            });
            let mut session = build_game_session(
                Arc::new(source),
                &ProfileConfig::new_default("default", "Default", 1),
                PlaySessionOptions {
                    double_option: DoubleOption::BattleAutoScratch,
                    replay_player: Some(ReplayPlayer::new(events.to_vec())),
                    ..Default::default()
                },
            );
            session.audio_clock.set_playback_rate_percent(viewing_rate);
            let mut audio = bmz_audio::queue::ScheduledSoundQueue::new();
            // A delayed frame crosses both key inputs and both scratch notes.
            session.audio_clock.pause_at(TimeUs(1_400_000));
            bmz_gameplay::session::advance_session_frame(&mut session, &mut audio);
            // beatoraja moves BATTLE AS scratches to the background: no judge, combo or miss.
            assert_eq!(session.scored_total_notes, 2, "{sp:?} {viewing_rate}");
            assert_eq!(session.score.ex_score(), 4, "{sp:?} {viewing_rate}");
            assert_eq!(session.score.cb(), 0);
            assert_eq!(session.score.max_combo, 2);
        }
    }
}
