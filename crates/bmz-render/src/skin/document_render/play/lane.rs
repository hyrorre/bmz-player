macro_rules! skin_document_render_play_lane_methods {
    () => {
        fn note_group_render_items(
            &self,
            note_y: f32,
            key_mode: KeyMode,
            state: &SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Vec<SkinRenderItem> {
            let Some(note) = self.note.as_ref() else {
                return Vec::new();
            };
            self.note_line_render_items(&note.group, note_y, key_mode, state, sources)
        }

        fn note_line_render_items(
            &self,
            destinations: &[SkinDestinationDef],
            note_y: f32,
            key_mode: KeyMode,
            state: &SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Vec<SkinRenderItem> {
            let images = self.image_map();
            let enabled_options = self.enabled_options();
            let Some(area) = self.note_lane_area(Lane::Key1, key_mode, &enabled_options) else {
                return Vec::new();
            };
            let canvas_h = self.h.max(1) as f32;
            let bottom_y = note_progress_to_y(area, note_y, state, canvas_h);
            let judge_bottom_px = canvas_h * (1.0 - note_judge_bottom_y(area, state, canvas_h));
            let timeline_bottom_px = canvas_h * (1.0 - bottom_y);
            let mut items = Vec::new();
            for destination in destinations {
                if !test_skin_ops(&destination.op, &enabled_options, state)
                    || !eval_skin_draw_condition(&destination.draw, state)
                {
                    continue;
                }
                let Some(elapsed) = skin_timer_elapsed_ms(destination.timer, state) else {
                    continue;
                };
                let Some(mut frame) =
                    resolve_destination_frame(destination, elapsed, &enabled_options, state)
                else {
                    continue;
                };
                frame.y += (timeline_bottom_px - judge_bottom_px).round() as i32;
                apply_bar_line_skin_offsets_to_frame(destination, &mut frame, state);
                let Some(image) = images.get(destination.id.as_str()) else {
                    continue;
                };
                let Some(source) = resolve_document_source(sources, &image.src) else {
                    continue;
                };
                let pixel_rect = skin_image_pixel_rect(image);
                let (rect, uv) = stretch_skin_image_geometry(
                    destination.stretch,
                    normalize_skin_frame_rect(frame, self.w, self.h),
                    skin_image_texture_region_for_state(
                        image,
                        source.source_size,
                        state,
                        pixel_rect,
                    ),
                    source.source_size,
                    self.w,
                    self.h,
                );
                let item = skin_image_item_for_frame(
                    source.texture,
                    rect,
                    uv,
                    frame,
                    destination.center,
                    skin_blend_mode(destination.blend),
                    Some(source.source_size),
                    destination.filter != 0,
                );
                items.push(item);
            }
            items
        }

        /// `note.dst` の中から有効な条件に一致するエントリを探し、
        /// 指定レーンのノートエリア矩形（正規化座標）を返す。
        /// ノートエリアはレーン列全体を表す。Y軸: 上端=ノートが最も早い時点、下端=判定ライン。
        ///
        /// note.dst の解釈は2通り:
        /// 1. `load_beatoraja_json` 経由で読んだ場合: `expand_json_skin_value` により条件ブロックが
        ///    展開済みで、dst はレーン順の Frame エントリ列になっている。
        ///    → 全 Frame をフラット配列として `lane_idx` 番目を使う。
        /// 2. 直接 JSON パースした場合: Conditional エントリの frames 配列がレーン対応を持つ。
        ///    → 条件を満たす Conditional を探し、その frames[lane_idx] を使う。
        fn note_lane_area(
            &self,
            lane: Lane,
            key_mode: KeyMode,
            enabled_options: &[i32],
        ) -> Option<Rect> {
            let note = self.note.as_ref()?;
            let lane_idx = beatoraja_note_index(lane, key_mode);
            let canvas_w = self.w as f32;
            let canvas_h = self.h as f32;

            // 全エントリを展開してフラット化。Conditional は条件が合うものだけ展開する。
            let mut flat: Vec<SkinAnimationDef> = Vec::new();
            for entry in &note.dst {
                match entry {
                    SkinDstEntry::Frame(f) => flat.push(*f),
                    SkinDstEntry::Conditional { if_ops, frames } => {
                        if test_skin_dst_if(if_ops, enabled_options) {
                            flat.extend_from_slice(frames);
                        }
                    }
                }
            }

            let frame = flat.get(lane_idx)?;
            if let (Some(x), Some(y), Some(w), Some(h)) = (frame.x, frame.y, frame.w, frame.h) {
                Some(normalize_skin_frame_rect(
                    ResolvedSkinFrame { x, y, w, h, ..ResolvedSkinFrame::default() },
                    canvas_w as u32,
                    canvas_h as u32,
                ))
            } else {
                None
            }
        }

        fn primary_note_lane_height_px(&self) -> Option<i32> {
            let enabled_options = self.enabled_options();
            self.note_lane_area(Lane::Scratch, KeyMode::K7, &enabled_options)
                .or_else(|| self.note_lane_area(Lane::Key1, KeyMode::K7, &enabled_options))
                .map(|area| (area.height * self.h.max(1) as f32).round() as i32)
                .filter(|height| *height > 0)
        }

        fn notes_destination_offset(&self, state: &SkinDrawState) -> SkinOffsetValue {
            let enabled_options = self.enabled_options();
            let Some(destination) = self
                .all_destinations(&enabled_options)
                .into_iter()
                .find(|destination| destination.id == "notes")
            else {
                return SkinOffsetValue::default();
            };
            normalized_destination_offset_ids(destination)
                .into_iter()
                .filter_map(|id| effective_skin_offset(id, state))
                .fold(SkinOffsetValue::default(), |mut total, offset| {
                    total.x = total.x.saturating_add(offset.x);
                    total.y = total.y.saturating_add(offset.y);
                    total.w = total.w.saturating_add(offset.w);
                    total.h = total.h.saturating_add(offset.h);
                    total.a = total.a.saturating_add(offset.a);
                    total
                })
        }

        fn apply_notes_offset_to_rect(&self, rect: Rect, state: &SkinDrawState) -> Rect {
            let offset = self.notes_destination_offset(state);
            let canvas_w = self.w.max(1) as f32;
            let canvas_h = self.h.max(1) as f32;
            let offset_w = offset.w as f32 / canvas_w;
            let offset_h = offset.h as f32 / canvas_h;
            Rect {
                // beatoraja `LaneRenderer.drawLane` はノートの左下を固定したまま
                // `dstw = width + offsetW`, `dsth = scale + offsetH` とする。
                x: rect.x + offset.x as f32 / canvas_w,
                y: rect.y - offset.y as f32 / canvas_h - offset_h,
                width: rect.width + offset_w,
                height: rect.height + offset_h,
            }
        }

        fn apply_notes_offset_to_long_body_rect(&self, rect: Rect, state: &SkinDrawState) -> Rect {
            let offset = self.notes_destination_offset(state);
            let canvas_w = self.w.max(1) as f32;
            let canvas_h = self.h.max(1) as f32;
            let offset_w = offset.w as f32 / canvas_w;
            let offset_h = offset.h as f32 / canvas_h;
            Rect {
                // beatoraja `drawLongNote` の BODY 高は `height - scale`。
                // キャップ高 `scale` に offsetH を加えた分だけ BODY を短くし、
                // キャップとの共有境界を維持する。
                x: rect.x + offset.x as f32 / canvas_w,
                y: rect.y - offset.y as f32 / canvas_h,
                width: rect.width + offset_w,
                height: rect.height - offset_h,
            }
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_play_lane_methods;
