use super::*;
use bmz_core::course::{CourseDefinition, CourseEntry, CourseKind, CourseTrophy};
use winit::platform::windows::EventLoopBuilderExtWindows;

fn active_course(
    course_id: i64,
    definition: &CourseDefinition,
    completed: usize,
) -> ActiveCourseSession {
    let entry_results = (0..completed)
        .map(|index| {
            let mut finished = debug_boot_finished_play_session();
            finished.result.clear_type = ClearType::Max;
            finished.result.total_notes = 1;
            finished.result.score = Default::default();
            finished.result.score.judges.fast_pgreat = 1;
            finished.result.score.past_notes = 1;
            finished.result.score.combo = 1;
            finished.result.score.max_combo = 1;
            finished.summary.total_notes = 1;
            finished.summary.ex_score = 2;
            finished.summary.clear_type = ClearType::Max;
            finished.applied_arrange.seed = Some(42 + index as i64);
            CourseEntryResult { chart_id: definition.entries[index].chart_id.unwrap(), finished }
        })
        .collect();
    ActiveCourseSession {
        course_id,
        definition: definition.clone(),
        ln_policy_setting: crate::ln_policy::LnPolicySetting::ForceLn,
        ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
        rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
        score_save_disabled: false,
        course_total_notes: definition.entries.len() as u32,
        course_ln_mode: None,
        current_index: completed,
        entry_results,
        entry_start_options: vec![PlayStartOptions::default(); definition.entries.len()],
        stage_charts: vec![Default::default(); definition.entries.len()],
        replay_stage_limit: None,
        auto_advance_intermediate_results: false,
    }
}

/// CLEAR is left-only and FAILED is right-only, so a mixed outcome is audible in PCM.
fn result_sound_processor(
    app: &mut WinitApp,
    variant: usize,
) -> bmz_audio::command::CommandedAudioEngine {
    use crate::system_sound::SoundType::*;
    let sounds = match variant {
        0 => vec![ResultClear, ResultFail, CourseClear, CourseFail],
        1 => vec![ResultClear, ResultFail],
        _ => vec![
            ResultClear,
            ResultFail,
            CourseClear,
            CourseFail,
            ResultEnterClear,
            ResultEnterFail,
            ResultBgmClear,
            ResultBgmFail,
            CourseEnterClear,
            CourseEnterFail,
            CourseBgmClear,
            CourseBgmFail,
        ],
    };
    let mut engine = bmz_audio::engine::AudioEngine::new(48_000);
    let ids = sounds
        .into_iter()
        .enumerate()
        .map(|(index, sound)| {
            let id = bmz_core::ids::SoundId(index as u32 + 1);
            let failed = matches!(
                sound,
                ResultFail
                    | CourseFail
                    | ResultEnterFail
                    | ResultBgmFail
                    | CourseEnterFail
                    | CourseBgmFail
            );
            let frame = if failed { [0.0, 0.125] } else { [0.125, 0.0] };
            engine.insert_sample(
                id,
                bmz_audio::sample::DecodedSample {
                    channels: 2,
                    sample_rate: 48_000,
                    frames: frame.repeat(64),
                },
            );
            (sound, id)
        })
        .collect();
    let handle = bmz_audio::command::AudioEngineHandle::new(engine);
    let processor = handle.processor();
    app.audio.system_sound =
        Some(crate::system_sound_manager::SystemSoundManager::with_id_map(handle, ids));
    let mix = &mut app.boot.profile_config.audio_mix;
    mix.master_volume = 100;
    mix.system_bgm_volume = 100;
    mix.system_se_volume = 100;
    processor
}

fn assert_result_pcm(
    processor: &mut bmz_audio::command::CommandedAudioEngine,
    frame: u64,
    failed: bool,
    variant: usize,
) {
    let amplitude = if variant == 2 { 0.25 } else { 0.125 };
    let expected = if failed { [0.0, amplitude] } else { [amplitude, 0.0] };
    let mut pcm = [0.0; 2];
    assert!(processor.render_stereo(frame, &mut pcm));
    assert_eq!(pcm, expected, "only the course outcome's SE and optional course BGM may play");
}

