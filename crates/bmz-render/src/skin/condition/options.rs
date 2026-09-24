use super::*;

/// destination の全 option 条件を現在の描画状態に対して評価する。
pub fn test_skin_ops(ops: &[i32], enabled_options: &[i32], state: &SkinDrawState) -> bool {
    ops.iter().all(|op| test_skin_op(*op, enabled_options, state))
}

pub(in crate::skin) fn destination_ops_match(
    destination: &SkinDestinationDef,
    enabled_options: &[i32],
    state: &SkinDrawState,
) -> bool {
    if is_grade_diff_rank_destination(destination, state) {
        return destination
            .op
            .iter()
            .all(|&op| test_grade_diff_rank_op(destination, op, enabled_options, state));
    }
    test_skin_ops(&destination.op, enabled_options, state)
}

fn test_grade_diff_rank_op(
    destination: &SkinDestinationDef,
    op: i32,
    enabled_options: &[i32],
    state: &SkinDrawState,
) -> bool {
    if op < 0 {
        return op.checked_neg().is_some_and(|positive| {
            !test_grade_diff_rank_op(destination, positive, enabled_options, state)
        });
    }
    match op {
        300..=307 => grade_diff_rank_destination_matches(destination, op, state),
        _ => test_skin_op(op, enabled_options, state),
    }
}

