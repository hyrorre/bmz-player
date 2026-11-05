use super::*;

/// The EMA belongs to a scene and a visualizer, never to a draw pass or a frame cache.
#[derive(Debug, Clone, Default)]
pub(super) struct HitErrorVisualizerRuntime {
    entries: HashMap<String, EmaEntry>,
}

#[derive(Debug, Clone, Default)]
struct EmaEntry {
    index: Option<usize>,
    last_ms: i32,
    value: i64,
}

impl HitErrorVisualizerRuntime {
    pub(super) fn advance(&mut self, document: &SkinDocument, state: &mut SkinDrawState) {
        state.hit_error_ema.clear();
        if !document.hiterror_visualizer.iter().any(|visualizer| visualizer.ema_mode != 0) {
            self.entries.clear();
            return;
        }
        let bad = timing_judge_areas(state)[3];
        let index = state.hit_error_ring_index % state.hit_error_ring.len();
        let sample = state.hit_error_ring[index];
        for visualizer in &document.hiterror_visualizer {
            if visualizer.ema_mode == 0 || state.hit_error_ema.contains_key(&visualizer.id) {
                continue;
            }
            let entry = self.entries.entry(visualizer.id.clone()).or_default();
            if state.elapsed_ms < entry.last_ms {
                *entry = EmaEntry::default();
            }
            // SkinHitErrorVisualizer updates once when the recent-judgement index
            // changes, using the latest sample even if several hits arrived together.
            if entry.index != Some(index) {
                if sample != bmz_gameplay::hit_error::HIT_ERROR_EMPTY
                    && sample as f32 > bad.late_ms
                    && (sample as f32) < bad.early_ms
                {
                    let alpha = if visualizer.alpha.is_finite() {
                        visualizer.alpha.clamp(0.0, 1.0)
                    } else {
                        0.1
                    };
                    entry.value = entry
                        .value
                        .saturating_add((alpha * sample.saturating_sub(entry.value) as f32) as i64);
                }
                entry.index = Some(index);
            }
            entry.last_ms = state.elapsed_ms;
            state.hit_error_ema.insert(visualizer.id.clone(), entry.value);
        }
        self.entries.retain(|id, _| state.hit_error_ema.contains_key(id));
    }
}
