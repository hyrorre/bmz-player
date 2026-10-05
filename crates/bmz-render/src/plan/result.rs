use super::*;

pub(super) fn plan_result(
    snapshot: &crate::scene::ResultSnapshot,
    skin: &SkinContext,
    dynamic_timers: &mut crate::skin::DynamicTimerRuntime,
) -> DrawPlan {
    if let Some(document) = skin.document().filter(|document| matches!(document.skin_type, 7 | 15))
    {
        let mut state = result_skin_draw_state_for_document(snapshot, document);
        state.start_input_ms =
            dynamic_timers.start_input_elapsed_ms(state.elapsed_ms, document.input);
        advance_skin_dynamic_timers(
            skin,
            dynamic_timers,
            &mut state,
            (snapshot.elapsed_time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32,
        );
        let grade_diff = crate::skin::result_grade_diff_label(&state).unwrap_or_default();
        let text = SkinTextState {
            player_name: snapshot.player_name.as_str(),
            resolved_target_name: Some(snapshot.target_name.as_str()),
            target: &snapshot.target,
            title: snapshot.title.as_str(),
            subtitle: snapshot.subtitle.as_str(),
            artist: snapshot.artist.as_str(),
            subartist: snapshot.subartist.as_str(),
            genre: snapshot.genre.as_str(),
            difficulty_name: snapshot.difficulty_name.as_str(),
            play_level: snapshot.play_level.as_str(),
            grade_diff: grade_diff.as_str(),
            table_level: snapshot.table_text_secondary.as_str(),
            table_text_primary: snapshot.table_text_primary.as_str(),
            table_text_secondary: snapshot.table_text_secondary.as_str(),
            table_text_fallback: snapshot.table_text_fallback.as_str(),
            ir_ranking: &snapshot.ir,
            course_titles: string_array_refs(&snapshot.course_titles),
            ..SkinTextState::default()
        };
        let items =
            skin.static_document_items_for_result_state_and_text(&snapshot.graph, &state, &text);
        if document.lr2 || !items.is_empty() {
            let mut commands = Vec::with_capacity(items.len() + 3);
            crate::skin::append_skin_render_items(&mut commands, &items);
            push_scene_overlays(&mut commands, &snapshot.overlay);
            return DrawPlan { clear: Color::rgb(0.0, 0.0, 0.0), commands };
        }
    }

    let mut plan = plan_result_fallback(ResultFallbackSummary {
        clear_type: snapshot.clear_type.as_str(),
        ex_score: snapshot.ex_score,
        ex_score_rate: snapshot.ex_score_rate,
        max_combo: snapshot.max_combo,
        gauge_value: snapshot.gauge_value,
        total_notes: snapshot.total_notes,
        judge_counts: &snapshot.judge_counts,
        fast_slow_counts: &snapshot.fast_slow_counts,
        graph: &snapshot.graph,
        score_history_id: snapshot.score_history_id,
        replay_saved: snapshot.replay_saved,
        difficulty_name: &snapshot.difficulty_name,
        ir: &snapshot.ir,
        play_level: &snapshot.play_level,
        grade_diff: crate::skin::result_grade_diff_label(&build_result_skin_draw_state(
            snapshot, 0,
        ))
        .unwrap_or_default(),
    });
    push_scene_overlays(&mut plan.commands, &snapshot.overlay);
    plan
}

pub(super) fn push_scene_overlays(
    commands: &mut Vec<DrawCommand>,
    overlay: &crate::snapshot::OverlaySnapshot,
) {
    push_scene_overlay_text_aligned(commands, &overlay.left_text, 0.015, TextAlign::Left);
    push_scene_overlay_text(commands, &overlay.fps_text, 0.015);
    push_scene_overlay_text(commands, &overlay.text, 0.975);
}

pub(super) fn push_scene_overlay_text(
    commands: &mut Vec<DrawCommand>,
    overlay: &str,
    origin_y: f32,
) {
    push_scene_overlay_text_aligned(commands, overlay, origin_y, TextAlign::Right);
}

pub(super) fn push_scene_overlay_text_aligned(
    commands: &mut Vec<DrawCommand>,
    overlay: &str,
    origin_y: f32,
    align: TextAlign,
) {
    if overlay.is_empty() {
        return;
    }
    // TextStyle.size は「画面高に対する比率」で扱われる (renderer.rs 側で * surface.height)。
    // ここでは 1080p を基準に 14px 相当へ合わせる。
    const OVERLAY_FONT_SIZE_RATIO: f32 = 14.0 / 1080.0;
    const OVERLAY_SHADOW_OFFSET_RATIO: f32 = 1.0 / 1080.0;
    // TextAlign::Right は max_width > 0 のときだけ効く (renderer.rs)。
    // origin.x を右端ボックスの左端、max_width をボックス幅にして右寄せする。
    let origin_x = if align == TextAlign::Left { 0.015 } else { -0.015 };
    commands.push(DrawCommand::Text {
        origin: Point { x: origin_x, y: origin_y },
        text: overlay.to_string(),
        caret: None,
        post_scale: Point { x: 1.0, y: 1.0 },
        style: TextStyle {
            font_id: None,
            size: OVERLAY_FONT_SIZE_RATIO,
            bitmap_size: None,
            color: Color::rgba(0.9, 0.9, 0.9, 0.65),
            layer: TextLayer::Ui,
            align,
            max_width: 1.0,
            overflow: TextOverflow::Overflow,
            wrapping: false,
            outline: None,
            shadow: Some(TextShadow {
                color: Color::rgba(0.0, 0.0, 0.0, 0.55),
                offset: Point { x: OVERLAY_SHADOW_OFFSET_RATIO, y: OVERLAY_SHADOW_OFFSET_RATIO },
            }),
        },
    });
}

/// リザルト画面の `SkinDrawState` を snapshot から構築する。
///
/// op 条件評価 (ランク別 BG など) を描画前に行いたい呼び出し側 (例: 動画ソースの
/// 可視判定) が、描画と同じ state を得るための公開エントリ。
pub fn result_skin_draw_state(
    snapshot: &crate::scene::ResultSnapshot,
    result_ranktime_ms: i32,
) -> crate::skin::SkinDrawState {
    build_result_skin_draw_state(snapshot, result_ranktime_ms)
}

pub fn result_skin_draw_state_for_document(
    snapshot: &crate::scene::ResultSnapshot,
    document: &SkinDocument,
) -> crate::skin::SkinDrawState {
    let mut state = build_result_skin_draw_state(snapshot, document.ranktime);
    if let Some(spec) = &document.lr2_result {
        state.lr2_result_flip = spec.flip;
        [state.result_graph_begin_ms, state.result_graph_end_ms, state.result_update_score_ms] =
            snapshot.lr2_progress.timers(spec, state.elapsed_ms);
    }
    if let Some((x, y)) = snapshot.mouse_position {
        state.mouse_x = Some(x.clamp(0.0, 1.0) * document.w as f32);
        state.mouse_y = Some((1.0 - y.clamp(0.0, 1.0)) * document.h as f32);
    }
    state
}

pub(super) fn build_result_skin_draw_state(
    snapshot: &crate::scene::ResultSnapshot,
    result_ranktime_ms: i32,
) -> crate::skin::SkinDrawState {
    let (gauge, gauge_type, gauge_max, gauge_border) = result_display_gauge(
        &snapshot.graph.gauge_points,
        snapshot.result_gauge_graph_type,
        snapshot.gauge_value,
        snapshot.gauge_type,
    );
    let timing_stats = if snapshot.graph.timing_metrics.initialized {
        snapshot.graph.timing_metrics.stats
    } else {
        snapshot
            .graph
            .timing_distribution
            .stats()
            .or_else(|| result_timing_stats(&snapshot.graph.timing_points))
    };
    let average_duration_us = if snapshot.graph.timing_metrics.initialized
        && snapshot.graph.timing_metrics.judged_notes <= snapshot.total_notes
    {
        snapshot.graph.timing_metrics.average_duration_us(snapshot.total_notes)
    } else {
        result_average_duration_us(&snapshot.graph.timing_points, snapshot.total_notes)
    };
    let elapsed_ms =
        (snapshot.elapsed_time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    let result_update_score_ms = if result_ranktime_ms <= 0 {
        Some(elapsed_ms)
    } else {
        elapsed_ms
            .checked_sub(result_ranktime_ms)
            .filter(|elapsed_after_rank| *elapsed_after_rank >= 0)
    };
    crate::skin::SkinDrawState {
        elapsed_ms,
        operating_time_ms: snapshot.operating_time_ms,
        current_fps: snapshot.current_fps,
        logical_input_held: snapshot.skin_input.held,
        skin_offsets: snapshot.skin_offsets,
        skin_attempt: snapshot.skin_attempt,
        select_arrange_index: crate::skin::select_arrange_index(&snapshot.arrange),
        select_arrange_2p_index: crate::skin::select_arrange_index(&snapshot.arrange_2p),
        select_double_option_index: crate::skin::select_double_option_index(
            &snapshot.double_option,
        ),
        result_arrange_index: crate::skin::select_arrange_index(&snapshot.arrange),
        result_arrange_2p_index: crate::skin::select_arrange_index(&snapshot.arrange_2p),
        select_extended_arrange_index: crate::skin::extended_arrange_index(&snapshot.arrange),
        select_extended_arrange_2p_index: crate::skin::extended_arrange_index(&snapshot.arrange_2p),
        result_extended_arrange_index: crate::skin::extended_arrange_index(&snapshot.arrange),
        result_extended_arrange_2p_index: crate::skin::extended_arrange_index(&snapshot.arrange_2p),
        random_lane_refs: crate::skin::fixed_random_lane_refs(
            &snapshot.lane_shuffle_pattern,
            snapshot.key_mode,
            &snapshot.arrange,
            &snapshot.arrange_2p,
        ),
        chart_has_long_notes: Some(snapshot.has_long_notes),
        result_ln_mode_index: Some(snapshot.ln_mode_index),
        rule_mode_index: snapshot.rule_mode_index,
        ln_score_policy_index: snapshot.ln_score_policy_index,
        assist_flags: snapshot.assist_flags,
        assist_extra_note_depth: snapshot.assist_extra_note_depth,
        assist_mine_mode: snapshot.assist_mine_mode,
        assist_scroll_mode: snapshot.assist_scroll_mode,
        assist_long_note_mode: snapshot.assist_long_note_mode,
        ex_score: snapshot.ex_score,
        total_notes: snapshot.total_notes,
        past_notes: snapshot.total_notes,
        total_duration_ms: snapshot.note_display_duration_ms.unwrap_or(snapshot.duration_ms),
        duration_green_ms: snapshot
            .note_display_duration_ms
            .map(crate::skin::duration_to_green_number_ms),
        result_duration_ms: snapshot.duration_ms,
        max_combo: snapshot.max_combo,
        judge_counts: snapshot.judge_counts,
        player_stats: snapshot.player_stats.clone(),
        course_result: snapshot.course_result,
        fast_slow_counts: Some(snapshot.fast_slow_counts),
        gauge,
        gauge_type,
        result_gauge_graph_type: Some(snapshot.result_gauge_graph_type),
        result_panel: Some(snapshot.result_panel),
        result_favorite_chart: Some(snapshot.favorite_chart),
        hispeed_auto_adjust: snapshot.hispeed_auto_adjust,
        gauge_max,
        gauge_border,
        play_progress: 1.0,
        end_of_note: true,
        best_ex_score: snapshot.best_ex_score,
        best_clear_index: snapshot.best_clear_type.map(|c| c as i64),
        target_ex_score: snapshot.target_ex_score,
        best_max_combo: snapshot.best_max_combo,
        target_max_combo: snapshot.target_max_combo,
        best_bp: snapshot.best_bp,
        result_bp: Some(snapshot.bp),
        result_cb: Some(snapshot.cb),
        ir_ranking: snapshot.ir.clone(),
        previous_best_ex_score: snapshot.previous_best_ex_score,
        previous_best_clear_index: snapshot.previous_best_clear_type.map(|c| c as i64),
        previous_best_max_combo: snapshot.previous_best_max_combo,
        previous_best_bp: snapshot.previous_best_bp,
        target_bp: snapshot.target_bp,
        target_clear_index: snapshot.target_clear_type.map(|c| c as i64),
        select_clear_index: snapshot.clear_type as i64,
        result_failed: Some(snapshot.result_failed),
        autoplay: snapshot.autoplay,
        play_level: skin_level_number(&snapshot.play_level),
        table_song: !snapshot.table_text_primary.is_empty(),
        difficulty: skin_difficulty_code(&snapshot.difficulty_name),
        judge_rank: snapshot.judge_rank,
        has_stagefile: snapshot.stagefile_background,
        stagefile_image_size: snapshot.stagefile_image_size,
        key_mode: snapshot.key_mode,
        now_bpm: snapshot.initial_bpm,
        min_bpm: snapshot.min_bpm,
        max_bpm: snapshot.max_bpm,
        main_bpm: snapshot.main_bpm,
        select_chart_total_gauge: snapshot.total_gauge,
        fadeout_ms: snapshot
            .fadeout_elapsed
            .map(|elapsed| (elapsed.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32),
        result_graph_begin_ms: Some(elapsed_ms),
        result_graph_end_ms: Some(elapsed_ms),
        result_update_score_ms,
        result_replay_slots: snapshot.replay_slots,
        result_saved_replay_slots: snapshot.saved_replay_slots,
        score_save_enabled: Some(snapshot.score_save_enabled),
        hit_error_ring: snapshot.graph.hit_error_ring.values,
        hit_error_ring_index: snapshot.graph.hit_error_ring.index,
        average_timing_ms: timing_stats.map(|stats| stats.0),
        average_duration_us,
        stddev_timing_ms: timing_stats.map(|stats| stats.1),
        ..crate::skin::SkinDrawState::default()
    }
}

pub(super) fn result_display_gauge(
    points: &[crate::snapshot::ResultGaugeGraphPoint],
    selected_type: i32,
    fallback_value: f32,
    fallback_type: i32,
) -> (f32, i32, f32, f32) {
    points
        .iter()
        .rev()
        .find(|point| point.gauge_type == selected_type)
        .map(|point| (point.value, point.gauge_type, point.max, point.border))
        .unwrap_or((fallback_value, fallback_type, 100.0, 80.0))
}

pub(super) fn result_timing_stats(
    points: &[crate::snapshot::ResultTimingPoint],
) -> Option<(f32, f32)> {
    if points.is_empty() {
        return None;
    }
    let count = points.len() as f32;
    let average_ms =
        points.iter().map(|point| point.delta_us as f32 / 1_000.0).sum::<f32>() / count;
    let variance = points
        .iter()
        .map(|point| {
            let diff = point.delta_us as f32 / 1_000.0 - average_ms;
            diff * diff
        })
        .sum::<f32>()
        / count;
    Some((average_ms, variance.sqrt()))
}

pub(super) fn result_average_duration_us(
    points: &[crate::snapshot::ResultTimingPoint],
    total_notes: u32,
) -> Option<i64> {
    if total_notes == 0 {
        return None;
    }
    const UNJUDGED_DURATION_US: u128 = 1_000_000;
    let judged_notes = points.len().min(total_notes as usize);
    let judged_duration = points
        .iter()
        .take(judged_notes)
        .map(|point| u128::from(point.delta_us.unsigned_abs()))
        .sum::<u128>();
    let unjudged_notes = total_notes as usize - judged_notes;
    let total_duration = judged_duration
        .saturating_add((unjudged_notes as u128).saturating_mul(UNJUDGED_DURATION_US));
    Some((total_duration / u128::from(total_notes)).min(i64::MAX as u128) as i64)
}

pub(super) struct ResultFallbackSummary<'a> {
    pub(super) clear_type: &'a str,
    pub(super) ex_score: u32,
    pub(super) ex_score_rate: f32,
    pub(super) max_combo: u32,
    pub(super) gauge_value: f32,
    pub(super) total_notes: u32,
    pub(super) judge_counts: &'a DisplayJudgeCounts,
    pub(super) fast_slow_counts: &'a FastSlowJudgeCounts,
    pub(super) graph: &'a ResultGraphSnapshot,
    pub(super) score_history_id: i64,
    pub(super) replay_saved: bool,
    pub(super) difficulty_name: &'a str,
    pub(super) play_level: &'a str,
    pub(super) grade_diff: String,
    pub(super) ir: &'a crate::scene::ResultIrSnapshot,
}

pub(super) fn plan_result_fallback(summary: ResultFallbackSummary<'_>) -> DrawPlan {
    let ResultFallbackSummary {
        clear_type,
        ex_score,
        ex_score_rate,
        max_combo,
        gauge_value,
        total_notes,
        judge_counts,
        fast_slow_counts,
        graph,
        score_history_id,
        replay_saved,
        difficulty_name,
        play_level,
        grade_diff,
        ir,
    } = summary;
    let mut commands = Vec::new();
    let text = TextRenderer;
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.1, y: 0.16, width: 0.8, height: 0.18 },
        color: Color::rgb(0.16, 0.13, 0.11),
    });
    text.push_text(
        &mut commands,
        "RESULT",
        BitmapTextStyle { x: 0.14, y: 0.205, cell: 0.014, color: Color::rgb(0.95, 0.9, 0.8) },
    );
    text.push_text(
        &mut commands,
        &display_label(clear_type, 18),
        BitmapTextStyle { x: 0.55, y: 0.22, cell: 0.008, color: Color::rgb(0.84, 0.93, 0.9) },
    );
    let metadata = difficulty_level_label(difficulty_name, play_level, "");
    if !metadata.is_empty() {
        text.push_text(
            &mut commands,
            &metadata,
            BitmapTextStyle {
                x: 0.14,
                y: 0.292,
                cell: 0.0055,
                color: Color::rgb(0.72, 0.82, 0.76),
            },
        );
    }
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.14, y: 0.42, width: 0.72, height: 0.045 },
        color: Color::rgb(0.065, 0.06, 0.058),
    });
    commands.push(DrawCommand::Rect {
        rect: Rect { x: 0.14, y: 0.42, width: 0.72 * ex_score_rate.clamp(0.0, 1.0), height: 0.045 },
        color: Color::rgb(0.55, 0.78, 0.86),
    });
    for index in 0..4 {
        commands.push(DrawCommand::Rect {
            rect: Rect { x: 0.14 + index as f32 * 0.18, y: 0.55, width: 0.14, height: 0.1 },
            color: Color::rgb(0.09, 0.08, 0.075),
        });
    }
    text.push_text(
        &mut commands,
        &format!("EX {}", ex_score),
        BitmapTextStyle { x: 0.16, y: 0.565, cell: 0.008, color: Color::rgb(0.86, 0.9, 0.92) },
    );
    text.push_text(
        &mut commands,
        &format!("MAX {}", max_combo),
        BitmapTextStyle { x: 0.34, y: 0.565, cell: 0.008, color: Color::rgb(0.86, 0.9, 0.92) },
    );
    text.push_text(
        &mut commands,
        &format!("GAUGE {}", gauge_value.round() as u32),
        BitmapTextStyle { x: 0.52, y: 0.565, cell: 0.008, color: Color::rgb(0.86, 0.9, 0.92) },
    );
    text.push_text(
        &mut commands,
        &format!("RATE {}", format_percent(ex_score_rate)),
        BitmapTextStyle { x: 0.16, y: 0.675, cell: 0.006, color: Color::rgb(0.72, 0.84, 0.86) },
    );
    text.push_text(
        &mut commands,
        &format!("GRADE {}", grade_diff),
        BitmapTextStyle { x: 0.52, y: 0.675, cell: 0.006, color: Color::rgb(0.72, 0.84, 0.86) },
    );
    text.push_text(
        &mut commands,
        &format!("NOTES {}", total_notes),
        BitmapTextStyle { x: 0.34, y: 0.675, cell: 0.006, color: Color::rgb(0.72, 0.84, 0.86) },
    );
    text.push_text(
        &mut commands,
        &format!("ID {}", score_history_id.max(0)),
        BitmapTextStyle { x: 0.52, y: 0.675, cell: 0.006, color: Color::rgb(0.72, 0.84, 0.86) },
    );
    text.push_text(
        &mut commands,
        if replay_saved { "REPLAY SAVED" } else { "REPLAY NONE" },
        BitmapTextStyle { x: 0.68, y: 0.675, cell: 0.005, color: Color::rgb(0.66, 0.78, 0.76) },
    );
    if let Some(ir_label) = ir_ranking_label(ir) {
        text.push_text(
            &mut commands,
            &ir_label,
            BitmapTextStyle { x: 0.68, y: 0.652, cell: 0.005, color: Color::rgb(0.78, 0.86, 0.7) },
        );
    }
    push_result_detail_panels(&text, &mut commands, judge_counts, fast_slow_counts, graph);
    text.push_text(
        &mut commands,
        "R RETRY  ENTER/ESC SELECT",
        BitmapTextStyle { x: 0.14, y: 0.925, cell: 0.005, color: Color::rgb(0.74, 0.78, 0.8) },
    );

    DrawPlan { clear: Color::rgb(0.025, 0.02, 0.018), commands }
}

