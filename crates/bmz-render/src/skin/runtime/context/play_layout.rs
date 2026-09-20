use super::*;

/// Geometry shared by all notes in one draw plan. Rebuilt each frame so option,
/// LIFT and user-offset changes take effect without invalidating a persistent cache.
pub(crate) struct PreparedNoteLayout<'a> {
    areas: [Option<Rect>; LANE_COUNT],
    heights: [Option<f32>; LANE_COUNT],
    offset: SkinOffsetValue,
    canvas_w: f32,
    canvas_h: f32,
    dst2: i32,
    state: &'a SkinDrawState,
}

impl SkinContext {
    pub(crate) fn prepare_note_layout<'a>(
        &self,
        key_mode: KeyMode,
        state: &'a SkinDrawState,
    ) -> PreparedNoteLayout<'a> {
        let mut layout = PreparedNoteLayout {
            areas: [None; LANE_COUNT],
            heights: [None; LANE_COUNT],
            offset: SkinOffsetValue::default(),
            canvas_w: 1.0,
            canvas_h: 1.0,
            dst2: i32::MIN,
            state,
        };
        if let Some(document) = &self.document {
            let options = document.enabled_options();
            for lane in Lane::ALL {
                layout.areas[lane.index()] = document.note_lane_area(lane, key_mode, &options);
                layout.heights[lane.index()] = document.note_height_for_lane(lane, key_mode);
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
