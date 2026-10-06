macro_rules! skin_document_render_play_gauge_methods {
    () => {
        fn gauge_render_items(
            &self,
            gauge: f32,
            elapsed_ms: i32,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<Vec<SkinRenderItem>> {
            let state = SkinDrawState { elapsed_ms, gauge, ..SkinDrawState::default() };
            let enabled_options = self.enabled_options();
            let destination =
                self.all_destinations(&enabled_options).into_iter().find(|destination| {
                    self.destination_uses_skin_gauge_bar_render(destination)
                        && destination.timer.is_none()
                        && destination_ops_match(destination, &enabled_options, &state)
                        && eval_skin_draw_condition(&destination.draw, &state)
                })?;
            self.resolve_gauge_destination_items(destination, &enabled_options, &state, sources)
        }

        fn destination_uses_skin_gauge_bar_render(&self, destination: &SkinDestinationDef) -> bool {
            self.skin_gauge_for_destination(destination).is_some()
                && destination.draw.trim().is_empty()
                && destination.blend != 2
        }

        fn destination_uses_skin_gauge_overlay_render(
            &self,
            destination: &SkinDestinationDef,
        ) -> bool {
            self.skin_gauge_for_destination(destination).is_some()
                && (!destination.draw.trim().is_empty() || destination.blend == 2)
        }

        fn skin_gauge_for_destination(
            &self,
            destination: &SkinDestinationDef,
        ) -> Option<&SkinGaugeDef> {
            self.gauges
                .iter()
                .find(|gauge| gauge.id == destination.id)
                .or_else(|| self.gauge.as_ref().filter(|gauge| gauge.id == destination.id))
        }

        fn resolve_gauge_destination_items(
            &self,
            destination: &SkinDestinationDef,
            enabled_options: &[i32],
            state: &SkinDrawState,
            sources: &HashMap<String, SkinDocumentTexture>,
        ) -> Option<Vec<SkinRenderItem>> {
            let gauge_def = self.skin_gauge_for_destination(destination)?;
            let elapsed_ms = skin_timer_elapsed_ms(destination.timer, state)?;
            let mut frame =
                resolve_destination_frame(destination, elapsed_ms, enabled_options, state)?;
            apply_skin_offset_to_frame(destination, &mut frame, state, false);
            let reverse_parts = skin_gauge_reverse_parts(frame);
            let rect = normalize_skin_frame_rect(frame, self.w, self.h);
            let parts = gauge_def.parts.max(1);
            let max = state.gauge_max.max(1.0);
            let border = state.gauge_border;
            let notes = skin_gauge_notes_count(state.gauge, parts, max);
            let animation = skin_gauge_animation_index(gauge_def, state);
            let exgauge = skin_gauge_node_base(state.gauge_type);
            let anim_type = gauge_def.gauge_type;
            let base_color = skin_gauge_frame_color(frame);
            let blend = frame.blend(skin_gauge_destination_blend(destination));
            let mut items = Vec::new();
            for part in 1..=parts {
                let part_border = part as f32 * max / parts as f32;
                let node_index = skin_gauge_sprite_node_index(
                    exgauge,
                    part,
                    notes,
                    animation,
                    border,
                    part_border,
                    gauge_def.nodes.len(),
                    anim_type,
                );
                let node_id = gauge_def.nodes.get(node_index)?;
                let part_rect = skin_gauge_part_rect(rect, parts, part, reverse_parts);
                if let Some(item) = self.gauge_image_render_item(
                    node_id,
                    part_rect,
                    elapsed_ms,
                    sources,
                    base_color,
                    blend,
                    frame.linear_filter(destination.filter != 0),
                ) {
                    items.push(item);
                }
                if anim_type == SKIN_GAUGE_ANIM_FLICKERING
                    && notes > 0
                    && part == notes
                    && let Some(tip_index) = skin_gauge_flicker_tip_node_index(
                        exgauge,
                        border,
                        part_border,
                        gauge_def.nodes.len(),
                    )
                    && let Some(tip_id) = gauge_def.nodes.get(tip_index)
                {
                    let flicker_alpha = skin_gauge_flicker_alpha(animation, gauge_def.cycle);
                    let flicker_color = Color::rgba(
                        base_color.r,
                        base_color.g,
                        base_color.b,
                        base_color.a * flicker_alpha,
                    );
                    if let Some(item) = self.gauge_image_render_item(
                        tip_id,
                        part_rect,
                        elapsed_ms,
                        sources,
                        flicker_color,
                        blend,
                        frame.linear_filter(destination.filter != 0),
                    ) {
                        items.push(item);
                    }
                }
            }
            Some(items)
        }
    };
}

pub(in crate::skin::document_render) use skin_document_render_play_gauge_methods;
