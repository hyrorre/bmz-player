use super::*;

pub(super) fn result_skin_click_action(event_id: i32) -> Option<ResultSkinClickAction> {
    match event_id {
        SKIN_EVENT_RESULT_PANEL_IR => Some(ResultSkinClickAction::SetPanel(1)),
        SKIN_EVENT_RESULT_PANEL_GRAPH => Some(ResultSkinClickAction::SetPanel(2)),
        SKIN_EVENT_IR_SCOPE_GLOBAL => Some(ResultSkinClickAction::SelectIrScope(
            crate::screens::result_ir::ResultRankingTab::Global,
        )),
        SKIN_EVENT_IR_SCOPE_RIVAL => Some(ResultSkinClickAction::SelectIrScope(
            crate::screens::result_ir::ResultRankingTab::SelfAndRivals,
        )),
        SKIN_EVENT_IR_SCOPE_TOGGLE => Some(ResultSkinClickAction::ToggleIrScope),
        SKIN_EVENT_DAILY_STATISTICS_RESET => Some(ResultSkinClickAction::ResetDailyStatistics),
        90 => Some(ResultSkinClickAction::ToggleFavoriteChart),
        19 => Some(ResultSkinClickAction::SaveReplay(0)),
        316..=318 => Some(ResultSkinClickAction::SaveReplay((event_id - 315) as u8)),
        _ => None,
    }
}

/// Result IR の相対スクロール量を返す。末尾方向を正とする。
pub(super) fn result_ir_scroll_rows_for_control(
    control: &str,
    bindings: &SelectKeyBindings,
) -> Option<i32> {
    match control {
        "ArrowUp" | "DPadUp" => Some(-1),
        "ArrowDown" | "DPadDown" => Some(1),
        _ if bindings.is_select_scratch_down(control) => Some(1),
        _ if bindings.is_select_scratch_up(control) => Some(-1),
        _ => None,
    }
}

/// コース曲間の中間リザルトかどうか。active_course を保持したまま finished_play
/// だけが立ち、finished_course はまだ無い状態を指す。中間リザルトでは retry を
/// 無効化し、次の曲へ進むだけにする (beatoraja MusicResult のコース分岐相当)。
pub(super) fn is_course_intermediate_result(
    active_course: bool,
    finished_course: bool,
    finished_play: bool,
) -> bool {
    active_course && finished_play && !finished_course
}

pub(super) fn toggled_result_panel(
    current: i32,
    supported: bool,
    ir_available: bool,
) -> Option<i32> {
    if !ir_available {
        return None;
    }
    let requested = match current {
        1 => 2,
        2 => 1,
        _ => return None,
    };
    selected_result_panel(current, requested, supported, ir_available)
}

pub(super) fn selected_result_panel(
    current: i32,
    requested: i32,
    supported: bool,
    ir_available: bool,
) -> Option<i32> {
    if !supported || current == requested {
        return None;
    }
    match requested {
        1 if ir_available => Some(1),
        2 if matches!(current, 1 | 2) => Some(2),
        _ => None,
    }
}

pub(super) fn result_failed_for_skin_ops(
    display_clear_type: ClearType,
    raw_clear_type: Option<ClearType>,
) -> bool {
    matches!(raw_clear_type.unwrap_or(display_clear_type), ClearType::Failed | ClearType::NoPlay)
}

pub(super) fn course_intermediate_exit_action_for_state(
    failed: bool,
    has_next_chart: bool,
) -> ResultExitAction {
    if failed || !has_next_chart {
        ResultExitAction::FinishCourse
    } else {
        ResultExitAction::AdvanceCourse
    }
}

pub(super) fn should_show_course_stage_result(
    failed: bool,
    has_next_entry: bool,
    has_next_chart: bool,
) -> bool {
    failed || has_next_chart || !has_next_entry
}