pub(in crate::skin) fn test_skin_op(
    op: i32,
    enabled_options: &[i32],
    state: &SkinDrawState,
) -> bool {
    if op < 0 {
        return op
            .checked_neg()
            .is_some_and(|positive| !test_skin_op(positive, enabled_options, state));
    }
    match op {
        40 => !state.bga_enabled,
        SKIN_OPTION_BMZ_BEST_SCORE_OPTIONS_AVAILABLE => {
            state.skin_attempt.best_score_options.is_some()
        }
        41 => state.bga_enabled,
        1901 => skin_hispeed_mode_is_floating(state),
        SKIN_OPTION_BMZ_RESULT_IR_SCOPE_GLOBAL => {
            state.ir_ranking.scope == crate::scene::ResultIrScope::Global
        }
        SKIN_OPTION_BMZ_RESULT_IR_SCOPE_RIVAL => {
            state.ir_ranking.scope == crate::scene::ResultIrScope::Rival
        }
        SKIN_OPTION_BMZ_RESULT_IR_SCOPE_GLOBAL_SUPPORTED => state.ir_ranking.global_scope_supported,
        SKIN_OPTION_BMZ_RESULT_IR_SCOPE_RIVAL_SUPPORTED => state.ir_ranking.rival_scope_supported,
        // Deprecated grade-difference mode options use a fixed NEXT fallback.
        SKIN_OPTION_BMZ_GRADE_DIFF_NEAREST => false,
        SKIN_OPTION_BMZ_GRADE_DIFF_NEXT => true,
        SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_CURRENT => {
            score_grade_facts(state).is_some_and(ScoreGradeFacts::nearest_is_current)
        }
        SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_NEXT => {
            score_grade_facts(state).is_some_and(ScoreGradeFacts::nearest_is_next)
        }
        SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_EXACT => {
            score_grade_facts(state).is_some_and(|facts| facts.nearest_diff == 0)
        }
        SKIN_OPTION_BMZ_SCORE_GRADE_NEAREST_TIE => {
            score_grade_facts(state).is_some_and(|facts| facts.nearest_tie)
        }
        SKIN_OPTION_BMZ_SCORE_GRADE_AVAILABLE => score_grade_facts(state).is_some(),
        SKIN_OPTION_BMZ_FIRST_PLAY => {
            if state.result_failed.is_some() {
                state.previous_best_ex_score.is_none()
            } else {
                state.play_screen && state.best_ex_score.is_none()
            }
        }
        SKIN_OPTION_BMZ_RULE_MODE_BASE..=SKIN_OPTION_BMZ_RULE_MODE_LAST => {
            state.rule_mode_index == (op - SKIN_OPTION_BMZ_RULE_MODE_BASE) as usize
        }
        SKIN_OPTION_BMZ_LN_POLICY_SETTING_BASE..=SKIN_OPTION_BMZ_LN_POLICY_SETTING_LAST => state
            .ln_policy_setting_index
            .is_some_and(|index| index == (op - SKIN_OPTION_BMZ_LN_POLICY_SETTING_BASE) as usize),
        SKIN_OPTION_BMZ_LN_POLICY_SETTING_AUTO => {
            state.ln_policy_setting_index.is_some_and(|index| index < 3)
        }
        SKIN_OPTION_BMZ_LN_POLICY_SETTING_FORCE => {
            state.ln_policy_setting_index.is_some_and(|index| index >= 3)
        }
        SKIN_OPTION_BMZ_LN_SCORE_POLICY_BASE..=SKIN_OPTION_BMZ_LN_SCORE_POLICY_LAST => state
            .ln_score_policy_index
            .is_some_and(|index| index == (op - SKIN_OPTION_BMZ_LN_SCORE_POLICY_BASE) as usize),
        SKIN_OPTION_BMZ_LN_SCORE_POLICY_AUTO => {
            state.ln_score_policy_index.is_some_and(|index| index < 3)
        }
        SKIN_OPTION_BMZ_LN_SCORE_POLICY_FORCE => {
            state.ln_score_policy_index.is_some_and(|index| index >= 3)
        }
        SKIN_OPTION_BMZ_LN_SCORE_POLICY_AVAILABLE => state.ln_score_policy_index.is_some(),
        SKIN_OPTION_BMZ_SOURCE_KEY_MODE_BASE..=SKIN_OPTION_BMZ_SOURCE_KEY_MODE_LAST => {
            state.skin_attempt.source_key_mode.is_some_and(|mode| {
                key_mode_option_matches(
                    SKIN_OPTION_BMZ_KEY_MODE_BASE + (op - SKIN_OPTION_BMZ_SOURCE_KEY_MODE_BASE),
                    mode,
                )
            })
        }
        SKIN_OPTION_BMZ_SEVEN_TO_SIX => state.skin_attempt.seven_to_six,
        SKIN_OPTION_BMZ_SOURCE_LN_UNDEFINED => state
            .skin_attempt
            .source_ln_profile_bits
            .is_some_and(|bits| bits & SKIN_SOURCE_LN_UNDEFINED_BIT != 0),
        SKIN_OPTION_BMZ_SOURCE_LN_DEFINED_LN => state
            .skin_attempt
            .source_ln_profile_bits
            .is_some_and(|bits| bits & SKIN_SOURCE_LN_DEFINED_LN_BIT != 0),
        SKIN_OPTION_BMZ_SOURCE_LN_DEFINED_CN => state
            .skin_attempt
            .source_ln_profile_bits
            .is_some_and(|bits| bits & SKIN_SOURCE_LN_DEFINED_CN_BIT != 0),
        SKIN_OPTION_BMZ_SOURCE_LN_DEFINED_HCN => state
            .skin_attempt
            .source_ln_profile_bits
            .is_some_and(|bits| bits & SKIN_SOURCE_LN_DEFINED_HCN_BIT != 0),
        SKIN_OPTION_BMZ_SOURCE_LN_MIXED => {
            state.skin_attempt.source_ln_profile_bits.is_some_and(|bits| bits.count_ones() > 1)
        }
        SKIN_OPTION_BMZ_SOURCE_LN_PROFILE_AVAILABLE => {
            state.skin_attempt.source_ln_profile_bits.is_some()
        }
        SKIN_OPTION_BMZ_INPUT_BASE..=SKIN_OPTION_BMZ_INPUT_LAST => {
            state.logical_input_held[(op - SKIN_OPTION_BMZ_INPUT_BASE) as usize]
        }
        SKIN_OPTION_BMZ_E1_E2_HELD => state.logical_input_held[0] || state.logical_input_held[1],
        SKIN_OPTION_BMZ_JUDGE_LANE_PGREAT_BASE..=SKIN_OPTION_BMZ_JUDGE_LANE_PGREAT_LAST => {
            state.judge_lane_index[(op - SKIN_OPTION_BMZ_JUDGE_LANE_PGREAT_BASE) as usize]
                == Some(0)
        }
        SKIN_OPTION_BMZ_JUDGE_LANE_FAST_BASE..=SKIN_OPTION_BMZ_JUDGE_LANE_FAST_LAST => {
            state.judge_lane_timing_sign[(op - SKIN_OPTION_BMZ_JUDGE_LANE_FAST_BASE) as usize]
                == Some(1)
        }
        SKIN_OPTION_BMZ_JUDGE_LANE_SLOW_BASE..=SKIN_OPTION_BMZ_JUDGE_LANE_SLOW_LAST => {
            state.judge_lane_timing_sign[(op - SKIN_OPTION_BMZ_JUDGE_LANE_SLOW_BASE) as usize]
                == Some(-1)
        }
        1 => matches!(
            state.select_row_kind,
            SelectRowKind::Folder
                | SelectRowKind::TableFolder
                | SelectRowKind::SearchFolder
                | SelectRowKind::Command
                | SelectRowKind::Container
                | SelectRowKind::SettingsRoot
                | SelectRowKind::SettingsFolder
                | SelectRowKind::SettingsBack
                | SelectRowKind::SettingsClose
        ),
        SKIN_OPTION_BMZ_SETTINGS_FOLDER => matches!(
            state.select_row_kind,
            SelectRowKind::SettingsRoot | SelectRowKind::SettingsFolder
        ),
        SKIN_OPTION_BMZ_SETTINGS_BACK => state.select_row_kind == SelectRowKind::SettingsBack,
        SKIN_OPTION_BMZ_SETTINGS_CLOSE => state.select_row_kind == SelectRowKind::SettingsClose,
        2 => select_song_detail_row(state),
        3 => state.select_row_kind == SelectRowKind::Course,
        1030 => state.select_row_kind == SelectRowKind::Executable,
        1031 => state.select_row_kind == SelectRowKind::RandomCourse,
        1008 => state.table_song,
        1002..=1017 => gradebar_constraint_op_matches(op, state),
        5 => {
            !state.in_settings
                && (matches!(state.select_row_kind, SelectRowKind::Executable)
                    || (state.select_in_library
                        && !state.select_is_folder
                        && matches!(
                            state.select_row_kind,
                            SelectRowKind::Song
                                | SelectRowKind::Course
                                | SelectRowKind::RandomCourse
                        )))
        }
        // OPTION_OFFLINE / OPTION_ONLINE. beatoraja は設定済み IR 接続の有無を
        // 返す。結果スキンでは 51 が IR 送信完了/失敗の timer 173/174 を
        // 描画する前提条件としても使われる。
        50 => !state.ir_ranking.online,
        51 => state.ir_ranking.online,
        // SelectedBarClearDrawCondition: these lamps describe the selected score,
        // not the gauge selected for the next play.
        100..=105 | 1100..=1104 => {
            let clear_index = match op {
                100 => 0,
                101 => 1,
                1100 => 2,
                1101 => 3,
                102 => 4,
                103 => 5,
                104 => 6,
                1102 => 7,
                105 => 8,
                1103 => 9,
                1104 => 10,
                _ => unreachable!(),
            };
            state.select_screen
                && !state.in_settings
                && state.select_clear_index == clear_index
                && (op != 100
                    || matches!(state.select_row_kind, SelectRowKind::Song | SelectRowKind::Course))
        }
        21 => state.select_option_panel == 1,
        22 => state.select_option_panel == 2,
        23 => state.select_option_panel == 3,
        160..=164 => select_key_mode_option_matches(op, state),
        1160 | 1161 => select_key_mode_option_matches(op, state),
        SKIN_OPTION_BMZ_KEY_MODE_BASE..=SKIN_OPTION_BMZ_KEY_MODE_LAST => {
            select_key_mode_option_matches(op, state)
        }
        SKIN_OPTION_BMZ_NO_SCRATCH | SKIN_OPTION_BMZ_SINGLE_PLAY | SKIN_OPTION_BMZ_DOUBLE_PLAY => {
            select_key_mode_option_matches(op, state)
        }
        196 | 197 | 198 | 1196..=1208 if state.result_failed.is_some() => {
            result_replay_op_matches(op, state)
        }
        126..=131 | 1128..=1131 if state.result_failed.is_some() => {
            result_arrange_op_matches(op, state)
        }
        196 | 197 | 198 | 1196..=1208 => select_replay_op_matches(op, state),
        200..=207 => select_rank_op_matches(op, state),
        // OPTION_AAA..OPTION_F (220..227)。beatoraja の ScoreDataProperty.rank
        // と同じく、現在の EX スコアが譜面全体の 27 段階閾値に到達したかを返す。
        220..=227 => play_rank_option_matches(op, state),
        300..=318 if state.result_failed.is_some() => result_rank_op_matches(op, state),
        300..=307 => select_small_rank_op_matches(op, state),
        320..=327 => best_rank_op_matches(op, state),
        // OPTION_NO_LN / OPTION_LN. 譜面未選択・未確定時は両方 false。
        // Play / ResultではLN policy適用後の実効譜面を使う。
        172 => state.chart_has_long_notes.is_some_and(|has_long_notes| !has_long_notes),
        173 => state.chart_has_long_notes == Some(true),
        170 => {
            state.skin_attempt.has_bga.or_else(|| (!state.select_screen).then_some(state.has_bga))
                == Some(false)
        }
        171 => {
            state.skin_attempt.has_bga.or_else(|| (!state.select_screen).then_some(state.has_bga))
                == Some(true)
        }
        // SongDataBooleanProperty returns false for both branches without a selected song.
        174 => select_song_option_matches(state) && !state.select_has_document,
        175 => select_song_option_matches(state) && state.select_has_document,
        // OPTION_BPMCHANGE (BPM変化あり) / OPTION_BPMSTOP (STOP命令あり)
        176 => chart_metadata_option_available(state) && state.min_bpm >= state.max_bpm,
        177 => chart_metadata_option_available(state) && state.min_bpm < state.max_bpm,
        178 => state.skin_attempt.has_random_sequence == Some(false),
        179 => state.skin_attempt.has_random_sequence == Some(true),
        1177 => state.has_bpm_stop,
        // OPTION_NOW_LOADING / OPTION_LOADED
        80 => !state.skin_loaded,
        81 => state.skin_loaded,
        // OPTION_NO_STAGEFILE / OPTION_STAGEFILE
        190 => !state.has_stagefile,
        191 => state.has_stagefile,
        // OPTION_NO_BANNER / OPTION_BANNER (192/193)
        192 => select_banner_option_matches(false, state),
        193 => select_banner_option_matches(true, state),
        // OPTION_NO_BACKBMP / OPTION_BACKBMP
        194 => !state.has_backbmp,
        195 => state.has_backbmp,
        // OPTION_LANECOVER1_CHANGING / OPTION_LANECOVER1_ON / OPTION_LIFT1_ON / OPTION_HIDDEN1_ON
        270 => state.lane_cover_changing,
        271 => state.lanecover_enabled,
        272 => state.lift_enabled,
        273 => state.hidden_enabled,
        400 => state.constant_enabled,
        // OPTION_1P_0_9 .. OPTION_1P_100. beatoraja evaluates these only on
        // BMSPlayer and compares the displayed gauge value with its configured maximum.
        230..=240 => gauge_range_option_matches(op, state),
        // Judgement-existence options are live on BMSPlayer as well as Result.
        // Play skins use negative BAD/POOR ops to gate full-combo effects.
        // EmptyPoor is beatoraja's MISS bucket.
        2241 => state.judge_counts.pgreat > 0,
        2242 => state.judge_counts.great > 0,
        2243 => state.judge_counts.good > 0,
        2244 => state.judge_counts.bad > 0,
        2245 => state.judge_counts.poor > 0,
        2246 => state.judge_counts.empty_poor > 0,
        // Result/update comparison options. In play skins these are often reused
        // as target-reached draw conditions.
        330 => state.previous_best_ex_score.is_some_and(|best| state.ex_score > best),
        1330 => state.previous_best_ex_score.is_some_and(|best| state.ex_score == best),
        331 => state.previous_best_max_combo.is_some_and(|best| state.max_combo > best),
        1331 => state.previous_best_max_combo.is_some_and(|best| state.max_combo == best),
        332 => state.previous_best_bp.is_some_and(|best| current_bp(state) < best),
        1332 => state.previous_best_bp.is_some_and(|best| current_bp(state) == best),
        335 => state.previous_best_ex_score.is_some_and(|best| {
            score_rate_cmp_value(state.ex_score, state.total_notes)
                > score_rate_cmp_value(best, state.total_notes)
        }),
        1335 => state.previous_best_ex_score.is_some_and(|best| {
            score_rate_cmp_value(state.ex_score, state.total_notes)
                == score_rate_cmp_value(best, state.total_notes)
        }),
        336 => state.target_ex_score.is_some_and(|target| state.ex_score > target),
        1336 => state.target_ex_score.is_some_and(|target| state.ex_score == target),
        350 => true,
        351 => false,
        352 => state.target_ex_score.is_some_and(|target| state.ex_score > target),
        353 => state.target_ex_score.is_some_and(|target| state.ex_score < target),
        354 => state.target_ex_score.is_some_and(|target| state.ex_score == target),
        // OPTION_GAUGE_GROOVE / OPTION_GAUGE_HARD / OPTION_GAUGE_EX.
        // beatoraja uses the current gauge type index: 0..2 are groove-family,
        // 3+ are hard-family, and 1046 is true for assist/easy/ex variants.
        42 => state.gauge_type <= 2,
        43 => state.gauge_type >= 3,
        1046 => matches!(state.gauge_type, 0 | 1 | 4 | 5 | 7 | 8),
        // OPTION_NOT_COMPARE_RIVAL / OPTION_COMPARE_RIVAL。
        // beatoraja は選択中ライバルの有無で判定し、対象譜面のスコア有無は見ない。
        624 => !state.rival_selected,
        625 => state.rival_selected,
        // OPTION_IR_LOADING / LOADED / NOPLAYER / FAILED (601..604)。
        601 => matches!(state.ir_ranking.state, crate::scene::ResultIrState::Loading),
        602 => matches!(state.ir_ranking.state, crate::scene::ResultIrState::Loaded),
        603 => {
            matches!(state.ir_ranking.state, crate::scene::ResultIrState::Loaded)
                && state.ir_ranking.total_player == Some(0)
        }
        604 => matches!(state.ir_ranking.state, crate::scene::ResultIrState::Failed),
        // beatoraja MusicSelector: ranking object生成前はWAITING。
        606 => matches!(state.ir_ranking.state, crate::scene::ResultIrState::Waiting),
        // BooleanPropertyFactory のIR_BUSYは現行beatorajaでFAILと同条件。
        608 => matches!(state.ir_ranking.state, crate::scene::ResultIrState::Failed),
        // BANNED / ACCESSING は現行beatorajaにもproperty実装がない。
        605 | 607 => false,
        // OPTION_DIFFICULTY0..5. 0 は UNKNOWN/OTHER、1..5 は BMS #DIFFICULTY。
        150 => state.difficulty <= 0 || state.difficulty > 5,
        151..=155 => state.difficulty == i64::from(op - 150),
        // OPTION_JUDGE_VERYHARD..VERYEASY (180..184)
        180..=184 => {
            !(state.select_screen && state.in_settings)
                && select_chart_metadata_available(state)
                && judge_rank_option_matches(op, state.judge_rank)
        }
        // OPTION_RESULT_CLEAR=90, OPTION_RESULT_FAIL=91
        // Result 画面以外 (result_failed == None) では両方 false。
        90 => state.result_failed == Some(false),
        91 => state.result_failed == Some(true),
        // OPTION_AUTOPLAYOFF / OPTION_AUTOPLAYON
        32 => !state.autoplay,
        33 => state.autoplay,
        // PlayerResource.updateScore. Select など対象外 scene では両方 false。
        60 => state.score_save_enabled == Some(false),
        61 => state.score_save_enabled == Some(true),
        // BMSPlayer play mode. beatoraja では 82 は PLAY/PRACTICE、84 は REPLAY。
        82 => state.play_screen && !state.autoplay && !state.replay_playback,
        84 => state.play_screen && state.replay_playback,
        1080 => state.play_screen && state.practice_mode,
        // LR2 OPTION_1P/2P/3P_PERFECT..MISS and EARLY/LATE judge-detail conditions.
        // beatoraja maps FAST/EARLY to positive recent judge timing, LATE/SLOW to negative.
        // beatoraja の EARLY/LATE は `getNowJudge(player) > 1` も要求するため、
        // ThresholdMs=0 で timing sign が残る場合でも PGREAT とは同時成立しない。
        241..=246 => state.judge_index[0] == Some((op - 241) as usize),
        1242 => {
            state.judge_index[0].is_some_and(|index| index > 0)
                && state.judge_timing_sign[0] == Some(1)
        }
        1243 => {
            state.judge_index[0].is_some_and(|index| index > 0)
                && state.judge_timing_sign[0] == Some(-1)
        }
        261..=266 => state.judge_index[1] == Some((op - 261) as usize),
        1262 => {
            state.judge_index[1].is_some_and(|index| index > 0)
                && state.judge_timing_sign[1] == Some(1)
        }
        1263 => {
            state.judge_index[1].is_some_and(|index| index > 0)
                && state.judge_timing_sign[1] == Some(-1)
        }
        361..=366 => state.judge_index[2] == Some((op - 361) as usize),
        1362 => {
            state.judge_index[2].is_some_and(|index| index > 0)
                && state.judge_timing_sign[2] == Some(1)
        }
        1363 => {
            state.judge_index[2].is_some_and(|index| index > 0)
                && state.judge_timing_sign[2] == Some(-1)
        }
        // OPTION_COURSE_STAGE1..4 / OPTION_COURSE_STAGE_FINAL
        280 => state.course_stage == Some(CourseStageMarker::Stage1),
        281 => state.course_stage == Some(CourseStageMarker::Stage2),
        282 => state.course_stage == Some(CourseStageMarker::Stage3),
        283 => state.course_stage == Some(CourseStageMarker::Stage4),
        289 => state.course_stage == Some(CourseStageMarker::Final),
        // OPTION_MODE_COURSE
        290 => state.course_stage.is_some(),
        // beatoraja defines OPTION_MODE_NONSTOP / EXPERT / GRADE (291..293)
        // but does not expose BooleanProperty handlers for them.  Return
        // false here instead of falling through to skin property defaults.
        291..=293 => false,
        value => test_json_option_number(value, enabled_options),
    }
}