/// 組み込みフォールバックリザルトの IR 行。Offline では表示しない。
pub(super) fn ir_ranking_label(ir: &crate::scene::ResultIrSnapshot) -> Option<String> {
    use crate::scene::ResultIrState;
    match ir.state {
        ResultIrState::Offline => None,
        ResultIrState::Loading => Some("IR LOADING...".to_string()),
        ResultIrState::Waiting => Some("IR WAITING...".to_string()),
        ResultIrState::Failed => Some("IR FAILED".to_string()),
        ResultIrState::Loaded => match (ir.rank, ir.total_player) {
            (Some(rank), Some(total)) => Some(format!("IR RANK {rank}/{total}")),
            (Some(rank), None) => Some(format!("IR RANK {rank}")),
            _ => Some("IR NO RECORD".to_string()),
        },
    }
}

pub(super) fn push_result_detail_panels(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    judge_counts: &DisplayJudgeCounts,
    fast_slow_counts: &FastSlowJudgeCounts,
    graph: &ResultGraphSnapshot,
) {
    push_result_panel(commands, Rect { x: 0.14, y: 0.715, width: 0.22, height: 0.17 });
    push_result_panel(commands, Rect { x: 0.38, y: 0.715, width: 0.22, height: 0.17 });
    push_result_panel(commands, Rect { x: 0.62, y: 0.715, width: 0.24, height: 0.17 });

    text.push_text(
        commands,
        "JUDGE DETAILS",
        BitmapTextStyle { x: 0.155, y: 0.732, cell: 0.0044, color: Color::rgb(0.86, 0.9, 0.88) },
    );
    text.push_text(
        commands,
        "FAST/SLOW DETAILS",
        BitmapTextStyle { x: 0.395, y: 0.732, cell: 0.0044, color: Color::rgb(0.86, 0.9, 0.88) },
    );
    text.push_text(
        commands,
        "TIMING DETAILS",
        BitmapTextStyle { x: 0.635, y: 0.732, cell: 0.0044, color: Color::rgb(0.86, 0.9, 0.88) },
    );

    push_result_judge_details(text, commands, judge_counts, graph);
    push_result_fast_slow_details(text, commands, fast_slow_counts);
    push_result_timing_details(text, commands, &graph.timing_points);
}