pub(super) fn result_skin_signature_for_config(
    skin: &crate::config::profile_config::SkinConfig,
    slot: ResultSkinSlot,
    mut runtime_state: bmz_skin::LuaLoadRuntimeState,
) -> ResultSkinSignature {
    runtime_state.offset_values.clear();
    runtime_state.offset_id_values.clear();
    match slot {
        ResultSkinSlot::Normal => {
            apply_skin_offsets_to_lua_runtime_state(&mut runtime_state, &skin.result_offsets);
            (
                slot,
                skin.result.trim().to_string(),
                skin.result_options.clone(),
                skin.result_files.clone(),
                runtime_state,
            )
        }
        ResultSkinSlot::Course => {
            apply_skin_offsets_to_lua_runtime_state(
                &mut runtime_state,
                &skin.course_result_offsets,
            );
            (
                slot,
                skin.course_result.trim().to_string(),
                skin.course_result_options.clone(),
                skin.course_result_files.clone(),
                runtime_state,
            )
        }
    }
}

pub(super) fn result_lua_runtime_number_values_for_summary(
    summary: &ResultSummary,
) -> BTreeMap<i32, i32> {
    let mut number_values = BTreeMap::new();
    let value = |value: u32| i32::try_from(value).unwrap_or(i32::MAX);
    let difference = |current: u32, previous: u32| value(current).saturating_sub(value(previous));
    number_values.insert(71, value(summary.ex_score));
    number_values.insert(74, value(summary.total_notes));
    number_values.insert(101, value(summary.ex_score));
    number_values.insert(171, value(summary.ex_score));
    number_values.insert(107, summary.gauge_value.floor().clamp(0.0, i32::MAX as f32) as i32);
    number_values.insert(110, value(summary.judge_counts.pgreat));
    number_values.insert(111, value(summary.judge_counts.great));
    number_values.insert(112, value(summary.judge_counts.good));
    number_values.insert(113, value(summary.judge_counts.bad));
    number_values.insert(114, value(summary.judge_counts.poor));
    let previous_best_ex_score = summary.previous_best_ex_score.unwrap_or(0);
    number_values.insert(150, value(previous_best_ex_score));
    number_values.insert(170, value(previous_best_ex_score));
    number_values.insert(152, difference(summary.ex_score, previous_best_ex_score));
    number_values.insert(172, difference(summary.ex_score, previous_best_ex_score));
    if let Some(target_ex_score) = summary.target_ex_score {
        number_values.insert(121, value(target_ex_score));
        number_values.insert(151, value(target_ex_score));
        number_values.insert(153, difference(summary.ex_score, target_ex_score));
    }
    number_values.insert(177, value(summary.bp));
    let counts = summary.fast_slow_counts;
    number_values.insert(410, value(counts.fast_pgreat));
    number_values.insert(411, value(counts.slow_pgreat));
    number_values.insert(412, value(counts.fast_great));
    number_values.insert(413, value(counts.slow_great));
    number_values.insert(414, value(counts.fast_good));
    number_values.insert(415, value(counts.slow_good));
    number_values.insert(416, value(counts.fast_bad));
    number_values.insert(417, value(counts.slow_bad));
    number_values.insert(418, value(counts.fast_poor));
    number_values.insert(419, value(counts.slow_poor));
    number_values.insert(420, value(summary.judge_counts.empty_poor));
    number_values.insert(421, value(counts.fast_empty_poor));
    number_values.insert(422, value(counts.slow_empty_poor));
    number_values.insert(
        423,
        value(
            counts
                .fast_great
                .saturating_add(counts.fast_good)
                .saturating_add(counts.fast_bad)
                .saturating_add(counts.fast_poor)
                .saturating_add(counts.fast_empty_poor),
        ),
    );
    number_values.insert(
        424,
        value(
            counts
                .slow_great
                .saturating_add(counts.slow_good)
                .saturating_add(counts.slow_bad)
                .saturating_add(counts.slow_poor)
                .saturating_add(counts.slow_empty_poor),
        ),
    );
    number_values.insert(425, i32::try_from(summary.cb).unwrap_or(i32::MAX));
    number_values.insert(
        426,
        value(summary.judge_counts.poor.saturating_add(summary.judge_counts.empty_poor)),
    );
    number_values.insert(
        427,
        value(
            summary
                .judge_counts
                .bad
                .saturating_add(summary.judge_counts.poor)
                .saturating_add(summary.judge_counts.empty_poor),
        ),
    );
    number_values.insert(370, summary.clear_type as i32);
    number_values.insert(371, summary.previous_best_clear_type.unwrap_or(ClearType::NoPlay) as i32);
    if let Some((average_timing_ms, _)) = summary.graph.timing_distribution.stats() {
        number_values.insert(374, average_timing_ms as i32);
        number_values.insert(375, (average_timing_ms * 100.0) as i32 % 100);
    }
    if let Some(previous_best_bp) = summary.previous_best_bp {
        number_values.insert(176, value(previous_best_bp));
        if let (Ok(current), Ok(previous)) =
            (i32::try_from(summary.bp), i32::try_from(previous_best_bp))
        {
            number_values.insert(178, current.saturating_sub(previous));
        }
    } else {
        // beatorajaの空ScoreDataはminbp=Integer.MAX_VALUEを持ち、skin ref
        // 176/178ではInteger.MIN_VALUEへ変換される。Luaの`misscount < 0`
        // 判定を保ちつつ、rendererの通常number表示はNoneのまま非表示にする。
        number_values.insert(176, i32::MIN);
        number_values.insert(178, i32::MIN);
    }
    number_values
}

