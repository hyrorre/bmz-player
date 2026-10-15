use super::*;

pub(super) fn push_receptors(
    skin_manifest: &SkinManifest,
    commands: &mut Vec<DrawCommand>,
    board: Rect,
    lift: f32,
    lane_width: f32,
    active_lanes: &[Lane],
) {
    let receptor = skin_manifest.play_receptor_image();
    let lift_offset = lift.clamp(0.0, 1.0) * board.height;
    let receptor_y = (board.y + board.height * 0.825 - lift_offset).max(board.y);
    for (display_index, &lane) in active_lanes.iter().enumerate() {
        let x = board.x + display_index as f32 * lane_width;
        append_skin_render_items(
            commands,
            &[SkinRenderItem::Image {
                texture: SkinTextureId(receptor.texture_for_lane(lane)),
                rect: Rect {
                    x: x + lane_width * 0.1,
                    y: receptor_y,
                    width: lane_width * 0.8,
                    height: 0.026,
                },
                uv: receptor.uv,
                tint: skin_image_tint(lane),
                blend: BlendMode::Normal,
                scale: receptor.scale,
                border: receptor.border,
                source_size: receptor.source_size,
                linear_filter: false,
            }],
        );
    }
}

pub(super) fn push_combo_panel(
    skin_manifest: &SkinManifest,
    commands: &mut Vec<DrawCommand>,
    combo: u32,
) {
    let width = if combo >= 1000 { 0.2 } else { 0.15 };
    let image = skin_manifest.play_combo_panel_image(combo > 0);
    append_skin_render_items(
        commands,
        &[SkinRenderItem::Image {
            texture: SkinTextureId(image.texture),
            rect: Rect { x: 0.425 - width / 2.0, y: 0.16, width, height: 0.07 },
            uv: image.uv,
            tint: Color::rgb(1.0, 1.0, 1.0),
            blend: BlendMode::Normal,
            scale: image.scale,
            border: image.border,
            source_size: image.source_size,
            linear_filter: false,
        }],
    );
}

pub(super) fn play_progress(snapshot: &RenderSnapshot) -> f32 {
    if snapshot.duration.0 <= 0 {
        0.0
    } else {
        (snapshot.time.0 as f32 / snapshot.duration.0 as f32).clamp(0.0, 1.0)
    }
}

pub(super) fn end_of_note(snapshot: &RenderSnapshot) -> bool {
    snapshot.duration.0 > 0 && snapshot.time.0 >= snapshot.duration.0
}

pub(super) fn push_play_text(
    text: &TextRenderer,
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    push_play_status_text(text, commands, snapshot);
    push_judge_count_text(text, commands, snapshot);
    let metadata = difficulty_level_label(&snapshot.difficulty_name, &snapshot.play_level, "");
    if !metadata.is_empty() {
        text.push_text(
            commands,
            &metadata,
            BitmapTextStyle {
                x: 0.055,
                y: 0.155,
                cell: 0.0045,
                color: Color::rgb(0.62, 0.76, 0.72),
            },
        );
    }
    text.push_text(
        commands,
        &format!("G{}", snapshot.gauge.round() as u32),
        BitmapTextStyle { x: 0.885, y: 0.08, cell: 0.007, color: Color::rgb(0.8, 0.92, 0.86) },
    );
    if let Some(judgement) = snapshot.recent_judgements.last() {
        text.push_text(
            commands,
            &format_delta_ms(judgement.delta_us),
            BitmapTextStyle {
                x: 0.405,
                y: 0.282,
                cell: 0.0045,
                color: Color::rgb(0.72, 0.82, 0.86),
            },
        );
    }
}