pub(super) fn push_result_panel(commands: &mut Vec<DrawCommand>, rect: Rect) {
    commands.push(DrawCommand::Rect { rect, color: Color::rgb(0.055, 0.052, 0.05) });
    commands.push(DrawCommand::Rect {
        rect: Rect { x: rect.x, y: rect.y, width: rect.width, height: 0.002 },
        color: Color::rgb(0.36, 0.46, 0.48),
    });
}

pub(super) fn push_result_judge_details(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    judge_counts: &DisplayJudgeCounts,
    graph: &ResultGraphSnapshot,
) {
    let values = [
        ("PG", judge_counts.pgreat, Color::rgb(0.68, 0.9, 1.0)),
        ("GR", judge_counts.great, Color::rgb(0.76, 0.94, 0.68)),
        ("GD", judge_counts.good, Color::rgb(0.95, 0.86, 0.48)),
        ("BD", judge_counts.bad, Color::rgb(0.96, 0.55, 0.42)),
        ("PR", judge_counts.poor, Color::rgb(0.84, 0.48, 0.58)),
        ("EP", judge_counts.empty_poor, Color::rgb(0.68, 0.58, 0.82)),
    ];
    let max = values.iter().map(|(_, value, _)| *value).max().unwrap_or(0).max(1) as f32;
    for (index, (label, value, color)) in values.iter().enumerate() {
        let y = 0.756 + index as f32 * 0.014;
        text.push_text(
            commands,
            &format!("{label} {value}"),
            BitmapTextStyle { x: 0.155, y, cell: 0.0039, color: Color::rgb(0.78, 0.82, 0.8) },
        );
        let width = 0.105 * (*value as f32 / max);
        commands.push(DrawCommand::Rect {
            rect: Rect { x: 0.245, y: y + 0.0015, width, height: 0.006 },
            color: *color,
        });
    }
    push_result_density_graph(
        commands,
        Rect { x: 0.245, y: 0.855, width: 0.095, height: 0.018 },
        &graph.judge_graph_density,
    );
}