pub(super) fn apply_course_mode_lua_options(
    runtime_state: &mut bmz_skin::LuaLoadRuntimeState,
    stage: Option<CourseStageMarker>,
) {
    runtime_state.option_values.insert(290, true);
    for option in [280, 281, 282, 283, 289] {
        runtime_state.option_values.insert(option, false);
    }
    let option = match stage {
        Some(CourseStageMarker::Stage1) => Some(280),
        Some(CourseStageMarker::Stage2) => Some(281),
        Some(CourseStageMarker::Stage3) => Some(282),
        Some(CourseStageMarker::Stage4) => Some(283),
        Some(CourseStageMarker::Final) => Some(289),
        None => None,
    };
    if let Some(option) = option {
        runtime_state.option_values.insert(option, true);
    }
}

pub(super) fn apply_course_result_lua_load_state(
    runtime_state: &mut bmz_skin::LuaLoadRuntimeState,
    course: &CourseResultSummary,
) {
    for (index, title) in course.course_titles.iter().enumerate() {
        runtime_state.text_values.insert(150 + index as i32, title.clone());
    }
    let course_result = course_result_skin_snapshot(course);
    runtime_state.number_values.insert(
        bmz_render::skin::SKIN_REF_BMZ_COURSE_STAGE_COUNT,
        course_result.stage_count as i32,
    );
    for (index, stage) in course_result.stages.iter().enumerate() {
        let index = index as i32;
        runtime_state.number_values.insert(
            bmz_render::skin::SKIN_REF_BMZ_COURSE_STAGE_EX_BASE + index,
            i32::try_from(stage.ex_score).unwrap_or(i32::MAX),
        );
        runtime_state.number_values.insert(
            bmz_render::skin::SKIN_REF_BMZ_COURSE_STAGE_GAUGE_BASE + index,
            stage.gauge.floor() as i32,
        );
        runtime_state.number_values.insert(
            bmz_render::skin::SKIN_REF_BMZ_COURSE_STAGE_BP_BASE + index,
            i32::try_from(stage.bp).unwrap_or(i32::MAX),
        );
        runtime_state.number_values.insert(
            bmz_render::skin::SKIN_REF_BMZ_COURSE_STAGE_RATE_BASE + index,
            i32::try_from(stage.rate_basis_points).unwrap_or(i32::MAX),
        );
    }

    // WMII stores these values from each intermediate MusicResult in
    // `skin/WMII_FHD/result/courseData.json`, then reads them from CourseResult.
    // Lua filesystem writes are intentionally disabled in BMZ, so expose the
    // equivalent attempt data as an app-owned, read-only virtual file.
    let songs = course
        .entry_summaries
        .iter()
        .enumerate()
        .map(|(index, summary)| {
            let max_ex_score = summary.total_notes.saturating_mul(2);
            let rate = if max_ex_score > 0 {
                f64::from(summary.ex_score) / f64::from(max_ex_score)
            } else {
                0.0
            };
            serde_json::json!({
                "stage": index + 1,
                "score": summary.ex_score,
                "gauge": summary.gauge_value.floor(),
                "miss": summary.bp,
                "rate": rate,
            })
        })
        .collect::<Vec<_>>();
    let course_data = serde_json::json!({ "songs": songs }).to_string();
    runtime_state
        .virtual_io_files
        .insert("skin/WMII_FHD/result/courseData.json".to_string(), course_data);
}

