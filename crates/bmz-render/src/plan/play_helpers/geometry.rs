use super::*;

pub(super) fn skin_lane_height_px(
    skin: &SkinContext,
    key_mode: KeyMode,
    fallback_canvas_h: f32,
) -> f32 {
    skin.document()
        .and_then(|document| {
            let enabled_options = document.enabled_options();
            document.note_lane_area(Lane::Key1, key_mode, &enabled_options)
        })
        .map_or(fallback_canvas_h, |rect| rect.height * fallback_canvas_h)
}

pub(super) fn skin_lift_offset_px(lift: f32, lane_h: f32) -> i32 {
    (lift * lane_h).round() as i32
}

pub(super) fn skin_lanecover_offset_px(lane_cover: f32, _lift: f32, lane_h: f32) -> i32 {
    (-(lane_h * lane_cover.clamp(0.0, 1.0))).round() as i32
}

pub(super) fn lane_cover_bottom_progress(lane_cover: f32, lift: f32) -> f32 {
    let lift = lift.clamp(0.0, 1.0);
    let visible = (1.0 - lift).max(f32::EPSILON);
    ((visible - lane_cover.clamp(0.0, visible)) / visible).clamp(0.0, 1.0)
}

pub(super) fn skin_hidden_cover_offset_px(lift: f32, hidden_cover: f32, lane_h: f32) -> i32 {
    ((1.0 - lift) * hidden_cover * lane_h).round() as i32
}

pub(super) fn push_judge_line(
    skin_manifest: &SkinManifest,
    commands: &mut Vec<DrawCommand>,
    board: Rect,
    lift: f32,
) {
    let image = skin_manifest.play_judge_line_image();
    let line_y = judge_line_y(board, lift);
    append_skin_render_items(
        commands,
        &[SkinRenderItem::Image {
            texture: SkinTextureId(image.texture),
            rect: Rect { x: board.x, y: line_y, width: board.width, height: 0.006 },
            uv: image.uv,
            tint: skin_image_tint(Lane::Key1),
            blend: BlendMode::Normal,
            scale: image.scale,
            border: image.border,
            source_size: image.source_size,
            linear_filter: false,
        }],
    );
}

pub(super) fn note_rect_y(board: Rect, lift: f32, progress_to_hit: f32) -> f32 {
    play_object_y(board, lift, progress_to_hit) - NOTE_HEIGHT
}

pub(super) fn play_object_y(board: Rect, lift: f32, progress_to_hit: f32) -> f32 {
    let judge_y = judge_line_y(board, lift);
    judge_y - progress_to_hit.clamp(0.0, 1.0) * (judge_y - board.y)
}

pub(super) fn push_play_bar_line(
    commands: &mut Vec<DrawCommand>,
    skin: &SkinContext,
    skin_state: &crate::skin::SkinDrawState,
    key_mode: KeyMode,
    board: Rect,
    lift: f32,
    bar: &crate::snapshot::VisibleBarLine,
    skin_offsets: &SkinOffsetValues,
) {
    let start = commands.len();
    let items = skin.document_bar_line_items(bar.y, key_mode, skin_state);
    if items.is_empty() {
        if skin.document().is_none() {
            push_bar_line_rect_geometry(commands, board, lift, bar.y, skin_offsets);
        }
    } else {
        let items = skin.apply_play_skin_global_offset(items, skin_state);
        append_skin_render_items(commands, &items);
    }
    apply_bar_line_alpha_offset(&mut commands[start..], skin_offsets);
    apply_draw_command_alpha(&mut commands[start..], bar.alpha);
}

pub(super) fn push_play_aux_lines(
    commands: &mut Vec<DrawCommand>,
    skin: &SkinContext,
    skin_state: &crate::skin::SkinDrawState,
    snapshot: &RenderSnapshot,
    key_mode: KeyMode,
    board: Rect,
    lift: f32,
    skin_offsets: &SkinOffsetValues,
) {
    if snapshot.practice_preview {
        for line in &snapshot.time_lines {
            push_play_aux_line(commands, skin, skin_state, line, skin_offsets, |skin, y| {
                skin.document_time_line_items(y, key_mode, skin_state)
            });
        }
    }
    let show_timing_guides =
        snapshot.practice_preview || (snapshot.bpm_guide && !snapshot.practice_mode);
    if show_timing_guides {
        for line in &snapshot.bpm_lines {
            push_play_aux_line(commands, skin, skin_state, line, skin_offsets, |skin, y| {
                skin.document_bpm_line_items(y, key_mode, skin_state)
            });
            push_bpm_guide_label(commands, board, lift, line, Color::rgb(0.0, 0.75, 0.0));
        }
        for line in &snapshot.stop_lines {
            push_play_aux_line(commands, skin, skin_state, line, skin_offsets, |skin, y| {
                skin.document_stop_line_items(y, key_mode, skin_state)
            });
            push_bpm_guide_label(commands, board, lift, line, Color::rgb(0.75, 0.75, 0.0));
        }
    }
}