fn play_rank_option_matches(op: i32, state: &SkinDrawState) -> bool {
    if state.total_notes == 0 {
        return false;
    }
    let rank_index = if op == 227 { 0 } else { 24_i64.saturating_sub(i64::from(op - 220) * 3) };
    let threshold_numerator = rank_index.max(0);
    i64::from(state.ex_score) * 27 >= i64::from(state.total_notes) * 2 * threshold_numerator
}

fn chart_metadata_option_available(state: &SkinDrawState) -> bool {
    state.chart_has_long_notes.is_some()
}

pub(in crate::skin) fn gauge_range_option_matches(op: i32, state: &SkinDrawState) -> bool {
    if !state.play_screen || state.gauge_max <= 0.0 {
        return false;
    }
    let range = (op - 230) as f32;
    let value = state.gauge / state.gauge_max;
    value >= range * 0.1 && value < (range + 1.0) * 0.1
}

pub(in crate::skin) fn gradebar_constraint_op_matches(op: i32, state: &SkinDrawState) -> bool {
    if state.select_row_kind != SelectRowKind::Course {
        return false;
    }
    let constraints = state.select_course_constraints;
    match op {
        1002 => constraints.class,
        1003 => constraints.mirror,
        1004 => constraints.random,
        1005 => constraints.no_speed,
        1006 => constraints.no_good,
        1007 => constraints.no_great,
        1010 => constraints.gauge_lr2,
        1011 => constraints.gauge_5k,
        1012 => constraints.gauge_7k,
        1013 => constraints.gauge_9k,
        1014 => constraints.gauge_24k,
        1015 => constraints.ln,
        1016 => constraints.cn,
        1017 => constraints.hcn,
        _ => false,
    }
}
