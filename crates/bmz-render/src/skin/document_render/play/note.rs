macro_rules! skin_document_render_play_note_methods {
    () => {
        fn note_image_render_item(
            &self,
            lane: Lane,
            key_mode: KeyMode,
            rect: Rect,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<SkinRenderItem> {
            let note = self.note.as_ref()?;
            let image_id = note.note.get(beatoraja_note_index(lane, key_mode))?;
            self.note_part_render_item(image_id, rect, 0, sources)
        }

        fn note_processed_render_item(
            &self,
            lane: Lane,
            key_mode: KeyMode,
            rect: Rect,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<SkinRenderItem> {
            let note = self.note.as_ref()?;
            let index = beatoraja_note_index(lane, key_mode);
            let image_id = note.processed.get(index)?;
            self.note_part_render_item(image_id, rect, 0, sources)
        }

        /// LN START（ヘッドキャップ）画像を描画する。
        /// HCN モードでは `hcnstart`（beatoraja: `longImage[5]`）を優先し、
        /// `lnstart` → `note` の順にフォールバックする。
        fn note_ln_start_render_item(
            &self,
            lane: Lane,
            key_mode: KeyMode,
            rect: Rect,
            mode: LongNoteMode,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<SkinRenderItem> {
            let note = self.note.as_ref()?;
            let index = beatoraja_note_index(lane, key_mode);
            let hcn = (mode == LongNoteMode::Hln)
                .then(|| note.hlnstart.get(index))
                .flatten()
                .filter(|id| !id.is_empty())
                .or_else(|| {
                    matches!(mode, LongNoteMode::Hcn | LongNoteMode::Hln)
                        .then(|| note.hcnstart.get(index))
                        .flatten()
                        .filter(|id| mode != LongNoteMode::Hln || !id.is_empty())
                });
            let image_id =
                hcn.or_else(|| note.lnstart.get(index)).or_else(|| note.note.get(index))?;
            self.note_part_render_item(image_id, rect, 0, sources)
        }

        /// LN END（テールキャップ）画像を描画する。
        /// HCN モードでは `hcnend`（beatoraja: `longImage[4]`）を優先し、
        /// `lnend` → `note` の順にフォールバックする。
        fn note_ln_end_render_item(
            &self,
            lane: Lane,
            key_mode: KeyMode,
            rect: Rect,
            mode: LongNoteMode,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<SkinRenderItem> {
            let note = self.note.as_ref()?;
            let index = beatoraja_note_index(lane, key_mode);
            let hcn = (mode == LongNoteMode::Hln)
                .then(|| note.hlnend.get(index))
                .flatten()
                .filter(|id| !id.is_empty())
                .or_else(|| {
                    matches!(mode, LongNoteMode::Hcn | LongNoteMode::Hln)
                        .then(|| note.hcnend.get(index))
                        .flatten()
                        .filter(|id| mode != LongNoteMode::Hln || !id.is_empty())
                });
            let image_id =
                hcn.or_else(|| note.lnend.get(index)).or_else(|| note.note.get(index))?;
            self.note_part_render_item(image_id, rect, 0, sources)
        }

        /// LN/CN 用の胴体画像 id を選択する。
        /// 新形式 (`lnbodyActive` 定義あり): 押下中=`lnbodyActive`, 非押下=`lnbody`。
        /// 旧形式: 押下中=`lnbody` (longImage\[2\]), 非押下=`lnactive` (longImage\[3\])。
        fn ln_body_image_id<'a>(
            &self,
            note: &'a SkinNoteSetDef,
            index: usize,
            pressing: bool,
        ) -> Option<&'a String> {
            if !note.lnbody_active.is_empty() {
                if pressing {
                    note.lnbody_active.get(index).or_else(|| note.lnbody.get(index))
                } else {
                    note.lnbody.get(index).or_else(|| note.lnbody_active.get(index))
                }
            } else if pressing {
                note.lnbody.get(index).or_else(|| note.lnactive.get(index))
            } else {
                note.lnactive.get(index).or_else(|| note.lnbody.get(index))
            }
        }

        /// HCN 用の胴体画像 id を選択する。beatoraja `JsonPlaySkinObjectLoader` の
        /// longImage 割り当てに準拠:
        /// 新形式 (`hcnbodyActive` 定義あり): \[6\]=`hcnbodyActive` \[7\]=`hcnbody`
        /// \[8\]=`hcnbodyReactive` \[9\]=`hcnbodyMiss`。
        /// 旧形式: \[6\]=`hcnbody` \[7\]=`hcnactive` \[8\]=`hcndamage` \[9\]=`hcnreactive`。
        fn hcn_body_image_id<'a>(
            &self,
            note: &'a SkinNoteSetDef,
            index: usize,
            state: LongBodyState,
        ) -> Option<&'a String> {
            let new_format = !note.hcnbody_active.is_empty();
            let primary = match state {
                LongBodyState::Processing => {
                    if new_format {
                        note.hcnbody_active.get(index)
                    } else {
                        note.hcnbody.get(index)
                    }
                }
                LongBodyState::Inactive => {
                    if new_format {
                        note.hcnbody.get(index)
                    } else {
                        note.hcnactive.get(index)
                    }
                }
                LongBodyState::HcnActive => {
                    if new_format {
                        note.hcnbody_reactive.get(index)
                    } else {
                        note.hcndamage.get(index)
                    }
                }
                LongBodyState::HcnDamage => {
                    if new_format {
                        note.hcnbody_miss.get(index)
                    } else {
                        note.hcnreactive.get(index)
                    }
                }
            };
            // 状態別画像が無い場合は HCN の基本 2 状態 → LN 胴体の順にフォールバック。
            primary
                .or_else(|| {
                    if new_format {
                        if state.is_processing() {
                            note.hcnbody_active.get(index).or_else(|| note.hcnbody.get(index))
                        } else {
                            note.hcnbody.get(index)
                        }
                    } else if state.is_processing() {
                        note.hcnbody.get(index).or_else(|| note.hcnactive.get(index))
                    } else {
                        note.hcnactive.get(index).or_else(|| note.hcnbody.get(index))
                    }
                })
                .or_else(|| self.ln_body_image_id(note, index, state.is_processing()))
        }

        /// ロングノート胴体画像を描画する。`mode` と `state` の組み合わせで
        /// beatoraja `drawLongNote` の longImage 選択を再現する。
        /// 該当画像が無ければ LN 胴体 → `note` の順にフォールバックする。
        fn note_long_body_render_item(
            &self,
            lane: Lane,
            key_mode: KeyMode,
            rect: Rect,
            mode: LongNoteMode,
            state: LongBodyState,
            draw_state: &SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<SkinRenderItem> {
            let note = self.note.as_ref()?;
            let index = beatoraja_note_index(lane, key_mode);
            let image_id = if mode == LongNoteMode::Hln {
                match state {
                    LongBodyState::Processing => note.hlnbody_active.get(index),
                    LongBodyState::Inactive => note.hlnbody.get(index),
                    LongBodyState::HcnActive => note.hlnbody_reactive.get(index),
                    LongBodyState::HcnDamage => note.hlnbody_miss.get(index),
                }
                .filter(|id| !id.is_empty())
                .or_else(|| self.hcn_body_image_id(note, index, state))
                .filter(|id| !id.is_empty())
                .or_else(|| self.ln_body_image_id(note, index, state.is_processing()))
            } else if mode == LongNoteMode::Hcn {
                self.hcn_body_image_id(note, index, state)
            } else {
                self.ln_body_image_id(note, index, state.is_processing())
            }
            .or_else(|| note.note.get(index))?;
            let image = self.image.iter().find(|image| image.id == *image_id)?;
            // LR2 `SRC_LN_BODY` uses the lane HOLD timer for the processing image,
            // while the inactive copy has no timer and must remain on frame zero.
            let elapsed_ms = skin_timer_elapsed_ms(image.timer, draw_state).unwrap_or(0);
            self.note_part_render_item(image_id, rect, elapsed_ms, sources)
        }

        /// Mine ノート画像（`note.mine`）を描画する。スキンが `mine` を定義していない、
        /// または該当レーンの index が空なら `None` を返し、呼び出し側でフォールバックを
        /// 使う想定。
        fn note_mine_render_item(
            &self,
            lane: Lane,
            key_mode: KeyMode,
            rect: Rect,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<SkinRenderItem> {
            let note = self.note.as_ref()?;
            let image_id = note.mine.get(beatoraja_note_index(lane, key_mode))?;
            self.note_part_render_item(image_id, rect, 0, sources)
        }

        fn note_height_for_lane(&self, lane: Lane, key_mode: KeyMode) -> Option<f32> {
            let note = self.note.as_ref()?;
            let index = beatoraja_note_index(lane, key_mode);
            if let Some(size) = note.size.get(index).copied().filter(|size| *size > 0) {
                return Some(size as f32 / self.h.max(1) as f32);
            }
            let image_id = note.note.get(index)?;
            let image = self.image.iter().find(|image| image.id == *image_id)?;
            let divy = image.divy.max(1);
            Some((image.h.max(1) as f32 / divy as f32) / self.h.max(1) as f32)
        }

        fn note_part_render_item(
            &self,
            image_id: &str,
            rect: Rect,
            elapsed_ms: i32,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<SkinRenderItem> {
            let image = self.image.iter().find(|image| image.id == image_id)?;
            let source = resolve_document_source(sources, &image.src)?;
            Some(SkinRenderItem::Image {
                texture: source.texture,
                rect,
                uv: skin_image_texture_region(image, source.source_size, elapsed_ms),
                tint: Color::rgb(1.0, 1.0, 1.0),
                blend: BlendMode::Normal,
                scale: SkinImageScale::Stretch,
                border: None,
                source_size: Some(source.source_size),
                linear_filter: false,
            })
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_play_note_methods;