fn push_bpm_guide_label(
    commands: &mut Vec<DrawCommand>,
    board: Rect,
    lift: f32,
    line: &crate::snapshot::VisibleBarLine,
    color: Color,
) {
    if line.label.is_empty() {
        return;
    }
    commands.push(DrawCommand::Text {
        origin: Point { x: board.x + 0.004, y: play_object_y(board, lift, line.y) - 0.003 },
        text: line.label.clone(),
        style: TextStyle {
            font_id: None,
            size: 0.018,
            bitmap_size: None,
            color,
            layer: TextLayer::Skin,
            align: TextAlign::Left,
            max_width: board.width,
            // This is BMZ fallback UI rather than skin text, so preserve its
            // existing uniform-shrink presentation.
            overflow: TextOverflow::ShrinkUniform,
            wrapping: false,
            outline: Some(TextOutline { color: Color::rgba(0.0, 0.0, 0.0, 0.9), width: 1.0 }),
            shadow: None,
        },
        caret: None,
        post_scale: Point { x: 1.0, y: 1.0 },
    });
    let start = commands.len() - 1;
    apply_draw_command_alpha(&mut commands[start..], line.alpha);
}

pub(super) fn push_judge_area(
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
    board: Rect,
    lift: f32,
    lane_width: f32,
    active_lanes: &[Lane],
) {
    if !snapshot.judge_area {
        return;
    }
    let colors = [
        Color::rgba(0.0, 0.0, 1.0, 0.125),
        Color::rgba(0.0, 1.0, 0.0, 0.125),
        Color::rgba(1.0, 1.0, 0.0, 0.125),
        Color::rgba(1.0, 0.5, 0.0, 0.125),
        Color::rgba(1.0, 0.0, 0.0, 0.125),
    ];
    for (display_index, &lane) in active_lanes.iter().enumerate() {
        let edges = if matches!(lane, Lane::Scratch | Lane::Scratch2) {
            snapshot.judge_area_scratch_y
        } else {
            snapshot.judge_area_key_y
        };
        let mut inner = judge_line_y(board, lift);
        for (edge, color) in edges.into_iter().zip(colors) {
            let outer = play_object_y(board, lift, edge);
            commands.push(DrawCommand::Rect {
                rect: Rect {
                    x: board.x + display_index as f32 * lane_width,
                    y: outer,
                    width: lane_width,
                    height: (inner - outer).max(0.0),
                },
                color,
            });
            inner = outer;
        }
    }
}

/// beatoraja `SkinNote` が processed sprite 未指定時に生成する cyan 枠。
pub(super) fn push_processed_note_fallback(commands: &mut Vec<DrawCommand>, rect: Rect) {
    let border_x = (rect.width / 16.0).min(rect.width * 0.5);
    let border_y = (rect.height / 4.0).min(rect.height * 0.5);
    let color = Color::rgb(0.0, 1.0, 1.0);
    for border in [
        Rect { width: border_x, ..rect },
        Rect { x: rect.x + rect.width - border_x, width: border_x, ..rect },
        Rect { width: rect.width, height: border_y, ..rect },
        Rect { y: rect.y + rect.height - border_y, width: rect.width, height: border_y, ..rect },
    ] {
        commands.push(DrawCommand::Rect { rect: border, color });
    }
}

pub(super) fn push_play_aux_line(
    commands: &mut Vec<DrawCommand>,
    skin: &SkinContext,
    skin_state: &crate::skin::SkinDrawState,
    line: &crate::snapshot::VisibleBarLine,
    skin_offsets: &SkinOffsetValues,
    render: impl FnOnce(&SkinContext, f32) -> Vec<crate::skin::SkinRenderItem>,
) {
    let start = commands.len();
    let items = skin.apply_play_skin_global_offset(render(skin, line.y), skin_state);
    append_skin_render_items(commands, &items);
    apply_bar_line_alpha_offset(&mut commands[start..], skin_offsets);
    apply_draw_command_alpha(&mut commands[start..], line.alpha);
}

