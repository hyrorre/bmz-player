use super::*;

pub(super) fn push_default_playfield(
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
    skin: &SkinContext,
    skin_state: &crate::skin::SkinDrawState,
    layout: PlayfieldLayout<'_>,
) {
    push_board(commands, layout.board);
    push_judge_area(
        commands,
        snapshot,
        layout.board,
        snapshot.lift,
        layout.lane_width,
        layout.active_lanes,
    );
    for (display_index, &lane) in layout.active_lanes.iter().enumerate() {
        push_lane(commands, snapshot, skin.manifest(), layout, display_index, lane);
    }

    push_receptors(
        skin.manifest(),
        commands,
        layout.board,
        snapshot.lift,
        layout.lane_width,
        layout.active_lanes,
    );
    for bar in &snapshot.bar_lines {
        push_play_bar_line(
            commands,
            skin,
            skin_state,
            snapshot.key_mode,
            layout.board,
            snapshot.lift,
            bar,
            &snapshot.skin_offsets,
        );
    }
    push_play_aux_lines(
        commands,
        skin,
        skin_state,
        snapshot,
        snapshot.key_mode,
        layout.board,
        snapshot.lift,
        &snapshot.skin_offsets,
    );
    push_judge_line(skin.manifest(), commands, layout.board, snapshot.lift);
    push_lane_cover(commands, snapshot, layout.board);
}

fn push_board(commands: &mut Vec<DrawCommand>, board: Rect) {
    commands.push(DrawCommand::Rect { rect: board, color: Color::rgb(0.025, 0.025, 0.028) });
    for x in [board.x - 0.006, board.x + board.width] {
        commands.push(DrawCommand::Rect {
            rect: Rect { x, y: board.y, width: 0.006, height: board.height },
            color: Color::rgb(0.18, 0.2, 0.21),
        });
    }
}

fn push_lane(
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
    manifest: &SkinManifest,
    layout: PlayfieldLayout<'_>,
    display_index: usize,
    lane: Lane,
) {
    let lane_index = lane.index();
    let x = layout.board.x + display_index as f32 * layout.lane_width;
    let color = if display_index.is_multiple_of(2) {
        Color::rgb(0.07, 0.075, 0.08)
    } else {
        Color::rgb(0.045, 0.05, 0.055)
    };
    commands.push(DrawCommand::Rect {
        rect: Rect { x, y: layout.board.y, width: layout.lane_width, height: layout.board.height },
        color,
    });
    if let Some(color) = lane_flash_color(snapshot, lane) {
        commands.push(DrawCommand::Rect {
            rect: Rect {
                x: x + layout.lane_width * 0.04,
                y: layout.board.y + layout.board.height * 0.76,
                width: layout.lane_width * 0.92,
                height: layout.board.height * 0.18,
            },
            color,
        });
    }

    push_long_notes(commands, snapshot, manifest, layout, x, lane);
    for note in &snapshot.visible_notes[lane_index] {
        let start = commands.len();
        let rect = note_rect(layout, snapshot.lift, x, note.y);
        match note.kind {
            NoteVisualKind::LnStart => push_ln_start_skin(manifest, commands, lane, rect),
            NoteVisualKind::LnEnd => push_ln_end_skin(manifest, commands, lane, rect),
            NoteVisualKind::Tap => {
                if snapshot.mark_processed_note && note.processed_judge.is_some() {
                    push_processed_note_fallback(commands, rect);
                } else {
                    push_default_note_skin(manifest, commands, lane, rect);
                }
            }
        }
        apply_draw_command_alpha(&mut commands[start..], note.alpha);
    }
    for mine in &snapshot.visible_mines[lane_index] {
        commands.push(DrawCommand::Image {
            rect: note_rect(layout, snapshot.lift, x, mine.y),
            uv: UvRect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            source_size: None,
            texture: DEFAULT_MINE_NOTE_TEXTURE,
            tint: Color::rgba(1.0, 1.0, 1.0, mine.alpha),
            blend: BlendMode::Normal,
            linear_filter: false,
        });
    }
}

fn push_long_notes(
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
    manifest: &SkinManifest,
    layout: PlayfieldLayout<'_>,
    x: f32,
    lane: Lane,
) {
    for body in snapshot.visible_long_notes.iter().filter(|body| body.lane == lane) {
        let start = commands.len();
        let top = play_object_y(layout.board, snapshot.lift, body.tail_y);
        let bottom = play_object_y(layout.board, snapshot.lift, body.head_y);
        commands.push(DrawCommand::Rect {
            rect: Rect {
                x: x + layout.lane_width * 0.18,
                y: top,
                width: layout.lane_width * 0.64,
                height: (bottom - top).max(0.0),
            },
            color: long_note_body_color(body.mode),
        });

        // キャップは胴体の上に重ね、head は押下中も判定ラインに留める。
        push_ln_start_skin(
            manifest,
            commands,
            lane,
            note_rect(layout, snapshot.lift, x, body.head_y),
        );
        if (!matches!(body.mode, LongNoteMode::Ln | LongNoteMode::Hln) || snapshot.show_ln_tail_cap)
            && body.tail_y < 1.0
        {
            push_ln_end_skin(
                manifest,
                commands,
                lane,
                note_rect(layout, snapshot.lift, x, body.tail_y),
            );
        }
        apply_draw_command_alpha(&mut commands[start..], body.alpha);
    }
}

fn note_rect(layout: PlayfieldLayout<'_>, lift: f32, x: f32, progress: f32) -> Rect {
    Rect {
        x: x + layout.lane_width * 0.08,
        y: note_rect_y(layout.board, lift, progress),
        width: layout.lane_width * 0.84,
        height: NOTE_HEIGHT,
    }
}

fn push_lane_cover(commands: &mut Vec<DrawCommand>, snapshot: &RenderSnapshot, board: Rect) {
    if snapshot.lane_cover <= 0.0 {
        return;
    }
    let cover_bottom = play_object_y(
        board,
        snapshot.lift,
        lane_cover_bottom_progress(snapshot.lane_cover, snapshot.lift),
    );
    let cover_height = (cover_bottom - board.y).max(0.0);
    commands.push(DrawCommand::Rect {
        rect: Rect { x: board.x, y: board.y, width: board.width, height: cover_height },
        color: Color::rgba(0.0, 0.0, 0.0, 1.0),
    });

    let line_height = 0.004_f32.min(cover_height);
    if line_height > 0.0 {
        commands.push(DrawCommand::Rect {
            rect: Rect {
                x: board.x,
                y: cover_bottom - line_height,
                width: board.width,
                height: line_height,
            },
            color: Color::rgb(0.95, 0.65, 0.25),
        });
    }
}
