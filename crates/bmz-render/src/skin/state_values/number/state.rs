use super::*;

pub(in crate::skin) fn skin_state_number(ref_id: i32, state: &SkinDrawState) -> Option<i64> {
    if state.select_screen
        && state.duration_green_ms.is_none()
        && (matches!(ref_id, 55 | 310 | 311 | 342 | 1900..=1902 | 1916..=1918)
            || (ref_id == 12 && state.select_option_panel == 3))
    {
        return None;
    }

    if select_folder_lamp_counts_available(state)
        && let Some(value) = select_folder_lamp_count(ref_id, &state.select_folder_lamp_counts)
    {
        return Some(value);
    }

    if state.select_screen
        && select_chart_score_number_requires_song(ref_id)
        && !select_score_metadata_available(state)
    {
        return None;
    }

    if state.select_screen
        && select_chart_detail_number_requires_song(ref_id)
        && !select_chart_metadata_available(state)
    {
        return None;
    }

    if state.select_screen && state.in_settings {
        if let Some(value) = select_settings_screen_number(ref_id, state) {
            return Some(value);
        }
        if select_settings_screen_number_hidden(ref_id) {
            return None;
        }
    }
    match ref_id {
        // Lua draw 畳み込みのプレースホルダ (`number(0) >= 0` 等)
        0 => Some(0),
        17 => Some(player_stat_u64(state.player_stats.playtime_seconds / 3600)),
        18 => Some(player_stat_u64((state.player_stats.playtime_seconds / 60) % 60)),
        19 => Some(player_stat_u64(state.player_stats.playtime_seconds % 60)),
        20 => Some(i64::from(state.current_fps)),
        21..=26 => current_datetime_number(ref_id),
        27 => Some(operating_time_seconds(state) / 3_600),
        28 => Some((operating_time_seconds(state) / 60) % 60),
        29 => Some(operating_time_seconds(state) % 60),
        161 => Some(i64::from(state.play_timer_ms.unwrap_or(0).max(0) / 60_000)),
        162 => Some(i64::from((state.play_timer_ms.unwrap_or(0).max(0) / 1_000) % 60)),
        42 => Some(arrange_ref_index(state) as i64),
        43 => Some(arrange_2p_ref_index(state) as i64),
        344 => Some(extended_arrange_ref_index(state) as i64),
        345 => Some(extended_arrange_2p_ref_index(state) as i64),
        54 => {
            Some(state.skin_attempt.double_option_index.unwrap_or(state.select_double_option_index)
                as i64)
        }
        55 => Some(state.skin_attempt.hsfix_index.unwrap_or(state.select_hs_fix_index) as i64),
        11 if state.select_screen => Some(state.select_mode_index as i64),
        12 if state.select_screen && state.select_option_panel == 3 => {
            Some(state.judge_timing_offset_ms as i64)
        }
        12 if state.select_screen => Some(state.select_sort_index as i64),
        221 if state.select_screen => Some(state.select_difficulty_filter_index as i64),
        300 if state.select_screen => state.select_folder_song_count.map(i64::from),
        30 => Some(player_stat_u64(state.player_stats.play_count)),
        31 => Some(player_stat_u64(state.player_stats.clear_count)),
        32 => Some(player_stat_u64(
            state.player_stats.play_count.saturating_sub(state.player_stats.clear_count),
        )),
        33 => Some(player_stat_u64(player_total_pgreat(&state.player_stats))),
        34 => Some(player_stat_u64(player_total_great(&state.player_stats))),
        35 => Some(player_stat_u64(player_total_good(&state.player_stats))),
        36 => Some(player_stat_u64(player_total_bad(&state.player_stats))),
        37 => Some(player_stat_u64(player_total_poor(&state.player_stats))),
        45..=49 | 96 => {
            Some(if state.play_level != 0 { state.play_level } else { state.select_play_level })
        }
        370 => Some(state.select_clear_index),
        92 if state.select_screen => {
            if !select_chart_metadata_available(state) {
                return None;
            }
            Some(select_chart_main_bpm(state).unwrap_or(state.select_bpm).round() as i64)
        }
        92 => Some(state.main_bpm.round() as i64),
        100 => Some(skin_point_score(state) as i64),
        71 | 101 | 171 => Some(state.ex_score as i64),
        72 => Some(state.total_notes as i64 * 2),
        74 | 106 => Some(state.total_notes.max(state.select_total_notes) as i64),
        333 => Some(player_stat_u64(player_total_play_notes(&state.player_stats))),
        350 if state.select_screen => Some(select_chart_normal_notes(state) as i64),
        351 if state.select_screen => Some(state.select_chart_long_notes as i64),
        352 if state.select_screen => Some(state.select_chart_scratch_notes as i64),
        353 if state.select_screen => Some(state.select_chart_long_scratch_notes as i64),
        354 if state.select_screen => Some(state.select_chart_mine_notes as i64),
        360 if state.select_screen => Some(state.select_chart_peak_density.floor() as i64),
        361 if state.select_screen => Some(decimal_afterdot(state.select_chart_peak_density)),
        360 => Some(i64::from(state.skin_attempt.seven_to_nine_pattern)),
        361 => Some(i64::from(state.skin_attempt.seven_to_nine_type)),
        362 if state.select_screen => Some(state.select_chart_end_density.floor() as i64),
        363 if state.select_screen => Some(decimal_afterdot(state.select_chart_end_density)),
        364 if state.select_screen => Some(state.select_chart_density.floor() as i64),
        365 if state.select_screen => Some(decimal_afterdot(state.select_chart_density)),
        // beatoraja chart_totalgauge(368): effective BMS #TOTAL from SongInformation.
        368 => Some(state.select_chart_total_gauge.floor() as i64),
        75 | 105 | 174 => Some(state.max_combo as i64),
        76 if state.select_screen => state.select_bp.map(|count| count as i64).or(Some(0)),
        76 if state.result_failed.is_some() => Some(current_bp(state) as i64),
        76 => Some((state.judge_counts.bad + state.judge_counts.poor) as i64),
        77 if state.select_screen => Some(state.select_play_count as i64),
        77 => Some(state.select_target_index as i64),
        78 if state.select_screen => Some(state.select_clear_count as i64),
        78 => Some(
            state.skin_attempt.gauge_auto_shift_index.unwrap_or(state.select_gauge_auto_shift_index)
                as i64,
        ),
        79 if state.select_screen
            && (state.select_ex_score.is_some()
                || state.select_play_count > 0
                || state.select_clear_count > 0) =>
        {
            Some(state.select_play_count.saturating_sub(state.select_clear_count) as i64)
        }
        341 => Some(
            state
                .skin_attempt
                .bottom_shiftable_gauge_index
                .unwrap_or(state.select_bottom_shiftable_gauge_index) as i64,
        ),
        342 => Some(i64::from(state.hispeed_auto_adjust)),
        80 | 110 => Some(state.judge_counts.pgreat as i64),
        81 | 111 => Some(state.judge_counts.great as i64),
        82 | 112 => Some(state.judge_counts.good as i64),
        83 | 113 => Some(state.judge_counts.bad as i64),
        84 | 114 => Some(state.judge_counts.poor as i64),
        85 => judge_rate_int(state.judge_counts.pgreat, state.total_notes),
        86 => judge_rate_int(state.judge_counts.great, state.total_notes),
        87 => judge_rate_int(state.judge_counts.good, state.total_notes),
        88 => judge_rate_int(state.judge_counts.bad, state.total_notes),
        89 => judge_rate_int(state.judge_counts.poor, state.total_notes),
        102 => Some(current_score_rate_parts(state).0 as i64),
        103 => Some(current_score_rate_parts(state).1 as i64),
        115 | 155 => Some(score_rate_parts(state.ex_score, state.total_notes).0 as i64),
        116 | 156 => Some(score_rate_parts(state.ex_score, state.total_notes).1 as i64),
        104 => Some(state.combo as i64),
        107 => Some(state.gauge.floor() as i64),
        SKIN_REF_BMZ_LR2_GAUGE_2P => state.opponent_gauge.map(|gauge| gauge.floor() as i64),
        SKIN_REF_BMZ_LR2_HISPEED => Some((state.hispeed * 100.0) as i64),
        407 => Some(gauge_after_dot(state.gauge) as i64),
        163 => Some((state.timeleft_ms / 60_000) as i64),
        164 => Some(((state.timeleft_ms / 1_000) % 60) as i64),
        165 => Some((state.resource_load_progress.clamp(0.0, 1.0) * 100.0) as i64),
        1163 => Some(result_or_select_length_ms(state) / 60_000),
        1164 => Some((result_or_select_length_ms(state) / 1_000) % 60),
        310 => Some(state.hispeed.floor() as i64),
        311 => Some(((state.hispeed * 100.0) as i64) % 100),
        1900 => Some(skin_hispeed_mode_index(state) as i64),
        1901 => Some(i64::from(skin_hispeed_mode_is_floating(state))),
        1902 => Some(skin_target_green_number(state)),
        SKIN_REF_BMZ_BASE_HISPEED => Some(skin_base_hispeed_index(state) as i64),
        SKIN_REF_BMZ_NORMAL_HISPEED_LEVEL => Some(skin_normal_hispeed_level(state)),
        SKIN_REF_BMZ_HISPEED_CONFIG => Some(skin_hispeed_config_index(state) as i64),
        SKIN_REF_BMZ_SELECT_SETTINGS_ROW_KIND => {
            Some(i64::from(select_settings_row_kind_index(state.select_row_kind)))
        }
        SKIN_REF_BMZ_SELECT_SESSION_MODE => {
            Some(state.skin_attempt.session_mode_index.unwrap_or(state.select_session_mode_index)
                as i64)
        }
        SKIN_REF_BMZ_RULE_MODE => Some(state.rule_mode_index as i64),
        SKIN_REF_BMZ_LN_POLICY_SETTING => state.ln_policy_setting_index.map(|index| index as i64),
        SKIN_REF_BMZ_LN_SCORE_POLICY => state.ln_score_policy_index.map(|index| index as i64),
        // Deprecated grade-difference mode: old BMZ skins fall back to NEXT.
        SKIN_REF_BMZ_GRADE_DIFF_DISPLAY => Some(1),
        SKIN_REF_BMZ_SCORE_GRADE_CURRENT => {
            score_grade_facts(state).map(|facts| facts.current_index as i64)
        }
        SKIN_REF_BMZ_SCORE_GRADE_NEXT => {
            score_grade_facts(state).map(|facts| facts.next_index as i64)
        }
        SKIN_REF_BMZ_SCORE_GRADE_NEAREST => {
            score_grade_facts(state).map(|facts| facts.nearest_index as i64)
        }
        SKIN_REF_BMZ_SCORE_GRADE_CURRENT_DIFF => {
            score_grade_facts(state).map(|facts| facts.current_diff)
        }
        SKIN_REF_BMZ_SCORE_GRADE_NEXT_DIFF => score_grade_facts(state).map(|facts| facts.next_diff),
        SKIN_REF_BMZ_SCORE_GRADE_NEAREST_DIFF => {
            score_grade_facts(state).map(|facts| facts.nearest_diff)
        }
        SKIN_REF_BMZ_SCORE_GRADE_NEAREST_ABS => {
            score_grade_facts(state).map(|facts| facts.nearest_diff.abs())
        }
        1930 => Some(player_stat_u64(state.player_stats.daily.play_count)),
        1931 => Some(player_stat_u64(state.player_stats.daily.clear_count)),
        1932 => Some(player_stat_u64(state.player_stats.daily.pgreat)),
        1933 => Some(player_stat_u64(state.player_stats.daily.great)),
        1934 => Some(player_stat_u64(state.player_stats.daily.good)),
        1935 => Some(player_stat_u64(state.player_stats.daily.bad)),
        1936 => Some(player_stat_u64(state.player_stats.daily.poor)),
        1937 => Some(player_stat_u64(state.player_stats.daily.empty_poor)),
        1938 => Some(player_stat_u64(daily_play_notes(&state.player_stats.daily))),
        1939 => Some(player_stat_u64(daily_completed_notes(&state.player_stats.daily))),
        1940 => Some(player_stat_u64(daily_ex_score(&state.player_stats.daily))),
        1941 => Some(player_stat_u64(daily_max_ex_score(&state.player_stats.daily))),
        1942 => Some(player_stat_u64(daily_rate_basis_points(&state.player_stats.daily))),
        1943 => Some(daily_rank_index(&state.player_stats.daily)),
        1944 => Some(player_stat_u64(state.player_stats.daily.score_update_count)),
        1945 => Some(player_stat_u64(state.player_stats.daily.clear_update_count)),
        1946 => Some(player_stat_u64(state.player_stats.daily.miss_count_update_count)),
        SKIN_REF_BMZ_COURSE_STAGE_COUNT => Some(i64::from(state.course_result.stage_count)),
        id if (SKIN_REF_BMZ_COURSE_STAGE_EX_BASE
            ..SKIN_REF_BMZ_COURSE_STAGE_EX_BASE + SKIN_BMZ_COURSE_STAGE_COUNT as i32)
            .contains(&id) =>
        {
            Some(
                state.course_result.stages[(ref_id - SKIN_REF_BMZ_COURSE_STAGE_EX_BASE) as usize]
                    .ex_score as i64,
            )
        }
        id if (SKIN_REF_BMZ_COURSE_STAGE_GAUGE_BASE
            ..SKIN_REF_BMZ_COURSE_STAGE_GAUGE_BASE + SKIN_BMZ_COURSE_STAGE_COUNT as i32)
            .contains(&id) =>
        {
            Some(
                state.course_result.stages[(ref_id - SKIN_REF_BMZ_COURSE_STAGE_GAUGE_BASE) as usize]
                    .gauge
                    .floor() as i64,
            )
        }
        id if (SKIN_REF_BMZ_COURSE_STAGE_BP_BASE
            ..SKIN_REF_BMZ_COURSE_STAGE_BP_BASE + SKIN_BMZ_COURSE_STAGE_COUNT as i32)
            .contains(&id) =>
        {
            Some(
                state.course_result.stages[(ref_id - SKIN_REF_BMZ_COURSE_STAGE_BP_BASE) as usize].bp
                    as i64,
            )
        }
        id if (SKIN_REF_BMZ_COURSE_STAGE_RATE_BASE
            ..SKIN_REF_BMZ_COURSE_STAGE_RATE_BASE + SKIN_BMZ_COURSE_STAGE_COUNT as i32)
            .contains(&id) =>
        {
            Some(
                state.course_result.stages[(ref_id - SKIN_REF_BMZ_COURSE_STAGE_RATE_BASE) as usize]
                    .rate_basis_points as i64,
            )
        }
        SKIN_REF_BMZ_KEY_MODE => effective_skin_key_mode(state).map(skin_key_mode_number_i64),
        SKIN_REF_BMZ_ACTIVE_LANE_COUNT => {
            effective_skin_key_mode(state).map(|mode| mode.lane_count() as i64)
        }
        SKIN_REF_BMZ_SOURCE_KEY_MODE => {
            state.skin_attempt.source_key_mode.map(skin_key_mode_number_i64)
        }
        SKIN_REF_BMZ_SOURCE_LN_PROFILE => state.skin_attempt.source_ln_profile_bits.map(i64::from),
        312 => {
            if state.select_screen && !duration_refs_available(state) {
                return None;
            }
            Some(state_duration_number_ms(state))
        }
        313 => {
            if state.select_screen && !duration_refs_available(state) {
                return None;
            }
            Some(state_duration_green_number_ms(state))
        }
        1312..=1327 => lane_cover_duration_number(ref_id, state),
        308 => Some(
            state
                .skin_attempt
                .ln_mode_index
                .or(state.result_ln_mode_index)
                .unwrap_or(state.select_ln_mode_index) as i64,
        ),
        340 => Some(
            state.skin_attempt.judge_algorithm_index.unwrap_or(state.select_judge_algorithm_index)
                as i64,
        ),
        // BPM 系: NUMBER_MAXBPM=90, NUMBER_MINBPM=91, NUMBER_NOWBPM=160
        90 => {
            if !select_chart_metadata_available(state) {
                return None;
            }
            Some(if state.max_bpm > 0.0 { state.max_bpm } else { state.select_max_bpm }.round()
                as i64)
        }
        91 => {
            if !select_chart_metadata_available(state) {
                return None;
            }
            Some(if state.min_bpm > 0.0 { state.min_bpm } else { state.select_min_bpm }.round()
                as i64)
        }
        160 => {
            if !select_chart_metadata_available(state) {
                return None;
            }
            Some(if state.now_bpm > 0.0 { state.now_bpm } else { state.select_bpm }.round() as i64)
        }
        // レーンカバー: NUMBER_LANECOVER1=14 (0-1000)
        14 => Some((bmz_lane_cover_for_lift(state.lane_cover, state.lift) * 1000.0).round() as i64),
        // リフト: NUMBER_LIFT1=314 (0-1000)
        314 => Some((state.lift.clamp(0.0, 1.0) * 1000.0).round() as i64),
        // 選曲画面の音量表示: MASTER/KEY/BGM volume (0-100)
        57 => Some(select_volume_number(state.select_master_volume)),
        58 => Some(select_volume_number(state.select_key_volume)),
        59 => Some(select_volume_number(state.select_bgm_volume)),
        // 判定タイミングずれ: VALUE_JUDGE_1P/2P/3P_DURATION=525/526/527 (ms、符号付き)
        // beatoraja getRecentJudgeTiming は note時刻 - 押下時刻 (FAST=正)。
        // bmz の judge_timing_ms は 押下時刻 - note時刻 (FAST=負) なので符号を反転する。
        525 => state.judge_timing_ms[0].map(|ms| -(ms as i64)),
        526 => state.judge_timing_ms[1].map(|ms| -(ms as i64)),
        527 => state.judge_timing_ms[2].map(|ms| -(ms as i64)),
        SKIN_REF_BMZ_JUDGE_LANE_DURATION_BASE..=SKIN_REF_BMZ_JUDGE_LANE_DURATION_LAST => state
            .judge_lane_timing_ms[(ref_id - SKIN_REF_BMZ_JUDGE_LANE_DURATION_BASE) as usize]
            .map(|ms| -(ms as i64)),
        // Modified LR2 / OpenLR2 ref=210. PGREAT is always blank even when a
        // BMZ ThresholdMs setting preserves its timing side.
        SKIN_REF_BMZ_LR2_FAST_SLOW_1P => {
            if state.judge_index[0] == Some(0) {
                Some(0)
            } else {
                Some(match state.judge_timing_sign[0] {
                    Some(1) => 1,
                    Some(-1) => 2,
                    _ => 0,
                })
            }
        }
        // 判定タイミングオフセット設定値 (NUMBER_JUDGETIMING=12)
        12 => Some(state.judge_timing_offset_ms as i64),
        // Result judgement duration / timing distribution stats.
        372 => state.average_duration_us.map(|value| value / 1_000),
        373 => state.average_duration_us.map(|value| (value / 10) % 100),
        374 => state.average_timing_ms.map(|value| value as i64),
        375 => state.average_timing_ms.map(timing_afterdot),
        376 => state.stddev_timing_ms.map(|value| value as i64),
        377 => state.stddev_timing_ms.map(|value| ((value.abs() * 100.0) as i64) % 100),
        SKIN_REF_BMZ_RESULT_IR_SCOPE => Some(state.ir_ranking.scope.index()),
        SKIN_REF_BMZ_RESULT_IR_SCOPE_TOTAL => state.ir_ranking.total_player,
        // IR numbers (beatoraja NUMBER_IR_*)。Offline / 未取得時は
        // beatoraja の Integer.MIN_VALUE と同じく値なしにする。
        179 => state.ir_ranking.rank,
        180 | 200 => state.ir_ranking.total_player,
        181 => state.ir_ranking.clear_rate,
        182 => state.ir_ranking.previous_rank,
        226 => ir_total_clear_count(&state.ir_ranking),
        227 => state.ir_ranking.clear_rate,
        241 => state.ir_ranking.clear_rate.map(|_| 0),
        380..=389 => {
            ir_ranking_entry(&state.ir_ranking, ref_id - 380).and_then(|entry| entry.ex_score)
        }
        390..=399 => ir_ranking_entry(&state.ir_ranking, ref_id - 390).and_then(|entry| entry.rank),
        201..=242 => None,
        // NUMBER_RIVAL_SCORE / MAXCOMBO / MISSCOUNT (IR ライバルベスト)。
        271 => state.rival_ex_score,
        275 => state.rival_max_combo,
        276 => state.rival_bp,
        280..=284 => {
            if let Some(counts) = state.rival_judge_counts {
                Some(i64::from(counts[(ref_id - 280) as usize]))
            } else {
                // rianIR の全曲ライバルスコアは判定内訳を持たない。BAD+POOR を
                // MISSCOUNT として使うスキン向けに、合計 BP を POOR 側へ寄せる。
                match ref_id {
                    283 => state.rival_bp.map(|_| 0),
                    284 => state.rival_bp,
                    _ => None,
                }
            }
        }
        285..=289 => {
            let notes = state.select_total_notes.max(state.total_notes);
            let count = state.rival_judge_counts?[(ref_id - 285) as usize];
            (notes > 0).then_some(i64::from(count) * 100 / i64::from(notes))
        }
        // NUMBER_HIGHSCORE / NUMBER_HIGHSCORE2。Playでは保存済み最終値、
        // Resultでは保存前ベストを返し、未プレイはいずれも0。
        150 | 170 => best_score_number(state).map(i64::from),
        121 | 151 => state.target_ex_score.map(|s| projected_score_at_progress(s, state) as i64),
        122 | 123 | 135 | 136 | 157 | 158 => {
            state.target_ex_score.map(|target| score_rate_parts(target, state.total_notes)).map(
                |parts| (if matches!(ref_id, 122 | 135 | 157) { parts.0 } else { parts.1 }) as i64,
            )
        }
        183 | 184 => result_mybest_ex_score_display(state).map(|best| {
            let parts = score_rate_parts(best, state.total_notes);
            (if ref_id == 183 { parts.0 } else { parts.1 }) as i64
        }),
        400 => state.judge_rank.map(|rank| rank as i64),
        154 => result_grade_diff_number(state),
        // NUMBER_DIFF_HIGHSCORE=152, NUMBER_DIFF_HIGHSCORE2=172
        // (符号付き、ex_score - 現在ノート位置までのbest)
        152 | 172 => {
            projected_best_score_at_progress(state).map(|best| state.ex_score as i64 - best as i64)
        }
        // NUMBER_DIFF_TARGETSCORE=153 (符号付き、ex_score - target)
        153 => state.target_ex_score.map(|target| {
            state.ex_score as i64 - projected_score_at_progress(target, state) as i64
        }),
        // NUMBER_TARGET_MAXCOMBO=173 is the old/my-best score on Result.
        173 if state.result_failed.is_some() => {
            state.previous_best_max_combo.filter(|combo| *combo > 0).map(|combo| combo as i64)
        }
        // NUMBER_DIFF_MAXCOMBO=175 (current - old/my-best).
        175 if state.result_failed.is_some() => state
            .previous_best_max_combo
            .filter(|combo| *combo > 0)
            .map(|previous| state.max_combo as i64 - previous as i64),
        // NUMBER_TARGET_MISSCOUNT=176 (Result では old/mybest min_bp)
        176 => result_mybest_bp(state).map(|c| c as i64),
        // NUMBER_MISSCOUNT2=177 (Result では今回の min_bp)
        177 => Some(current_bp(state) as i64),
        // NUMBER_DIFF_MISSCOUNT=178 (符号付き、今回 min_bp - old/mybest min_bp)
        178 => result_diff_misscount(state),
        // NUMBER_TARGET_CLEAR=371
        371 if state.result_failed.is_some() => result_mybest_clear_index_display(state),
        371 => result_mybest_clear_index(state).or(state.target_clear_index),
        // Fast/Slow split (PGREAT/GREAT/GOOD/BAD/POOR)
        410 | 411 if state.autoplay => Some(0),
        410 => state.fast_slow_counts.map(|c| c.fast_pgreat as i64),
        411 => state.fast_slow_counts.map(|c| c.slow_pgreat as i64),
        412 => state.fast_slow_counts.map(|c| c.fast_great as i64),
        413 => state.fast_slow_counts.map(|c| c.slow_great as i64),
        414 => state.fast_slow_counts.map(|c| c.fast_good as i64),
        415 => state.fast_slow_counts.map(|c| c.slow_good as i64),
        416 => state.fast_slow_counts.map(|c| c.fast_bad as i64),
        417 => state.fast_slow_counts.map(|c| c.slow_bad as i64),
        418 => state.fast_slow_counts.map(|c| c.fast_poor as i64),
        419 => state.fast_slow_counts.map(|c| c.slow_poor as i64),
        420 => Some(state.judge_counts.empty_poor as i64),
        421 => state.fast_slow_counts.map(|c| c.fast_empty_poor as i64),
        422 => state.fast_slow_counts.map(|c| c.slow_empty_poor as i64),
        // NUMBER_TOTALEARLY=423, NUMBER_TOTALLATE=424
        423 => state.fast_slow_counts.map(|c| c.fast_total() as i64),
        424 => state.fast_slow_counts.map(|c| c.slow_total() as i64),
        425 | 427 if state.select_screen => state.select_cb.map(|count| count as i64).or(Some(0)),
        425 | 427 if state.result_failed.is_some() => Some(current_cb(state) as i64),
        425 => Some((state.judge_counts.bad + state.judge_counts.poor) as i64),
        426 => Some(poor_plus_miss(state.judge_counts) as i64),
        427 => Some(bad_plus_poor_plus_miss(state.judge_counts) as i64),
        ref_id if random_lane_ref_slot(ref_id).is_some() => {
            skin_random_lane_ref_number(ref_id, state)
        }
        _ => None,
    }
}
