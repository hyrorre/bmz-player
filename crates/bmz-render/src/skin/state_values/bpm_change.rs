use super::*;

/// BMZ BPM change preview refs. Times are real-time milliseconds.
pub(super) fn bpm_change_number(ref_id: i32, state: &SkinDrawState) -> Option<i64> {
    let next = state.bpm_change_preview.next?;
    match ref_id {
        SKIN_REF_BMZ_BPM_CHANGE_REMAINING_MS => Some(i64::from(next.remaining_ms.max(0))),
        SKIN_REF_BMZ_BPM_CHANGE_NEXT_BPM => Some(next.to_bpm.round() as i64),
        SKIN_REF_BMZ_BPM_CHANGE_PREVIOUS_BPM => Some(next.from_bpm.round() as i64),
        SKIN_REF_BMZ_BPM_CHANGE_RATIO_PERCENT => (next.from_bpm > 0.0)
            .then(|| (f64::from(next.to_bpm) * 100.0 / f64::from(next.from_bpm)).round() as i64),
        _ => None,
    }
}

pub(super) fn bpm_change_option(op: i32, state: &SkinDrawState) -> bool {
    let next = state.bpm_change_preview.next;
    match op {
        SKIN_OPTION_BMZ_BPM_CHANGE_PREVIEW_ACTIVE => bpm_change_preview_elapsed_ms(state).is_some(),
        SKIN_OPTION_BMZ_BPM_CHANGE_INCREASE => next.is_some_and(|next| next.to_bpm > next.from_bpm),
        SKIN_OPTION_BMZ_BPM_CHANGE_DECREASE => next.is_some_and(|next| next.to_bpm < next.from_bpm),
        SKIN_OPTION_BMZ_BPM_CHANGE_AVAILABLE => next.is_some(),
        _ => false,
    }
}

/// Timer 19068: zero at `change - leadMs`, active until the change is reached.
pub(super) fn bpm_change_preview_elapsed_ms(state: &SkinDrawState) -> Option<i32> {
    let lead_ms = state.bpm_change_preview_lead_ms?;
    let remaining_ms = state.bpm_change_preview.next?.remaining_ms.max(0);
    (remaining_ms <= lead_ms).then_some(lead_ms - remaining_ms)
}

/// Timer 19069: elapsed since the latest BPM change.
pub(super) fn bpm_changed_elapsed_ms(state: &SkinDrawState) -> Option<i32> {
    state.bpm_change_preview.since_last_change_ms.map(|elapsed| elapsed.max(0))
}
