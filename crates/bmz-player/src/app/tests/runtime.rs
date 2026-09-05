use super::*;
use crate::app::input_lifecycle::{
    gamepad_runtime_config_changed, keyboard_runtime_config_changed,
};
use crate::app::lifecycle::clear_window_cursor_state;

#[test]
fn winit_app_stack_size_stays_bounded() {
    let size = std::mem::size_of::<WinitApp>();
    assert!(size < 64 * 1024, "WinitApp is {size} bytes");
}

#[test]
fn cursor_leave_clears_hover_position_and_slider_drag() {
    let mut position = Some(PhysicalPosition::new(320.0, 180.0));
    let mut dragging_slider_type = Some(17);

    clear_window_cursor_state(&mut position, &mut dragging_slider_type);

    assert_eq!(position, None);
    assert_eq!(dragging_slider_type, None);
}

#[test]
fn gpu_upload_channels_apply_backpressure_at_the_configured_capacity() {
    let (bga_tx, _bga_rx) = bounded_gpu_upload_channel::<u8>(MAX_PENDING_BGA_TEXTURE_UPLOADS);
    for value in 0..MAX_PENDING_BGA_TEXTURE_UPLOADS {
        bga_tx.try_send(value as u8).expect("BGA queue should accept its capacity");
    }
    assert!(matches!(bga_tx.try_send(255), Err(mpsc::TrySendError::Full(255))));

    let (skin_tx, _skin_rx) = bounded_gpu_upload_channel::<u8>(MAX_PENDING_SKIN_UPLOADS);
    for value in 0..MAX_PENDING_SKIN_UPLOADS {
        skin_tx.try_send(value as u8).expect("skin queue should accept its capacity");
    }
    assert!(matches!(skin_tx.try_send(255), Err(mpsc::TrySendError::Full(255))));
}

#[test]
fn config_present_mode_maps_vsync_modes() {
    let mut config = AppConfig::default().video;

    config.vsync_mode = VsyncModeConfig::Vsync;
    assert_eq!(config_present_mode(&config), bmz_render::WgpuPresentMode::Fifo);

    config.vsync_mode = VsyncModeConfig::AdaptiveVsync;
    assert_eq!(config_present_mode(&config), bmz_render::WgpuPresentMode::FifoRelaxed);

    config.vsync_mode = VsyncModeConfig::VsyncOff;
    assert_eq!(config_present_mode(&config), bmz_render::WgpuPresentMode::Immediate);

    config.vsync_mode = VsyncModeConfig::FastVsync;
    assert_eq!(config_present_mode(&config), bmz_render::WgpuPresentMode::Mailbox);
}

#[test]
fn config_frame_latency_mode_maps_video_setting() {
    let mut config = AppConfig::default().video;

    config.frame_latency_mode = FrameLatencyModeConfig::Auto;
    assert_eq!(config_frame_latency_mode(&config), bmz_render::WgpuFrameLatencyMode::Auto);

    config.frame_latency_mode = FrameLatencyModeConfig::LowLatency;
    assert_eq!(config_frame_latency_mode(&config), bmz_render::WgpuFrameLatencyMode::LowLatency);

    config.frame_latency_mode = FrameLatencyModeConfig::Stable;
    assert_eq!(config_frame_latency_mode(&config), bmz_render::WgpuFrameLatencyMode::Stable);
}

#[test]
fn config_internal_resolution_mode_maps_video_setting() {
    let mut config = AppConfig::default().video;

    config.internal_resolution = InternalResolutionModeConfig::Native;
    assert_eq!(
        config_internal_resolution_mode(&config),
        bmz_render::InternalResolutionMode::Native
    );

    config.internal_resolution = InternalResolutionModeConfig::Skin;
    assert_eq!(config_internal_resolution_mode(&config), bmz_render::InternalResolutionMode::Skin);
}

#[test]
fn macos_window_focus_uses_native_state() {
    assert!(resolve_window_focus_update(true, false, true, true).effective_focused);
    assert!(!resolve_window_focus_update(true, false, false, true).effective_focused);
    assert!(resolve_window_focus_update(false, true, true, true).effective_focused);
}

#[test]
fn non_macos_window_focus_preserves_event_state() {
    assert!(!resolve_window_focus_update(true, false, true, false).effective_focused);
    assert!(resolve_window_focus_update(false, true, false, false).effective_focused);
}