pub(super) fn apply_result_summary_lua_load_state(
    runtime_state: &mut bmz_skin::LuaLoadRuntimeState,
    summary: &ResultSummary,
    table_primary: &str,
    table_level: &str,
    table_full: &str,
) {
    runtime_state.option_values.insert(
        bmz_render::skin::SKIN_OPTION_BMZ_FIRST_PLAY,
        summary.previous_best_ex_score.is_none(),
    );
    for option in 320..=327 {
        runtime_state.option_values.insert(option, false);
    }
    if let Some(option) =
        result_best_rank_option_id(summary.previous_best_ex_score.unwrap_or(0), summary.total_notes)
    {
        runtime_state.option_values.insert(option, true);
    }
    let full_title = if summary.subtitle.is_empty() {
        summary.title.clone()
    } else {
        format!("{} {}", summary.title, summary.subtitle)
    };
    let full_artist = if summary.subartist.is_empty() {
        summary.artist.clone()
    } else {
        format!("{} {}", summary.artist, summary.subartist)
    };
    runtime_state.text_values.extend([
        (1, summary.target_name.clone()),
        (3, bmz_render::skin::target_setting_name(&summary.target.as_string())),
        (10, summary.title.clone()),
        (11, summary.subtitle.clone()),
        (12, full_title),
        (13, summary.genre.clone()),
        (14, summary.artist.clone()),
        (15, summary.subartist.clone()),
        (16, full_artist),
        (1001, table_primary.to_string()),
        (1002, table_level.to_string()),
        (1003, table_full.to_string()),
    ]);
    for option in 180..=184 {
        runtime_state.option_values.insert(option, false);
    }
    if let Some(option) = result_judge_rank_option_id(summary.judge_rank) {
        runtime_state.option_values.insert(option, true);
    }
    runtime_state.event_index_values.insert(
        308,
        i32::try_from(result_long_note_mode_index(summary.long_note_mode)).unwrap_or_default(),
    );
    runtime_state.event_index_values.insert(
        42,
        i32::try_from(bmz_render::skin::select_arrange_index(&summary.arrange)).unwrap_or_default(),
    );
    runtime_state.event_index_values.insert(
        43,
        i32::try_from(bmz_render::skin::select_arrange_index(&summary.arrange_2p))
            .unwrap_or_default(),
    );
    runtime_state.event_index_values.insert(
        344,
        i32::try_from(bmz_render::skin::extended_arrange_index(&summary.arrange))
            .unwrap_or_default(),
    );
    runtime_state.event_index_values.insert(
        345,
        i32::try_from(bmz_render::skin::extended_arrange_index(&summary.arrange_2p))
            .unwrap_or_default(),
    );
}

fn result_best_rank_option_id(ex_score: u32, total_notes: u32) -> Option<i32> {
    let max_score = total_notes.checked_mul(2)?;
    if max_score == 0 {
        return None;
    }
    let score = u64::from(ex_score.min(max_score));
    let max = u64::from(max_score);
    let rank = if score * 9 >= max * 8 {
        0
    } else if score * 9 >= max * 7 {
        1
    } else if score * 9 >= max * 6 {
        2
    } else if score * 9 >= max * 5 {
        3
    } else if score * 9 >= max * 4 {
        4
    } else if score * 9 >= max * 3 {
        5
    } else if score * 9 >= max * 2 {
        6
    } else {
        7
    };
    Some(320 + rank)
}

