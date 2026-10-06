use super::super::super::*;

pub(in crate::skin::document_render) struct SelectSearchInputAnchor<'a> {
    pub destination: &'a SkinDestinationDef,
    pub text: &'a SkinTextDef,
    pub frame: ResolvedSkinFrame,
}

pub(in crate::skin::document_render) fn select_search_input_anchors<'a>(
    document: &'a SkinDocument,
    snapshot: &SelectSnapshot,
    settings_dest_index: &crate::select_settings_dest::SelectSettingsDestIndex,
    state: &SkinDrawState,
    selected_row: Option<&SelectRowSnapshot>,
    enabled_options: &[i32],
) -> Vec<SelectSearchInputAnchor<'a>> {
    let destinations = document.all_destinations(enabled_options);
    let mut anchors = Vec::new();

    for destination in destinations {
        let Some(text) =
            document.text.iter().find(|text| text.ref_id == 30 && text.id == destination.id)
        else {
            continue;
        };
        if !crate::select_settings_dest::test_select_destination_visible(
            settings_dest_index,
            destination,
            enabled_options,
            state,
            snapshot,
            selected_row,
            eval_skin_draw_condition,
            |ops, enabled_options, state| {
                if ops.len() == destination.op.len() && ops.iter().eq(destination.op.iter()) {
                    destination_ops_match(destination, enabled_options, state)
                } else {
                    destination_property_conditions_match(destination, enabled_options, state)
                        && test_skin_ops(ops, enabled_options, state)
                }
            },
        ) {
            continue;
        }
        // Preserve timer on/off gating, but do not feed its repeating elapsed
        // value into the separately drawn input text.
        if destination_timer_elapsed_ms(destination, state).is_none() {
            continue;
        }
        let Some(mut frame) =
            resolve_destination_terminal_frame(destination, enabled_options, state)
        else {
            continue;
        };
        apply_skin_offset_to_frame(destination, &mut frame, state, false);
        if !destination_mouse_rect_contains(destination, frame, state) {
            continue;
        }
        anchors.push(SelectSearchInputAnchor { destination, text, frame });
    }

    anchors
}