fn video_mode(width: u32, height: u32, refresh_hz: u32) -> VideoModeSpec {
    VideoModeSpec { width, height, refresh_millihertz: refresh_hz * 1_000, bit_depth: 32 }
}

#[test]
fn exclusive_video_mode_keeps_configured_resolution_before_refresh_rate() {
    let modes = [video_mode(3840, 2160, 160), video_mode(1920, 1080, 240)];
    let selected = select_exclusive_video_mode(&modes, PhysicalSize::new(1920, 1080), 240).unwrap();
    assert_eq!(selected.index, 1);
    assert_eq!(selected.resolution_reason, VideoModeResolutionReason::Configured);
}

#[test]
fn linux_legacy_exclusive_video_mode_preserves_largest_resolution_policy() {
    let modes = [video_mode(3840, 2160, 160), video_mode(1920, 1080, 240)];
    let selected = select_platform_exclusive_video_mode(
        &modes,
        PhysicalSize::new(1920, 1080),
        240,
        ExclusiveVideoModePolicy::LegacyLargestResolution,
    )
    .unwrap();
    assert_eq!(selected.index, 0);
    assert_eq!(selected.resolution_reason, VideoModeResolutionReason::LegacyLargest);
    assert_eq!(selected.refresh_reason, VideoModeRefreshReason::LegacyHighestAtLargestResolution);
}

#[test]
fn windows_policy_selects_configured_resolution_and_target_refresh_rate() {
    let modes = [
        video_mode(1920, 1080, 144),
        video_mode(1920, 1080, 120),
        video_mode(1280, 720, 144),
        video_mode(1280, 720, 120),
        video_mode(1280, 720, 60),
    ];
    let selected = select_platform_exclusive_video_mode(
        &modes,
        PhysicalSize::new(1280, 720),
        120,
        ExclusiveVideoModePolicy::ConfiguredResolution,
    )
    .unwrap();
    assert_eq!(selected.index, 3);
    assert_eq!(selected.resolution_reason, VideoModeResolutionReason::Configured);
    assert_eq!(selected.refresh_reason, VideoModeRefreshReason::ClosestAtOrAbove);
}

#[test]
fn exclusive_video_mode_selects_refresh_rate_for_target() {
    let modes = [
        video_mode(1920, 1080, 60),
        video_mode(1920, 1080, 120),
        video_mode(1920, 1080, 160),
        video_mode(1920, 1080, 240),
    ];
    assert_eq!(
        select_exclusive_video_mode(&modes, PhysicalSize::new(1920, 1080), 240).unwrap().index,
        3
    );

    let below_target = &modes[..3];
    let selected =
        select_exclusive_video_mode(below_target, PhysicalSize::new(1920, 1080), 240).unwrap();
    assert_eq!(selected.index, 2);
    assert_eq!(selected.refresh_reason, VideoModeRefreshReason::HighestBelow);
}

#[test]
fn exclusive_video_mode_prefers_rate_at_or_above_target() {
    let modes = [video_mode(1920, 1080, 144), video_mode(1920, 1080, 120)];
    let selected = select_exclusive_video_mode(&modes, PhysicalSize::new(1920, 1080), 100).unwrap();
    assert_eq!(selected.index, 1);
    assert_eq!(selected.refresh_reason, VideoModeRefreshReason::ClosestAtOrAbove);
}

#[test]
fn exclusive_video_mode_uses_highest_rate_when_all_candidates_are_below_target() {
    let modes =
        [video_mode(1920, 1080, 60), video_mode(1920, 1080, 120), video_mode(1920, 1080, 144)];
    let selected = select_exclusive_video_mode(&modes, PhysicalSize::new(1920, 1080), 165).unwrap();
    assert_eq!(selected.index, 2);
    assert_eq!(selected.refresh_reason, VideoModeRefreshReason::HighestBelow);
}

#[test]
fn exclusive_video_mode_uses_highest_rate_for_unlimited_target() {
    let modes = [video_mode(1920, 1080, 60), video_mode(1920, 1080, 240)];
    let selected = select_exclusive_video_mode(&modes, PhysicalSize::new(1920, 1080), 0).unwrap();
    assert_eq!(selected.index, 1);
    assert_eq!(selected.refresh_reason, VideoModeRefreshReason::HighestUnlimited);
}

