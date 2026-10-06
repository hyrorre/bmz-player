use super::*;

pub(super) fn plan_decide(
    snapshot: &RenderSnapshot,
    skin: &SkinContext,
    dynamic_timers: &mut crate::skin::DynamicTimerRuntime,
) -> DrawPlan {
    if skin.document().is_some_and(|document| document.skin_type == 6) {
        let play_elapsed_ms =
            (snapshot.play_elapsed_time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        let mut state = crate::skin::SkinDrawState {
            screen_size: snapshot.screen_size,
            elapsed_ms: play_elapsed_ms,
            start_input_ms: dynamic_timers.start_input_elapsed_ms(
                play_elapsed_ms,
                skin.document().map_or(0, |document| document.input),
            ),
            current_fps: snapshot.current_fps,
            operating_time_ms: snapshot.operating_time_ms,
            ready_timer_ms: Some(play_elapsed_ms),
            key_mode: snapshot.key_mode,
            skin_attempt: snapshot.skin_attempt,
            chart_has_long_notes: snapshot.has_long_notes,
            rule_mode_index: snapshot.rule_mode_index,
            ln_score_policy_index: snapshot.ln_score_policy_index,
            logical_input_held: snapshot.skin_input.held,
            total_notes: snapshot.total_notes,
            select_chart_total_gauge: snapshot.chart_total_gauge,
            past_notes: snapshot.past_notes,
            ex_score: snapshot.ex_score,
            judge_counts: snapshot.judge_counts,
            fast_slow_counts: Some(snapshot.fast_slow_counts),
            gauge: snapshot.gauge,
            gauge_type: snapshot.gauge_type,
            gauge_auto_shift: snapshot.gauge_auto_shift,
            gauge_max: snapshot.gauge_max,
            gauge_border: snapshot.gauge_border,
            play_level: skin_level_number(&snapshot.play_level),
            table_song: !snapshot.table_text_primary.is_empty(),
            difficulty: skin_difficulty_code(&snapshot.difficulty_name),
            judge_rank: snapshot.judge_rank,
            has_stagefile: snapshot.stagefile_background,
            stagefile_image_size: snapshot.stagefile_image_size,
            now_bpm: snapshot.now_bpm,
            min_bpm: snapshot.min_bpm,
            max_bpm: snapshot.max_bpm,
            has_bga: snapshot.has_bga,
            has_bpm_stop: snapshot.has_bpm_stop,
            bga_enabled: snapshot.bga_enabled,
            skin_offsets: snapshot.skin_offsets,
            hispeed: snapshot.hispeed,
            hispeed_mode_index: snapshot.hispeed_mode_index,
            base_hispeed_index: snapshot.base_hispeed_index,
            normal_hispeed_level: snapshot.normal_hispeed_level,
            hispeed_config_index: snapshot.hispeed_config_index,
            target_green_number: snapshot.target_green_number,
            total_duration_ms: snapshot.note_display_duration_ms,
            duration_green_ms: Some(crate::skin::duration_to_green_number_ms(
                snapshot.note_display_duration_ms,
            )),
            lane_cover: snapshot.lane_cover,
            hidden_cover: snapshot.hidden_cover,
            fadeout_ms: snapshot.fadeout_elapsed_ms,
            gauge_increase_ms: snapshot.gauge_increase_elapsed_ms,
            gauge_max_ms: snapshot.gauge_max_elapsed_ms,
            score_save_enabled: Some(snapshot.score_save_enabled),
            course_stage: snapshot.course_stage,
            ..crate::skin::SkinDrawState::default()
        };
        advance_skin_dynamic_timers(skin, dynamic_timers, &mut state, play_elapsed_ms);
        let text = SkinTextState {
            player_name: &snapshot.player_name,
            title: &snapshot.title,
            subtitle: &snapshot.subtitle,
            artist: &snapshot.artist,
            subartist: &snapshot.subartist,
            genre: &snapshot.genre,
            difficulty_name: &snapshot.difficulty_name,
            play_level: &snapshot.play_level,
            table_level: &snapshot.table_text_secondary,
            table_text_primary: &snapshot.table_text_primary,
            table_text_secondary: &snapshot.table_text_secondary,
            table_text_fallback: &snapshot.table_text_fallback,
            course_titles: string_array_refs(&snapshot.course_titles),
            ..SkinTextState::default()
        };
        let items = skin.static_document_items_for_state_and_text(&state, &text);
        if !items.is_empty() {
            let mut commands = Vec::new();
            crate::skin::append_skin_render_items(&mut commands, &items);
            push_scene_overlays(&mut commands, &snapshot.overlay);
            return DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands };
        }
    }

    plan_play(snapshot, &SkinContext::default(), dynamic_timers)
}
