use super::*;

pub(super) mod visible;

use visible::populate_visible_playfield;

/// WAV/BMP の完了を待たず、オプション適用済み譜面から確定できる Play skin 値を
/// placeholder snapshot へ反映する。
///
/// 入力で変化するレーン設定やリソース進捗は保持する。これにより PRELOAD 中でも
/// beatoraja と同様にノーツ分布、BPM graph、譜面メタデータを利用できる。
pub fn apply_prepared_chart_to_render_snapshot(
    snapshot: &mut RenderSnapshot,
    chart: &PlayableChart,
    cache: &PlayRenderSnapshotCache,
    battle: bool,
) {
    let total_notes = if battle {
        bmz_gameplay::score::scored_note_count_excluding_lanes(
            chart,
            &crate::screens::play_session::second_player_lane_mask(),
        )
    } else {
        scored_note_count(chart)
    };
    snapshot.duration = chart.end_time;
    snapshot.title.clone_from(&chart.metadata.title);
    snapshot.subtitle.clone_from(&chart.metadata.subtitle);
    snapshot.artist.clone_from(&chart.metadata.artist);
    snapshot.subartist.clone_from(&chart.metadata.subartist);
    snapshot.genre.clone_from(&chart.metadata.genre);
    snapshot.difficulty_name.clone_from(&chart.metadata.difficulty_name);
    snapshot.judge_rank = chart.metadata.judge_rank;
    snapshot.play_level.clone_from(&chart.metadata.play_level);
    snapshot.total_notes = total_notes;
    snapshot.chart_total_gauge = gauge_total_for_chart(chart.metadata.total, total_notes) as f32;
    snapshot.now_bpm = chart.metadata.initial_bpm as f32;
    snapshot.main_bpm = chart.metadata.initial_bpm as f32;
    snapshot.min_bpm = cache.min_bpm;
    snapshot.max_bpm = cache.max_bpm;
    snapshot.has_bga = chart.metadata.has_bga;
    snapshot.has_long_notes = Some(!chart.long_notes.is_empty());
    snapshot.has_bpm_stop = cache.has_bpm_stop;
    snapshot.key_mode = chart.metadata.key_mode;
    snapshot.skin_attempt.ln_mode_index =
        Some(crate::skin_extension::long_note_mode_index(chart.metadata.long_note_mode));
    snapshot.skin_attempt.has_bga = Some(chart.metadata.has_bga);
    snapshot.skin_attempt.has_random_sequence = Some(chart.metadata.has_bms_random);
    let primary_key_mode = if battle {
        match chart.metadata.key_mode {
            KeyMode::K10 => KeyMode::K5,
            KeyMode::K14 => KeyMode::K7,
            key_mode => key_mode,
        }
    } else {
        chart.metadata.key_mode
    };
    snapshot.skin_attempt.effective_key_mode = Some(primary_key_mode);
    snapshot.fs_threshold_ms = rm_skin_fs_threshold_ms(chart.metadata.judge_rank, primary_key_mode);
    snapshot.judge_graph_density = Arc::clone(&cache.judge_graph_density);
    snapshot.bpm_graph_segments = Arc::clone(&cache.bpm_graph_segments);

    snapshot.note_display_duration_ms = display_duration_ms_for_bpm_hispeed(
        snapshot.now_bpm,
        snapshot.hispeed,
        snapshot.lane_cover,
        snapshot.lift,
        1.0,
    )
    .round()
    .clamp(0.0, i32::MAX as f32) as i32;
    snapshot.adjusted_cover_progress = compute_adjusted_cover_progress(
        snapshot.hidden_enabled,
        snapshot.lane_cover,
        snapshot.lift,
        snapshot.hsfix_index,
        snapshot.now_bpm,
        snapshot.max_bpm,
        snapshot.main_bpm,
    );
    snapshot.adjusted_rate = compute_adjusted_rate(
        snapshot.hidden_enabled,
        snapshot.lanecover_enabled,
        snapshot.hsfix_index,
        snapshot.now_bpm,
        snapshot.max_bpm,
        snapshot.main_bpm,
    );
    snapshot.adjusted_rate_adot = snapshot.adjusted_rate.map(|rate| (rate * 100.0).floor() as i32);
}

pub fn build_render_snapshot(
    session: &GameSession,
    chart_now: TimeUs,
    recent_judgements: &[JudgementEvent],
    best_ex_score: Option<u32>,
) -> RenderSnapshot {
    build_render_snapshot_with_bga_frames(
        session,
        chart_now,
        recent_judgements,
        best_ex_score,
        &BgaFrameCatalog::new(),
    )
}

