use super::*;

#[test]
fn course_decide_title_override_preserves_play_metadata_and_course_context() {
    let transition = DecideTransition {
        chart_id: 1,
        options: PlayStartOptions::default(),
        launch: DecideLaunch::Play,
        started_at: Instant::now(),
        fadeout_started_at: None,
        cancel: false,
        snapshot: RenderSnapshot {
            title: "Song Title".to_string(),
            subtitle: "Song Subtitle".to_string(),
            course_stage: Some(CourseStageMarker::Stage1),
            ..RenderSnapshot::default()
        },
        title_override: Some(DecideTitleOverride {
            title: "Course Title".to_string(),
            subtitle: String::new(),
        }),
    };

    let decide_snapshot = transition.snapshot_for_render();

    assert_eq!(decide_snapshot.title, "Course Title");
    assert_eq!(decide_snapshot.subtitle, "");
    assert_eq!(decide_snapshot.course_stage, Some(CourseStageMarker::Stage1));
    assert_eq!(transition.snapshot.title, "Song Title");
    assert_eq!(transition.snapshot.subtitle, "Song Subtitle");
}

#[test]
fn course_play_snapshot_uses_fallback_metadata_when_chart_row_is_absent() {
    let mut chart = select_chart_row(7).chart.unwrap();
    chart.title = "Resolved Song".to_string();
    chart.subtitle = "Resolved Subtitle".to_string();
    let items = vec![SelectItem::Course(select_course_row(1, 1))];
    let (chart, best_ex_score) = chart_snapshot_metadata_for_chart(&items, 7, |chart_id| {
        assert_eq!(chart_id, 7);
        Some(chart)
    })
    .expect("library chart metadata");
    let mut snapshot = RenderSnapshot::default();

    apply_chart_metadata_to_snapshot(&mut snapshot, &chart, 123, best_ex_score);

    assert_eq!(snapshot.title, "Resolved Song");
    assert_eq!(snapshot.subtitle, "Resolved Subtitle");
    assert_eq!(snapshot.total_notes, 123);
    assert_eq!(snapshot.best_ex_score, None);
}

