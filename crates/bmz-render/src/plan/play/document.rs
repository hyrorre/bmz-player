use super::*;
use crate::skin::{SkinImageScale, TextureRegion};

pub(super) fn push_document_playfield(
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
    skin: &SkinContext,
    skin_state: &crate::skin::SkinDrawState,
    layout: PlayfieldLayout<'_>,
) {
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
    push_judge_area(
        commands,
        snapshot,
        layout.board,
        snapshot.lift,
        layout.lane_width,
        layout.active_lanes,
    );
    let notes_alpha_offset = skin.document_notes_offset_alpha(skin_state);
    push_document_long_notes(commands, snapshot, skin, skin_state, notes_alpha_offset);
    for &lane in layout.active_lanes {
        push_document_lane(commands, snapshot, skin, skin_state, lane, notes_alpha_offset);
    }
}

fn push_document_long_notes(
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
    skin: &SkinContext,
    skin_state: &crate::skin::SkinDrawState,
    notes_alpha_offset: i32,
) {
    for body in &snapshot.visible_long_notes {
        let start = commands.len();
        if let Some(rect) =
            skin.note_body_rect(body.lane, snapshot.key_mode, body.head_y, body.tail_y, skin_state)
            && let Some(item) = skin.document_long_body_item(
                body.lane,
                snapshot.key_mode,
                rect,
                body.mode,
                body.body_state,
                skin_state,
            )
        {
            append_document_item(commands, skin, skin_state, item);
        }

        let note_height =
            skin.document_note_height(body.lane, snapshot.key_mode).unwrap_or(NOTE_HEIGHT);
        if let Some(rect) = skin.note_rect_for_progress(
            body.lane,
            snapshot.key_mode,
            body.head_y,
            note_height,
            skin_state,
        ) && let Some(item) =
            skin.document_ln_start_item(body.lane, snapshot.key_mode, rect, body.mode)
        {
            append_document_item(commands, skin, skin_state, item);
        }
        if (body.mode != LongNoteMode::Ln || snapshot.show_ln_tail_cap)
            && body.tail_y < 1.0
            && let Some(rect) = skin.note_rect_for_progress(
                body.lane,
                snapshot.key_mode,
                body.tail_y,
                note_height,
                skin_state,
            )
            && let Some(item) =
                skin.document_ln_end_item(body.lane, snapshot.key_mode, rect, body.mode)
        {
            append_document_item(commands, skin, skin_state, item);
        }
        apply_draw_command_alpha_offset(&mut commands[start..], notes_alpha_offset);
        apply_draw_command_alpha(&mut commands[start..], body.alpha);
    }
}

fn push_document_lane(
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
    skin: &SkinContext,
    skin_state: &crate::skin::SkinDrawState,
    lane: Lane,
    notes_alpha_offset: i32,
) {
    let lane_index = lane.index();
    let note_height = skin.document_note_height(lane, snapshot.key_mode).unwrap_or(NOTE_HEIGHT);
    for note in &snapshot.visible_notes[lane_index] {
        let start = commands.len();
        let Some(mut rect) =
            document_note_rect(snapshot, skin, skin_state, lane, note.y, note_height)
        else {
            continue;
        };
        if snapshot.key_mode == KeyMode::K9 {
            apply_note_expansion(&mut rect, skin.document_note_expansion_scale(skin_state));
        }
        let item = match note.kind {
            NoteVisualKind::LnStart => {
                skin.document_ln_start_item(lane, snapshot.key_mode, rect, LongNoteMode::Ln)
            }
            NoteVisualKind::LnEnd => {
                skin.document_ln_end_item(lane, snapshot.key_mode, rect, LongNoteMode::Ln)
            }
            NoteVisualKind::Tap => {
                if snapshot.mark_processed_note && note.processed_judge.is_some() {
                    skin.document_processed_note_item(lane, snapshot.key_mode, rect)
                } else {
                    skin.document_note_item(lane, snapshot.key_mode, rect)
                }
            }
        };
        if let Some(item) = item {
            append_document_item(commands, skin, skin_state, item);
        } else if snapshot.mark_processed_note && note.processed_judge.is_some() {
            push_processed_note_fallback(commands, rect);
        }
        apply_draw_command_alpha_offset(&mut commands[start..], notes_alpha_offset);
        apply_draw_command_alpha(&mut commands[start..], note.alpha);
    }

    for mine in &snapshot.visible_mines[lane_index] {
        let start = commands.len();
        let Some(rect) =
            skin.note_rect_for_progress(lane, snapshot.key_mode, mine.y, note_height, skin_state)
        else {
            continue;
        };
        if let Some(item) = skin.document_mine_item(lane, snapshot.key_mode, rect) {
            append_document_item(commands, skin, skin_state, item);
        } else {
            append_document_item(
                commands,
                skin,
                skin_state,
                SkinRenderItem::Image {
                    rect,
                    uv: TextureRegion { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                    source_size: None,
                    texture: SkinTextureId(DEFAULT_MINE_NOTE_TEXTURE.0),
                    tint: Color::rgba(1.0, 1.0, 1.0, 1.0),
                    blend: BlendMode::Normal,
                    scale: SkinImageScale::Stretch,
                    border: None,
                    linear_filter: false,
                },
            );
        }
        apply_draw_command_alpha_offset(&mut commands[start..], notes_alpha_offset);
        apply_draw_command_alpha(&mut commands[start..], mine.alpha);
    }
}

fn document_note_rect(
    snapshot: &RenderSnapshot,
    skin: &SkinContext,
    skin_state: &crate::skin::SkinDrawState,
    lane: Lane,
    progress: f32,
    note_height: f32,
) -> Option<Rect> {
    if progress < 0.0 {
        skin.missed_note_rect_for_fall(lane, snapshot.key_mode, -progress, note_height, skin_state)
    } else {
        skin.note_rect_for_progress(lane, snapshot.key_mode, progress, note_height, skin_state)
    }
}

fn apply_note_expansion(rect: &mut Rect, (scale_x, scale_y): (f32, f32)) {
    let center_x = rect.x + rect.width / 2.0;
    let center_y = rect.y + rect.height / 2.0;
    rect.width *= scale_x;
    rect.height *= scale_y;
    rect.x = center_x - rect.width / 2.0;
    rect.y = center_y - rect.height / 2.0;
}

fn append_document_item(
    commands: &mut Vec<DrawCommand>,
    skin: &SkinContext,
    skin_state: &crate::skin::SkinDrawState,
    item: SkinRenderItem,
) {
    let item = skin.apply_play_skin_global_offset_to_item(item, skin_state);
    append_skin_render_item(commands, &item);
}