#[test]
#[ignore = "requires an isolated Winit event loop and offscreen GPU; run alone with --ignored"]
fn course_retirement_and_retry_preserve_results_and_cancel_pending_launches() {
    let event_loop =
        EventLoop::<AppUserEvent>::with_user_event().with_any_thread(true).build().unwrap();
    let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
    let (mut boot, path, copy) = super::boot_chart::registered_charts(&data);
    boot.app_config.input.gamepad_enabled = false;
    let chart_id = boot.library_db.chart_id_by_chart_file_path(&path).unwrap().unwrap();
    let sha256 = boot.library_db.chart_sha256_by_chart_id(chart_id).unwrap().unwrap();
    let definition = CourseDefinition {
        key: "retire-test".into(),
        title: "Retire test".into(),
        kind: CourseKind::Course,
        entries: vec![
            CourseEntry {
                title_hint: "Original".into(),
                md5: None,
                sha256: Some(crate::storage::common::hash_to_hex(&sha256)),
                chart_id: Some(chart_id),
            };
            3
        ],
        constraints: Default::default(),
        trophies: vec![CourseTrophy {
            name: "gold".into(),
            max_miss_rate: 100.0,
            min_score_rate: 0.0,
        }],
        release: false,
    };
    let course_id = boot.library_db.upsert_course("test", &definition, 0, 1).unwrap();
    let (maintenance_tx, _) = tokio::sync::watch::channel(false);
    let mut app = WinitApp::new(
        boot,
        AppOptions { viewer_play: true, ..Default::default() },
        Instant::now(),
        None,
        None,
        Arc::new(AtomicBool::new(false)),
        event_loop.create_proxy(),
        LogBuffer::default(),
        maintenance_tx,
        None,
    )
    .unwrap();
    app.renderer
        .attach_offscreen(bmz_render::renderer::SurfaceSize { width: 32, height: 32 })
        .unwrap();
    app.viewer_mode = false;
    app.viewer_waiting = false;
    let attempts = |app: &WinitApp| -> i64 {
        app.boot
            .score_db
            .conn()
            .query_row("SELECT COUNT(*) FROM course_scores", [], |row| row.get(0))
            .unwrap()
    };

    for mode in 0..3 {
        for completed in 0..3 {
            for gesture in 0..4 {
                let sound_variant = gesture % 3;
                let mut sound_processor = result_sound_processor(&mut app, sound_variant);
                app.integrations.last_scene_kind = Some(AppSceneKind::Play);
                app.clear_finished_course();
                app.result.finished_play = None;
                let mut course = active_course(course_id, &definition, completed);
                for entry in &mut course.entry_results {
                    entry.finished.result.autoplay = mode == 1;
                    entry.finished.replay_playback = mode == 2;
                }
                app.play.active_course = Some(course);
                let snapshot = RenderSnapshot::default();
                app.play.pending_play_start = Some(PendingPlayStart::from_snapshot(
                    chart_id,
                    PlayStartOptions::default(),
                    &snapshot,
                    &app.boot.profile_config,
                    KeyMode::K7,
                    KeyMode::K7,
                    Default::default(),
                ));
                app.play.last_play_snapshot = Some(snapshot);
                app.play.play_option_input =
                    Some(play_option_input_for(&app.boot.profile_config.input, KeyMode::K7));
                let generation = app.play.play_preload_generation;
                app.play.pending_course_stage_launch = Some(PendingCourseStageLaunch {
                    course_id,
                    entry_index: completed,
                    chart_id,
                    options: PlayStartOptions::default(),
                    preload_generation: generation,
                    preload_error: None,
                });
                let (tx, rx) = mpsc::channel();
                app.play.pending_play_preload = Some(PendingPlayPreload {
                    generation,
                    chart_id,
                    input: Default::default(),
                    audio_progress: Arc::new(AtomicU32::new(0)),
                    prepared_chart: Arc::new(OnceLock::new()),
                    rx,
                });
                let before = attempts(&app);
                match gesture {
                    0 => assert!(app.stop_play_like_escape("test Escape")),
                    1 | 3 => {
                        let key = if gesture == 1 { "Q" } else { "R" };
                        app.update_play_e1_control_state(
                            W_KEYBOARD_DEVICE_ID,
                            &PhysicalControl::KeyboardKey(key.into()),
                            true,
                        );
                        app.update_play_exit_control_state(
                            W_KEYBOARD_DEVICE_ID,
                            &PhysicalControl::KeyboardKey("W".into()),
                            true,
                        );
                        app.play.play_exit_hold_started_at =
                            Some(Instant::now() - Duration::from_secs(60));
                        assert!(app.stop_play_if_exit_hold_elapsed());
                    }
                    _ => {
                        app.update_play_exit_control_state(
                            W_KEYBOARD_DEVICE_ID,
                            &PhysicalControl::KeyboardKey("W".into()),
                            true,
                        );
                        assert!(app.update_play_exit_control_state(
                            W_KEYBOARD_DEVICE_ID,
                            &PhysicalControl::KeyboardKey("E".into()),
                            true,
                        ));
                    }
                }
                let ending = app.play.play_ending.as_ref().unwrap();
                assert_eq!(
                    ending.completion,
                    if completed == 0 {
                        PlayEndingCompletion::Select
                    } else {
                        PlayEndingCompletion::RetiredCourse
                    }
                );
                assert!(ending.fadeout_started_at.is_some());
                assert!(app.stop_play_like_escape("repeated retire"));
                assert!(!app.handle_quick_retry_control("Q"));
                assert!(
                    tx.send(PlayPreloadResult {
                        generation,
                        chart_id,
                        result: Err("late worker error".into()),
                    })
                    .is_err()
                );
                app.poll_play_preload();
                app.finish_play_ending();
                let scene = app.current_scene_kind();
                app.update_window_title_for_scene(scene);
                assert!(app.play.active_course.is_none());
                assert!(app.play.pending_play_start.is_none());
                assert!(app.play.pending_course_stage_launch.is_none());
                assert!(app.play.play_preload_generation > generation);
                if completed == 0 {
                    assert_eq!(app.view_state(), AppViewState::Select);
                    assert!(app.result.finished_course.is_none());
                } else {
                    assert_result_pcm(&mut sound_processor, 0, true, sound_variant);
                    assert_eq!(app.view_state(), AppViewState::Result);
                    let result = app.result.finished_course.as_ref().unwrap();
                    assert_eq!(result.played_entries, completed);
                    assert_eq!(result.total_ex_score, completed as u32 * 2);
                    assert_eq!(result.bp, 3 - completed as u32);
                    assert_eq!(result.final_clear_type, ClearType::Failed);
                    assert!(!result.course_clear);
                    assert!(result.trophy_results.iter().all(|trophy| !trophy.achieved));
                    assert_eq!(result.entry_arranges.len(), completed);
                }
                let expected = before + i64::from(completed > 0 && mode == 0);
                assert_eq!(attempts(&app), expected);
                app.finish_play_ending();
                app.poll_play_preload();
                assert_eq!(attempts(&app), expected, "retire saves at most one attempt");
            }
        }
    }

    let retained = app.result.finished_course.clone().unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(&copy).unwrap();
    for action in [ResultExitAction::RetryCourseSameArrange, ResultExitAction::HeldCourseLanes] {
        app.result.result_key5_held = true;
        app.result.result_exit = Some(ResultExit {
            started_at: Instant::now() - Duration::from_secs(60),
            action,
            skip_requested: false,
            skip_final_frame_held: false,
        });
        app.advance_result_exit();
        assert_eq!(app.view_state(), AppViewState::Result);
        assert_eq!(
            app.result.finished_course.as_ref().unwrap().total_ex_score,
            retained.total_ex_score
        );
        assert!(app.result.finished_play.is_some());
        assert!(app.result.result_exit.is_none());
        assert!(app.play.active_course.is_none());
        assert!(app.play.pending_decide.is_none());
        assert!(app.smoke.left_overlay_toast.is_some());
        assert!(!app.result.result_key5_held);
    }

    std::fs::write(&path, "#TITLE Original\n#BPM 120\n#00011:01\n").unwrap();
    for mode in [ResultRetryMode::SameArrange, ResultRetryMode::DifferentArrange] {
        app.install_finished_course(retained.clone(), None, None, None);
        app.retry_course_with_mode(mode);
        assert_eq!(app.view_state(), AppViewState::Decide);
        assert!(app.result.finished_course.is_none());
        let course = app.play.active_course.as_ref().unwrap();
        assert_eq!(course.current_index, 0);
        assert!(course.entry_results.is_empty());
        let options = &course.entry_start_options[0];
        if mode == ResultRetryMode::SameArrange {
            assert_eq!(options.arrange_seed, retained.entry_arranges[0].seed);
        }
        assert!(options.initial_gauge_values.is_none());
        assert!(options.initial_course_combo.is_none());
        app.abort_pending_play_start();
    }

    // The final course result can follow either Play directly or a stage Result.
    for previous in [AppSceneKind::Play, AppSceneKind::Result] {
        for completed in [1, 3] {
            for variant in 0..3 {
                app.clear_finished_course();
                app.result.finished_play = None;
                app.play.active_course = Some(active_course(course_id, &definition, completed));
                app.integrations.last_scene_kind = Some(previous);
                let mut processor = result_sound_processor(&mut app, variant);
                app.finish_active_course();
                app.update_window_title_for_scene(AppSceneKind::Result);
                assert_result_pcm(&mut processor, 0, completed < 3, variant);
                app.update_window_title_for_scene(AppSceneKind::Result);
                assert_result_pcm(&mut processor, 1, completed < 3, variant);
            }
        }
    }
}