#[test]
fn exclusive_video_mode_uses_explicit_closest_resolution_fallback() {
    let modes = [video_mode(1280, 720, 240), video_mode(2560, 1440, 240)];
    let selected = select_exclusive_video_mode(&modes, PhysicalSize::new(1920, 1080), 240).unwrap();
    assert_eq!(selected.index, 0);
    assert_eq!(selected.resolution_reason, VideoModeResolutionReason::ClosestSupported);
}

#[test]
fn surface_attach_fallback_retries_only_first_exclusive_failure_on_windows() {
    assert_eq!(
        surface_attach_fallback_mode(
            &WindowMode::ExclusiveFullscreen,
            &WindowMode::ExclusiveFullscreen,
            false,
            true,
        ),
        Some(WindowMode::BorderlessFullscreen)
    );
    assert_eq!(
        surface_attach_fallback_mode(&WindowMode::Windowed, &WindowMode::Windowed, false, true,),
        None
    );
    assert_eq!(
        surface_attach_fallback_mode(
            &WindowMode::BorderlessFullscreen,
            &WindowMode::BorderlessFullscreen,
            false,
            true,
        ),
        None
    );
    assert_eq!(
        surface_attach_fallback_mode(
            &WindowMode::ExclusiveFullscreen,
            &WindowMode::BorderlessFullscreen,
            true,
            true,
        ),
        None
    );
    assert_eq!(
        surface_attach_fallback_mode(
            &WindowMode::ExclusiveFullscreen,
            &WindowMode::ExclusiveFullscreen,
            false,
            false,
        ),
        None
    );
}

#[test]
fn focus_release_runs_only_on_effective_true_to_false_transition() {
    let first_loss = resolve_window_focus_update(true, false, false, true);
    assert!(first_loss.focus_lost);

    let repeated_loss =
        resolve_window_focus_update(first_loss.effective_focused, false, false, true);
    assert!(!repeated_loss.focus_lost);

    let stale_false = resolve_window_focus_update(true, false, true, true);
    assert!(!stale_false.focus_lost);
}

#[test]
fn keyboard_input_backend_uses_raw_input_on_windows_auto() {
    let mut config = AppConfig::default();
    config.input.backend = InputBackendKind::Auto;
    let expected_auto = if cfg!(target_os = "windows") {
        KeyboardInputBackend::RawInput
    } else {
        KeyboardInputBackend::Window
    };
    assert_eq!(keyboard_input_backend_for_config(&config), Some(expected_auto));

    config.input.backend = InputBackendKind::Winit;
    assert_eq!(keyboard_input_backend_for_config(&config), Some(KeyboardInputBackend::Window));

    config.input.keyboard_enabled = false;
    assert_eq!(keyboard_input_backend_for_config(&config), None);
}

#[test]
fn runtime_input_change_detection_separates_keyboard_and_gamepad() {
    let before = AppConfig::default().input;
    let mut after = before.clone();
    assert!(!keyboard_runtime_config_changed(&before, &after));
    assert!(!gamepad_runtime_config_changed(&before, &after));

    after.backend = InputBackendKind::RawInput;
    assert!(keyboard_runtime_config_changed(&before, &after));
    assert!(!gamepad_runtime_config_changed(&before, &after));

    after = before.clone();
    after.gamepad_backend = GamepadBackendKind::RawInput;
    assert!(!keyboard_runtime_config_changed(&before, &after));
    assert!(gamepad_runtime_config_changed(&before, &after));
}

#[test]
fn runtime_input_change_detection_reacts_to_enable_flags() {
    let before = AppConfig::default().input;
    let mut after = before.clone();
    after.keyboard_enabled = !before.keyboard_enabled;
    assert!(keyboard_runtime_config_changed(&before, &after));

    after = before.clone();
    after.gamepad_enabled = !before.gamepad_enabled;
    assert!(gamepad_runtime_config_changed(&before, &after));
}

#[test]
fn settings_key_repeat_is_accepted_only_while_editing_value() {
    assert!(should_route_settings_key_event(ElementState::Pressed, false, false));
    assert!(!should_route_settings_key_event(ElementState::Pressed, true, false));
    assert!(should_route_settings_key_event(ElementState::Pressed, true, true));
    assert!(!should_route_settings_key_event(ElementState::Released, true, true));
}

