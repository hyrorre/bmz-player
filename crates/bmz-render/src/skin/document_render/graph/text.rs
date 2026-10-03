macro_rules! skin_document_render_graph_text_methods {
    () => {
        #[cfg(test)]
        fn text_render_item(
            &self,
            text: &SkinTextDef,
            frame: ResolvedSkinFrame,
            state: &SkinTextState<'_>,
        ) -> Option<SkinRenderItem> {
            self.text_render_item_with_draw_state(text, frame, None, state)
        }

        fn text_render_item_with_draw_state(
            &self,
            text: &SkinTextDef,
            frame: ResolvedSkinFrame,
            draw_state: Option<&SkinDrawState>,
            state: &SkinTextState<'_>,
        ) -> Option<SkinRenderItem> {
            let content = skin_state_text_with_draw_state(text, draw_state, state);
            let rect = normalize_skin_frame_rect(frame, self.w, self.h);
            if text.lr2_system_font.is_some() && (frame.w == 0 || frame.h == 0) {
                return None;
            }
            // LR2 limits width at the declared font size, then scales both axes
            // by DST height / FONT size. Alignment uses the scaled width.
            let lr2_scale = text
                .lr2_system_font
                .map_or(1.0, |font| frame.h.abs() as f32 / font.size.max(1) as f32);
            let max_width = rect.width * lr2_scale;
            // beatoraja は dst.x を align 基準点として扱う（align=1=center なら
            // dst.x がテキストの中央, align=2=right なら dst.x がテキストの右端）。
            // bmz の renderer は origin を「テキストボックスの左端」として扱うので、
            // align に応じて origin.x を平行移動してから渡す。
            let origin_x = match text.align {
                1 => rect.x - max_width / 2.0,
                2 => rect.x - max_width,
                _ => rect.x,
            };
            // beatoraja `STRING_SEARCHWORD` (ref=30) は placeholder 状態で
            // messageFontColor=GRAY (半透明) になる。bmz では state から渡される
            // multiplier を skin 由来の alpha に掛け合わせて同様の見た目を再現する。
            let mut alpha = frame.a.clamp(0, 255) as f32 / 255.0;
            if text.ref_id == 30 {
                alpha *= state.search_word_alpha.clamp(0.0, 1.0);
            }
            let mut color = Color::rgba(
                frame.r as f32 / 255.0,
                frame.g as f32 / 255.0,
                frame.b as f32 / 255.0,
                alpha,
            );
            if text.judge_color
                && let Some(draw_state) = draw_state
                && let Some(region) = text.judge_region
                && let Some(judge_color) = skin_judge_region_color(draw_state, region, alpha)
            {
                color = judge_color;
            }
            if text.judge_timing_color
                && let Some(draw_state) = draw_state
                && let Some(region) = text.judge_timing_region
                && let Some(judge_color) = skin_judge_timing_color(draw_state, region, alpha)
            {
                color = judge_color;
            }
            let caret = if text.ref_id == 30 {
                state.search_caret_byte_index.map(|byte_index| TextCaret { byte_index, color })
            } else {
                None
            };
            if content.is_empty() && caret.is_none() {
                return None;
            }
            Some(SkinRenderItem::Text {
                origin: Point { x: origin_x, y: rect.y },
                text: content,
                style: TextStyle {
                    font_id: (!text.font.is_empty()).then(|| text.font.clone()),
                    size: frame.h.abs().max(text.size).max(1) as f32 / self.h.max(1) as f32,
                    bitmap_size: skin_text_bitmap_size(text, &self.font, self.h, frame.h),
                    color,
                    layer: TextLayer::Ui,
                    align: skin_text_align(text.align),
                    max_width,
                    overflow: skin_text_overflow(text.overflow, text.shrink_mode),
                    wrapping: text.wrapping,
                    // The resolved destination opacity also fades decorations, while
                    // preserving their own RGB and alpha. Body/caret already use alpha.
                    outline: text
                        .lr2_system_font
                        .filter(|font| font.font_type & 1 != 0)
                        .map(|_| TextOutline {
                            color: Color::rgb(0.0, 0.0, 0.0),
                            width: lr2_scale / self.h.max(1) as f32,
                        })
                        .or_else(|| skin_text_outline(text, self.h))
                        .map(|mut outline| {
                            outline.color.a *= alpha;
                            outline
                        }),
                    shadow: skin_text_shadow(text, self.w, self.h).map(|mut shadow| {
                        shadow.color.a *= alpha;
                        shadow
                    }),
                },
                caret,
                blend: frame.blend(BlendMode::Normal),
                post_scale: Point { x: 1.0, y: 1.0 },
            })
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_graph_text_methods;