pub fn build_render_snapshot_with_bga_frames(
    session: &GameSession,
    chart_now: TimeUs,
    recent_judgements: &[JudgementEvent],
    best_ex_score: Option<u32>,
    bga_frames: &BgaFrameCatalog,
) -> RenderSnapshot {
    build_render_snapshot_with_target_and_bga_frames(
        session,
        chart_now,
        recent_judgements,
        best_ex_score,
        None,
        None,
        bga_frames,
    )
}

pub fn build_render_snapshot_with_target_and_bga_frames(
    session: &GameSession,
    chart_now: TimeUs,
    recent_judgements: &[JudgementEvent],
    best_ex_score: Option<u32>,
    best_ghost: Option<&[u8]>,
    target_ex_score: Option<u32>,
    bga_frames: &BgaFrameCatalog,
) -> RenderSnapshot {
    let cache = PlayRenderSnapshotCache::from_chart(&session.chart);
    build_render_snapshot_with_target_and_bga_frames_cached(
        session,
        chart_now,
        recent_judgements,
        best_ex_score,
        best_ghost,
        target_ex_score,
        bga_frames,
        &cache,
    )
}

pub fn build_render_snapshot_with_target_and_bga_frames_cached(
    session: &GameSession,
    chart_now: TimeUs,
    recent_judgements: &[JudgementEvent],
    best_ex_score: Option<u32>,
    best_ghost: Option<&[u8]>,
    target_ex_score: Option<u32>,
    bga_frames: &BgaFrameCatalog,
    cache: &PlayRenderSnapshotCache,
) -> RenderSnapshot {
    let refreshed = (cache.conditional_revision != session.chart.metadata.conditional_revision)
        .then(|| cache.for_updated_chart(&session.chart));
    let cache = refreshed.as_ref().unwrap_or(cache);
    let mut snapshot = build_render_state_with_target_and_bga_frames_cached(
        session,
        chart_now,
        recent_judgements,
        best_ex_score,
        best_ghost,
        target_ex_score,
        bga_frames,
        cache,
    );
    populate_visible_playfield(
        &mut snapshot,
        &super::projection::PlayfieldView::from(session),
        chart_now,
        cache,
        lane_render_time(session, chart_now),
    );
    snapshot
}

