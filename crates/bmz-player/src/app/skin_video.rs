use super::*;

pub(super) fn skin_video_texture_visible_in_plan(
    plan: &bmz_render::plan::DrawPlan,
    texture: SkinTextureId,
) -> bool {
    plan.commands.iter().any(|command| match command {
        bmz_render::plan::DrawCommand::Image { texture: used, tint, rect, .. }
        | bmz_render::plan::DrawCommand::RotatedImage { texture: used, tint, rect, .. } => {
            used.0 == texture.0 && tint.a > 0.0 && rect.width != 0.0 && rect.height != 0.0
        }
        _ => false,
    })
}

pub(super) fn skin_video_sources_from_decoded(decoded: &DecodedSkin) -> Vec<ActiveSkinVideoSource> {
    let enabled_options = decoded.document.enabled_options();
    decoded
        .sources
        .iter()
        .filter(|source| source.is_video)
        .map(|source| {
            let gating = skin_video_source_gating(&decoded.document, &source.source_id);
            ActiveSkinVideoSource {
                texture: source.texture,
                path: source.path.clone(),
                decoder: None,
                last_pts: None,
                loop_start_us: 0,
                active: gating.active,
                gating_op_sets: gating.op_sets,
                enabled_options: enabled_options.clone(),
                result_ranktime_ms: decoded.document.ranktime,
                failed: false,
            }
        })
        .collect()
}

pub(super) fn skin_video_sources_need_runtime_state(sources: &[ActiveSkinVideoSource]) -> bool {
    sources
        .iter()
        .any(|source| source.active && !source.failed && !source.gating_op_sets.is_empty())
}

pub(super) fn play_skin_video_draw_state(
    snapshot: &RenderSnapshot,
    skin_height: Option<u32>,
    note_lane_height_px: Option<i32>,
    skin_input_ms: i32,
) -> bmz_render::skin::SkinDrawState {
    let play_elapsed_ms = time_us_to_skin_ms(snapshot.play_elapsed_time);
    let skin_height = skin_height.unwrap_or(1080).max(1) as f32;
    let note_lane_height = note_lane_height_px
        .filter(|height| *height > 0)
        .map_or(skin_height, |height| height as f32);
    let lift = snapshot.lift.clamp(0.0, 1.0);
    let lane_cover = crate::config::play::clamp_lane_cover_for_lift(snapshot.lane_cover, lift);
    let offset_lift_px = (lift * note_lane_height).round() as i32;
    let visible_lane_height = note_lane_height * (1.0 - lift);
    let offset_lanecover_px = (-(note_lane_height * lane_cover)).round() as i32;
    let offset_hidden_cover_px =
        (snapshot.hidden_cover.clamp(0.0, 1.0) * visible_lane_height).round() as i32;
    bmz_render::skin::SkinDrawState {
        elapsed_ms: play_elapsed_ms,
        start_input_ms: if snapshot.seamless_play_entry {
            bmz_render::skin::skin_start_input_elapsed_ms(play_elapsed_ms, skin_input_ms)
        } else {
            None
        },
        operating_time_ms: snapshot.operating_time_ms,
        ready_timer_ms: snapshot.ready_elapsed_time.map(time_us_to_skin_ms),
        play_timer_ms: (snapshot.time.0 >= 0).then_some(time_us_to_skin_ms(snapshot.time)),
        rhythm_timer_ms: snapshot.rhythm_timer_elapsed_ms,
        quarter_note_elapsed_ms: snapshot.quarter_note_elapsed_ms,
        key_mode: snapshot.key_mode,
        combo: snapshot.combo,
        max_combo: snapshot.max_combo,
        ex_score: snapshot.ex_score,
        total_notes: snapshot.total_notes,
        past_notes: snapshot.past_notes,
        judge_counts: snapshot.judge_counts,
        fast_slow_counts: Some(snapshot.fast_slow_counts),
        gauge: snapshot.gauge,
        gauge_type: snapshot.gauge_type,
        gauge_auto_shift: snapshot.gauge_auto_shift,
        gauge_max: snapshot.gauge_max,
        gauge_border: snapshot.gauge_border,
        play_progress: play_skin_video_progress(snapshot),
        end_of_note: play_skin_video_end_of_note(snapshot),
        end_of_note_ms: snapshot.end_of_note_elapsed_ms,
        fadeout_ms: snapshot.fadeout_elapsed_ms,
        failed_ms: snapshot.failed_elapsed_ms,
        music_end_ms: snapshot.music_end_elapsed_ms,
        skin_offsets: snapshot.skin_offsets,
        hispeed: snapshot.hispeed,
        hispeed_mode_index: snapshot.hispeed_mode_index,
        base_hispeed_index: snapshot.base_hispeed_index,
        normal_hispeed_level: snapshot.normal_hispeed_level,
        hispeed_config_index: snapshot.hispeed_config_index,
        target_green_number: snapshot.target_green_number,
        timeleft_ms: play_skin_video_timeleft_ms(snapshot),
        total_duration_ms: snapshot.note_display_duration_ms,
        duration_green_ms: Some(bmz_render::skin::duration_to_green_number_ms(
            snapshot.note_display_duration_ms,
        )),
        lane_cover: snapshot.lane_cover,
        lift: snapshot.lift,
        offset_lift_px,
        offset_lanecover_px,
        offset_hidden_cover_px,
        lane_cover_changing: snapshot.lane_cover_changing,
        lanecover_enabled: snapshot.lanecover_enabled,
        lift_enabled: snapshot.lift_enabled,
        hidden_enabled: snapshot.hidden_enabled,
        hispeed_auto_adjust: snapshot.hispeed_auto_adjust,
        hidden_cover: snapshot.hidden_cover,
        play_level: skin_video_play_level_number(&snapshot.play_level),
        table_song: !snapshot.table_text_primary.is_empty(),
        difficulty: skin_video_difficulty_code(&snapshot.difficulty_name),
        judge_rank: snapshot.judge_rank,
        now_bpm: snapshot.now_bpm,
        min_bpm: snapshot.min_bpm,
        max_bpm: snapshot.max_bpm,
        has_bga: snapshot.has_bga,
        has_bpm_stop: snapshot.has_bpm_stop,
        bga_enabled: snapshot.bga_enabled,
        has_backbmp: snapshot.backbmp_background,
        bga_stretch: snapshot.bga_stretch,
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
        course_stage: snapshot.course_stage,
        hit_error_ring: snapshot.hit_error_ring.values,
        hit_error_ring_index: snapshot.hit_error_ring.index,
        skin_loaded: snapshot.seamless_play_entry || snapshot.ready_elapsed_time.is_some(),
        resource_load_progress: snapshot.resource_load_progress,
        ..bmz_render::skin::SkinDrawState::default()
    }
}