pub(super) fn result_judge_rank_option_id(judge_rank: Option<i32>) -> Option<i32> {
    let Some(rank) = judge_rank else {
        return Some(182);
    };
    match rank {
        0 | 10..=34 => Some(180),
        1 | 35..=59 => Some(181),
        2 | 60..=84 => Some(182),
        3 | 85..=109 => Some(183),
        4 | 110.. => Some(184),
        _ => None,
    }
}

pub(super) fn result_long_note_mode_index(mode: bmz_chart::model::LongNoteMode) -> usize {
    match mode {
        bmz_chart::model::LongNoteMode::Ln => 0,
        bmz_chart::model::LongNoteMode::Cn => 1,
        bmz_chart::model::LongNoteMode::Hcn => 2,
    }
}

pub(super) fn result_ir_skin_name(
    ir_config: &crate::config::profile_config::IrConfig,
) -> Option<&str> {
    let provider = crate::ir::provider_key::primary_provider_config(ir_config)?;
    crate::ir::provider_key::configured_provider_display_name(provider)
}

pub(super) fn lua_runtime_state_for_result(
    table_song: bool,
    ir_name: Option<&str>,
    score_save_enabled: bool,
    autoplay: bool,
    key_mode: KeyMode,
    mut number_values: BTreeMap<i32, i32>,
    player_name: &str,
) -> bmz_skin::LuaLoadRuntimeState {
    let ir_online = ir_name.is_some();
    let mut option_values = BTreeMap::new();
    option_values.insert(1008, table_song);
    option_values.insert(50, !ir_online);
    option_values.insert(51, ir_online);
    option_values.insert(60, !score_save_enabled);
    option_values.insert(61, score_save_enabled);
    option_values.insert(32, !autoplay);
    option_values.insert(33, autoplay);
    for option in 160..=164 {
        option_values.insert(option, result_key_mode_option_matches(option, key_mode));
    }
    extend_bmz_key_mode_lua_state(&mut number_values, &mut option_values, key_mode);
    bmz_skin::LuaLoadRuntimeState {
        number_values,
        text_values: BTreeMap::from([
            (2, player_name.to_string()),
            (1020, ir_name.unwrap_or_default().to_string()),
        ]),
        option_values,
        ..Default::default()
    }
}

pub(super) fn lua_runtime_state_for_play(
    options: &PlayStartOptions,
    profile_autoplay: bool,
    key_mode: KeyMode,
    previous_best_ex_score: Option<u32>,
    player_name: &str,
    skin_attempt: bmz_render::snapshot::SkinAttemptState,
) -> bmz_skin::LuaLoadRuntimeState {
    let replay_playback = options.replay_player.is_some();
    let practice_mode = options.session_mode.is_practice();
    let autoplay = !replay_playback
        && !practice_mode
        && (options.session_mode.primary_autoplay() || profile_autoplay || options.autoplay);
    let score_save_enabled = options.session_mode.score_save_enabled()
        && !autoplay
        && !replay_playback
        && !options.score_save_disabled;
    let mut option_values = BTreeMap::from([
        (32, !autoplay),
        (33, autoplay),
        (60, !score_save_enabled),
        (61, score_save_enabled),
        (82, !autoplay && !replay_playback),
        (84, replay_playback),
        (1080, practice_mode),
        (bmz_render::skin::SKIN_OPTION_BMZ_FIRST_PLAY, previous_best_ex_score.is_none()),
    ]);
    // beatorajaのPracticePlayerは保存済みベストを読まず、highscoreを0にする。
    // 初回判定optionは実際の保存履歴から独立して保持する。
    let previous_best_ex_score =
        if practice_mode { 0 } else { previous_best_ex_score.unwrap_or(0) };
    let mut number_values = BTreeMap::from([
        (150, i32::try_from(previous_best_ex_score).unwrap_or(i32::MAX)),
        (170, i32::try_from(previous_best_ex_score).unwrap_or(i32::MAX)),
    ]);
    extend_bmz_key_mode_lua_state(&mut number_values, &mut option_values, key_mode);
    let target_name = bmz_render::skin::play_target_name(
        &options.target.as_string(),
        options
            .rival_name
            .as_deref()
            .or_else(|| options.resolved_target.as_ref().map(|target| target.name.as_str())),
    );
    let mut runtime_state = bmz_skin::LuaLoadRuntimeState {
        number_values,
        text_values: BTreeMap::from([
            (1, target_name.clone()),
            (2, player_name.to_string()),
            (3, bmz_render::skin::target_setting_name(&options.target.as_string())),
        ]),
        option_values,
        ..Default::default()
    };
    apply_skin_attempt_lua_load_state(&mut runtime_state, skin_attempt);
    runtime_state
}