pub(crate) fn build_render_state_with_target_and_bga_frames_cached(
    session: &GameSession,
    chart_now: TimeUs,
    recent_judgements: &[JudgementEvent],
    best_ex_score: Option<u32>,
    best_ghost: Option<&[u8]>,
    target_ex_score: Option<u32>,
    bga_frames: &BgaFrameCatalog,
    cache: &PlayRenderSnapshotCache,
) -> RenderSnapshot {
    let refreshed = (cache.conditional_revision != session.chart.metadata.conditional_revision)
        .then(|| cache.for_updated_chart(&session.chart));
    let cache = refreshed.as_ref().unwrap_or(cache);
    let projected_best_ex_score =
        best_ghost.map(|ghost| ghost_ex_score_at_progress(ghost, session.score.past_notes));
    let lane_render_now = lane_render_time(session, chart_now);
    let play_elapsed_time = if chart_now.0 < 0 { TimeUs(0) } else { chart_now };
    let gauge_graph_time_ms = (chart_now.0.max(0) / 1_000).clamp(0, i32::MAX as i64) as i32;
    // beatoraja の LaneRenderer と同じく、現在 BPM と緑数字は表示オフセットを
    // 適用したレーン時刻から求める。判定演出や BGA の時計には適用しない。
    let now_bpm = session.timing_map.bpm_at_time(lane_render_now) as f32;
    let cursor_tick = session.timing_map.time_to_tick_f64(scroll_render_time(lane_render_now));
    let scroll_multiplier = current_scroll_multiplier_from_segments(
        &cache.scroll_integral,
        &cache.speed_segments,
        cursor_tick,
    );
    let note_display_duration_ms = note_display_duration_ms(session, now_bpm, scroll_multiplier);
    let lane_cover = if session.lanecover_enabled && session.lane_cover_visible {
        crate::config::play::clamp_lane_cover_for_lift(session.lane_cover, session.lift)
    } else {
        0.0
    };
    let adjusted_cover_progress = compute_adjusted_cover_progress(
        session.hidden_enabled,
        lane_cover,
        session.lift,
        session.hsfix_index,
        now_bpm,
        cache.max_bpm,
        session.chart.metadata.initial_bpm as f32,
    );
    let adjusted_rate = compute_adjusted_rate(
        session.hidden_enabled,
        session.lanecover_enabled,
        session.hsfix_index,
        now_bpm,
        cache.max_bpm,
        session.chart.metadata.initial_bpm as f32,
    );
    let gauge_graph_points = session
        .gauge
        .gauges
        .iter()
        .map(|gauge| ResultGaugeGraphPoint {
            time_ms: gauge_graph_time_ms,
            value: gauge.value,
            max: gauge.definition.max,
            border: gauge.definition.border,
            gauge_type: gauge.definition.gauge_type as i32,
            course_section_start: false,
        })
        .collect();
    let independent_opponent = session.battle_opponent.as_ref().map(|opponent| {
        let gauge = opponent.gauge.current();
        OpponentRenderSnapshot {
            combo: opponent.score.combo,
            max_combo: opponent.score.max_combo,
            ex_score: opponent.score.ex_score(),
            total_notes: opponent.scored_total_notes,
            past_notes: opponent.score.past_notes,
            judge_counts: display_judge_counts_for_score(&opponent.score),
            gauge: gauge.value,
            gauge_type: gauge.definition.gauge_type as i32,
            gauge_max: gauge.definition.max,
            gauge_border: gauge.definition.border,
            full_combo_elapsed_ms: optional_skin_timer_elapsed_ms(
                chart_now,
                opponent.full_combo_started_at,
            ),
            end_of_note_elapsed_ms: end_of_note_elapsed_ms(chart_now, cache.end_of_note_time),
            gauge_increase_elapsed_ms: optional_skin_timer_elapsed_ms(
                chart_now,
                opponent.gauge_increase_started_at,
            ),
            gauge_max_elapsed_ms: optional_skin_timer_elapsed_ms(
                chart_now,
                opponent.gauge_max_started_at,
            ),
        }
    });
    let legacy_opponent = session.opponent_score.as_ref().zip(session.opponent_gauge.as_ref()).map(
        |(score, gauge)| OpponentRenderSnapshot {
            combo: score.combo,
            max_combo: score.max_combo,
            ex_score: score.ex_score(),
            total_notes: session.scored_total_notes,
            past_notes: score.past_notes,
            judge_counts: display_judge_counts_for_score(score),
            gauge: gauge.current().value,
            gauge_type: gauge.current().definition.gauge_type as i32,
            gauge_max: gauge.current().definition.max,
            gauge_border: gauge.current().definition.border,
            full_combo_elapsed_ms: session.opponent_full_combo_started_at.and_then(|started_at| {
                (chart_now.0 >= started_at.0).then_some(
                    ((chart_now.0 - started_at.0) / 1_000).clamp(0, i32::MAX as i64) as i32,
                )
            }),
            end_of_note_elapsed_ms: end_of_note_elapsed_ms(chart_now, cache.end_of_note_time),
            gauge_increase_elapsed_ms: optional_skin_timer_elapsed_ms(
                chart_now,
                session.opponent_gauge_increase_started_at,
            ),
            gauge_max_elapsed_ms: optional_skin_timer_elapsed_ms(
                chart_now,
                session.opponent_gauge_max_started_at,
            ),
        },
    );
    let opponent = independent_opponent.or(legacy_opponent);
    RenderSnapshot {
        time: chart_now,
        player_name: String::new(),
        current_fps: 0,
        play_elapsed_time,
        operating_time_ms: 0,
        skin_input: Default::default(),
        skin_attempt: bmz_render::snapshot::SkinAttemptState {
            effective_key_mode: Some(session.primary_key_mode),
            hsfix_index: usize::try_from(session.hsfix_index).ok(),
            gauge_auto_shift_index: Some(crate::skin_extension::gauge_auto_shift_index(
                session.gauge.auto_shift_mode,
            )),
            bottom_shiftable_gauge_index: Some(
                crate::skin_extension::bottom_shiftable_gauge_index(
                    session.gauge.bottom_shiftable_gauge,
                ),
            ),
            judge_algorithm_index: Some(crate::skin_extension::judge_algorithm_index(
                session.judge.algorithm,
            )),
            ln_mode_index: Some(crate::skin_extension::long_note_mode_index(
                session.chart.metadata.long_note_mode,
            )),
            has_bga: Some(session.chart.metadata.has_bga),
            has_random_sequence: Some(session.chart.metadata.has_bms_random),
            ..Default::default()
        },
        ready_elapsed_time: None,
        seamless_play_entry: false,
        rhythm_timer_elapsed_ms: rhythm_timer_elapsed_ms(
            &session.timing_map,
            &session.chart.bar_lines,
            chart_now,
        ),
        quarter_note_elapsed_ms: quarter_note_elapsed_ms(
            &session.timing_map,
            &session.chart.bar_lines,
            chart_now,
        ),
        // session が構築できている時点で WAV 等のロードは完了している。
        resources_loaded: true,
        resource_load_progress: 1.0,
        duration: session.chart.end_time,
        title: session.chart.metadata.title.clone(),
        subtitle: session.chart.metadata.subtitle.clone(),
        artist: session.chart.metadata.artist.clone(),
        subartist: session.chart.metadata.subartist.clone(),
        genre: session.chart.metadata.genre.clone(),
        difficulty_name: session.chart.metadata.difficulty_name.clone(),
        judge_rank: session.chart.metadata.judge_rank,
        play_level: session.chart.metadata.play_level.clone(),
        arrange: "NORMAL".to_string(),
        arrange_2p: "NORMAL".to_string(),
        lane_shuffle_pattern: Vec::new(),
        target: String::new(),
        resolved_target_name: None,
        combo: session.display_combo(),
        max_combo: session.display_max_combo(),
        ex_score: session.score.ex_score(),
        total_notes: session.scored_total_notes,
        chart_total_gauge: gauge_total_for_chart(
            session.chart.metadata.total,
            session.scored_total_notes,
        ) as f32,
        past_notes: session.score.past_notes,
        judge_counts: display_judge_counts(session),
        fast_slow_counts: display_fast_slow_counts(session),
        gauge: session.gauge.current().value,
        gauge_type: session.gauge.current().definition.gauge_type as i32,
        gauge_graph_points,
        gauge_auto_shift: session.gauge.auto_shift,
        gauge_max: session.gauge.current().definition.max,
        gauge_border: session.gauge.current().definition.border,
        opponent,
        hispeed: session.hispeed,
        hispeed_mode_index: hispeed_mode_index(session.hispeed_mode),
        base_hispeed_index: base_hispeed_index(session.base_hispeed_mode),
        normal_hispeed_level: session.normal_hispeed_level,
        hispeed_config_index: hispeed_config_index(
            session.base_hispeed_mode,
            session.floating_policy,
        ),
        target_green_number: session.target_green_number,
        lift: session.lift,
        lane_cover,
        lane_cover_changing: session.lane_cover_changing,
        lanecover_enabled: session.lanecover_enabled,
        lift_enabled: session.lift_enabled,
        hidden_enabled: session.hidden_enabled,
        hispeed_auto_adjust: session.hispeed_auto_adjust,
        note_display_duration_ms,
        hidden_cover: session.hidden_cover,
        skin_offsets: skin_offsets_from_session(session, chart_now, play_elapsed_time),
        now_bpm,
        min_bpm: cache.min_bpm,
        max_bpm: cache.max_bpm,
        has_bga: session.chart.metadata.has_bga,
        has_long_notes: Some(!session.chart.long_notes.is_empty()),
        has_bpm_stop: cache.has_bpm_stop,
        bga_enabled: session.bga_enabled,
        bga_base: session
            .bga_enabled
            .then(|| current_bga_frame(cache, chart_now, BgaEventKind::Base, bga_frames))
            .flatten(),
        bga_layer: session
            .bga_enabled
            .then(|| {
                current_keybound_bga_frame(session, cache, chart_now, bga_frames).or_else(|| {
                    current_bga_frame(cache, chart_now, BgaEventKind::Layer, bga_frames)
                })
            })
            .flatten(),
        bga_layer2: session
            .bga_enabled
            .then(|| current_bga_frame(cache, chart_now, BgaEventKind::Layer2, bga_frames))
            .flatten(),
        bga_poor: session
            .bga_enabled
            .then(|| {
                current_poor_bga_frame(
                    cache,
                    chart_now,
                    recent_judgements,
                    bga_frames,
                    session.poor_bga_duration_us,
                )
            })
            .flatten(),
        bga_stretch: session.bga_stretch,
        best_ex_score,
        projected_best_ex_score,
        target_ex_score,
        judge_timing_offset_ms: (session.offsets.visual_offset_us / 1_000) as i32,
        judge_timing_auto_adjust: session.input_offset_auto_adjust_enabled,
        main_bpm: session.chart.metadata.initial_bpm as f32,
        hsfix_index: session.hsfix_index,
        fs_threshold_ms: rm_skin_fs_threshold_ms(
            session.chart.metadata.judge_rank,
            session.primary_key_mode,
        ),
        adjusted_cover_progress,
        adjusted_rate,
        adjusted_rate_adot: adjusted_rate.map(|rate| (rate * 100.0).floor() as i32),
        judge_graph_density: Arc::clone(&cache.judge_graph_density),
        bpm_graph_segments: Arc::clone(&cache.bpm_graph_segments),
        autoplay: session.autoplay.as_ref().is_some_and(|autoplay| autoplay.is_full()),
        replay_playback: session.replay_player.is_some() && session.replay_lane_mask.is_none(),
        rule_mode_index: crate::skin_extension::rule_mode_index(session.rule_mode),
        ln_score_policy_index: None,
        practice_mode: false,
        practice_preview: false,
        assist_flags: crate::assist::mask_flags(session.assist.configured_mask),
        assist_extra_note_depth: session.assist.extra_note_depth,
        assist_mine_mode: session.assist.mine_mode,
        assist_scroll_mode: session.assist.scroll_mode,
        assist_long_note_mode: session.assist.long_note_mode,
        guide_se_enabled: session.guide_se_enabled,
        constant_enabled: session.constant_enabled,
        constant_fade_ms: session.constant_fade_ms,
        judge_area: session.assist.judge_area,
        mark_processed_note: session.assist.mark_note,
        bpm_guide: session.assist.bpm_guide,
        judge_area_key_y: [0.0; 5],
        judge_area_scratch_y: [0.0; 5],
        score_save_enabled: !(session.autoplay.as_ref().is_some_and(|autoplay| autoplay.is_full())
            || session.replay_player.is_some() && session.replay_lane_mask.is_none())
            && session.assist.score_update_enabled(),
        course_stage: None,
        course_titles: Default::default(),
        table_text_primary: String::new(),
        table_text_secondary: String::new(),
        table_text_fallback: String::new(),
        key_mode: session.chart.metadata.key_mode,
        visible_notes: std::array::from_fn(|_| Vec::new()),
        visible_mines: std::array::from_fn(|_| Vec::new()),
        recent_inputs: session
            .recent_inputs
            .iter()
            .map(|input| DisplayInput { lane: input.lane, time: input.time })
            .collect(),
        recent_judgements: if session.recent_display_judgements.len() != recent_judgements.len()
            || !session
                .recent_display_judgements
                .iter()
                .zip(recent_judgements)
                .all(|(display, judgement)| display.judgement == *judgement)
        {
            // Snapshot 単体テストなど、session を経由せず判定列を渡す呼び出しの
            // 互換経路。通常プレイでは下の判定時点コンボを使う。
            recent_judgements
                .iter()
                .map(|event| {
                    let combo = if session.display_only_lane_mask[event.lane.index()] {
                        session.opponent_score.as_ref().map_or(0, |score| score.combo)
                    } else {
                        session.display_combo()
                    };
                    display_judgement(event, combo)
                })
                .collect()
        } else {
            session
                .recent_display_judgements
                .iter()
                .map(|event| display_judgement(&event.judgement, event.combo))
                .collect()
        },
        skin_events: Vec::new(),
        hit_error_ring: bmz_render::snapshot::HitErrorRingSnapshot {
            values: session.hit_error_ring.values,
            index: session.hit_error_ring.index,
        },
        full_combo_elapsed_ms: session.full_combo_started_at.and_then(|started_at| {
            (chart_now.0 >= started_at.0)
                .then_some(((chart_now.0 - started_at.0) / 1_000).clamp(0, i32::MAX as i64) as i32)
        }),
        end_of_note_elapsed_ms: end_of_note_elapsed_ms(chart_now, cache.end_of_note_time),
        fadeout_elapsed_ms: None,
        failed_elapsed_ms: None,
        music_end_elapsed_ms: None,
        gauge_increase_elapsed_ms: optional_skin_timer_elapsed_ms(
            chart_now,
            session.gauge_increase_started_at,
        ),
        gauge_max_elapsed_ms: optional_skin_timer_elapsed_ms(
            chart_now,
            session.gauge_max_started_at,
        ),
        bar_lines: Vec::new(),
        bpm_lines: Vec::new(),
        stop_lines: Vec::new(),
        time_lines: Vec::new(),
        visible_long_notes: Vec::new(),
        keyon_ms: lane_keyon_ms(session, chart_now, play_elapsed_time),
        keyoff_ms: lane_keyoff_ms(session, chart_now, play_elapsed_time),
        show_ln_tail_cap: session.show_ln_tail_cap,
        // beatoraja の TIMER_HCN_ACTIVE / TIMER_HCN_DAMAGE: HCN passing 中のみアクティブ。
        hcn_active_ms: std::array::from_fn(|lane_index| {
            session.lane_hcn_timer[lane_index].filter(|t| t.inclease).map(|t| {
                ((chart_now.0 - t.since.0) / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32
            })
        }),
        hcn_damage_ms: std::array::from_fn(|lane_index| {
            session.lane_hcn_timer[lane_index].filter(|t| !t.inclease).map(|t| {
                ((chart_now.0 - t.since.0) / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32
            })
        }),
        // beatoraja の TIMER_HOLD: LN ホールド中 (processing != null) のみアクティブ。
        hold_ms: std::array::from_fn(|lane_index| {
            session.judge.lanes[lane_index].active_long.map(|active| {
                ((chart_now.0 - active.started_at.0) / 1_000)
                    .clamp(i32::MIN as i64, i32::MAX as i64) as i32
            })
        }),
        overlay: OverlaySnapshot::default(),
        stagefile_background: false,
        stagefile_image_size: None,
        backbmp_background: false,
        chart_text: bmz_chart::text::chart_text_at_time(&session.chart.text_events, chart_now)
            .to_string(),
    }
}

pub fn update_render_snapshot_play_options(
    snapshot: &mut RenderSnapshot,
    session: &GameSession,
    chart_now: TimeUs,
) {
    snapshot.hispeed = session.hispeed;
    snapshot.hispeed_mode_index = hispeed_mode_index(session.hispeed_mode);
    snapshot.base_hispeed_index = base_hispeed_index(session.base_hispeed_mode);
    snapshot.normal_hispeed_level = session.normal_hispeed_level;
    snapshot.hispeed_config_index =
        hispeed_config_index(session.base_hispeed_mode, session.floating_policy);
    snapshot.target_green_number = session.target_green_number;
    snapshot.lift = session.lift;
    snapshot.lane_cover = if session.lanecover_enabled && session.lane_cover_visible {
        crate::config::play::clamp_lane_cover_for_lift(session.lane_cover, session.lift)
    } else {
        0.0
    };
    snapshot.lane_cover_changing = session.lane_cover_changing;
    snapshot.lanecover_enabled = session.lanecover_enabled;
    snapshot.lift_enabled = session.lift_enabled;
    snapshot.hidden_enabled = session.hidden_enabled;
    snapshot.hispeed_auto_adjust = session.hispeed_auto_adjust;
    let lane_render_now = lane_render_time(session, chart_now);
    snapshot.note_display_duration_ms = note_display_duration_ms(
        session,
        session.timing_map.bpm_at_time(lane_render_now) as f32,
        current_scroll_multiplier(&session.chart, &session.timing_map, lane_render_now),
    );
    snapshot.hidden_cover = session.hidden_cover;
}

pub(super) fn hispeed_mode_index(mode: bmz_gameplay::session::HispeedMode) -> i32 {
    match mode {
        bmz_gameplay::session::HispeedMode::Normal
        | bmz_gameplay::session::HispeedMode::Classic => 0,
        bmz_gameplay::session::HispeedMode::Floating => 1,
    }
}

pub(super) const fn base_hispeed_index(mode: bmz_gameplay::session::HispeedMode) -> i32 {
    match mode {
        bmz_gameplay::session::HispeedMode::Normal => 1,
        bmz_gameplay::session::HispeedMode::Classic
        | bmz_gameplay::session::HispeedMode::Floating => 0,
    }
}

pub(super) const fn hispeed_config_index(
    base: bmz_gameplay::session::HispeedMode,
    floating: bmz_gameplay::session::FloatingPolicy,
) -> i32 {
    match (base, floating) {
        (_, bmz_gameplay::session::FloatingPolicy::Locked) => 2,
        (
            bmz_gameplay::session::HispeedMode::Normal,
            bmz_gameplay::session::FloatingPolicy::Disabled,
        ) => 0,
        (_, bmz_gameplay::session::FloatingPolicy::Disabled) => 1,
        (
            bmz_gameplay::session::HispeedMode::Normal,
            bmz_gameplay::session::FloatingPolicy::Toggle,
        ) => 3,
        (_, bmz_gameplay::session::FloatingPolicy::Toggle) => 4,
    }
}
