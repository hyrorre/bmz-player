use super::*;
use std::cell::OnceCell;

#[derive(Clone, Copy)]
enum NotePart {
    Tap,
    Processed,
    Start(LongNoteMode),
    End(LongNoteMode),
    Body(LongNoteMode, LongBodyState),
    Mine,
}

/// Geometry and lazily resolved tap sprites shared within one draw plan. Rebuilt
/// each frame so skin/source, option, LIFT and user-offset changes take effect.
pub(crate) struct PreparedNoteLayout<'a> {
    skin: &'a SkinContext,
    key_mode: KeyMode,
    tap_sprites: [OnceCell<Option<NoteSprite>>; LANE_COUNT],
    processed_sprites: [OnceCell<Option<NoteSprite>>; LANE_COUNT],
    areas: [Option<Rect>; LANE_COUNT],
    heights: [Option<f32>; LANE_COUNT],
    frames: [Option<ResolvedSkinFrame>; LANE_COUNT],
    offset: SkinOffsetValue,
    canvas_w: f32,
    canvas_h: f32,
    dst2: i32,
    state: &'a SkinDrawState,
}

impl SkinContext {
    pub(crate) fn prepare_note_layout<'a>(
        &'a self,
        key_mode: KeyMode,
        state: &'a SkinDrawState,
    ) -> PreparedNoteLayout<'a> {
        let mut layout = PreparedNoteLayout {
            skin: self,
            key_mode,
            tap_sprites: std::array::from_fn(|_| OnceCell::new()),
            processed_sprites: std::array::from_fn(|_| OnceCell::new()),
            areas: [None; LANE_COUNT],
            heights: [None; LANE_COUNT],
            frames: [None; LANE_COUNT],
            offset: SkinOffsetValue::default(),
            canvas_w: 1.0,
            canvas_h: 1.0,
            dst2: i32::MIN,
            state,
        };
        if let Some(document) = &self.document {
            let options = document.enabled_options();
            for lane in Lane::ALL {
                layout.areas[lane.index()] =
                    note_lane_area_for_state(document, lane, key_mode, &options, state);
                layout.heights[lane.index()] = document.note_height_for_lane(lane, key_mode);
                layout.frames[lane.index()] =
                    lr2_note_frame(document, lane, key_mode, &options, state);
                if let Some(frame) = layout.frames[lane.index()] {
                    layout.heights[lane.index()] =
                        Some(frame.h.abs() as f32 / document.h.max(1) as f32);
                }
            }
            layout.offset = document.notes_destination_offset(state);
            layout.canvas_w = document.w.max(1) as f32;
            layout.canvas_h = document.h.max(1) as f32;
            layout.dst2 = document.note.as_ref().map_or(i32::MIN, |note| note.dst2);
        }
        layout
    }
}