pub(super) fn apply_skin_attempt_lua_load_state(
    runtime_state: &mut bmz_skin::LuaLoadRuntimeState,
    attempt: bmz_render::snapshot::SkinAttemptState,
) {
    use bmz_render::skin::*;
    use bmz_render::snapshot::{
        SKIN_SOURCE_LN_DEFINED_CN_BIT, SKIN_SOURCE_LN_DEFINED_HCN_BIT,
        SKIN_SOURCE_LN_DEFINED_LN_BIT, SKIN_SOURCE_LN_UNDEFINED_BIT,
    };
    runtime_state
        .option_values
        .insert(SKIN_OPTION_BMZ_BEST_SCORE_OPTIONS_AVAILABLE, attempt.best_score_options.is_some());
    for ref_id in SKIN_REF_BMZ_BEST_SCORE_ARRANGE_1P..=SKIN_REF_BMZ_BEST_SCORE_DOUBLE_OPTION {
        let index = attempt
            .best_score_options
            .and_then(|options| options.index(ref_id))
            .map_or(-1, |index| index as i32);
        runtime_state.number_values.insert(ref_id, index);
        runtime_state.event_index_values.insert(ref_id, index);
        runtime_state.text_values.insert(
            ref_id,
            attempt
                .best_score_options
                .map(|options| options.label(ref_id).to_string())
                .unwrap_or_default(),
        );
    }

    if let Some(mode) = attempt.effective_key_mode {
        extend_bmz_key_mode_lua_state(
            &mut runtime_state.number_values,
            &mut runtime_state.option_values,
            mode,
        );
    }
    if let Some(mode) = attempt.source_key_mode {
        let value = bmz_key_mode_number(mode);
        runtime_state.number_values.insert(SKIN_REF_BMZ_SOURCE_KEY_MODE, value);
        runtime_state.event_index_values.insert(SKIN_REF_BMZ_SOURCE_KEY_MODE, value);
        for option in SKIN_OPTION_BMZ_SOURCE_KEY_MODE_BASE..=SKIN_OPTION_BMZ_SOURCE_KEY_MODE_LAST {
            let effective_option =
                SKIN_OPTION_BMZ_KEY_MODE_BASE + option - SKIN_OPTION_BMZ_SOURCE_KEY_MODE_BASE;
            runtime_state
                .option_values
                .insert(option, bmz_key_mode_option_matches(effective_option, mode));
        }
    }
    runtime_state.option_values.insert(SKIN_OPTION_BMZ_SEVEN_TO_SIX, attempt.seven_to_six);
    for (ref_id, value) in [
        (360, i32::from(attempt.seven_to_nine_pattern)),
        (361, i32::from(attempt.seven_to_nine_type)),
    ] {
        runtime_state.number_values.insert(ref_id, value);
        runtime_state.event_index_values.insert(ref_id, value);
    }

    if let Some(bits) = attempt.source_ln_profile_bits {
        runtime_state.number_values.insert(SKIN_REF_BMZ_SOURCE_LN_PROFILE, i32::from(bits));
        runtime_state.event_index_values.insert(SKIN_REF_BMZ_SOURCE_LN_PROFILE, i32::from(bits));
        runtime_state
            .option_values
            .insert(SKIN_OPTION_BMZ_SOURCE_LN_UNDEFINED, bits & SKIN_SOURCE_LN_UNDEFINED_BIT != 0);
        runtime_state.option_values.insert(
            SKIN_OPTION_BMZ_SOURCE_LN_DEFINED_LN,
            bits & SKIN_SOURCE_LN_DEFINED_LN_BIT != 0,
        );
        runtime_state.option_values.insert(
            SKIN_OPTION_BMZ_SOURCE_LN_DEFINED_CN,
            bits & SKIN_SOURCE_LN_DEFINED_CN_BIT != 0,
        );
        runtime_state.option_values.insert(
            SKIN_OPTION_BMZ_SOURCE_LN_DEFINED_HCN,
            bits & SKIN_SOURCE_LN_DEFINED_HCN_BIT != 0,
        );
        runtime_state.option_values.insert(SKIN_OPTION_BMZ_SOURCE_LN_MIXED, bits.count_ones() > 1);
        runtime_state.option_values.insert(SKIN_OPTION_BMZ_SOURCE_LN_PROFILE_AVAILABLE, true);
    }

    for (ref_id, value) in [
        (SKIN_REF_BMZ_SELECT_SESSION_MODE, attempt.session_mode_index),
        (54, attempt.double_option_index),
        (55, attempt.hsfix_index),
        (78, attempt.gauge_auto_shift_index),
        (308, attempt.ln_mode_index),
        (340, attempt.judge_algorithm_index),
        (341, attempt.bottom_shiftable_gauge_index),
    ] {
        if let Some(value) = value.and_then(|value| i32::try_from(value).ok()) {
            runtime_state.number_values.insert(ref_id, value);
            runtime_state.event_index_values.insert(ref_id, value);
        }
    }
    if let Some(has_bga) = attempt.has_bga {
        runtime_state.option_values.insert(170, !has_bga);
        runtime_state.option_values.insert(171, has_bga);
    }
    if let Some(has_random) = attempt.has_random_sequence {
        runtime_state.option_values.insert(178, !has_random);
        runtime_state.option_values.insert(179, has_random);
    }
}