#[test]
fn settings_browse_keeps_cursor_navigation_direction() {
    let profile = ProfileConfig::new_default("default", "Default", 0);
    let bindings = SettingsBindings::from_profile(&profile.input);
    let select_bindings = SelectKeyBindings::from_profile(&profile.input);

    assert_eq!(
        settings_browse_move_control("ArrowUp", &bindings, &select_bindings),
        Some(SelectMove::Previous)
    );
    assert_eq!(
        settings_browse_move_control("ArrowDown", &bindings, &select_bindings),
        Some(SelectMove::Next)
    );
    assert_eq!(
        settings_browse_move_control("DPadUp", &bindings, &select_bindings),
        Some(SelectMove::Previous)
    );
    assert_eq!(
        settings_browse_move_control("DPadDown", &bindings, &select_bindings),
        Some(SelectMove::Next)
    );
    assert_eq!(
        settings_browse_move_control("LShift", &bindings, &select_bindings),
        Some(SelectMove::Previous)
    );
    assert_eq!(
        settings_browse_move_control("LControl", &bindings, &select_bindings),
        Some(SelectMove::Next)
    );
}

#[test]
fn final_notes_fadeout_accepts_e1_and_e2_controls() {
    let keys = default_select_keys();

    assert!(play_fadeout_after_final_notes_control("Q", &keys));
    assert!(play_fadeout_after_final_notes_control("W", &keys));
    assert!(!play_fadeout_after_final_notes_control("Escape", &keys));
    assert!(!play_fadeout_after_final_notes_control("Z", &keys));
}

#[test]
fn final_notes_fadeout_requires_active_finished_note_state() {
    let keys = default_select_keys();

    assert!(should_begin_play_fadeout_after_final_notes(
        "Q",
        &keys,
        true,
        false,
        bmz_gameplay::session::PlayState::Playing,
        true,
    ));
    assert!(should_begin_play_fadeout_after_final_notes(
        "Escape",
        &keys,
        true,
        false,
        bmz_gameplay::session::PlayState::Playing,
        true,
    ));
    assert!(!should_begin_play_fadeout_after_final_notes(
        "Q",
        &keys,
        false,
        false,
        bmz_gameplay::session::PlayState::Playing,
        true,
    ));
    assert!(!should_begin_play_fadeout_after_final_notes(
        "Escape",
        &keys,
        true,
        true,
        bmz_gameplay::session::PlayState::Playing,
        true,
    ));
    assert!(!should_begin_play_fadeout_after_final_notes(
        "Escape",
        &keys,
        true,
        false,
        bmz_gameplay::session::PlayState::Playing,
        false,
    ));
    assert!(!should_begin_play_fadeout_after_final_notes(
        "Q",
        &keys,
        true,
        false,
        bmz_gameplay::session::PlayState::Playing,
        false,
    ));
    assert!(!should_begin_play_fadeout_after_final_notes(
        "Q",
        &keys,
        true,
        true,
        bmz_gameplay::session::PlayState::Playing,
        true,
    ));
    assert!(!should_begin_play_fadeout_after_final_notes(
        "Q",
        &keys,
        true,
        false,
        bmz_gameplay::session::PlayState::Failed,
        true,
    ));
}

#[test]
fn final_notes_control_returns_practice_to_configuration() {
    assert_eq!(
        final_notes_control_action(true, true),
        Some(FinalNotesControlAction::ReturnToPractice)
    );
    assert_eq!(
        final_notes_control_action(true, false),
        Some(FinalNotesControlAction::BeginResultFadeout)
    );
    assert_eq!(final_notes_control_action(false, true), None);
}

#[test]
fn failed_transition_retire_sound_only_starts_on_new_failure() {
    use bmz_gameplay::session::PlayState;

    assert!(should_play_retire_sound_for_failed_transition(PlayState::Playing, PlayState::Failed));
    assert!(!should_play_retire_sound_for_failed_transition(PlayState::Failed, PlayState::Failed));
    assert!(!should_play_retire_sound_for_failed_transition(PlayState::Ready, PlayState::Failed));
    assert!(!should_play_retire_sound_for_failed_transition(
        PlayState::Playing,
        PlayState::Finished
    ));
}