pub(super) fn time_us_to_skin_ms(time: TimeUs) -> i32 {
    (time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

pub(super) fn play_skin_video_progress(snapshot: &RenderSnapshot) -> f32 {
    if snapshot.duration.0 <= 0 {
        0.0
    } else {
        (snapshot.time.0 as f32 / snapshot.duration.0 as f32).clamp(0.0, 1.0)
    }
}

pub(super) fn play_skin_video_end_of_note(snapshot: &RenderSnapshot) -> bool {
    snapshot.duration.0 > 0 && snapshot.time.0 >= snapshot.duration.0
}

pub(super) fn play_skin_video_timeleft_ms(snapshot: &RenderSnapshot) -> i32 {
    (snapshot.duration.0.saturating_sub(snapshot.time.0) / 1_000)
        .saturating_add(1_000)
        .clamp(0, i32::MAX as i64) as i32
}

pub(super) fn skin_video_play_level_number(label: &str) -> i64 {
    let mut value = 0_i64;
    for digit in label.bytes().filter(u8::is_ascii_digit) {
        value = value.saturating_mul(10).saturating_add((digit - b'0') as i64);
    }
    value
}

pub(super) fn skin_video_difficulty_code(label: &str) -> i64 {
    let label = label.trim();
    match label {
        "1" => 1,
        "2" => 2,
        "3" => 3,
        "4" => 4,
        "5" => 5,
        _ if label.eq_ignore_ascii_case("BEGINNER") => 1,
        _ if label.eq_ignore_ascii_case("NORMAL") => 2,
        _ if label.eq_ignore_ascii_case("HYPER") => 3,
        _ if label.eq_ignore_ascii_case("ANOTHER") => 4,
        _ if label.eq_ignore_ascii_case("INSANE") => 5,
        _ => 0,
    }
}

/// 動画ソースの可視判定に必要なゲーティング情報。
pub(super) struct SkinVideoSourceGating {
    /// スキン config の option による静的な有効判定。
    pub(super) active: bool,
    /// このソースを参照する各 destination の op 条件。conditional destination の
    /// outer `if` 条件も合成済み。空なら参照されていない (= 常時可視)。
    pub(super) op_sets: Vec<Vec<i32>>,
}

pub(super) fn skin_video_source_gating(
    document: &SkinDocument,
    source_id: &str,
) -> SkinVideoSourceGating {
    let image_ids: HashSet<&str> = document
        .image
        .iter()
        .filter(|image| image.src == source_id)
        .map(|image| image.id.as_str())
        .collect();
    if image_ids.is_empty() {
        return SkinVideoSourceGating { active: true, op_sets: Vec::new() };
    }

    let mut render_object_ids = image_ids.clone();
    for imageset in &document.imageset {
        if imageset.images.iter().any(|id| image_ids.contains(id.as_str())) {
            render_object_ids.insert(imageset.id.as_str());
        }
    }

    let property_ops = skin_document_property_ops(document);
    let enabled_options = document.enabled_options();
    let mut referenced = false;
    let mut active = false;
    let mut op_sets = Vec::new();
    for (destination, op_set) in skin_document_destination_op_sets(document) {
        if !render_object_ids.contains(destination.id.as_str()) {
            continue;
        }
        referenced = true;
        if destination_property_ops_allow(&op_set, &enabled_options, &property_ops) {
            active = true;
        }
        op_sets.push(op_set);
    }
    if !referenced {
        return SkinVideoSourceGating { active: true, op_sets: Vec::new() };
    }
    SkinVideoSourceGating { active, op_sets }
}

pub(super) fn skin_document_property_ops(document: &SkinDocument) -> HashSet<i32> {
    document
        .property
        .iter()
        .flat_map(|property| property.item.iter().filter_map(|item| item.op.checked_abs()))
        .collect()
}

pub(super) fn apply_skin_video_source_enabled_options(
    sources: &mut [ActiveSkinVideoSource],
    enabled_options: &[i32],
    property_ops: &HashSet<i32>,
) {
    for source in sources {
        let was_active = source.active;
        source.enabled_options.clear();
        source.enabled_options.extend_from_slice(enabled_options);
        source.active =
            skin_video_source_static_active(&source.gating_op_sets, enabled_options, property_ops);
        if was_active && !source.active {
            source.decoder = None;
            source.last_pts = None;
        }
    }
}

pub(super) fn skin_video_source_static_active(
    op_sets: &[Vec<i32>],
    enabled_options: &[i32],
    property_ops: &HashSet<i32>,
) -> bool {
    op_sets.is_empty()
        || op_sets
            .iter()
            .any(|ops| destination_property_ops_allow(ops, enabled_options, property_ops))
}

/// 実行時 state に対して、動画ソースが現在のシーン状態で表示されるかどうかを判定する。
/// `op_sets` が空 (= destination から参照されていない) 場合は常時可視。
pub(super) fn skin_video_source_runtime_visible(
    source: &ActiveSkinVideoSource,
    state: &bmz_render::skin::SkinDrawState,
) -> bool {
    if source.gating_op_sets.is_empty() {
        return true;
    }
    source
        .gating_op_sets
        .iter()
        .any(|ops| bmz_render::skin::test_skin_ops(ops, &source.enabled_options, state))
}

pub(super) fn skin_document_destination_op_sets(
    document: &SkinDocument,
) -> Vec<(&SkinDestinationDef, Vec<i32>)> {
    document
        .destination
        .iter()
        .flat_map(|entry| match entry {
            DestinationListEntry::Single(destination) => {
                vec![(destination, destination.op.clone())]
            }
            DestinationListEntry::Conditional { if_ops, destinations } => destinations
                .iter()
                .map(|destination| {
                    let mut op_set = if_ops.clone();
                    op_set.extend(destination.op.iter().copied());
                    (destination, op_set)
                })
                .collect::<Vec<_>>(),
        })
        .collect()
}

pub(super) fn destination_property_ops_allow(
    ops: &[i32],
    enabled_options: &[i32],
    property_ops: &HashSet<i32>,
) -> bool {
    ops.iter().all(|op| {
        let Some(abs_op) = op.checked_abs() else {
            return true;
        };
        if !property_ops.contains(&abs_op) {
            return true;
        }
        if *op >= 0 { enabled_options.contains(op) } else { !enabled_options.contains(&abs_op) }
    })
}