pub(super) fn lua_runtime_state_for_frontend(
    player_name: &str,
    ir_name: Option<&str>,
) -> bmz_skin::LuaLoadRuntimeState {
    bmz_skin::LuaLoadRuntimeState {
        text_values: BTreeMap::from([
            (2, player_name.to_string()),
            (1020, ir_name.unwrap_or_default().to_string()),
        ]),
        option_values: BTreeMap::from([(50, ir_name.is_none()), (51, ir_name.is_some())]),
        ..bmz_skin::LuaLoadRuntimeState::default()
    }
}

pub(super) fn result_key_mode_option_matches(option: i32, key_mode: KeyMode) -> bool {
    match option {
        160 => matches!(key_mode, KeyMode::K7 | KeyMode::K8),
        161 => key_mode == KeyMode::K5,
        162 => key_mode == KeyMode::K14,
        163 => key_mode == KeyMode::K10,
        164 => key_mode == KeyMode::K9,
        _ => false,
    }
}

pub(super) fn extend_bmz_key_mode_lua_state(
    number_values: &mut BTreeMap<i32, i32>,
    option_values: &mut BTreeMap<i32, bool>,
    key_mode: KeyMode,
) {
    number_values.insert(SKIN_REF_BMZ_KEY_MODE, bmz_key_mode_number(key_mode));
    number_values.insert(SKIN_REF_BMZ_ACTIVE_LANE_COUNT, key_mode.lane_count() as i32);
    for option in SKIN_OPTION_BMZ_KEY_MODE_BASE
        ..SKIN_OPTION_BMZ_KEY_MODE_BASE + SKIN_OPTION_BMZ_KEY_MODE_COUNT as i32
    {
        option_values.insert(option, bmz_key_mode_option_matches(option, key_mode));
    }
    for option in
        [SKIN_OPTION_BMZ_NO_SCRATCH, SKIN_OPTION_BMZ_SINGLE_PLAY, SKIN_OPTION_BMZ_DOUBLE_PLAY]
    {
        option_values.insert(option, bmz_key_mode_option_matches(option, key_mode));
    }
}

