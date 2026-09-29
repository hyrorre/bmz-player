use super::*;

/// Per-scene RANDOM gauge values, shared by every draw pass for a gauge.
#[derive(Debug, Clone, Default)]
pub(super) struct GaugeAnimationRuntime {
    entries: HashMap<String, GaugeAnimationEntry>,
}

#[derive(Debug, Clone)]
struct GaugeAnimationEntry {
    cycle: i32,
    range: i32,
    last_ms: i32,
    next_ms: i64,
    value: i32,
}

impl GaugeAnimationRuntime {
    pub(super) fn advance(&mut self, document: &SkinDocument, state: &mut SkinDrawState) {
        self.advance_with(document, state, random_gauge_index);
    }

    fn advance_with(
        &mut self,
        document: &SkinDocument,
        state: &mut SkinDrawState,
        mut sample: impl FnMut(i32) -> i32,
    ) {
        // Scene time continues through READY -> PLAY; the play timer restarts there.
        let now = state.elapsed_ms;
        state.gauge_random_indices.clear();
        let fallback = document
            .gauge
            .iter()
            .filter(|gauge| !document.gauges.iter().any(|other| other.id == gauge.id));
        for gauge in document.gauges.iter().chain(fallback) {
            // Match skin_gauge_for_destination's precedence for duplicate ids.
            if state.gauge_random_indices.contains_key(&gauge.id) {
                continue;
            }
            if gauge.gauge_type != 0 {
                continue;
            }
            let cycle = gauge.cycle.max(1);
            let range = gauge.range.max(0);
            let entry = self.entries.entry(gauge.id.clone()).or_insert(GaugeAnimationEntry {
                cycle,
                range,
                last_ms: now,
                next_ms: i64::MIN,
                value: 0,
            });
            if now < entry.last_ms || cycle != entry.cycle || range != entry.range {
                entry.next_ms = i64::MIN;
            }
            // Like beatoraja, update once after the deadline, even if frames were skipped.
            if i64::from(now) > entry.next_ms {
                entry.value = if range == 0 { 0 } else { sample(range) };
                entry.next_ms = i64::from(now) + i64::from(cycle);
            }
            entry.last_ms = now;
            entry.cycle = cycle;
            entry.range = range;
            state.gauge_random_indices.insert(gauge.id.clone(), entry.value);
        }
        self.entries.retain(|id, _| state.gauge_random_indices.contains_key(id));
    }
}

fn random_gauge_index(range: i32) -> i32 {
    let bound = range as u64 + 1;
    let limit = (1_u128 << 64) / u128::from(bound) * u128::from(bound);
    loop {
        let mut bytes = [0; 8];
        let value = match getrandom::getrandom(&mut bytes) {
            Ok(()) => u64::from_le_bytes(bytes),
            Err(error) => {
                use std::hash::BuildHasher;
                tracing::warn!(%error, "failed to obtain OS randomness for gauge animation");
                std::collections::hash_map::RandomState::new().hash_one(range)
            }
        };
        if u128::from(value) < limit {
            return (value % bound) as i32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> SkinDocument {
        serde_json::from_str(r#"{"gauge":{"id":"g","nodes":[],"type":0,"range":3,"cycle":33}}"#)
            .unwrap()
    }

    #[test]
    fn random_gauge_holds_value_until_deadline_and_allows_repeats() {
        let document = document();
        let mut runtime = GaugeAnimationRuntime::default();
        let mut state = SkinDrawState::default();
        let mut samples = [3, 3, 1].into_iter();
        for (now, expected) in [(0, 3), (0, 3), (32, 3), (33, 3), (34, 3), (67, 3), (1000, 1)] {
            state.elapsed_ms = now;
            runtime.advance_with(&document, &mut state, |_| samples.next().unwrap());
            assert_eq!(state.gauge_random_indices["g"], expected);
        }
        assert_eq!(samples.next(), None);
    }

    #[test]
    fn random_gauges_are_independent_and_reset_on_time_rewind() {
        let mut document = document();
        let mut other = document.gauge.clone().unwrap();
        other.id = "other".into();
        document.gauges.push(other);
        let mut runtime = GaugeAnimationRuntime::default();
        let mut state = SkinDrawState { elapsed_ms: 100, ..Default::default() };
        let mut samples = [2, 1, 0, 3].into_iter();
        runtime.advance_with(&document, &mut state, |_| samples.next().unwrap());
        assert_eq!(state.gauge_random_indices["other"], 2);
        assert_eq!(state.gauge_random_indices["g"], 1);
        state.play_timer_ms = Some(0);
        state.elapsed_ms = 110;
        runtime
            .advance_with(&document, &mut state, |_| panic!("play timer must not reset animation"));
        state.elapsed_ms = 0;
        runtime.advance_with(&document, &mut state, |_| samples.next().unwrap());
        assert_eq!(state.gauge_random_indices["other"], 0);
        assert_eq!(state.gauge_random_indices["g"], 3);
        assert_eq!(samples.next(), None);
    }

    #[test]
    fn random_gauge_range_changes_invalidate_held_value() {
        let mut document = document();
        let mut runtime = GaugeAnimationRuntime::default();
        let mut state = SkinDrawState::default();
        runtime.advance_with(&document, &mut state, |_| 3);
        document.gauge.as_mut().unwrap().range = 0;
        runtime.advance_with(&document, &mut state, |_| panic!("zero range needs no randomness"));
        assert_eq!(state.gauge_random_indices["g"], 0);
    }

    #[test]
    fn random_gauge_new_scene_samples_again_at_the_same_time() {
        let document = document();
        let mut runtime = GaugeAnimationRuntime::default();
        let mut state = SkinDrawState::default();
        runtime.advance_with(&document, &mut state, |_| 2);
        assert_eq!(state.gauge_random_indices["g"], 2);
        runtime = GaugeAnimationRuntime::default();
        runtime.advance_with(&document, &mut state, |_| 1);
        assert_eq!(state.gauge_random_indices["g"], 1);
    }

    #[test]
    fn non_random_gauge_takes_precedence_over_singular_fallback() {
        let mut document = document();
        let mut replacement = document.gauge.clone().unwrap();
        replacement.gauge_type = 2;
        document.gauges.push(replacement);
        let mut runtime = GaugeAnimationRuntime::default();
        let mut state = SkinDrawState::default();
        runtime.advance_with(&document, &mut state, |_| panic!("non-random gauge"));
        assert!(state.gauge_random_indices.is_empty());
    }
}