pub(super) fn push_result_fast_slow_details(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    counts: &FastSlowJudgeCounts,
) {
    let rows = [
        ("PG", counts.fast_pgreat, counts.slow_pgreat),
        ("GR", counts.fast_great, counts.slow_great),
        ("GD", counts.fast_good, counts.slow_good),
        ("BD", counts.fast_bad, counts.slow_bad),
        ("PR", counts.fast_poor, counts.slow_poor),
        ("EP", counts.fast_empty_poor, counts.slow_empty_poor),
    ];
    let max =
        rows.iter().map(|(_, fast, slow)| fast.saturating_add(*slow)).max().unwrap_or(0).max(1)
            as f32;
    for (index, (label, fast, slow)) in rows.iter().enumerate() {
        let y = 0.756 + index as f32 * 0.014;
        text.push_text(
            commands,
            label,
            BitmapTextStyle { x: 0.395, y, cell: 0.0039, color: Color::rgb(0.78, 0.82, 0.8) },
        );
        let total = fast.saturating_add(*slow) as f32;
        let bar_total_w = 0.122 * (total / max);
        let fast_w = if total <= 0.0 { 0.0 } else { bar_total_w * *fast as f32 / total };
        commands.push(DrawCommand::Rect {
            rect: Rect { x: 0.425, y: y + 0.0015, width: fast_w, height: 0.006 },
            color: Color::rgb(0.45, 0.86, 0.96),
        });
        commands.push(DrawCommand::Rect {
            rect: Rect {
                x: 0.425 + fast_w,
                y: y + 0.0015,
                width: (bar_total_w - fast_w).max(0.0),
                height: 0.006,
            },
            color: Color::rgb(0.96, 0.52, 0.64),
        });
        text.push_text(
            commands,
            &format!("{fast}/{slow}"),
            BitmapTextStyle { x: 0.55, y, cell: 0.0034, color: Color::rgb(0.7, 0.75, 0.74) },
        );
    }
    text.push_text(
        commands,
        &format!("F {}  S {}", counts.fast_total(), counts.slow_total()),
        BitmapTextStyle { x: 0.395, y: 0.868, cell: 0.0038, color: Color::rgb(0.72, 0.84, 0.86) },
    );
}

