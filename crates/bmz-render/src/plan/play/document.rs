use super::*;
use crate::skin::{PreparedNoteLayout, SkinImageScale, TextureRegion};
use crate::snapshot::VisibleNote;
use bmz_core::judge::Judge;

pub(super) fn push_document_playfield(
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
    skin: &SkinContext,
    skin_state: &crate::skin::SkinDrawState,
    skin_text: &SkinTextState<'_>,
    layout: PlayfieldLayout<'_>,
) {
    let clip = skin.document_playfield_clip(skin_state, skin_text);
    let first_command = commands.len();
    if let Some(rect) = clip {
        append_document_item(commands, skin, skin_state, SkinRenderItem::PushClip { rect });
    }
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
    let notes = skin.prepare_note_layout(snapshot.key_mode, skin_state);
    push_document_long_notes(commands, snapshot, skin, skin_state, &notes);
    for &lane in layout.active_lanes {
        push_document_lane(commands, snapshot, skin, skin_state, lane, &notes);
    }
    if clip.is_some() {
        if commands.len() == first_command + 1 {
            commands.pop();
        } else {
            append_document_item(commands, skin, skin_state, SkinRenderItem::PopClip);
        }
    }
}

fn push_document_long_notes(
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
    skin: &SkinContext,
    skin_state: &crate::skin::SkinDrawState,
    notes: &PreparedNoteLayout<'_>,
) {
    for body in &snapshot.visible_long_notes {
        let start = commands.len();
        if let Some(rect) = notes.body_rect(body.lane, body.head_y, body.tail_y)
            && let Some(item) = notes.body_item(body.lane, rect, body.mode, body.body_state)
        {
            append_document_item(commands, skin, skin_state, item);
        }

        let note_height = notes.note_height(body.lane).unwrap_or(NOTE_HEIGHT);
        if let Some(rect) = notes.note_rect(body.lane, body.head_y, note_height)
            && let Some(item) = notes.cap_item(body.lane, rect, body.mode, false)
        {
            append_document_item(commands, skin, skin_state, item);
        }
        if (body.mode != LongNoteMode::Ln || snapshot.show_ln_tail_cap)
            && body.tail_y < 1.0
            && let Some(rect) = notes.note_rect(body.lane, body.tail_y, note_height)
            && let Some(item) = notes.cap_item(body.lane, rect, body.mode, true)
        {
            append_document_item(commands, skin, skin_state, item);
        }
        apply_draw_command_alpha_offset(&mut commands[start..], notes.alpha_offset());
        apply_draw_command_alpha(&mut commands[start..], body.alpha);
    }
}

fn push_document_lane(
    commands: &mut Vec<DrawCommand>,
    snapshot: &RenderSnapshot,
    skin: &SkinContext,
    skin_state: &crate::skin::SkinDrawState,
    lane: Lane,
    notes: &PreparedNoteLayout<'_>,
) {
    let lane_index = lane.index();
    let note_height = notes.note_height(lane).unwrap_or(NOTE_HEIGHT);
    for note in &snapshot.visible_notes[lane_index] {
        let start = commands.len();
        if hidden_by_missed_note_target(notes, note) {
            continue;
        }
        let Some(mut rect) = document_note_rect(notes, lane, note, note_height) else {
            continue;
        };
        if snapshot.key_mode == KeyMode::K9 {
            apply_note_expansion(&mut rect, skin.document_note_expansion_scale(skin_state));
        }
        // beatoraja は見逃しノーツを markprocessednote によらず通常画像で描く。
        let processed = snapshot.mark_processed_note
            && note.processed_judge.is_some()
            && note.pms_miss_fall.is_none();
        let item = match note.kind {
            NoteVisualKind::LnStart => notes.cap_item(lane, rect, LongNoteMode::Ln, false),
            NoteVisualKind::LnEnd => notes.cap_item(lane, rect, LongNoteMode::Ln, true),
            NoteVisualKind::Tap => notes.tap_item(lane, rect, processed),
        };
        if let Some(item) = item {
            append_document_item(commands, skin, skin_state, item);
        } else if processed {
            push_processed_note_fallback(commands, rect);
        }
        apply_draw_command_alpha_offset(&mut commands[start..], notes.alpha_offset());
        apply_draw_command_alpha(&mut commands[start..], note.alpha);
    }

    for mine in &snapshot.visible_mines[lane_index] {
        let start = commands.len();
        let Some(rect) = notes.note_rect(lane, mine.y, note_height) else {
            continue;
        };
        if let Some(item) = notes.mine_item(lane, rect) {
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
        apply_draw_command_alpha_offset(&mut commands[start..], notes.alpha_offset());
        apply_draw_command_alpha(&mut commands[start..], mine.alpha);
    }
}

/// beatoraja LaneRenderer は `DST_NOTE2` がある skin で、判定時刻前でも
/// PG/GR/GD のノーツをその場で消す (BAD/POOR は残す)。
fn hidden_by_missed_note_target(notes: &PreparedNoteLayout<'_>, note: &VisibleNote) -> bool {
    note.kind == NoteVisualKind::Tap
        && note.pms_miss_fall.is_none()
        && notes.has_missed_note_target()
        && matches!(note.processed_judge, Some(Judge::PGreat | Judge::Great | Judge::Good))
}

fn document_note_rect(
    notes: &PreparedNoteLayout<'_>,
    lane: Lane,
    note: &VisibleNote,
    note_height: f32,
) -> Option<Rect> {
    match note.pms_miss_fall {
        Some(fall) => notes.missed_rect(lane, fall, note_height),
        None => notes.note_rect(lane, note.y, note_height),
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
