use super::*;

pub(super) fn play_elapsed_ms(snapshot: &RenderSnapshot) -> i32 {
    (snapshot.play_elapsed_time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

pub(in crate::plan) fn build_play_skin_state(
    snapshot: &RenderSnapshot,
    skin: &SkinContext,
    play_elapsed_ms: i32,
) -> crate::skin::SkinDrawState {
    let (bomb_ms, lane_judge) = recent_judgement_state(snapshot, skin);
    let judge_region_count =
        skin.document().map(|document| document.judge_region_count()).unwrap_or(1);
    let judge_region_state = crate::skin::build_judge_region_state(
        &snapshot.recent_judgements,
        snapshot.time.0,
        judge_region_count,
    );
    let ready_timer_ms = snapshot
        .ready_elapsed_time
        .map(|time| (time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32);
    let skin_canvas_h = skin.document().map_or(720, |document| document.h) as f32;
    let skin_lane_h = skin_lane_height_px(skin, snapshot.key_mode, skin_canvas_h);

    crate::skin::SkinDrawState {
        elapsed_ms: play_elapsed_ms,
        start_input_ms: crate::skin::skin_start_input_elapsed_ms(
            play_elapsed_ms,
            skin.document().map_or(0, |document| document.input),
        ),
        current_fps: snapshot.current_fps,
        operating_time_ms: snapshot.operating_time_ms,
        ready_timer_ms,
        play_timer_ms: (snapshot.time.0 >= 0)
            .then_some((snapshot.time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32),
        rhythm_timer_ms: snapshot.rhythm_timer_elapsed_ms,
        quarter_note_elapsed_ms: snapshot.quarter_note_elapsed_ms,
        key_mode: snapshot.key_mode,
        skin_attempt: snapshot.skin_attempt,
        chart_has_long_notes: snapshot.has_long_notes,
        rule_mode_index: snapshot.rule_mode_index,
        ln_score_policy_index: snapshot.ln_score_policy_index,
        logical_input_held: snapshot.skin_input.held,
        select_arrange_index: crate::skin::select_arrange_index(&snapshot.arrange),
        select_arrange_2p_index: crate::skin::select_arrange_index(&snapshot.arrange_2p),
        select_target_index: crate::skin::play_target_image_index(&snapshot.target),
        assist_flags: snapshot.assist_flags,
        assist_extra_note_depth: snapshot.assist_extra_note_depth,
        assist_mine_mode: snapshot.assist_mine_mode,
        assist_scroll_mode: snapshot.assist_scroll_mode,
        assist_long_note_mode: snapshot.assist_long_note_mode,
        guide_se_enabled: snapshot.guide_se_enabled,
        constant_enabled: snapshot.constant_enabled,
        select_extended_arrange_index: crate::skin::extended_arrange_index(&snapshot.arrange),
        select_extended_arrange_2p_index: crate::skin::extended_arrange_index(&snapshot.arrange_2p),
        random_lane_refs: crate::skin::fixed_random_lane_refs(
            &snapshot.lane_shuffle_pattern,
            snapshot.key_mode,
            &snapshot.arrange,
            &snapshot.arrange_2p,
        ),
        combo: snapshot.combo,
        max_combo: snapshot.max_combo,
        ex_score: snapshot.ex_score,
        total_notes: snapshot.total_notes,
        select_chart_total_gauge: snapshot.chart_total_gauge,
        past_notes: snapshot.past_notes,
        judge_counts: snapshot.judge_counts,
        fast_slow_counts: Some(snapshot.fast_slow_counts),
        gauge: snapshot.gauge,
        gauge_type: snapshot.gauge_type,
        opponent_gauge: snapshot.opponent.as_ref().map(|opponent| opponent.gauge),
        opponent_gauge_type: snapshot.opponent.as_ref().map(|opponent| opponent.gauge_type),
        gauge_auto_shift: snapshot.gauge_auto_shift,
        gauge_max: snapshot.gauge_max,
        gauge_border: snapshot.gauge_border,
        play_progress: play_progress(snapshot),
        end_of_note: end_of_note(snapshot),
        end_of_note_ms: snapshot.end_of_note_elapsed_ms,
        bomb_ms,
        // keyon/keyoff は session 側で追跡済みの開始時刻から算出される。
        keyon_ms: snapshot.keyon_ms,
        keyoff_ms: snapshot.keyoff_ms,
        hold_ms: snapshot.hold_ms,
        hcn_active_ms: snapshot.hcn_active_ms,
        hcn_damage_ms: snapshot.hcn_damage_ms,
        lane_judge,
        judge_ms: judge_region_state.judge_ms,
        full_combo_ms: snapshot.full_combo_elapsed_ms,
        full_combo_2p_ms: snapshot
            .opponent
            .as_ref()
            .and_then(|opponent| opponent.full_combo_elapsed_ms),
        fadeout_ms: snapshot.fadeout_elapsed_ms,
        failed_ms: snapshot.failed_elapsed_ms,
        music_end_ms: snapshot.music_end_elapsed_ms,
        gauge_increase_ms: snapshot.gauge_increase_elapsed_ms,
        gauge_increase_2p_ms: snapshot
            .opponent
            .as_ref()
            .and_then(|opponent| opponent.gauge_increase_elapsed_ms),
        gauge_max_ms: snapshot.gauge_max_elapsed_ms,
        gauge_max_2p_ms: snapshot
            .opponent
            .as_ref()
            .and_then(|opponent| opponent.gauge_max_elapsed_ms),
        end_of_note_2p_ms: snapshot
            .opponent
            .as_ref()
            .and_then(|opponent| opponent.end_of_note_elapsed_ms),
        judge_index: judge_region_state.judge_index,
        judge_combo: judge_region_state.judge_combo,
        judge_timing_sign: judge_region_state.judge_timing_sign,
        offset_lift_px: skin_lift_offset_px(snapshot.lift, skin_lane_h),
        offset_lanecover_px: skin_lanecover_offset_px(
            snapshot.lane_cover,
            snapshot.lift,
            skin_lane_h,
        ),
        offset_hidden_cover_px: skin_hidden_cover_offset_px(
            snapshot.lift,
            snapshot.hidden_cover,
            skin_lane_h,
        ),
        skin_offsets: snapshot.skin_offsets,
        hispeed: snapshot.hispeed,
        hispeed_mode_index: snapshot.hispeed_mode_index,
        base_hispeed_index: snapshot.base_hispeed_index,
        normal_hispeed_level: snapshot.normal_hispeed_level,
        hispeed_config_index: snapshot.hispeed_config_index,
        target_green_number: snapshot.target_green_number,
        timeleft_ms: (snapshot.duration.0.saturating_sub(snapshot.time.0) / 1_000)
            .saturating_add(1_000)
            .clamp(0, i32::MAX as i64) as i32,
        total_duration_ms: snapshot.note_display_duration_ms,
        duration_green_ms: Some(crate::skin::duration_to_green_number_ms(
            snapshot.note_display_duration_ms,
        )),
        lane_cover: snapshot.lane_cover,
        lift: snapshot.lift,
        lane_cover_changing: snapshot.lane_cover_changing,
        lanecover_enabled: snapshot.lanecover_enabled,
        lift_enabled: snapshot.lift_enabled,
        hidden_enabled: snapshot.hidden_enabled,
        hidden_cover: snapshot.hidden_cover,
        play_level: skin_level_number(&snapshot.play_level),
        table_song: !snapshot.table_text_primary.is_empty(),
        difficulty: skin_difficulty_code(&snapshot.difficulty_name),
        judge_rank: snapshot.judge_rank,
        now_bpm: snapshot.now_bpm,
        min_bpm: snapshot.min_bpm,
        max_bpm: snapshot.max_bpm,
        has_bga: snapshot.has_bga,
        has_bpm_stop: snapshot.has_bpm_stop,
        bga_enabled: snapshot.bga_enabled,
        has_stagefile: snapshot.stagefile_background,
        stagefile_image_size: snapshot.stagefile_image_size,
        has_backbmp: snapshot.backbmp_background,
        bga_base: snapshot.bga_base.map(skin_bga_frame_from_display),
        bga_layer: snapshot.bga_layer.map(skin_bga_frame_from_display),
        bga_layer2: snapshot.bga_layer2.map(skin_bga_frame_from_display),
        bga_poor: snapshot.bga_poor.map(skin_bga_frame_from_display),
        bga_stretch: snapshot.bga_stretch,
        judge_timing_ms: judge_region_state.judge_timing_ms,
        best_ex_score: snapshot.best_ex_score,
        projected_best_ex_score: snapshot.projected_best_ex_score,
        target_ex_score: snapshot.target_ex_score,
        judge_timing_offset_ms: snapshot.judge_timing_offset_ms,
        judge_timing_auto_adjust: snapshot.judge_timing_auto_adjust,
        main_bpm: snapshot.main_bpm,
        hsfix_index: snapshot.hsfix_index,
        fs_threshold_ms: snapshot.fs_threshold_ms,
        adjusted_cover_progress: snapshot.adjusted_cover_progress,
        adjusted_rate: snapshot.adjusted_rate,
        adjusted_rate_adot: snapshot.adjusted_rate_adot,
        autoplay: snapshot.autoplay,
        play_screen: true,
        replay_playback: snapshot.replay_playback,
        practice_mode: snapshot.practice_mode,
        score_save_enabled: Some(snapshot.score_save_enabled),
        rival_ex_score: snapshot.opponent.as_ref().map(|opponent| i64::from(opponent.ex_score)),
        rival_max_combo: snapshot.opponent.as_ref().map(|opponent| i64::from(opponent.max_combo)),
        rival_bp: snapshot.opponent.as_ref().map(|opponent| {
            i64::from(opponent.judge_counts.bad.saturating_add(opponent.judge_counts.poor))
        }),
        rival_judge_counts: snapshot.opponent.as_ref().map(|opponent| {
            [
                opponent.judge_counts.pgreat,
                opponent.judge_counts.great,
                opponent.judge_counts.good,
                opponent.judge_counts.bad,
                opponent.judge_counts.poor,
            ]
        }),
        course_stage: snapshot.course_stage,
        hit_error_ring: snapshot.hit_error_ring.values,
        hit_error_ring_index: snapshot.hit_error_ring.index,
        // op 80/81 はリソースロード状態ではなく PRELOAD state を表す。
        skin_loaded: snapshot.seamless_play_entry || snapshot.ready_elapsed_time.is_some(),
        resource_load_progress: snapshot.resource_load_progress,
        ..crate::skin::SkinDrawState::default()
    }
}

fn recent_judgement_state(
    snapshot: &RenderSnapshot,
    skin: &SkinContext,
) -> ([Option<i32>; LANE_COUNT], [Option<usize>; LANE_COUNT]) {
    let mut bomb_ms = [None; LANE_COUNT];
    let mut lane_judge = [None; LANE_COUNT];
    let judge_timer_limit =
        skin.document().map_or(1, |document| document.judgetimer).max(0) as usize;

    // 見逃し POOR はボムエフェクトを出さない。
    for judgement in &snapshot.recent_judgements {
        if judgement.is_miss {
            continue;
        }
        let lane_index = judgement.lane.index();
        let elapsed = ((snapshot.time.0 - judgement.time.0) / 1_000)
            .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        let judge_index = judge_image_index(&judgement.text);
        lane_judge[lane_index] = judge_index;
        if judge_starts_bomb(judge_index, judge_timer_limit) {
            bomb_ms[lane_index] = Some(elapsed);
        }
    }

    (bomb_ms, lane_judge)
}

pub(super) fn build_play_skin_text(snapshot: &RenderSnapshot) -> SkinTextState<'_> {
    SkinTextState {
        player_name: &snapshot.player_name,
        title: &snapshot.title,
        subtitle: &snapshot.subtitle,
        artist: &snapshot.artist,
        subartist: &snapshot.subartist,
        genre: &snapshot.genre,
        difficulty_name: &snapshot.difficulty_name,
        play_level: &snapshot.play_level,
        target: &snapshot.target,
        resolved_target_name: snapshot.resolved_target_name.as_deref(),
        table_level: &snapshot.table_text_secondary,
        table_text_primary: &snapshot.table_text_primary,
        table_text_secondary: &snapshot.table_text_secondary,
        table_text_fallback: &snapshot.table_text_fallback,
        course_stage: snapshot.course_stage,
        course_titles: string_array_refs(&snapshot.course_titles),
        ..SkinTextState::default()
    }
}