pub(super) fn push_result_timing_details(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    points: &[ResultTimingPoint],
) {
    let graph_rect = Rect { x: 0.635, y: 0.758, width: 0.21, height: 0.07 };
    commands.push(DrawCommand::Rect { rect: graph_rect, color: Color::rgb(0.032, 0.034, 0.036) });
    commands.push(DrawCommand::Rect {
        rect: Rect {
            x: graph_rect.x + graph_rect.width / 2.0,
            y: graph_rect.y,
            width: 0.001,
            height: graph_rect.height,
        },
        color: Color::rgb(0.5, 0.56, 0.56),
    });
    push_result_timing_distribution(commands, graph_rect, points);

    if let Some((average, stddev)) = result_timing_stats(points) {
        text.push_text(
            commands,
            &format!("AVG {}ms", format_timing_ms(average)),
            BitmapTextStyle {
                x: 0.635,
                y: 0.845,
                cell: 0.0038,
                color: Color::rgb(0.74, 0.84, 0.86),
            },
        );
        text.push_text(
            commands,
            &format!("DEV {}ms", format_timing_ms(stddev)),
            BitmapTextStyle {
                x: 0.735,
                y: 0.845,
                cell: 0.0038,
                color: Color::rgb(0.74, 0.84, 0.86),
            },
        );
        text.push_text(
            commands,
            &format!("N {}", points.len()),
            BitmapTextStyle {
                x: 0.635,
                y: 0.868,
                cell: 0.0038,
                color: Color::rgb(0.68, 0.76, 0.74),
            },
        );
    } else {
        text.push_text(
            commands,
            "NO TIMING DATA",
            BitmapTextStyle {
                x: 0.635,
                y: 0.845,
                cell: 0.004,
                color: Color::rgb(0.68, 0.72, 0.72),
            },
        );
    }
}