#[test]
fn course_intermediate_result_waits_for_input_without_auto_advance() {
    let document: SkinDocument = serde_json::from_str(r#"{ "type": 7, "scene": 2345 }"#).unwrap();

    assert_eq!(result_auto_exit_duration_for_document(Some(&document), true, false), None);
}

#[test]
fn boot_course_intermediate_result_falls_back_when_scene_is_zero() {
    let document: SkinDocument = serde_json::from_str(r#"{ "type": 7, "scene": 0 }"#).unwrap();

    assert_eq!(
        result_auto_exit_duration_for_document(Some(&document), true, true),
        Some(FALLBACK_RESULT_SCENE_DURATION)
    );
}

#[test]
fn course_result_summary_for_skin_uses_aggregate_course_values() {
    fn entry_summary(ex_score: u32, notes: u32, max_combo: u32, duration_ms: i32) -> ResultSummary {
        ResultSummary {
            last_played_at: None,
            clear_type: ClearType::NoPlay,
            skin_attempt: Default::default(),
            target_name: "RANK AAA".to_string(),
            target: Default::default(),
            arrange: "NORMAL".to_string(),
            arrange_2p: "NORMAL".to_string(),
            lane_shuffle_pattern: Vec::new(),
            ex_score,
            max_combo,
            bp: 0,
            cb: 0,
            gauge_value: 80.0,
            gauge_type: GaugeType::Normal,
            total_notes: notes,
            duration_ms,
            initial_bpm: 128.0,
            min_bpm: 128.0,
            max_bpm: 128.0,
            main_bpm: 128.0,
            total_gauge: 260.0,
            judge_rank: Some(2),
            note_judge_window: None,
            key_mode: KeyMode::K7,
            has_long_notes: false,
            long_note_mode: bmz_chart::model::LongNoteMode::Ln,
            judge_counts: crate::screens::result_model::ResultJudgeCounts {
                pgreat: ex_score / 2,
                ..Default::default()
            },
            fast_slow_counts: ResultFastSlowJudgeCounts {
                fast_pgreat: ex_score / 2,
                ..Default::default()
            },
            replay_path: String::new(),
            replay_slots: [false; 4],
            saved_replay_slots: [false; 4],
            score_history_id: 0,
            best_ex_score: None,
            best_clear_type: None,
            best_max_combo: None,
            best_bp: None,
            previous_best_ex_score: None,
            previous_best_clear_type: None,
            previous_best_max_combo: None,
            previous_best_bp: None,
            target_ex_score: Some(ex_score + 40),
            target_max_combo: None,
            target_bp: None,
            target_clear_type: None,
            ir_queued_jobs: 0,
            ir_last_error: None,
            title: String::new(),
            subtitle: String::new(),
            artist: String::new(),
            subartist: String::new(),
            genre: String::new(),
            difficulty_name: String::new(),
            play_level: String::new(),
            graph: Arc::new(bmz_render::snapshot::ResultGraphSnapshot {
                gauge_points: vec![bmz_render::snapshot::ResultGaugeGraphPoint {
                    time_ms: duration_ms,
                    value: 80.0,
                    max: 100.0,
                    border: 20.0,
                    gauge_type: GaugeType::Normal as i32,
                    course_section_start: false,
                }],
                timing_points: vec![bmz_render::snapshot::ResultTimingPoint {
                    time_ms: duration_ms,
                    delta_us: i64::from(duration_ms),
                    judge: bmz_core::judge::Judge::PGreat,
                }],
                judge_graph_density: vec![notes as u8],
                bpm_graph_segments: vec![bmz_render::snapshot::BpmGraphSegment {
                    start_ratio: 0.0,
                    end_ratio: 1.0,
                    bpm: 120.0 + duration_ms as f32,
                    is_stop: false,
                }],
                ..Default::default()
            }),
        }
    }

    let mut course = CourseResultSummary {
        course_id: 1,
        course_score_id: None,
        course_played_at: None,
        ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
        rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
        title: "Course Title".to_string(),
        kind: bmz_core::course::CourseKind::Dan,
        course_titles: [
            "Stage 1".to_string(),
            "Stage 2".to_string(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ],
        entry_summaries: vec![
            entry_summary(120, 100, 80, 1_000),
            entry_summary(200, 120, 90, 2_000),
        ],
        entry_arranges: Vec::new(),
        total_ex_score: 320,
        max_ex_score: 800,
        // Failed course results keep the full course notes as the rank/rate
        // denominator even when only a subset of entries produced summaries.
        total_notes: 400,
        course_ln_mode: Some(bmz_chart::model::LongNoteMode::Cn),
        bp: 37,
        final_clear_type: ClearType::Hard,
        final_gauge_type: GaugeType::ExClass,
        final_gauge_value: 42.5,
        course_max_combo: 170,
        judge_counts: crate::screens::result_model::ResultJudgeCounts {
            pgreat: 160,
            bad: 2,
            ..Default::default()
        },
        trophy_results: Vec::new(),
        course_clear: true,
        course_failed: false,
        total_entries: 2,
        played_entries: 2,
        replay_slots: [true, false, true, false],
        saved_replay_slots: [false, false, true, false],
        best_score: Some(crate::storage::score_db::CourseBestScore {
            last_played_at: None,
            course_score_id: 22,
            course_hash: "course-hash".to_string(),
            ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
            rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
            ex_score: 340,
            max_ex_score: 800,
            clear_type: "ExHard".to_string(),
            gauge_type: "ExHardClass".to_string(),
            gauge_value: 64.0,
            max_combo: 180,
            bp: 4,
            cb: 2,
            judge_counts: Default::default(),
            fast_slow_counts: Default::default(),
            course_failed: false,
            course_clear: true,
            play_count: 3,
            clear_count: 2,
            played_at: 2,
        }),
        previous_best_score: Some(crate::storage::score_db::CourseBestScore {
            last_played_at: None,
            course_score_id: 21,
            course_hash: "course-hash".to_string(),
            ln_policy: crate::ln_policy::LnScorePolicy::ForceLn,
            rule_mode: bmz_gameplay::rule::RuleMode::Beatoraja,
            ex_score: 300,
            max_ex_score: 800,
            clear_type: "Normal".to_string(),
            gauge_type: "Class".to_string(),
            gauge_value: 60.0,
            max_combo: 150,
            bp: 12,
            cb: 8,
            judge_counts: Default::default(),
            fast_slow_counts: Default::default(),
            course_failed: false,
            course_clear: true,
            play_count: 2,
            clear_count: 1,
            played_at: 1,
        }),
    };

    let first_window = bmz_gameplay::judge::model::JudgeWindow::symmetric(
        20_000, 60_000, 150_000, 200_000, 500_000, 150_000, 0,
    );
    let last_window = bmz_gameplay::judge::model::JudgeWindow::symmetric(
        25_000, 50_000, 87_500, 100_000, 500_000, 112_500, 0,
    );
    course.entry_summaries[0].note_judge_window = Some(first_window);
    course.entry_summaries[1].note_judge_window = Some(last_window);
    let mut summary = course_result_summary_for_skin(&course);
    assert_eq!(summary.note_judge_window, Some(last_window));
    assert_eq!(summary.title, "Course Title");
    assert_eq!(summary.genre, "DAN");
    assert_eq!(summary.clear_type, ClearType::Hard);
    assert_eq!(summary.gauge_type, GaugeType::ExClass);
    assert_eq!(summary.gauge_value, 42.5);
    assert_eq!(summary.ex_score, 320);
    assert_eq!(summary.total_notes, 400);
    assert_eq!(summary.bp, 37);
    assert!((summary.ex_score_rate() - 0.4).abs() < 0.001);
    assert_eq!(summary.max_combo, 170);
    assert_eq!(summary.score_history_id, 22);
    assert_eq!(summary.best_ex_score, Some(300));
    assert_eq!(summary.best_clear_type, Some(ClearType::Normal));
    assert_eq!(summary.previous_best_ex_score, Some(300));
    assert_eq!(summary.previous_best_clear_type, Some(ClearType::Normal));
    assert_eq!(summary.previous_best_bp, Some(12));
    assert_eq!(summary.target_ex_score, Some(400));
    let number_values = result_lua_runtime_number_values_for_summary(&summary);
    assert_eq!(summary.last_played_at, None);
    assert_eq!(number_values.get(&243), Some(&i32::MIN));
    let mut saved_course = course.clone();
    saved_course.course_played_at = Some(1_700_000_000);
    let saved_summary = course_result_summary_for_skin(&saved_course);
    assert_eq!(saved_summary.last_played_at, Some(1_700_000_000));
    assert_eq!(result_lua_runtime_number_values_for_summary(&saved_summary)[&243], 1_700_000_000);
    assert_eq!(number_values.get(&74), Some(&400));
    assert_eq!(number_values.get(&110), Some(&160));
    assert_eq!(number_values.get(&113), Some(&2));
    assert_eq!(number_values.get(&426), Some(&0));
    assert_eq!(number_values.get(&178), Some(&25));
    assert_eq!(number_values.get(&425).copied(), Some(i32::try_from(summary.cb).unwrap()));
    assert_eq!(summary.replay_slots, [true, false, true, false]);
    assert_eq!(summary.saved_replay_slots, [false, false, true, false]);
    assert_eq!(summary.judge_counts.pgreat, 160);
    assert_eq!(summary.fast_slow_counts.fast_pgreat, 160);
    assert_eq!(
        summary.graph.gauge_points.iter().map(|point| point.time_ms).collect::<Vec<_>>(),
        vec![1_000, 3_000]
    );
    assert_eq!(
        summary
            .graph
            .gauge_points
            .iter()
            .map(|point| point.course_section_start)
            .collect::<Vec<_>>(),
        vec![false, true]
    );
    assert_eq!(
        summary.graph.timing_points.iter().map(|point| point.time_ms).collect::<Vec<_>>(),
        vec![1_000, 3_000]
    );
    assert_eq!(summary.graph.judge_graph_density, vec![100, 120]);
    assert_eq!(summary.graph.bpm_graph_segments[0].start_ratio, 0.0);
    assert!((summary.graph.bpm_graph_segments[0].end_ratio - 1.0 / 3.0).abs() < 0.001);
    assert!((summary.graph.bpm_graph_segments[1].start_ratio - 1.0 / 3.0).abs() < 0.001);
    assert_eq!(summary.graph.bpm_graph_segments[1].end_ratio, 1.0);

    summary.judge_rank = Some(3);
    summary.long_note_mode = bmz_chart::model::LongNoteMode::Hcn;
    summary.arrange = "RANDOM".to_string();
    summary.arrange_2p = "MIRROR".to_string();
    summary.target_name = "RANK AAA".to_string();
    summary.target = crate::select_options::TargetOption::RankAaa;
    let mut runtime_state = lua_runtime_state_for_result(
        false,
        None,
        true,
        false,
        KeyMode::K7,
        number_values,
        "Player",
    );
    apply_result_summary_lua_load_state(&mut runtime_state, &summary, "Table", "★12", "Table ★12");
    assert_eq!(runtime_state.text_values.get(&1).map(String::as_str), Some("RANK AAA"));
    assert_eq!(runtime_state.text_values.get(&3).map(String::as_str), Some("RANK AAA"));
    apply_course_result_lua_load_state(&mut runtime_state, &course);
    assert_eq!(runtime_state.text_values.get(&10).map(String::as_str), Some("Course Title"));
    assert_eq!(runtime_state.text_values.get(&12).map(String::as_str), Some("Course Title"));
    assert_eq!(runtime_state.text_values.get(&1003).map(String::as_str), Some("Table ★12"));
    assert_eq!(runtime_state.text_values.get(&150).map(String::as_str), Some("Stage 1"));
    assert_eq!(runtime_state.option_values.get(&180), Some(&false));
    assert_eq!(runtime_state.option_values.get(&183), Some(&true));
    assert_eq!(runtime_state.option_values.get(&184), Some(&false));
    assert_eq!(runtime_state.event_index_values.get(&308), Some(&2));
    assert_eq!(runtime_state.event_index_values.get(&42), Some(&2));
    assert_eq!(runtime_state.event_index_values.get(&43), Some(&1));
    assert_eq!(runtime_state.event_index_values.get(&344), Some(&2));
    assert_eq!(runtime_state.event_index_values.get(&345), Some(&1));
    summary.arrange = "F-RANDOM".to_string();
    summary.arrange_2p = "MF-RANDOM".to_string();
    let mut extended_runtime_state = bmz_skin::LuaLoadRuntimeState::default();
    apply_result_summary_lua_load_state(
        &mut extended_runtime_state,
        &summary,
        "Table",
        "★12",
        "Table ★12",
    );
    assert_eq!(extended_runtime_state.event_index_values.get(&42), Some(&2));
    assert_eq!(extended_runtime_state.event_index_values.get(&43), Some(&2));
    assert_eq!(extended_runtime_state.event_index_values.get(&344), Some(&10));
    assert_eq!(extended_runtime_state.event_index_values.get(&345), Some(&11));
    assert_eq!(
        runtime_state.number_values.get(&bmz_render::skin::SKIN_REF_BMZ_COURSE_STAGE_COUNT),
        Some(&2)
    );
    assert_eq!(
        runtime_state.number_values.get(&(bmz_render::skin::SKIN_REF_BMZ_COURSE_STAGE_EX_BASE + 1)),
        Some(&200)
    );
    assert_eq!(
        runtime_state
            .number_values
            .get(&(bmz_render::skin::SKIN_REF_BMZ_COURSE_STAGE_GAUGE_BASE + 1)),
        Some(&80)
    );
    let data: serde_json::Value = serde_json::from_str(
        &runtime_state.virtual_io_files["skin/WMII_FHD/result/courseData.json"],
    )
    .unwrap();
    assert_eq!(data["songs"].as_array().map(Vec::len), Some(2));
    assert_eq!(data["songs"][1]["score"], serde_json::json!(200));

    mark_course_replay_slot_saved(&mut course, Some(&mut summary), 1);
    assert_eq!(course.replay_slots, [true, true, true, false]);
    assert_eq!(course.saved_replay_slots, [false, true, true, false]);
    assert_eq!(summary.replay_slots, course.replay_slots);
    assert_eq!(summary.saved_replay_slots, course.saved_replay_slots);
}

#[test]
fn course_entry_title_hints_are_hydrated_for_unplayed_stages() {
    let mut definition = bmz_core::course::CourseDefinition {
        key: "course".to_string(),
        title: "Course".to_string(),
        kind: bmz_core::course::CourseKind::Course,
        entries: vec![
            bmz_core::course::CourseEntry {
                title_hint: String::new(),
                md5: None,
                sha256: None,
                chart_id: Some(10),
            },
            bmz_core::course::CourseEntry {
                title_hint: "stale".to_string(),
                md5: None,
                sha256: None,
                chart_id: Some(20),
            },
            bmz_core::course::CourseEntry {
                title_hint: "Missing".to_string(),
                md5: None,
                sha256: None,
                chart_id: None,
            },
        ],
        constraints: bmz_core::course::CourseConstraints::default(),
        trophies: Vec::new(),
        release: true,
    };
    apply_course_entry_title_hints(
        &mut definition,
        &HashMap::from([(10, "Resolved One".to_string()), (20, "Resolved Two".to_string())]),
    );

    assert_eq!(definition.entries[0].title_hint, "Resolved One");
    assert_eq!(definition.entries[1].title_hint, "Resolved Two");
    assert_eq!(definition.entries[2].title_hint, "Missing");
}

#[test]
fn course_metadata_metrics_use_stored_ln_counts_and_double_multiplier() {
    let mut first = select_chart_row(10).chart.unwrap();
    first.title = "First".to_string();
    first.mode = "7K".to_string();
    first.total_notes = 100;
    first.ln_profile =
        crate::ln_policy::ChartLnProfile { has_undefined_ln: true, ..Default::default() };
    first.ln_counts =
        crate::ln_policy::ChartLnCounts { undefined_ln_pairs: 1, ..Default::default() };
    let mut second = select_chart_row(20).chart.unwrap();
    second.title = "Second".to_string();
    second.total_notes = 200;
    second.ln_profile =
        crate::ln_policy::ChartLnProfile { has_defined_cn: true, ..Default::default() };
    second.ln_counts =
        crate::ln_policy::ChartLnCounts { defined_cn_pairs: 2, ..Default::default() };
    let definition = bmz_core::course::CourseDefinition {
        key: "course".to_string(),
        title: "Course".to_string(),
        kind: bmz_core::course::CourseKind::Course,
        entries: vec![
            bmz_core::course::CourseEntry {
                title_hint: String::new(),
                md5: None,
                sha256: None,
                chart_id: Some(10),
            },
            bmz_core::course::CourseEntry {
                title_hint: String::new(),
                md5: None,
                sha256: None,
                chart_id: Some(20),
            },
        ],
        constraints: Default::default(),
        trophies: Vec::new(),
        release: true,
    };
    let options = vec![
        PlayStartOptions {
            key_mode_conversion: KeyModeConversionConfig::SevenToNine,
            ..Default::default()
        },
        PlayStartOptions { double_option: DoubleOption::Battle, ..Default::default() },
    ];

    let score_enabled_snapshot = course_play_metrics_from_chart_metadata(
        &definition,
        crate::ln_policy::LnPolicySetting::ForceCn,
        &options,
        vec![first.clone(), second.clone()],
    )
    .unwrap();
    assert!(!score_enabled_snapshot.has_score_disabling_key_mode_conversion);

    let mut options = options;
    options[0].seven_to_nine_rule_mode = crate::config::profile_config::SevenToNineRuleMode::Keys9;

    let snapshot = course_play_metrics_from_chart_metadata(
        &definition,
        crate::ln_policy::LnPolicySetting::ForceCn,
        &options,
        vec![first, second],
    )
    .unwrap();

    assert_eq!(snapshot.first_chart.chart_id, 10);
    assert!(snapshot.has_score_disabling_key_mode_conversion);
    assert_eq!(snapshot.titles.get(&20).map(String::as_str), Some("Second"));
    assert_eq!(snapshot.metrics.total_notes, 505);
    assert_eq!(snapshot.metrics.ln_mode, Some(bmz_chart::model::LongNoteMode::Cn));
    assert_eq!(snapshot.metrics.ln_policy, crate::ln_policy::LnScorePolicy::ForceCn);
}

#[test]
fn exact_course_metrics_reuse_first_prepared_chart_without_library_access() {
    let db = LibraryDatabase::from_connection(rusqlite::Connection::open_in_memory().unwrap());
    let definition = bmz_core::course::CourseDefinition {
        key: "course".to_string(),
        title: "Course".to_string(),
        kind: bmz_core::course::CourseKind::Course,
        entries: vec![bmz_core::course::CourseEntry {
            title_hint: "First".to_string(),
            md5: None,
            sha256: None,
            chart_id: Some(999),
        }],
        constraints: Default::default(),
        trophies: Vec::new(),
        release: true,
    };
    let first_metrics = crate::screens::play_session::ScoredChartMetrics {
        total_notes: 321,
        ln_mode: Some(bmz_chart::model::LongNoteMode::Hcn),
        source_ln_profile: crate::ln_policy::ChartLnProfile {
            has_defined_hcn: true,
            ..Default::default()
        },
    };

    let metrics = course_play_metrics_for_definition_reusing_first(
        &db,
        &definition,
        &AppConfig::default(),
        crate::ln_policy::LnPolicySetting::AutoLn,
        bmz_gameplay::rule::RuleMode::Beatoraja,
        &[PlayStartOptions::default()],
        first_metrics,
    )
    .unwrap();

    assert_eq!(metrics.total_notes, 321);
    assert_eq!(metrics.ln_mode, Some(bmz_chart::model::LongNoteMode::Hcn));
}

#[test]
fn course_intermediate_result_only_with_active_course_and_no_course_result() {
    // active_course 保持 + finished_play あり + finished_course 無し → 中間リザルト。
    assert!(is_course_intermediate_result(true, false, true));
    // コース最終結果 (finished_course あり) は中間リザルトではない。
    assert!(!is_course_intermediate_result(true, true, true));
    // 単曲 (非コース) リザルトは中間リザルトではない。
    assert!(!is_course_intermediate_result(false, false, true));
    // 結果未表示なら中間リザルトではない。
    assert!(!is_course_intermediate_result(true, false, false));
}

#[test]
fn course_intermediate_result_keeps_rounded_clear_type_for_skin_display() {
    let mut finished = debug_boot_finished_play_session();
    finished.result.clear_type = ClearType::Normal;
    finished.summary.clear_type = ClearType::NoPlay;

    assert_eq!(finished.summary.clear_type, ClearType::NoPlay);
}

#[test]
fn course_intermediate_result_skin_ops_use_raw_clear_result() {
    assert!(!result_failed_for_skin_ops(ClearType::NoPlay, Some(ClearType::Normal)));
    assert!(result_failed_for_skin_ops(ClearType::NoPlay, Some(ClearType::Failed)));
    assert!(result_failed_for_skin_ops(ClearType::NoPlay, None));
}

#[test]
fn course_intermediate_exit_action_finishes_failed_or_final_stage() {
    assert_eq!(
        course_intermediate_exit_action_for_state(false, true),
        ResultExitAction::AdvanceCourse
    );
    assert_eq!(
        course_intermediate_exit_action_for_state(true, true),
        ResultExitAction::FinishCourse
    );
    assert_eq!(
        course_intermediate_exit_action_for_state(false, false),
        ResultExitAction::FinishCourse
    );
}

#[test]
fn course_stage_result_is_shown_for_next_failed_or_final_stage() {
    assert!(should_show_course_stage_result(false, true, true));
    assert!(should_show_course_stage_result(true, true, false));
    assert!(should_show_course_stage_result(false, false, false));
    assert!(!should_show_course_stage_result(false, true, false));
}

#[test]
fn course_normalizes_battle_session_modes() {
    let mut autoplay_battle = PlayStartOptions {
        session_mode: SessionMode::AutoplayBattle,
        autoplay: true,
        replay_player: Some(bmz_gameplay::replay::ReplayPlayer::default()),
        ..PlayStartOptions::default()
    };
    normalize_session_mode_for_course(&mut autoplay_battle);
    assert_eq!(autoplay_battle.session_mode, SessionMode::Autoplay);
    assert!(autoplay_battle.autoplay);
    assert!(autoplay_battle.replay_player.is_none());

    let mut practice =
        PlayStartOptions { session_mode: SessionMode::Practice, ..PlayStartOptions::default() };
    normalize_session_mode_for_course(&mut practice);
    assert_eq!(practice.session_mode, SessionMode::Normal);
    assert!(!practice.autoplay);

    let mut g_battle = PlayStartOptions {
        session_mode: SessionMode::GBattle,
        battle_target: Some(crate::screens::play_start::BattleTarget {
            provider: "local".to_string(),
            score_id: "score".to_string(),
            player_id: "rival".to_string(),
            player_name: "RIVAL".to_string(),
            rank: 1,
            ex_score: 100,
            gauge: None,
            playback: crate::screens::play_start::BattleTargetPlayback::Seed {
                arrange: ArrangeOption::Normal,
                arrange_2p: ArrangeOption::Normal,
                double_option: DoubleOption::Off,
                packed_seed: None,
            },
        }),
        ..PlayStartOptions::default()
    };
    normalize_session_mode_for_course(&mut g_battle);
    assert_eq!(g_battle.session_mode, SessionMode::Normal);
    assert!(!g_battle.autoplay);
    assert!(g_battle.battle_target.is_none());
}

#[test]
fn result_exit_sound_prefers_course_close_for_course_results() {
    use crate::system_sound::SoundType;

    assert_eq!(result_exit_sound_for_context(false, false), SoundType::ResultClose);
    assert_eq!(result_exit_sound_for_context(true, true), SoundType::CourseClose);
    assert_eq!(result_exit_sound_for_context(true, false), SoundType::ResultClose);
}

#[test]
fn result_entry_sound_clear_type_uses_raw_result_for_course_stage() {
    let mut finished = debug_boot_finished_play_session();
    finished.summary.clear_type = ClearType::NoPlay;

    finished.result.clear_type = ClearType::Normal;
    assert_eq!(result_entry_clear_type_for_sound(&finished), ClearType::Normal);

    finished.result.clear_type = ClearType::Failed;
    assert_eq!(result_entry_clear_type_for_sound(&finished), ClearType::Failed);
}

#[test]
fn course_rows_are_playable_only_when_all_entries_resolve() {
    let rows = vec![
        SelectItem::Course(select_course_row(4, 4)),
        SelectItem::Course(select_course_row(3, 4)),
    ];

    let profile = ProfileConfig::new_default("default", "Default", 0);
    let snapshot_rows = select_snapshot_rows(&rows, 0, 2, &profile, None, &HashMap::new());

    assert!(snapshot_rows.iter().any(|row| row.title == "Course 4/4" && row.in_library));
    assert!(snapshot_rows.iter().any(|row| row.title == "Course 3/4" && !row.in_library));
    let partial = snapshot_rows.iter().find(|row| row.title == "Course 3/4").unwrap();
    assert_eq!(partial.course_titles[0], "Stage 1");
    assert_eq!(partial.course_titles[3], "(no song) Stage 4");
}

#[test]
fn course_constraint_flags_match_beatoraja_gradebar_ops() {
    let constraints = bmz_core::course::CourseConstraints {
        class: bmz_core::course::CourseClassConstraint::GradeRandomAllowed,
        speed: bmz_core::course::CourseSpeedConstraint::NoSpeed,
        judge: bmz_core::course::CourseJudgeConstraint::NoGood,
        gauge: bmz_core::course::CourseGaugeConstraint::Keys24,
        ln: bmz_core::course::CourseLnConstraint::Cn,
        source_constraints: Vec::new(),
    };

    let flags = course_constraint_flags(&constraints);

    assert!(!flags.class);
    assert!(!flags.mirror);
    assert!(flags.random);
    assert!(flags.no_speed);
    assert!(flags.no_good);
    assert!(!flags.no_great);
    assert!(flags.gauge_24k);
    assert!(!flags.gauge_7k);
    assert!(flags.cn);
    assert!(!flags.hcn);
}
