use super::*;

#[test]
fn result_load_numbers_follow_ir_ranking_updates() {
    let mut runtime = bmz_skin::LuaLoadRuntimeState::default();
    apply_result_ir_lua_load_numbers(&mut runtime, Default::default());
    assert_eq!(runtime.number_values[&380], i32::MIN);
    let mut ir = bmz_render::scene::ResultIrSnapshot::default();
    ir.entries[0].ex_score = Some(2849);
    ir.entries[0].rank = Some(1);
    apply_result_ir_lua_load_numbers(&mut runtime, ir);
    assert_eq!(runtime.number_values[&380], 2849);
    assert_eq!(runtime.number_values[&390], 1);
    apply_result_ir_lua_load_numbers(&mut runtime, Default::default());
    assert_eq!(runtime.number_values[&380], i32::MIN);
}
use crate::app::result_flow_ending::{
    FinishedPlayAction, finished_play_action, play_ending_completion,
};
use crate::app::result_flow_timing::{
    play_fadeout_duration_for_skin, practice_play_fadeout_duration_for_skin,
};

#[test]
fn replay_slot_digits_accept_top_row_and_numpad() {
    for (normal, numpad, expected) in [
        (KeyCode::Digit1, KeyCode::Numpad1, 0),
        (KeyCode::Digit2, KeyCode::Numpad2, 1),
        (KeyCode::Digit3, KeyCode::Numpad3, 2),
        (KeyCode::Digit4, KeyCode::Numpad4, 3),
    ] {
        assert_eq!(digit_to_replay_slot(PhysicalKey::Code(normal)), Some(expected));
        assert_eq!(digit_to_replay_slot(PhysicalKey::Code(numpad)), Some(expected));
    }
}

#[test]
fn result_skin_signature_changes_when_only_offset_changes() {
    let mut skin = crate::config::profile_config::SkinConfig::default();
    let before = result_skin_signature_for_config(
        &skin,
        ResultSkinSlot::Normal,
        bmz_skin::LuaLoadRuntimeState::default(),
    );
    skin.result_offsets.push(SkinOffsetConfig {
        name: Some("Mascot".to_string()),
        id: 90,
        x: 12,
        ..Default::default()
    });
    let after = result_skin_signature_for_config(
        &skin,
        ResultSkinSlot::Normal,
        bmz_skin::LuaLoadRuntimeState::default(),
    );

    assert_ne!(before, after);
    assert_eq!(after.4.offset_values["Mascot"].x, 12);
    assert_eq!(after.4.offset_id_values[&90].x, 12);
}

#[test]
fn finished_play_action_distinguishes_viewer_wait_and_explicit_exit() {
    assert_eq!(finished_play_action(true, false, false), FinishedPlayAction::ViewerWait);
    assert_eq!(finished_play_action(true, true, false), FinishedPlayAction::Exit);
    assert_eq!(finished_play_action(false, true, false), FinishedPlayAction::Exit);
    assert_eq!(finished_play_action(false, false, false), FinishedPlayAction::ShowResult);
    assert_eq!(finished_play_action(true, true, true), FinishedPlayAction::ShowResult);
}

#[test]
fn viewer_play_ending_freezes_play_instead_of_starting_result_fade() {
    assert_eq!(play_ending_completion(true, false), PlayEndingCompletion::ViewerWait);
    assert_eq!(play_ending_completion(true, true), PlayEndingCompletion::Result);
    assert_eq!(play_ending_completion(false, false), PlayEndingCompletion::Result);
}

#[test]
fn fallback_result_scene_uses_nonzero_duration() {
    assert_eq!(result_input_duration_for_document(None), Duration::ZERO);
    assert_eq!(result_scene_duration_for_document(None), FALLBACK_RESULT_SCENE_DURATION);
}

#[test]
fn play_fadeout_duration_uses_skin_timer_or_black_fallback() {
    assert_eq!(
        play_fadeout_duration_for_skin(0, 0),
        Duration::from_millis(bmz_render::snapshot::DEFAULT_PLAY_FADEOUT_DURATION_MS as u64)
    );
    assert_eq!(play_fadeout_duration_for_skin(300, 0), Duration::from_millis(300));
    assert_eq!(play_fadeout_duration_for_skin(300, 700), Duration::from_millis(700));
}

