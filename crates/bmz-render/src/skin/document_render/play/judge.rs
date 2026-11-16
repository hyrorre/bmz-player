macro_rules! skin_document_render_play_judge_public_methods {
    () => {
        fn judge_render_items(
            &self,
            judge: &str,
            combo: u32,
            elapsed_ms: i32,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<Vec<SkinRenderItem>> {
            self.judge_render_items_with_offsets(
                judge,
                combo,
                elapsed_ms,
                &SkinOffsetValues::default(),
                sources,
            )
        }

        fn judge_render_items_for_def(
            &self,
            judge: &SkinJudgeDef,
            judge_index: usize,
            combo: u32,
            elapsed_ms: i32,
            sources: &HashMap<String, SkinDocumentTexture>,
            state: &SkinDrawState,
        ) -> Option<Vec<SkinRenderItem>> {
            let gauge_is_max = state.gauge_max > 0.0 && state.gauge >= state.gauge_max;
            let effective_judge_index = if judge_index == 0 && gauge_is_max {
                judge.images.get(6).map_or(0, |_| 6)
            } else {
                judge_index
            };
            let image_destination = judge.images.get(effective_judge_index)?;
            let enabled_options = self.enabled_options();
            if !destination_ops_match(image_destination, &enabled_options, state)
                || !eval_skin_draw_condition(&image_destination.draw, state)
            {
                return None;
            }
            let image_elapsed_ms =
                if image_destination.timer.is_some() || !image_destination.timer_expr.is_empty() {
                    destination_timer_elapsed_ms(image_destination, state)?
                } else {
                    elapsed_ms
                };
            let mut image_frame = resolve_destination_frame_until_end(
                image_destination,
                image_elapsed_ms,
                &enabled_options,
                state,
            )?;
            let offset_state = SkinDrawState {
                skin_offsets: state.skin_offsets,
                offset_lift_px: state.offset_lift_px,
                offset_lanecover_px: state.offset_lanecover_px,
                ..SkinDrawState::default()
            };
            // OFFSET_JUDGE_1P (id 32) は beatoraja では明示注入されず、destination の
            // `offsets` フィールドで宣言されたぶんだけ適用される。ここで重ねて
            // 注入すると、`offsets: [32]` を持つ skin (beatoraja 標準形) で
            // 二重適用になり、判定文字とコンボ数の Y が乖離する原因になる。
            apply_skin_offset_to_frame(image_destination, &mut image_frame, &offset_state, false);
            let number_destination = (if judge_index == 0 && gauge_is_max {
                judge.numbers.get(6).or_else(|| judge.numbers.first())
            } else if judge_index < 3 {
                judge.numbers.get(judge_index)
            } else {
                None
            })
            .filter(|destination| {
                destination_ops_match(destination, &enabled_options, state)
                    && eval_skin_draw_condition(&destination.draw, state)
            });
            let number_elapsed_ms = number_destination.and_then(|destination| {
                if destination.timer.is_some() || !destination.timer_expr.is_empty() {
                    destination_timer_elapsed_ms(destination, state)
                } else {
                    Some(elapsed_ms)
                }
            });
            let mut number_frame = number_destination.zip(number_elapsed_ms).and_then(
                |(destination, number_elapsed_ms)| {
                    resolve_destination_frame_until_end(
                        destination,
                        number_elapsed_ms,
                        &enabled_options,
                        state,
                    )
                },
            );
            let judge_align = number_destination
                .and_then(|destination| {
                    self.value
                        .iter()
                        .find(|value| value.id == destination.id)
                        .map(|value| value.judge_align.unwrap_or(2))
                })
                .unwrap_or(2);

            if let (Some(destination), Some(frame)) = (number_destination, number_frame.as_mut()) {
                // beatoraja の JsonPlaySkinObjectLoader は、relative offset を適用する
                // 前の destination 幅で X 補正を焼き込む。その後 SkinNumber の
                // relative offset は幅だけを変更するため、ここも同じ順序にする。
                if judge_align == 2
                    && let Some(value) = self.value.iter().find(|value| value.id == destination.id)
                {
                    Self::apply_beatoraja_judge_number_dst_x(frame, value.digit);
                }
                apply_skin_offset_to_frame_relative(destination, frame, &offset_state);
            }

            // beatoraja はコンボ数字をシフト前の判定文字 X を基準に配置する。
            let image_frame_for_numbers = image_frame;
            if judge.shift
                && combo > 0
                && let Some(number_destination) = number_destination
                && let Some(number_frame) = number_frame
            {
                image_frame.x -=
                    self.value_number_length(&number_destination.id, combo as i64, number_frame)
                        / 2;
            }
            let image = self.image.iter().find(|image| image.id == image_destination.id)?;
            let source = resolve_document_source(sources, &image.src)?;
            let uv = if image_destination.lr2_timing {
                skin_image_texture_region_for_destination(
                    image,
                    image_destination,
                    source.source_size,
                    state,
                    skin_image_pixel_rect(image),
                )
            } else {
                skin_image_texture_region(image, source.source_size, image_elapsed_ms)
            };
            let (rect, uv) = stretch_skin_image_geometry(
                image_destination.stretch,
                normalize_skin_frame_rect(image_frame, self.w, self.h),
                uv,
                source.source_size,
                self.w,
                self.h,
            );
            let mut items = vec![skin_image_item_for_frame(
                source.texture,
                rect,
                uv,
                image_frame,
                image_destination.center,
                BlendMode::Normal,
                Some(source.source_size),
                image_destination.filter != 0,
            )];
            if combo > 0
                && let Some(number_destination) = number_destination
                && let Some(number_frame) = number_frame
            {
                // beatoraja は SkinNumber に `setRelative(true)` を立てるため、
                // destination の offsets を適用しても x/y は移動せず w/h/r/a だけ
                // 加算される。上で X 補正と offset 適用を済ませた frame を使う。
                let signed_render = if self
                    .value
                    .iter()
                    .find(|value| value.id == number_destination.id)
                    .is_some_and(value_layout_is_signed)
                {
                    SignedNumberRender::Signed(SignedNumberRowOrder::PositiveFirst)
                } else {
                    SignedNumberRender::Unsigned
                };
                let source_elapsed = if number_destination.lr2_timing {
                    self.value
                        .iter()
                        .find(|value| value.id == number_destination.id)
                        .and_then(|value| skin_timer_elapsed_ms(value.timer, state))
                        .unwrap_or(0)
                        .max(0)
                } else {
                    number_elapsed_ms.unwrap_or(elapsed_ms)
                };
                items.extend(self.value_number_render_items(
                    &number_destination.id,
                    combo as i64,
                    image_frame_for_numbers,
                    number_frame,
                    source_elapsed,
                    sources,
                    number_destination.filter != 0,
                    Some(judge_align),
                    signed_render,
                ));
            }
            Some(items)
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_play_judge_public_methods;

macro_rules! skin_document_render_play_judge_methods {
    () => {
        fn judge_render_items_with_offsets(
            &self,
            judge: &str,
            combo: u32,
            elapsed_ms: i32,
            skin_offsets: &SkinOffsetValues,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<Vec<SkinRenderItem>> {
            let judge_image_index = judge_image_index(judge)?;
            let judge_def = self.judge.first()?;
            let region = judge_def.index.clamp(0, MAX_JUDGE_REGIONS as i32 - 1) as usize;
            let mut state =
                SkinDrawState { skin_offsets: *skin_offsets, ..SkinDrawState::default() };
            state.judge_ms[region] = Some(elapsed_ms);
            state.judge_index[region] = Some(judge_image_index);
            state.judge_combo[region] = combo;
            self.judge_render_items_for_def(
                judge_def,
                judge_image_index,
                combo,
                elapsed_ms,
                sources,
                &state,
            )
        }

        /// beatoraja `JsonPlaySkinObjectLoader` が judge number の各 dst に適用する X 補正。
        fn beatoraja_judge_number_dst_x(dst_w: i32, digit: i32) -> i32 {
            dst_w.saturating_mul(digit.max(0)) / 2
        }

        fn apply_beatoraja_judge_number_dst_x(frame: &mut ResolvedSkinFrame, digit: i32) {
            frame.x -= Self::beatoraja_judge_number_dst_x(frame.w, digit);
        }

        fn value_number_length(
            &self,
            value_id: &str,
            number: i64,
            frame: ResolvedSkinFrame,
        ) -> i32 {
            let Some(value) = self.value.iter().find(|value| value.id == value_id) else {
                return 0;
            };
            let max_digits = value.digit.max(0) as usize;
            let padding = number_padding(value);
            let digits = if value_layout_is_signed(value) {
                display_signed_number_digits(
                    number,
                    max_digits,
                    signed_value_padding(value, padding),
                    value.divx.max(1) as u32,
                )
            } else {
                display_number_digits(number, max_digits, padding)
            };
            if digits.is_empty() { 0 } else { digits.len() as i32 * (frame.w + value.space) }
        }

        #[cfg(test)]
        fn judge_image_render_item(
            &self,
            judge: &str,
            elapsed_ms: i32,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<SkinRenderItem> {
            self.judge_render_items(judge, 0, elapsed_ms, sources)?.into_iter().next()
        }

        fn value_number_render_items(
            &self,
            value_id: &str,
            number: i64,
            base_frame: ResolvedSkinFrame,
            frame: ResolvedSkinFrame,
            elapsed_ms: i32,
            sources: &HashMap<String, SkinDocumentTexture>,
            linear_filter: bool,
            align_override: Option<i32>,
            signed_render: SignedNumberRender,
        ) -> Vec<SkinRenderItem> {
            // SkinNumber.prepare in beatoraja reserves both int extrema for
            // hidden values. Keep the sentinel visible to Lua conditions/text,
            // but never turn it into digit images (including signed numbers).
            if number == i64::from(i32::MIN) || number == i64::from(i32::MAX) {
                return Vec::new();
            }
            let Some(value) = self.value.iter().find(|value| value.id == value_id) else {
                return Vec::new();
            };
            let Some(source) = sources.get(&value.src) else {
                return Vec::new();
            };
            if value.lr2_number
                && (lr2_number_frame_cells(value).is_none()
                    || (number < 0 && !value_layout_is_signed(value)))
            {
                return Vec::new();
            }
            let divx = value.divx.max(1);
            let padding = number_padding(value);
            let max_digits = value.digit.max(0) as usize;
            let align = align_override.unwrap_or(value.align);
            let digits = if value.lr2_number {
                let Some(digits) = lr2_number_digits(value, number, align) else {
                    return Vec::new();
                };
                digits
            } else {
                match signed_render {
                    SignedNumberRender::Signed(row_order) => {
                        display_signed_number_digits_with_row_order(
                            number,
                            max_digits,
                            signed_value_padding(value, padding),
                            divx as u32,
                            row_order,
                        )
                    }
                    SignedNumberRender::Unsigned => {
                        display_number_digits(number, max_digits, padding)
                    }
                }
            };
            number_cells::render(
                self,
                value,
                &digits,
                number_cells::Layout {
                    max_digits,
                    align,
                    center: 0,
                    blend: BlendMode::Normal,
                    filter: linear_filter,
                    hidden_cell: Some(u32::from(u8::MAX)),
                },
                base_frame,
                frame,
                elapsed_ms,
                source,
            )
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_play_judge_methods;