pub(super) fn push_result_density_graph(
    commands: &mut Vec<DrawCommand>,
    rect: Rect,
    density: &[u8],
) {
    if density.is_empty() {
        return;
    }
    let max = density.iter().copied().max().unwrap_or(1).max(1) as f32;
    let bar_w = (rect.width / density.len().max(1) as f32).max(0.001);
    for (index, value) in density.iter().enumerate() {
        if *value == 0 {
            continue;
        }
        let height = rect.height * (*value as f32 / max);
        commands.push(DrawCommand::Rect {
            rect: Rect {
                x: rect.x + index as f32 * bar_w,
                y: rect.y + rect.height - height,
                width: bar_w * 0.8,
                height,
            },
            color: Color::rgba(0.64, 0.75, 0.9, 0.75),
        });
    }
}

pub(super) fn push_result_timing_distribution(
    commands: &mut Vec<DrawCommand>,
    rect: Rect,
    points: &[ResultTimingPoint],
) {
    if points.is_empty() {
        return;
    }
    const BUCKETS: usize = 21;
    const RANGE_MS: f32 = 50.0;
    let mut counts = [0u32; BUCKETS];
    for point in points {
        let delta_ms = (point.delta_us as f32 / 1_000.0).clamp(-RANGE_MS, RANGE_MS);
        let bucket =
            (((delta_ms + RANGE_MS) / (RANGE_MS * 2.0)) * (BUCKETS as f32 - 1.0)).round() as usize;
        counts[bucket.min(BUCKETS - 1)] += 1;
    }
    let max = counts.iter().copied().max().unwrap_or(1).max(1) as f32;
    let bar_w = rect.width / BUCKETS as f32;
    for (index, count) in counts.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        let height = rect.height * (*count as f32 / max);
        let t = index as f32 / (BUCKETS - 1) as f32;
        let color = if t < 0.5 {
            Color::rgba(0.45, 0.86, 0.96, 0.78)
        } else {
            Color::rgba(0.96, 0.52, 0.64, 0.78)
        };
        commands.push(DrawCommand::Rect {
            rect: Rect {
                x: rect.x + index as f32 * bar_w,
                y: rect.y + rect.height - height,
                width: bar_w * 0.8,
                height,
            },
            color,
        });
    }
}

pub(super) fn format_timing_ms(value: f32) -> String {
    format!("{value:.2}")
}