#[test]
fn practice_fadeout_duration_uses_only_the_skin_header() {
    assert_eq!(practice_play_fadeout_duration_for_skin(0), Duration::ZERO);
    assert_eq!(practice_play_fadeout_duration_for_skin(300), Duration::from_millis(300));
    assert_eq!(practice_play_fadeout_duration_for_skin(-1), Duration::ZERO);
}

#[test]
fn result_scene_duration_respects_skin_document() {
    let document: SkinDocument =
        serde_json::from_str(r#"{ "type": 7, "input": 1500, "scene": 2345 }"#).unwrap();

    assert_eq!(result_input_duration_for_document(Some(&document)), Duration::from_millis(1500));
    assert_eq!(result_scene_duration_for_document(Some(&document)), Duration::from_millis(2345));
}

#[test]
fn normal_result_scene_zero_disables_auto_leave() {
    let document: SkinDocument =
        serde_json::from_str(r#"{ "type": 7, "input": 1500, "scene": 0 }"#).unwrap();

    assert_eq!(result_auto_exit_duration_for_document(Some(&document), false, false), None);
}

#[test]
fn result_auto_exit_uses_scene_when_positive() {
    let document: SkinDocument = serde_json::from_str(r#"{ "type": 7, "scene": 2345 }"#).unwrap();

    assert_eq!(
        result_auto_exit_duration_for_document(Some(&document), false, false),
        Some(Duration::from_millis(2345))
    );
}

#[test]
fn pre_play_abort_starts_fadeout_and_returns_to_select_without_result() {
    let started_at = Instant::now();
    let ending = pre_play_abort_ending(started_at);

    assert_eq!(ending.started_at, started_at);
    assert!(!ending.failed);
    assert_eq!(ending.completion, PlayEndingCompletion::Select);
    assert!(ending.music_end_started_at.is_none());
    assert!(ending.finished.is_none());
    assert_eq!(ending.fadeout_started_at, Some(started_at));
    assert!(ending.full_combo_elapsed_at_finish_ms.is_none());
}

#[test]
fn viewer_app_exit_uses_the_pre_ready_fade_route() {
    let started_at = Instant::now();
    let ending = viewer_exit_ending(started_at);

    assert_eq!(ending.started_at, started_at);
    assert_eq!(ending.completion, PlayEndingCompletion::ViewerExit);
    assert_eq!(ending.fadeout_started_at, Some(started_at));
    assert!(ending.music_end_started_at.is_none());
    assert!(ending.finished.is_none());
    assert!(!ending.failed);
}

#[test]
fn practice_endings_match_beatoraja_timer_routes() {
    let started_at = Instant::now();

    let natural = practice_natural_finish_ending(started_at);
    assert_eq!(natural.completion, PlayEndingCompletion::PracticeConfig);
    assert_eq!(natural.music_end_started_at, Some(started_at));
    assert!(natural.fadeout_started_at.is_none());
    assert!(!natural.failed);

    let requested = practice_requested_finish_ending(started_at);
    assert_eq!(requested.completion, PlayEndingCompletion::PracticeConfig);
    assert!(requested.music_end_started_at.is_none());
    assert_eq!(requested.fadeout_started_at, Some(started_at));
    assert!(!requested.failed);

    let failed = practice_failed_ending(started_at);
    assert_eq!(failed.completion, PlayEndingCompletion::PracticeConfig);
    assert!(failed.music_end_started_at.is_none());
    assert!(failed.fadeout_started_at.is_none());
    assert!(failed.failed);

    let leave = practice_leave_ending(started_at);
    assert_eq!(leave.completion, PlayEndingCompletion::PracticeLeave);
    assert!(leave.music_end_started_at.is_none());
    assert_eq!(leave.fadeout_started_at, Some(started_at));
    assert!(!leave.failed);
}

#[test]
fn stored_result_keeps_select_score_refresh_pending_until_reload() {
    let mut refresh = SelectScoreRefreshState::default();

    refresh.mark_score_data_changed(false);
    assert!(!refresh.take_dirty(), "unsaved autoplay/replay results must not dirty select scores");

    refresh.mark_score_data_changed(true);
    refresh.mark_score_data_changed(false);
    // Result -> Retry does not consume this state. The next Select reload does.
    assert!(refresh.take_dirty());
    assert!(!refresh.take_dirty(), "a completed Select reload must consume the dirty state");
}

#[test]
fn debug_boot_result_summary_has_stat_graph_data() {
    let finished = debug_boot_finished_play_session();
    let summary = &finished.summary;

    assert_eq!(summary.title, "Debug Result Boot [ANOTHER]");
    assert_eq!(summary.key_mode, KeyMode::K7);
    assert!(summary.ex_score > 0);
    assert!(!summary.graph.gauge_points.is_empty());
    assert!(!summary.graph.judge_graph_buckets.is_empty());
    assert!(!summary.graph.early_late_graph_buckets.is_empty());
    assert!(!summary.graph.timing_points.is_empty());
    assert!(summary.graph.timing_distribution.total() > 0);
}

#[test]
fn result_lua_runtime_values_cover_load_time_result_decisions() {
    let mut summary = debug_boot_result_summary();
    let graph = Arc::make_mut(&mut summary.graph);
    graph.timing_distribution = bmz_render::snapshot::ResultTimingDistribution::new(150);
    graph.timing_distribution.add(-13);
    graph.timing_distribution.add(-12);

    let values = result_lua_runtime_number_values_for_summary(&summary);

    assert_eq!(values.get(&150), Some(&760));
    assert_eq!(values.get(&170), Some(&760));
    assert_eq!(values.get(&171), Some(&(summary.ex_score as i32)));
    assert_eq!(values.get(&121), Some(&1_056));
    assert_eq!(values.get(&151), Some(&1_056));
    assert_eq!(values.get(&152), Some(&((summary.ex_score as i32).saturating_sub(760))));
    assert_eq!(values.get(&153), Some(&((summary.ex_score as i32).saturating_sub(1_056))));
    assert_eq!(values.get(&370), Some(&(ClearType::Failed as i32)));
    assert_eq!(values.get(&371), Some(&(ClearType::Normal as i32)));
    assert_eq!(values.get(&374), Some(&-12));
    assert_eq!(values.get(&375), Some(&-50));
    assert_eq!(values.get(&410), Some(&128));
    assert_eq!(values.get(&422), Some(&2));
    assert_eq!(values.get(&423), Some(&46));
    assert_eq!(values.get(&424), Some(&104));
}

#[test]
fn result_lua_next_rank_matches_rendering_before_skin_load() {
    let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
    std::fs::create_dir_all(&data.paths.data_dir).unwrap();
    let skin_path = data.paths.data_dir.join("result.luaskin");
    std::fs::write(
        &skin_path,
        r#"
        local state = require("main_state")
        return {type = 7, value = {{id = "rank_diff", ref = 154,
            y = state.number(154) == 0 and 165 or 292}}}
        "#,
    )
    .unwrap();
    let mut summary = debug_boot_result_summary();
    summary.total_notes = 1552;
    // Legacy Result skins use ref 154 == 0 to choose their MAX sheet at load time.
    for (score, expected) in [(2969, 135), (3104, 0), (2759, 1), (2760, 344)] {
        summary.ex_score = score;
        let values = result_lua_runtime_number_values_for_summary(&summary);
        assert_eq!(values.get(&154), Some(&expected), "EX SCORE {score}");
        let state = bmz_render::skin::SkinDrawState {
            ex_score: score,
            total_notes: summary.total_notes,
            ..Default::default()
        };
        assert_eq!(i64::from(values[&154]), bmz_render::skin::lua_main_state_number(154, &state));
        let runtime = lua_runtime_state_for_result(
            false,
            None,
            true,
            false,
            summary.key_mode,
            values,
            "test",
        );
        assert_eq!(runtime.number_values.get(&154), Some(&expected));
        let decoded = crate::skin_loader::decode_beatoraja_skin_with_options_and_runtime_state(
            &skin_path,
            SkinKind::Result,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &runtime,
        )
        .expect("decode ref 154 Result fixture with summary");
        let value = decoded
            .document
            .value
            .iter()
            .find(|value| value.id == "rank_diff")
            .expect("next-rank number");
        assert_eq!(value.y, if expected == 0 { 165 } else { 292 }, "EX SCORE {score}");
        assert_eq!(value.ref_id, 154);
    }
}

#[test]
fn first_play_result_lua_values_match_beatoraja_missing_score_sentinels() {
    let mut summary = debug_boot_result_summary();
    summary.previous_best_ex_score = None;
    summary.previous_best_bp = None;

    let values = result_lua_runtime_number_values_for_summary(&summary);
    assert_eq!(values.get(&150), Some(&0));
    assert_eq!(values.get(&170), Some(&0));
    assert_eq!(values.get(&176), Some(&i32::MIN));
    assert_eq!(values.get(&178), Some(&i32::MIN));

    let mut runtime_state = bmz_skin::LuaLoadRuntimeState::default();
    apply_result_summary_lua_load_state(&mut runtime_state, &summary, "", "", "");
    assert_eq!(
        runtime_state.option_values.get(&bmz_render::skin::SKIN_OPTION_BMZ_FIRST_PLAY),
        Some(&true)
    );
    assert_eq!(runtime_state.option_values.get(&327), Some(&true));
    assert!((320..327).all(|option| runtime_state.option_values.get(&option) == Some(&false)));

    summary.previous_best_ex_score = Some(0);
    apply_result_summary_lua_load_state(&mut runtime_state, &summary, "", "", "");
    assert_eq!(
        runtime_state.option_values.get(&bmz_render::skin::SKIN_OPTION_BMZ_FIRST_PLAY),
        Some(&false)
    );
    assert_eq!(runtime_state.option_values.get(&327), Some(&true));
}

#[test]
fn result_lua_runtime_state_exposes_scene_options() {
    let online = lua_runtime_state_for_result(
        false,
        Some("BMZ IR"),
        true,
        true,
        KeyMode::K7,
        BTreeMap::new(),
        "Player",
    );
    assert_eq!(online.option_values.get(&50), Some(&false));
    assert_eq!(online.option_values.get(&51), Some(&true));
    assert_eq!(online.option_values.get(&60), Some(&false));
    assert_eq!(online.option_values.get(&61), Some(&true));
    assert_eq!(online.option_values.get(&32), Some(&false));
    assert_eq!(online.option_values.get(&33), Some(&true));
    assert_eq!(online.option_values.get(&160), Some(&true));
    assert_eq!(online.option_values.get(&161), Some(&false));
    assert_eq!(online.text_values.get(&1020).map(String::as_str), Some("BMZ IR"));

    let offline = lua_runtime_state_for_result(
        false,
        None,
        false,
        false,
        KeyMode::K5,
        BTreeMap::new(),
        "Player",
    );
    assert_eq!(offline.option_values.get(&50), Some(&true));
    assert_eq!(offline.option_values.get(&51), Some(&false));
    assert_eq!(offline.option_values.get(&60), Some(&true));
    assert_eq!(offline.option_values.get(&61), Some(&false));
    assert_eq!(offline.option_values.get(&32), Some(&true));
    assert_eq!(offline.option_values.get(&33), Some(&false));
    assert_eq!(offline.option_values.get(&160), Some(&false));
    assert_eq!(offline.option_values.get(&161), Some(&true));
    assert_eq!(offline.text_values.get(&1020).map(String::as_str), Some(""));
}

#[test]
fn result_ir_skin_name_uses_primary_provider_instead_of_registration_order() {
    use crate::config::profile_config::{
        IrConfig, IrProviderConfig, IrProviderRoleConfig, IrSendPolicyConfig,
    };

    let provider = |provider: &str, provider_key: &str, role| IrProviderConfig {
        provider: provider.to_string(),
        provider_key: provider_key.to_string(),
        base_url: "https://example.test/".to_string(),
        enabled: true,
        account_display_name: String::new(),
        account_id: String::new(),
        send_policy: IrSendPolicyConfig::default(),
        role,
        last_login_at: None,
        last_success_at: None,
    };
    let ir = IrConfig {
        primary_provider: "rian-ir".to_string(),
        providers: vec![
            provider("bmz", "bmz", IrProviderRoleConfig::SubmitOnly),
            provider("rian-ir", "rian-ir", IrProviderRoleConfig::Primary),
        ],
        ..IrConfig::default()
    };

    assert_eq!(result_ir_skin_name(&ir), Some("rianIR"));
}

#[test]
fn result_judge_rank_options_match_beatoraja_ranges() {
    for (rank, expected) in [
        (Some(0), Some(180)),
        (Some(34), Some(180)),
        (Some(1), Some(181)),
        (Some(59), Some(181)),
        (Some(2), Some(182)),
        (Some(84), Some(182)),
        (Some(3), Some(183)),
        (Some(109), Some(183)),
        (Some(4), Some(184)),
        (Some(110), Some(184)),
        (None, Some(182)),
    ] {
        assert_eq!(result_judge_rank_option_id(rank), expected, "rank {rank:?}");
    }
    assert_eq!(result_judge_rank_option_id(Some(9)), None);
}

#[test]
fn skin_video_source_runtime_visibility_follows_result_rank_op() {
    use bmz_render::skin::SkinDrawState;

    // ランク別 BG を op で出し分けるリザルトスキン構成 (Starseeker 相当)。
    let document: SkinDocument = serde_json::from_str(
            r#"
            {
                "type": 7,
                "source": [
                    { "id": "BG_A", "path": "BG/A/a.mp4" },
                    { "id": "BG_AAA", "path": "BG/AAA/aaa.mp4" }
                ],
                "image": [
                    { "id": "BG_A", "src": "BG_A", "x": 0, "y": 0, "w": 10, "h": 10 },
                    { "id": "BG_AAA", "src": "BG_AAA", "x": 0, "y": 0, "w": 10, "h": 10 }
                ],
                "destination": [
                    { "id": "BG_A", "op": [90, 302], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] },
                    { "id": "BG_AAA", "op": [90, 300], "dst": [{ "x": 0, "y": 0, "w": 10, "h": 10 }] }
                ]
            }
            "#,
        )
        .unwrap();

    let make_source = |source_id: &str| {
        let gating = skin_video_source_gating(&document, source_id);
        ActiveSkinVideoSource {
            texture: SkinTextureId(0),
            path: PathBuf::new(),
            decoder: None,
            last_pts: None,
            loop_start_us: 0,
            active: gating.active,
            gating_op_sets: gating.op_sets,
            enabled_options: document.enabled_options(),
            result_ranktime_ms: document.ranktime,
            failed: false,
        }
    };
    let bg_a = make_source("BG_A");
    let bg_aaa = make_source("BG_AAA");

    // ex_score / total_notes でランクが決まる。9/9 = AAA, 6/9 = A 付近。
    let aaa_state = SkinDrawState {
        result_failed: Some(false),
        ex_score: 18,
        total_notes: 9,
        ..SkinDrawState::default()
    };
    assert!(skin_video_source_runtime_visible(&bg_aaa, &aaa_state));
    assert!(!skin_video_source_runtime_visible(&bg_a, &aaa_state));

    // 13/18 = 72.2% は rank index 2 (= A), op 302 に対応する。
    let a_state = SkinDrawState {
        result_failed: Some(false),
        ex_score: 13,
        total_notes: 9,
        ..SkinDrawState::default()
    };
    assert!(skin_video_source_runtime_visible(&bg_a, &a_state));
    assert!(!skin_video_source_runtime_visible(&bg_aaa, &a_state));
}

#[test]
fn result_action_accepts_retry_and_leave_keys() {
    assert_eq!(
        result_action(PhysicalKey::Code(KeyCode::KeyR), ElementState::Pressed, false),
        Some(ResultAction::Retry)
    );
    assert_eq!(
        result_action(PhysicalKey::Code(KeyCode::Enter), ElementState::Pressed, false),
        Some(ResultAction::Leave)
    );
    assert_eq!(
        result_action(PhysicalKey::Code(KeyCode::Escape), ElementState::Pressed, false),
        Some(ResultAction::Leave)
    );
}

#[test]
fn result_action_rejects_releases_repeats_and_other_keys() {
    assert_eq!(
        result_action(PhysicalKey::Code(KeyCode::KeyR), ElementState::Released, false),
        None
    );
    assert_eq!(
        result_action(PhysicalKey::Code(KeyCode::Escape), ElementState::Pressed, true),
        None
    );
    assert_eq!(
        result_action(PhysicalKey::Code(KeyCode::Space), ElementState::Pressed, false),
        None
    );
}

#[test]
fn result_exit_skip_key_accepts_enter_and_escape_only_on_pressed() {
    assert!(result_exit_skip_key(PhysicalKey::Code(KeyCode::Enter), ElementState::Pressed, false));
    assert!(result_exit_skip_key(PhysicalKey::Code(KeyCode::Escape), ElementState::Pressed, false));
    assert!(!result_exit_skip_key(
        PhysicalKey::Code(KeyCode::Enter),
        ElementState::Released,
        false
    ));
    assert!(!result_exit_skip_key(PhysicalKey::Code(KeyCode::Escape), ElementState::Pressed, true));
    assert!(!result_exit_skip_key(PhysicalKey::Code(KeyCode::Space), ElementState::Pressed, false));
}

#[test]
fn result_exit_skip_waits_for_animation_and_holds_final_frame_once() {
    let animation_duration = Duration::from_millis(1_000);
    let fadeout = Duration::from_millis(3_000);

    assert!(!result_exit_transition_ready(
        Duration::from_millis(999),
        fadeout,
        animation_duration,
        true,
        false,
    ));
    assert!(!result_exit_transition_ready(
        animation_duration,
        fadeout,
        animation_duration,
        true,
        false,
    ));
    assert!(result_exit_transition_ready(
        animation_duration,
        fadeout,
        animation_duration,
        true,
        true,
    ));
}

#[test]
fn result_exit_without_skip_still_waits_for_skin_fadeout() {
    let fadeout = Duration::from_millis(3_000);
    let animation_duration = Duration::from_millis(1_000);

    assert!(!result_exit_transition_ready(
        animation_duration,
        fadeout,
        animation_duration,
        false,
        false,
    ));
    assert!(result_exit_transition_ready(fadeout, fadeout, animation_duration, false, false,));
}

#[test]
fn lane_skips_result_exit_matches_1p_and_2p_requested_keys() {
    for lane in [Lane::Key1, Lane::Key3, Lane::Key8, Lane::Key10, Lane::Key12, Lane::Key14] {
        assert!(lane_skips_result_exit(lane), "{lane:?} should skip");
    }
    for lane in [
        Lane::Scratch,
        Lane::Key2,
        Lane::Key4,
        Lane::Key5,
        Lane::Key6,
        Lane::Key7,
        Lane::Key9,
        Lane::Key11,
        Lane::Key13,
        Lane::Scratch2,
    ] {
        assert!(!lane_skips_result_exit(lane), "{lane:?} should not skip");
    }
}

#[test]
fn result_exit_lanes_match_requested_mapping() {
    // BMZ では Key2 を「戻る」系に寄せるため、終了開始から外す。
    for lane in [Lane::Key1, Lane::Key3, Lane::Key4, Lane::Key5, Lane::Key7] {
        assert!(lane_starts_result_exit(lane), "{lane:?} should start result exit");
    }
    // Key6 は CHANGE_GRAPH、scratch は無割り当て。
    for lane in [Lane::Scratch, Lane::Key2, Lane::Key6] {
        assert!(!lane_starts_result_exit(lane), "{lane:?} should not start result exit");
    }
}

#[test]
fn result_gauge_graph_cycle_matches_beatoraja_order() {
    assert_eq!(cycle_result_gauge_graph_type(GaugeType::Normal as i32), GaugeType::Hard as i32);
    assert_eq!(cycle_result_gauge_graph_type(GaugeType::Hard as i32), GaugeType::ExHard as i32);
    assert_eq!(
        cycle_result_gauge_graph_type(GaugeType::Hazard as i32),
        GaugeType::AssistEasy as i32
    );
    assert_eq!(cycle_result_gauge_graph_type(GaugeType::Class as i32), GaugeType::ExClass as i32);
    assert_eq!(
        cycle_result_gauge_graph_type(GaugeType::ExHardClass as i32),
        GaugeType::Class as i32
    );
}

#[test]
fn result_skin_event_90_toggles_favorite_without_invisible_state() {
    assert_eq!(result_skin_click_action(90), Some(ResultSkinClickAction::ToggleFavoriteChart));
    assert_eq!(
        result_skin_click_action(SKIN_EVENT_DAILY_STATISTICS_RESET),
        Some(ResultSkinClickAction::ResetDailyStatistics)
    );
    assert_eq!(
        result_skin_click_action(SKIN_EVENT_RESULT_PANEL_IR),
        Some(ResultSkinClickAction::SetPanel(1))
    );
    assert_eq!(
        result_skin_click_action(SKIN_EVENT_IR_SCOPE_GLOBAL),
        Some(ResultSkinClickAction::SelectIrScope(
            crate::screens::result_ir::ResultRankingTab::Global
        ))
    );
    assert_eq!(
        result_skin_click_action(SKIN_EVENT_IR_SCOPE_RIVAL),
        Some(ResultSkinClickAction::SelectIrScope(
            crate::screens::result_ir::ResultRankingTab::SelfAndRivals
        ))
    );
    assert_eq!(
        result_skin_click_action(SKIN_EVENT_IR_SCOPE_TOGGLE),
        Some(ResultSkinClickAction::ToggleIrScope)
    );
    assert_eq!(result_skin_click_action(91), None);
}

#[test]
fn result_skin_replay_events_map_all_four_slots() {
    assert_eq!(result_skin_click_action(19), Some(ResultSkinClickAction::SaveReplay(0)));
    assert_eq!(result_skin_click_action(316), Some(ResultSkinClickAction::SaveReplay(1)));
    assert_eq!(result_skin_click_action(317), Some(ResultSkinClickAction::SaveReplay(2)));
    assert_eq!(result_skin_click_action(318), Some(ResultSkinClickAction::SaveReplay(3)));
    assert_eq!(result_skin_click_action(319), None);
}

#[test]
fn result_panel_toggle_requires_supported_skin_and_ir() {
    assert_eq!(toggled_result_panel(1, true, true), Some(2));
    assert_eq!(toggled_result_panel(2, true, true), Some(1));
    assert_eq!(toggled_result_panel(0, true, true), None);
    assert_eq!(toggled_result_panel(1, false, true), None);
    assert_eq!(toggled_result_panel(1, true, false), None);
}

#[test]
fn result_panel_arrow_keys_match_luxe_flat_direction() {
    assert_eq!(
        result_panel_for_control(&PhysicalControl::KeyboardKey("ArrowLeft".to_string())),
        Some(2)
    );
    assert_eq!(
        result_panel_for_control(&PhysicalControl::KeyboardKey("ArrowRight".to_string())),
        Some(1)
    );
    assert_eq!(
        result_panel_for_control(&PhysicalControl::KeyboardKey("ArrowUp".to_string())),
        None
    );
}

#[test]
fn result_panel_support_requires_default_and_compiled_or_lua_draw_gate() {
    let document: SkinDocument = serde_json::from_value(serde_json::json!({
        "type": 7,
        "resultPanelDefault": 2,
        "destination": [{
            "id": "panel",
            "draw": "result_panel(2)",
            "dst": [{"x": 0, "y": 0, "w": 1, "h": 1}]
        }]
    }))
    .unwrap();
    assert!(result_panel_supported(&document));

    let mut compat = document.clone();
    let DestinationListEntry::Single(destination) = &mut compat.destination[0] else {
        panic!("single destination");
    };
    destination.draw = "bmz:lua_draw_callback:0".into();
    assert!(result_panel_supported(&compat));
    compat.result_panel_default = None;
    assert!(!result_panel_supported(&compat));

    let without_gate: SkinDocument = serde_json::from_value(serde_json::json!({
        "type": 7,
        "resultPanelDefault": 2,
        "destination": []
    }))
    .unwrap();
    assert!(!result_panel_supported(&without_gate));
}

#[test]
fn result_ir_scroll_supports_panel_and_always_visible_skins() {
    let always_visible: SkinDocument = serde_json::from_value(serde_json::json!({
        "type": 7,
        "slider": [{"id": "ir", "type": 8}]
    }))
    .unwrap();
    assert!(result_ir_scroll_supported(&always_visible, 0));

    let panel: SkinDocument = serde_json::from_value(serde_json::json!({
        "type": 7,
        "resultPanelDefault": 2,
        "slider": [{"id": "ir", "type": 8}],
        "destination": [{
            "id": "ir",
            "draw": "result_panel(1)",
            "dst": [{"x": 0, "y": 0, "w": 1, "h": 1}]
        }]
    }))
    .unwrap();
    assert!(result_ir_scroll_supported(&panel, 1));
    assert!(!result_ir_scroll_supported(&panel, 2));

    let without_slider: SkinDocument =
        serde_json::from_value(serde_json::json!({"type": 7})).unwrap();
    assert!(!result_ir_scroll_supported(&without_slider, 0));
}

#[test]
fn result_ir_scroll_controls_match_select_navigation() {
    let keys = default_select_keys();
    assert_eq!(result_ir_scroll_rows_for_control("ArrowUp", &keys), Some(-1));
    assert_eq!(result_ir_scroll_rows_for_control("ArrowDown", &keys), Some(1));
    assert_eq!(result_ir_scroll_rows_for_control("DPadUp", &keys), Some(-1));
    assert_eq!(result_ir_scroll_rows_for_control("DPadDown", &keys), Some(1));
    assert_eq!(result_ir_scroll_rows_for_control("Axis1+", &keys), Some(-1));
    assert_eq!(result_ir_scroll_rows_for_control("Axis1-", &keys), Some(1));
    assert_eq!(result_ir_scroll_rows_for_control("Button1", &keys), None);
}

#[test]
fn result_action_resolves_from_held_lanes() {
    // beatoraja 準拠: Key5 のみ → 別配置 (REPLAY_DIFFERENT)。
    assert_eq!(result_action_for_held_lanes(true, false), Some(ResultRetryMode::DifferentArrange));
    // Key7 のみ → 同配置 (REPLAY_SAME)。
    assert_eq!(result_action_for_held_lanes(false, true), Some(ResultRetryMode::SameArrange));
    // 両押し → 同配置 (ユーザー仕様)。
    assert_eq!(result_action_for_held_lanes(true, true), Some(ResultRetryMode::SameArrange));
    // どちらも非押下 → 選曲へ戻る。
    assert_eq!(result_action_for_held_lanes(false, false), None);
}

#[test]
fn result_exit_audio_gain_uses_shorter_skin_fadeout() {
    let fadeout = Duration::from_millis(600);

    assert_eq!(result_exit_audio_gain(Duration::ZERO, fadeout), 1.0);
    assert!((result_exit_audio_gain(Duration::from_millis(300), fadeout) - 0.5).abs() < 0.001);
    assert_eq!(result_exit_audio_gain(fadeout, fadeout), 0.0);
}

#[test]
fn result_exit_audio_gain_caps_long_skin_fadeout() {
    let fadeout = Duration::from_millis(3_000);

    assert!((result_exit_audio_gain(Duration::from_millis(750), fadeout) - 0.5).abs() < 0.001);
    assert_eq!(result_exit_audio_gain(RESULT_EXIT_AUDIO_FADE, fadeout), 0.0);
}

#[test]
fn result_exit_audio_gain_is_zero_for_zero_fadeout() {
    assert_eq!(result_exit_audio_gain(Duration::ZERO, Duration::ZERO), 0.0);
}

#[test]
fn result_exit_cleanup_only_targets_result_sounds() {
    use crate::system_sound::SoundType;

    let sounds = result_exit_system_sounds();

    assert!(sounds.contains(&SoundType::ResultClear));
    assert!(sounds.contains(&SoundType::ResultFail));
    assert!(sounds.contains(&SoundType::ResultClose));
    assert!(sounds.contains(&SoundType::CourseClear));
    assert!(sounds.contains(&SoundType::CourseFail));
    assert!(sounds.contains(&SoundType::CourseClose));
    assert!(!sounds.contains(&SoundType::Select));
    assert!(!sounds.contains(&SoundType::Decide));
    assert!(!sounds.contains(&SoundType::OptionChange));
    assert!(!sounds.contains(&SoundType::Landmine));
}

#[test]
fn result_entry_sound_uses_fail_for_failed_play() {
    use crate::system_sound::SoundType;

    assert_eq!(result_entry_sound_for_clear(ClearType::Failed), SoundType::ResultFail);
    assert_eq!(result_entry_sound_for_clear(ClearType::Normal), SoundType::ResultClear);
    assert_eq!(course_result_entry_sound_for_clear(ClearType::Failed), SoundType::CourseFail);
    assert_eq!(course_result_entry_sound_for_clear(ClearType::Normal), SoundType::CourseClear);
}
