use super::*;

pub fn normalize_chart(
    source_path: &Path,
    intermediate: IntermediateChart,
    warnings: &mut Vec<ImportWarning>,
    check_resource_existence: bool,
) -> Result<PlayableChart, ImportError> {
    let metadata = normalize_metadata(&intermediate.metadata);
    let sound_table =
        build_sound_table(source_path, &intermediate, warnings, check_resource_existence);
    let bga_table = build_bga_table(source_path, &intermediate, warnings, check_resource_existence);
    let tick_objects = materialize_tick_objects(&intermediate)?;
    let tick_timing_events = collect_timing_events(&intermediate, warnings)?;
    let timing_map = build_timing_map_with_tick_scale(
        intermediate.metadata.initial_bpm.max(1.0),
        tick_timing_events.clone(),
        IMPORT_TICK_SCALE,
    );

    let mut draft = PlayableChartDraft::new(
        intermediate.identity.clone(),
        metadata,
        sound_table.assets.clone(),
        bga_table.assets.clone(),
    );
    draft.total_is_bmson_percent = intermediate.metadata.total_is_bmson_percent;
    let lane_buckets = collect_lane_objects(&tick_objects, &timing_map);

    let mut next_note_id = 0_u32;
    for lane in Lane::ALL {
        let resolved = normalize_lane_objects(
            lane,
            &lane_buckets[lane.index()],
            intermediate.lnobj_wav_key,
            warnings,
        );
        emit_resolved_lane_events(
            lane,
            resolved,
            &sound_table,
            &mut draft,
            &mut next_note_id,
            warnings,
        );
    }
    apply_layered_note_sounds(
        &intermediate.layered_note_sounds,
        &intermediate.measures,
        &sound_table,
        &mut draft,
        warnings,
    )?;

    draft.bgm_events = build_bgm_events(&tick_objects, &timing_map, &sound_table, warnings);
    draft.bga_events = build_bga_events(&tick_objects, &timing_map, &bga_table, warnings);
    draft.timing_events = build_timing_events(
        intermediate.metadata.initial_bpm.max(1.0),
        &tick_timing_events,
        &timing_map,
    );
    draft.scroll_events = build_scroll_events(&intermediate, &timing_map)?;
    draft.speed_events = build_speed_events(&intermediate, &timing_map)?;
    draft.judge_rank_events = build_judge_rank_events(&intermediate, &timing_map)?;
    draft.bgm_volume_events = build_chart_volume_events(&intermediate, &timing_map, true)?;
    draft.key_volume_events = build_chart_volume_events(&intermediate, &timing_map, false)?;
    draft.text_events = build_text_events(&intermediate, &timing_map)?;
    draft.bga_opacity_events = build_bga_opacity_events(&intermediate, &timing_map)?;
    draft.bga_argb_events = build_bga_argb_events(&intermediate, &timing_map)?;
    draft.swbga_definitions = build_swbga_definitions(&intermediate);
    draft.bga_keybound_events = build_bga_keybound_events(&intermediate, &timing_map)?;
    draft.bga_asset_by_bmp_key = bga_table.by_bmp_key.clone();
    draft.bar_lines = if intermediate.metadata.suppress_bar_lines {
        Vec::new()
    } else {
        build_bar_lines(&intermediate.measures, &timing_map)
    };

    compress_import_ticks(&mut draft);

    Ok(finalize_playable_chart(draft))
}

pub(super) fn normalize_metadata(input: &IntermediateMetadata) -> ChartMetadata {
    ChartMetadata {
        source_format: crate::model::ChartSourceFormat::Unknown,
        title: input.title.clone(),
        subtitle: input.subtitle.clone(),
        artist: input.artist.clone(),
        subartist: input.subartist.clone(),
        genre: input.genre.clone(),
        difficulty_name: input.difficulty_name.clone(),
        judge_rank: input.judge_rank,
        judge_rank_spec: input.judge_rank_spec,
        play_level: input.play_level.clone(),
        initial_bpm: input.initial_bpm,
        total: input.total,
        stage_file: input.stage_file.clone(),
        banner_file: input.banner_file.clone(),
        backbmp_file: input.backbmp_file.clone(),
        preview_file: input.preview_file.clone(),
        volwav_percent: input.volwav_percent,
        long_note_mode: input.long_note_mode,
        long_note_mode_defined: input.long_note_mode_defined,
        has_bga: input.has_bga,
        has_bms_random: input.has_bms_random,
        source_url: input.source_url.clone(),
        append_url: input.append_url.clone(),
        bms_headers: input.bms_headers.clone(),
        miss_text: input.miss_text.clone(),
        key_mode: input.key_mode,
    }
}