pub(super) fn bmz_key_mode_number(key_mode: KeyMode) -> i32 {
    match key_mode {
        KeyMode::K4 => 4,
        KeyMode::K5 => 5,
        KeyMode::K6 => 6,
        KeyMode::K7 => 7,
        KeyMode::K8 => 8,
        KeyMode::K9 => 9,
        KeyMode::K10 => 10,
        KeyMode::K14 => 14,
    }
}

pub(super) fn bmz_key_mode_option_matches(option: i32, key_mode: KeyMode) -> bool {
    match option - SKIN_OPTION_BMZ_KEY_MODE_BASE {
        0 => key_mode == KeyMode::K4,
        1 => key_mode == KeyMode::K5,
        2 => key_mode == KeyMode::K6,
        3 => key_mode == KeyMode::K7,
        4 => key_mode == KeyMode::K8,
        5 => key_mode == KeyMode::K9,
        6 => key_mode == KeyMode::K10,
        7 => key_mode == KeyMode::K14,
        _ if option == SKIN_OPTION_BMZ_NO_SCRATCH => {
            matches!(key_mode, KeyMode::K4 | KeyMode::K6 | KeyMode::K8 | KeyMode::K9)
        }
        _ if option == SKIN_OPTION_BMZ_SINGLE_PLAY => matches!(key_mode, KeyMode::K5 | KeyMode::K7),
        _ if option == SKIN_OPTION_BMZ_DOUBLE_PLAY => {
            matches!(key_mode, KeyMode::K10 | KeyMode::K14)
        }
        _ => false,
    }
}

/// リザルト画面で押すと終了アニメーションを開始するレーン。
/// BMZ では Key1/3/5/7 を「次へ進む」、Key2/4/6 を「戻る/変更」系に寄せるため、
/// beatoraja と異なり Key2 は終了開始に使わない。
/// Key6 は CHANGE_GRAPH、scratch は無割り当てなので開始しない。
pub(super) fn lane_starts_result_exit(lane: Lane) -> bool {
    matches!(lane, Lane::Key1 | Lane::Key3 | Lane::Key4 | Lane::Key5 | Lane::Key7)
}

pub(super) fn lane_skips_result_exit(lane: Lane) -> bool {
    matches!(lane, Lane::Key1 | Lane::Key3 | Lane::Key8 | Lane::Key10 | Lane::Key12 | Lane::Key14)
}

pub(super) fn retry_preload_kind(
    mode: ResultRetryMode,
    cached_chart_available: bool,
) -> RetryPreloadKind {
    match mode {
        ResultRetryMode::SameArrange if cached_chart_available => {
            RetryPreloadKind::CachedChartWithFreshAudio
        }
        ResultRetryMode::SameArrange | ResultRetryMode::DifferentArrange => {
            RetryPreloadKind::ReimportedChartWithFreshAudio
        }
    }
}

/// フェードアウト終了時の Key5/Key7 押下状態から遷移を決める。
/// beatoraja 準拠: Key5=別配置 (REPLAY_DIFFERENT)、Key7=同配置 (REPLAY_SAME)。
/// - Key7 押下 (両押し含む) → 同配置 (SameArrange)
/// - Key5 のみ押下 → 別配置 (DifferentArrange)
/// - どちらも非押下 → None (選曲へ戻る)
///
/// beatoraja は両押し時に index の若い Key5 (DIFFERENT) を優先するが、
/// 本実装はユーザー仕様として両押しを SameArrange とする。
pub(super) fn result_action_for_held_lanes(
    key5_held: bool,
    key7_held: bool,
) -> Option<ResultRetryMode> {
    match (key5_held, key7_held) {
        (_, true) => Some(ResultRetryMode::SameArrange),
        (true, false) => Some(ResultRetryMode::DifferentArrange),
        (false, false) => None,
    }
}