impl PreparedNoteLayout<'_> {
    pub(crate) fn tap_item(
        &self,
        lane: Lane,
        rect: Rect,
        processed: bool,
    ) -> Option<SkinRenderItem> {
        let slots = if processed { &self.processed_sprites } else { &self.tap_sprites };
        let sprite = slots[lane.index()].get_or_init(|| {
            self.part_sprite(lane, if processed { NotePart::Processed } else { NotePart::Tap })
        });
        sprite.map(|sprite| sprite.render_item(rect))
    }

    pub(crate) fn cap_item(
        &self,
        lane: Lane,
        rect: Rect,
        mode: LongNoteMode,
        end: bool,
    ) -> Option<SkinRenderItem> {
        self.part_sprite(lane, if end { NotePart::End(mode) } else { NotePart::Start(mode) })
            .map(|sprite| sprite.render_item(rect))
    }

    pub(crate) fn body_item(
        &self,
        lane: Lane,
        rect: Rect,
        mode: LongNoteMode,
        state: LongBodyState,
    ) -> Option<SkinRenderItem> {
        self.part_sprite(lane, NotePart::Body(mode, state)).map(|sprite| sprite.render_item(rect))
    }

    pub(crate) fn mine_item(&self, lane: Lane, rect: Rect) -> Option<SkinRenderItem> {
        self.part_sprite(lane, NotePart::Mine).map(|sprite| sprite.render_item(rect))
    }

    fn part_sprite(&self, lane: Lane, part: NotePart) -> Option<NoteSprite> {
        let document = self.skin.document.as_ref()?;
        let note = document.note.as_ref()?;
        let index = beatoraja_note_index(lane, self.key_mode);
        fn select<'a>(
            document: &SkinDocument,
            note: &'a SkinNoteSetDef,
            index: usize,
            part: NotePart,
        ) -> Option<&'a str> {
            let id = match part {
                NotePart::Tap => note.note.get(index),
                NotePart::Processed => note.processed.get(index),
                NotePart::Start(mode) => (mode == LongNoteMode::Hcn)
                    .then(|| note.hcnstart.get(index))
                    .flatten()
                    .or_else(|| note.lnstart.get(index))
                    .or_else(|| note.note.get(index)),
                NotePart::End(mode) => (mode == LongNoteMode::Hcn)
                    .then(|| note.hcnend.get(index))
                    .flatten()
                    .or_else(|| note.lnend.get(index))
                    .or_else(|| note.note.get(index)),
                NotePart::Body(mode, state) => if mode == LongNoteMode::Hcn {
                    document.hcn_body_image_id(note, index, state)
                } else {
                    document.ln_body_image_id(note, index, state.is_processing())
                }
                .or_else(|| note.note.get(index)),
                NotePart::Mine => note.mine.get(index),
            };
            id.filter(|id| !id.is_empty()).map(String::as_str)
        }
        let auto =
            self.state.auto_note_lanes[lane.index()].then_some(note.lr2_auto.as_deref()).flatten();
        let id = auto
            .and_then(|auto| select(document, auto, index, part))
            .or_else(|| select(document, note, index, part))?;
        let image = document.image.iter().find(|image| image.id == id)?;
        // Cache lasts only this frame, so advancing the source timer cannot freeze tap sprites.
        let destination = lr2_note_destination(document, lane, self.key_mode);
        let elapsed = if let Some(destination) = destination {
            let inactive_cap = matches!(part, NotePart::Start(_) | NotePart::End(_))
                && self.state.hold_ms[lane.index()].is_none();
            if inactive_cap { 0 } else { lr2_source_elapsed(image.timer, destination, self.state) }
        } else if matches!(part, NotePart::Body(..)) {
            skin_timer_elapsed_ms(image.timer, self.state).unwrap_or(0)
        } else {
            0
        };
        let mut sprite = document.note_part_sprite(id, elapsed, &self.skin.document_sources)?;
        if destination.is_some() {
            let mut frame = self.frames[lane.index()]?;
            // LR2 play-area drawing forces normal blending and gameplay alpha.
            frame.a = 255;
            if let Some(style) = &mut frame.lr2_style {
                style.blend = 1;
                if matches!(part, NotePart::Tap | NotePart::Processed | NotePart::Mine) {
                    style.filter = 0;
                }
            }
            sprite.frame = frame;
        }
        Some(sprite)
    }

    pub(crate) fn alpha_offset(&self) -> i32 {
        self.offset.a
    }

    pub(crate) fn note_height(&self, lane: Lane) -> Option<f32> {
        self.heights[lane.index()]
    }

    pub(crate) fn note_rect(&self, lane: Lane, progress: f32, height: f32) -> Option<Rect> {
        let area = self.areas[lane.index()]?;
        let bottom = note_progress_to_y(area, progress, self.state, self.canvas_h);
        Some(self.rect_at_bottom(area, bottom, height))
    }

    pub(crate) fn missed_rect(&self, lane: Lane, fall: f32, height: f32) -> Option<Rect> {
        if self.dst2 == i32::MIN {
            return None;
        }
        let area = self.areas[lane.index()]?;
        let judge = note_judge_bottom_y(area, self.state, self.canvas_h);
        let target = (self.canvas_h - self.dst2 as f32) / self.canvas_h;
        Some(self.rect_at_bottom(area, judge + (target - judge) * fall.clamp(0.0, 1.0), height))
    }

    fn rect_at_bottom(&self, area: Rect, bottom: f32, height: f32) -> Rect {
        let offset_h = self.offset.h as f32 / self.canvas_h;
        Rect {
            x: area.x + self.offset.x as f32 / self.canvas_w,
            y: bottom - height - self.offset.y as f32 / self.canvas_h - offset_h,
            width: area.width + self.offset.w as f32 / self.canvas_w,
            height: height + offset_h,
        }
    }

    pub(crate) fn body_rect(&self, lane: Lane, head: f32, tail: f32) -> Option<Rect> {
        let area = self.areas[lane.index()]?;
        let height = self.heights[lane.index()]?;
        let head = note_progress_to_y(area, head, self.state, self.canvas_h);
        let tail = note_progress_to_y(area, tail, self.state, self.canvas_h);
        let top = head.min(tail);
        let bottom = head.max(tail) - height;
        Some(Rect {
            x: area.x + self.offset.x as f32 / self.canvas_w,
            y: top - self.offset.y as f32 / self.canvas_h,
            width: area.width + self.offset.w as f32 / self.canvas_w,
            // Offset height belongs to the cap; shorten the body by the same amount.
            height: bottom - top - self.offset.h as f32 / self.canvas_h,
        })
    }
}