#[test]
fn landmine_se_should_play_respects_auto_keysound_mine_flag() {
    use crate::app::integrations::landmine_se_should_play;
    use bmz_core::ids::{NoteId, SoundId};
    use bmz_core::lane::Lane;
    use bmz_core::time::TimeUs;
    use bmz_gameplay::judge::model::MineHitEvent;
    use bmz_gameplay::session::PlayAudioMix;

    fn audio_mix(auto_keysound: bool, auto_keysound_mine: bool) -> PlayAudioMix {
        PlayAudioMix {
            master_volume: 1.0,
            chart_normalization_gain: 1.0,
            normalize_chart_volume: true,
            key_volume: 1.0,
            bgm_volume: 1.0,
            auto_keysound,
            auto_keysound_fallback: false,
            auto_keysound_mine,
        }
    }

    let no_chart_sound = MineHitEvent {
        note_id: NoteId(1),
        lane: Lane::Key1,
        damage: 10.0,
        sound: None,
        time: TimeUs(0),
    };
    let with_chart_sound = MineHitEvent { sound: Some(SoundId(1)), ..no_chart_sound };

    // auto_keysound OFF: 既定 SE は常に鳴る (従来どおり)。
    assert!(landmine_se_should_play(&[no_chart_sound], audio_mix(false, false)));
    // auto_keysound ON かつ auto_keysound_mine ON: 鳴る。
    assert!(landmine_se_should_play(&[no_chart_sound], audio_mix(true, true)));
    // auto_keysound ON かつ auto_keysound_mine OFF: 既定 SE も抑制する。
    assert!(!landmine_se_should_play(&[no_chart_sound], audio_mix(true, false)));
    // 譜面指定音があるヒットは既定 SE の対象外 (通常の keysound 経路で鳴る)。
    assert!(!landmine_se_should_play(&[with_chart_sound], audio_mix(true, true)));
}

#[test]
fn target_cycle_maps_start_arrow_and_scratch_controls() {
    let keys = default_select_keys();
    let gamepad_keys =
        SelectKeyBindings::from_profile(&ProfileConfig::new_default("default", "Default", 1).input);

    assert_eq!(target_cycle_from_key(PhysicalKey::Code(KeyCode::ArrowUp)), Some(TargetCycle::Next));
    assert_eq!(
        target_cycle_from_key(PhysicalKey::Code(KeyCode::ArrowDown)),
        Some(TargetCycle::Previous)
    );
    assert_eq!(target_cycle_from_control("ScratchUp", &keys), Some(TargetCycle::Next));
    assert_eq!(target_cycle_from_control("ScratchDown", &keys), Some(TargetCycle::Previous));
    assert_eq!(target_cycle_from_control("Axis1+", &gamepad_keys), Some(TargetCycle::Next));
    assert_eq!(target_cycle_from_control("Axis1-", &gamepad_keys), Some(TargetCycle::Previous));
    assert_eq!(target_cycle_from_control("Axis2-", &gamepad_keys), None);
    assert_eq!(target_cycle_from_control("Axis2+", &gamepad_keys), None);
}

#[test]
fn volume_f32_to_unit_clamps_and_rounds() {
    assert_eq!(volume_f32_to_unit(-0.5), 0);
    assert_eq!(volume_f32_to_unit(0.345), 35);
    assert_eq!(volume_f32_to_unit(1.5), 100);
}

#[test]
fn detail_option_control_maps_key5_and_key7_to_visual_offset() {
    let keys = select_keys_with_full_2p_bindings();

    assert_eq!(visual_offset_delta_control("C", &keys), Some(-1));
    assert_eq!(visual_offset_delta_control("V", &keys), Some(1));
    assert_eq!(visual_offset_delta_control("Period", &keys), Some(-1));
    assert_eq!(visual_offset_delta_control("P2K7", &keys), Some(1));
    assert_eq!(visual_offset_delta_control("Z", &keys), None);
    assert_eq!(green_number_delta_control("D", &keys), Some(-1));
    assert_eq!(green_number_delta_control("F", &keys), Some(1));
    assert_eq!(green_number_delta_control("C", &keys), None);
}

#[test]
fn window_title_uses_scene_name() {
    assert_eq!(window_title_for_scene(AppSceneKind::Select), "bmz-player - Select");
    assert_eq!(window_title_for_scene(AppSceneKind::Play), "bmz-player - Play");
    assert_eq!(window_title_for_scene(AppSceneKind::Result), "bmz-player - Result");
}

#[test]
fn viewer_wait_and_shutdown_keep_the_last_play_snapshot_as_the_active_scene() {
    use crate::app::scene_state::viewer_uses_play_scene;

    assert!(viewer_uses_play_scene(true, true, false, true));
    assert!(viewer_uses_play_scene(true, false, true, true));
    assert!(!viewer_uses_play_scene(true, true, false, false));
    assert!(!viewer_uses_play_scene(true, false, false, true));
    assert!(!viewer_uses_play_scene(false, true, true, true));
}