pub(super) fn apply_draw_command_alpha(commands: &mut [DrawCommand], alpha: f32) {
    let alpha = alpha.clamp(0.0, 1.0);
    for command in commands {
        match command {
            DrawCommand::Image { tint, .. } | DrawCommand::RotatedImage { tint, .. } => {
                tint.a *= alpha;
            }
            DrawCommand::Rect { color, .. } => color.a *= alpha,
            DrawCommand::RectBatch { rects, .. } => {
                for rect in Arc::make_mut(rects) {
                    rect.color.a *= alpha;
                }
            }
            DrawCommand::Text { style, caret, .. } => {
                style.color.a *= alpha;
                if let Some(outline) = &mut style.outline {
                    outline.color.a *= alpha;
                }
                if let Some(shadow) = &mut style.shadow {
                    shadow.color.a *= alpha;
                }
                if let Some(caret) = caret {
                    caret.color.a *= alpha;
                }
            }
        }
    }
}

/// beatoraja `SkinObject.prepareColor` 相当。小節線コマンド列に alpha offset を加算する。
pub(super) fn apply_bar_line_alpha_offset(
    commands: &mut [DrawCommand],
    skin_offsets: &SkinOffsetValues,
) {
    let offset = skin_offsets.get(SKIN_OFFSET_BAR_LINE).unwrap_or_default();
    apply_draw_command_alpha_offset(commands, offset.a);
}

pub(super) fn apply_draw_command_alpha_offset(commands: &mut [DrawCommand], alpha_offset: i32) {
    if alpha_offset == 0 {
        return;
    }
    let alpha_delta = alpha_offset as f32 / 255.0;
    for command in commands {
        match command {
            DrawCommand::Image { tint, .. } | DrawCommand::RotatedImage { tint, .. } => {
                tint.a = (tint.a + alpha_delta).clamp(0.0, 1.0);
            }
            DrawCommand::Rect { color, .. } => {
                color.a = (color.a + alpha_delta).clamp(0.0, 1.0);
            }
            DrawCommand::RectBatch { rects, .. } => {
                for rect in Arc::make_mut(rects) {
                    rect.color.a = (rect.color.a + alpha_delta).clamp(0.0, 1.0);
                }
            }
            DrawCommand::Text { .. } => {}
        }
    }
}

pub(super) fn push_bar_line_rect_geometry(
    commands: &mut Vec<DrawCommand>,
    board: Rect,
    lift: f32,
    bar_y: f32,
    skin_offsets: &SkinOffsetValues,
) {
    let y = play_object_y(board, lift, bar_y);
    let offset = skin_offsets.get(SKIN_OFFSET_BAR_LINE).unwrap_or_default();
    let height = (0.004 + offset.h as f32 / 1080.0).max(0.0);
    commands.push(DrawCommand::Rect {
        rect: Rect { x: board.x, y, width: board.width, height },
        color: Color::rgba(0.45, 0.48, 0.5, 1.0),
    });
}

pub(super) fn judge_line_y(board: Rect, lift: f32) -> f32 {
    let lift_offset = lift.clamp(0.0, 1.0) * board.height;
    let raw = board.y + board.height * JUDGE_LINE_Y_RATIO - lift_offset;
    raw.max(board.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bpm_snapshot(
        practice_mode: bool,
        practice_preview: bool,
        bpm_guide: bool,
    ) -> RenderSnapshot {
        RenderSnapshot {
            practice_mode,
            practice_preview,
            bpm_guide,
            bpm_lines: vec![crate::snapshot::VisibleBarLine {
                time: bmz_core::time::TimeUs(1_000_000),
                y: 0.5,
                alpha: 1.0,
                label: "BPM180".to_string(),
            }],
            ..Default::default()
        }
    }

    fn aux_line_commands(snapshot: &RenderSnapshot) -> Vec<DrawCommand> {
        let mut commands = Vec::new();
        push_play_aux_lines(
            &mut commands,
            &SkinContext::default(),
            &crate::skin::SkinDrawState::default(),
            snapshot,
            KeyMode::K7,
            Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            0.0,
            &SkinOffsetValues::default(),
        );
        commands
    }

    #[test]
    fn practice_guides_are_hidden_during_the_round() {
        assert!(aux_line_commands(&bpm_snapshot(true, false, false)).is_empty());
        assert!(aux_line_commands(&bpm_snapshot(true, false, true)).is_empty());
        assert!(!aux_line_commands(&bpm_snapshot(true, true, false)).is_empty());
        assert!(!aux_line_commands(&bpm_snapshot(false, false, true)).is_empty());
    }
}