pub(super) fn push_default_play_skin(
    skin_context: &SkinContext,
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
) {
    let skin = default_play_skin(snapshot);
    let recent_judgement = snapshot.recent_judgements.last();
    let judge_text = recent_judgement.map(|judgement| judgement.text.clone()).unwrap_or_default();
    let judge_image = recent_judgement.and_then(|judgement| {
        let region_count = skin_context.document().map(|d| d.judge_region_count()).unwrap_or(1);
        let region = crate::skin::lane_judge_region(
            judgement.lane.index(),
            bmz_core::lane::LANE_COUNT,
            region_count,
        );
        skin_context.document_judge_items(
            &judgement.text,
            snapshot.combo,
            ((snapshot.time.0 - judgement.time.0) / 1_000).clamp(i32::MIN as i64, i32::MAX as i64)
                as i32,
            &snapshot.skin_offsets,
            region,
        )
    });
    let has_judge_image = judge_image.is_some();
    if let Some(judge_items) = judge_image {
        append_skin_render_items(commands, &judge_items);
    }
    let text_values = [(TextSlot::Judge, judge_text)];
    let number_values = [
        (NumberSlot::Combo, snapshot.combo as i64),
        (NumberSlot::Gauge, snapshot.gauge.round() as i64),
        (
            NumberSlot::Hispeed,
            (snapshot.hispeed_display.unwrap_or(snapshot.hispeed) * 100.0).round() as i64,
        ),
    ];
    let items = skin.resolve(&SkinRenderContext {
        phase: SkinPhase::Play,
        elapsed_ms: (snapshot.time.0 / 1_000).clamp(i32::MIN as i64, i32::MAX as i64) as i32,
        text: &text_values,
        numbers: &number_values,
    });
    let items = if has_judge_image {
        items
            .into_iter()
            .filter(|item| {
                !matches!(item, SkinRenderItem::Text { text, .. } if text == &text_values[0].1)
                    && !matches!(item, SkinRenderItem::Text { text, .. } if text == &snapshot.combo.to_string())
            })
            .collect::<Vec<_>>()
    } else {
        items
    };
    append_skin_render_items(commands, &items);
}

pub(super) fn push_default_note_skin(
    skin_manifest: &SkinManifest,
    commands: &mut Vec<DrawCommand>,
    lane: Lane,
    rect: Rect,
) {
    push_note_skin_image(commands, lane, rect, skin_manifest.play_note_image());
}

pub(super) fn push_ln_start_skin(
    skin_manifest: &SkinManifest,
    commands: &mut Vec<DrawCommand>,
    lane: Lane,
    rect: Rect,
) {
    push_note_skin_image(commands, lane, rect, skin_manifest.play_ln_start_image());
}

pub(super) fn push_ln_end_skin(
    skin_manifest: &SkinManifest,
    commands: &mut Vec<DrawCommand>,
    lane: Lane,
    rect: Rect,
) {
    push_note_skin_image(commands, lane, rect, skin_manifest.play_ln_end_image());
}

pub(super) fn push_note_skin_image(
    commands: &mut Vec<DrawCommand>,
    lane: Lane,
    rect: Rect,
    note: SkinImageManifest,
) {
    append_skin_render_items(
        commands,
        &[SkinRenderItem::Image {
            texture: SkinTextureId(note.texture_for_lane(lane)),
            rect,
            uv: note.uv,
            tint: skin_image_tint(lane),
            blend: BlendMode::Normal,
            scale: note.scale,
            border: note.border,
            source_size: note.source_size,
            linear_filter: false,
        }],
    );
}

pub(super) fn default_play_skin(snapshot: &RenderSnapshot) -> SkinDefinition {
    let mut objects = Vec::new();
    if snapshot.combo > 0 {
        objects.push(SkinObject {
            id: SkinObjectId(1),
            source: SkinSource::Number {
                slot: NumberSlot::Combo,
                style: TextStyle {
                    font_id: None,
                    size: 0.05,
                    bitmap_size: None,
                    color: Color::rgb(0.94, 0.98, 1.0),
                    layer: TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.0,
                    overflow: TextOverflow::Overflow,
                    wrapping: false,
                    outline: None,
                    shadow: None,
                },
                digits: 0,
            },
            placements: vec![skin_placement(Rect { x: 0.38, y: 0.18, width: 0.18, height: 0.07 })],
        });
    }

    if snapshot.recent_judgements.last().is_some() {
        objects.push(SkinObject {
            id: SkinObjectId(2),
            source: SkinSource::Text {
                slot: TextSlot::Judge,
                style: TextStyle {
                    font_id: None,
                    size: 0.03,
                    bitmap_size: None,
                    color: Color::rgb(0.96, 0.92, 0.54),
                    layer: TextLayer::Skin,
                    align: TextAlign::Left,
                    max_width: 0.0,
                    overflow: TextOverflow::Overflow,
                    wrapping: false,
                    outline: None,
                    shadow: None,
                },
            },
            placements: vec![skin_placement(Rect { x: 0.38, y: 0.245, width: 0.3, height: 0.04 })],
        });
    }

    SkinDefinition { objects }
}

pub(super) fn skin_placement(rect: Rect) -> SkinPlacement {
    SkinPlacement {
        phase: SkinPhase::Play,
        time_ms: 0,
        rect,
        alpha: 1.0,
        blend: BlendMode::Normal,
        animation: Animation::none(),
    }
}
