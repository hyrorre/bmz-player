use super::*;

impl SkinContext {
    pub fn document_note_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
    ) -> Option<SkinRenderItem> {
        let document = self.document.as_ref()?;
        document.note_image_render_item(lane, key_mode, rect, &self.document_sources)
    }

    pub fn document_processed_note_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
    ) -> Option<SkinRenderItem> {
        let document = self.document.as_ref()?;
        document.note_processed_render_item(lane, key_mode, rect, &self.document_sources)
    }

    pub fn document_ln_start_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
        mode: LongNoteMode,
    ) -> Option<SkinRenderItem> {
        let document = self.document.as_ref()?;
        document.note_ln_start_render_item(lane, key_mode, rect, mode, &self.document_sources)
    }

    pub fn document_ln_end_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
        mode: LongNoteMode,
    ) -> Option<SkinRenderItem> {
        let document = self.document.as_ref()?;
        document.note_ln_end_render_item(lane, key_mode, rect, mode, &self.document_sources)
    }

    /// ロングノート胴体（`note.lnbody` 系 / `note.hcnbody` 系）を指定矩形に伸縮描画する。
    pub fn document_long_body_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
        mode: LongNoteMode,
        state: LongBodyState,
        draw_state: &SkinDrawState,
    ) -> Option<SkinRenderItem> {
        let document = self.document.as_ref()?;
        document.note_long_body_render_item(
            lane,
            key_mode,
            rect,
            mode,
            state,
            draw_state,
            &self.document_sources,
        )
    }

    /// Mine ノート（`note.mine`）を指定矩形に描画する。スキン側に定義が無ければ
    /// `None` を返すため、呼び出し側はデフォルトテクスチャ等のフォールバックへ
    /// 落ちる。
    pub fn document_mine_item(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        rect: Rect,
    ) -> Option<SkinRenderItem> {
        let document = self.document.as_ref()?;
        document.note_mine_render_item(lane, key_mode, rect, &self.document_sources)
    }

    pub fn document_notes_offset_alpha(&self, state: &SkinDrawState) -> i32 {
        self.document.as_ref().map_or(0, |document| document.notes_destination_offset(state).a)
    }

    pub fn document_note_height(&self, lane: Lane, key_mode: KeyMode) -> Option<f32> {
        let document = self.document.as_ref()?;
        document.note_height_for_lane(lane, key_mode)
    }

    pub fn document_note_expansion_scale(&self, state: &SkinDrawState) -> (f32, f32) {
        let Some(note) = self.document.as_ref().and_then(|document| document.note.as_ref()) else {
            return (1.0, 1.0);
        };
        let elapsed = state.quarter_note_elapsed_ms.unwrap_or(i32::MAX).max(0) as f32;
        let pulse = if elapsed < 9.0 {
            elapsed / 9.0
        } else if elapsed <= 159.0 {
            (159.0 - elapsed) / 150.0
        } else {
            0.0
        };
        let width = note.expansionrate.first().copied().unwrap_or(100) as f32 / 100.0;
        let height = note.expansionrate.get(1).copied().unwrap_or(100) as f32 / 100.0;
        (1.0 + (width - 1.0) * pulse, 1.0 + (height - 1.0) * pulse)
    }

    pub fn document_bar_line_items(
        &self,
        note_y: f32,
        key_mode: KeyMode,
        state: &SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        let Some(document) = self.document.as_ref() else {
            return Vec::new();
        };
        let state = self.state_with_lua_runtime(state, &SkinTextState::default());
        document.note_group_render_items(note_y, key_mode, &state, &self.document_sources)
    }

    pub fn document_bpm_line_items(
        &self,
        note_y: f32,
        key_mode: KeyMode,
        state: &SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        let Some(document) = self.document.as_ref() else {
            return Vec::new();
        };
        let Some(note) = document.note.as_ref() else {
            return Vec::new();
        };
        let state = self.state_with_lua_runtime(state, &SkinTextState::default());
        document.note_line_render_items(&note.bpm, note_y, key_mode, &state, &self.document_sources)
    }

    pub fn document_stop_line_items(
        &self,
        note_y: f32,
        key_mode: KeyMode,
        state: &SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        let Some(document) = self.document.as_ref() else {
            return Vec::new();
        };
        let Some(note) = document.note.as_ref() else {
            return Vec::new();
        };
        let state = self.state_with_lua_runtime(state, &SkinTextState::default());
        document.note_line_render_items(
            &note.stop,
            note_y,
            key_mode,
            &state,
            &self.document_sources,
        )
    }

    pub fn document_time_line_items(
        &self,
        note_y: f32,
        key_mode: KeyMode,
        state: &SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        let Some(document) = self.document.as_ref() else {
            return Vec::new();
        };
        let Some(note) = document.note.as_ref() else {
            return Vec::new();
        };
        let state = self.state_with_lua_runtime(state, &SkinTextState::default());
        document.note_line_render_items(
            &note.time,
            note_y,
            key_mode,
            &state,
            &self.document_sources,
        )
    }

    pub fn document_gauge_items(&self, gauge: f32, elapsed_ms: i32) -> Option<Vec<SkinRenderItem>> {
        let document = self.document.as_ref()?;
        document.gauge_render_items(gauge, elapsed_ms, &self.document_sources)
    }

    pub fn timer_animation_duration_ms(&self, timer: i32) -> i32 {
        self.document.as_ref().map_or(0, |document| {
            let enabled_options = document.enabled_options();
            document
                .all_destinations(&enabled_options)
                .into_iter()
                .filter(|destination| destination.timer == Some(timer))
                .filter_map(|destination| {
                    flatten_dst_entries(&destination.dst, &enabled_options)
                        .into_iter()
                        .map(|frame| frame.time.unwrap_or(0))
                        .max()
                })
                .max()
                .unwrap_or(0)
                .max(0)
        })
    }

    pub fn has_timer_destination(&self, timer: i32) -> bool {
        self.document.as_ref().is_some_and(|document| {
            let enabled_options = document.enabled_options();
            document
                .all_destinations(&enabled_options)
                .into_iter()
                .any(|destination| destination.timer == Some(timer))
        })
    }

    pub fn document_judge_items(
        &self,
        judge: &str,
        combo: u32,
        elapsed_ms: i32,
        skin_offsets: &SkinOffsetValues,
        region: usize,
    ) -> Option<Vec<SkinRenderItem>> {
        let document = self.document.as_ref()?;
        let judge_image_index = judge_image_index(judge)?;
        let judge_def = document
            .judge
            .iter()
            .find(|j| j.index == region as i32)
            .or_else(|| document.judge.first())?;
        let region = judge_def.index.clamp(0, MAX_JUDGE_REGIONS as i32 - 1) as usize;
        let mut state = SkinDrawState { skin_offsets: *skin_offsets, ..SkinDrawState::default() };
        state.judge_ms[region] = Some(elapsed_ms);
        state.judge_index[region] = Some(judge_image_index);
        state.judge_combo[region] = combo;
        document.judge_render_items_for_def(
            judge_def,
            judge_image_index,
            combo,
            elapsed_ms,
            &self.document_sources,
            &state,
        )
    }

    pub fn apply_play_skin_global_offset(
        &self,
        items: Vec<SkinRenderItem>,
        state: &SkinDrawState,
    ) -> Vec<SkinRenderItem> {
        if self.document.is_none() {
            return items;
        }
        items.into_iter().map(|item| apply_all_offset_to_render_item(item, state)).collect()
    }

    pub fn apply_play_skin_global_offset_to_item(
        &self,
        item: SkinRenderItem,
        state: &SkinDrawState,
    ) -> SkinRenderItem {
        if self.document.is_none() {
            return item;
        }
        apply_all_offset_to_render_item(item, state)
    }

    /// beatoraja スキンの `note.dst` からレーンのノートエリアを取得し、
    /// `note_y`（0.0=判定ライン, 1.0=最上部）に対応するノート矩形を返す。
    /// `note_height` は正規化座標での高さ。ドキュメントスキンが無い場合は `None`。
    pub fn note_rect_for_progress(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        note_y: f32,
        note_height: f32,
        state: &SkinDrawState,
    ) -> Option<Rect> {
        let document = self.document.as_ref()?;
        let enabled_options = document.enabled_options();
        let area = document.note_lane_area(lane, key_mode, &enabled_options)?;
        let canvas_h = document.h.max(1) as f32;
        let bottom_y = note_progress_to_y(area, note_y, state, canvas_h);
        let rect =
            Rect { x: area.x, y: bottom_y - note_height, width: area.width, height: note_height };
        Some(document.apply_notes_offset_to_rect(rect, state))
    }

    pub fn missed_note_rect_for_fall(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        fall: f32,
        note_height: f32,
        state: &SkinDrawState,
    ) -> Option<Rect> {
        let document = self.document.as_ref()?;
        let note = document.note.as_ref()?;
        if note.dst2 == i32::MIN {
            return None;
        }
        let enabled_options = document.enabled_options();
        let area = document.note_lane_area(lane, key_mode, &enabled_options)?;
        let canvas_h = document.h.max(1) as f32;
        let judge_bottom = note_judge_bottom_y(area, state, canvas_h);
        let target_bottom = (canvas_h - note.dst2 as f32) / canvas_h;
        let bottom_y = judge_bottom + (target_bottom - judge_bottom) * fall.clamp(0.0, 1.0);
        let rect =
            Rect { x: area.x, y: bottom_y - note_height, width: area.width, height: note_height };
        Some(document.apply_notes_offset_to_rect(rect, state))
    }

    /// ロングノート胴体の矩形を計算する。`head_y`/`tail_y` は `VisibleNote::y` と同じ
    /// 正規化座標（0.0=判定ライン, 1.0=最奥）。
    pub fn note_body_rect(
        &self,
        lane: Lane,
        key_mode: KeyMode,
        head_y: f32,
        tail_y: f32,
        state: &SkinDrawState,
    ) -> Option<Rect> {
        let document = self.document.as_ref()?;
        let enabled_options = document.enabled_options();
        let area = document.note_lane_area(lane, key_mode, &enabled_options)?;
        let canvas_h = document.h.max(1) as f32;
        let note_height = document.note_height_for_lane(lane, key_mode)?;
        let head_bottom = note_progress_to_y(area, head_y, state, canvas_h);
        let tail_bottom = note_progress_to_y(area, tail_y, state, canvas_h);
        // beatoraja の drawLongNote に合わせる:
        //   body = [dsty+scale, dsty+dy]  (LibGDX y-up)
        //       = [tail_bottom, head_bottom - note_height]  (y-down)
        // 胴体は tail キャップの下端から head キャップの上端まで、キャップと重ならない。
        let top = head_bottom.min(tail_bottom);
        let bottom = head_bottom.max(tail_bottom) - note_height;
        Some(document.apply_notes_offset_to_long_body_rect(
            Rect { x: area.x, y: top, width: area.width, height: bottom - top },
            state,
        ))
    }
}