#[test]
fn viewer_shutdown_completes_and_retains_the_faded_play_snapshot() {
    use crate::app::viewer::complete_viewer_exit_fade;

    let mut snapshot = Some(RenderSnapshot::default());
    complete_viewer_exit_fade(&mut snapshot, 300);

    assert_eq!(
        snapshot.and_then(|snapshot| snapshot.fadeout_elapsed_ms),
        Some(bmz_render::snapshot::DEFAULT_PLAY_FADEOUT_DURATION_MS)
    );
}

#[test]
fn cli_mode_flags_resolve_without_using_the_profile_default() {
    use crate::app::constructor::initial_session_mode;

    assert_eq!(initial_session_mode(true, true, false), SessionMode::AutoplayBattle);
    assert_eq!(initial_session_mode(true, false, false), SessionMode::GBattle);
    assert_eq!(initial_session_mode(false, true, false), SessionMode::Autoplay);
    assert_eq!(initial_session_mode(false, false, true), SessionMode::Practice);
    assert_eq!(initial_session_mode(false, false, false), SessionMode::Normal);
}

#[test]
fn deferred_boot_action_keeps_practice_boot_after_window_init() {
    let mut options = AppOptions {
        boot_practice: true,
        practice_start_ms: Some(5_000),
        practice_end_ms: Some(120_000),
        ..AppOptions::default()
    };

    assert_eq!(
        deferred_boot_action(Some(42), &options),
        Some(DeferredBoot::Practice {
            chart_id: 42,
            start_time_ms: Some(5_000),
            end_time_ms: Some(120_000),
        })
    );

    options.boot_practice = false;
    assert_eq!(
        deferred_boot_action(Some(42), &options),
        Some(DeferredBoot::Chart {
            chart_id: 42,
            replay_slot: None,
            skip_decide: false,
            score_save_disabled: false,
            start_time_us: None,
            bms_random_seed: None,
        })
    );

    options.viewer_play = true;
    options.skip_decide = true;
    options.boot_start_time_us = Some(2_500_000);
    options.boot_bms_random_seed = Some(99);
    assert_eq!(
        deferred_boot_action(Some(42), &options),
        Some(DeferredBoot::Chart {
            chart_id: 42,
            replay_slot: None,
            skip_decide: true,
            score_save_disabled: true,
            start_time_us: Some(2_500_000),
            bms_random_seed: Some(99),
        })
    );
}

#[test]
fn window_attributes_use_configured_video_size() {
    let mut config = crate::config::app_config::AppConfig::default().video;
    config.width = 1920;
    config.height = 1080;

    let attributes = window_attributes_from_config(&config);

    assert_eq!(attributes.inner_size, Some(PhysicalSize::new(1920, 1080).into()));
    assert!(attributes.window_icon.is_some());
}

#[test]
fn song_scan_progress_atomic_value_roundtrips() {
    let progress = ScanProgress { done: 123, total: 456 };

    assert_eq!(unpack_scan_progress(pack_scan_progress(progress)), progress);
}

#[test]
fn left_overlay_expires_toast() {
    let toast = Some(("スクリーンショットを保存しました", LEFT_OVERLAY_TOAST_DURATION));
    assert_eq!(resolve_left_overlay_text(false, toast, ""), "");
}

#[test]
fn screenshot_dir_defaults_when_empty() {
    let data_dir = Path::new("user-data");

    assert_eq!(screenshot_dir("", data_dir), PathBuf::from("user-data/screenshots"));
    assert_eq!(screenshot_dir("   ", data_dir), PathBuf::from("user-data/screenshots"));
}

#[test]
fn screenshot_dir_uses_configured_path() {
    let data_dir = Path::new("user-data");

    assert_eq!(screenshot_dir("captures", data_dir), PathBuf::from("user-data/captures"));
}

#[test]
fn screenshot_dir_maps_legacy_data_default_to_data_dir() {
    let data_dir = Path::new("user-data");

    assert_eq!(
        screenshot_dir("data/screenshots", data_dir),
        PathBuf::from("user-data/screenshots")
    );
}

#[test]
fn screenshot_dir_keeps_absolute_configured_path() {
    let data_dir = Path::new("user-data");
    let absolute_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("captures");

    assert_eq!(screenshot_dir(&absolute_dir.to_string_lossy(), data_dir), absolute_dir);
}
