use super::*;

pub(super) fn frame_index(animation: &SkinQwilightAnimation, elapsed_ms: i32) -> usize {
    let count = animation.sources.len().max(animation.count).max(1);
    let elapsed = u64::try_from(elapsed_ms).unwrap_or(0);
    let frame = if animation.cycle_ms > 0 {
        elapsed * count as u64 / animation.cycle_ms as u64
    } else {
        elapsed * u64::from(animation.fps) / 1000
    } as usize;
    match animation.mode.as_str() {
        "once" | "clear" => frame.min(count - 1),
        "reverse" => (count - 1).saturating_sub(frame),
        "long" => (frame % animation.loop_frame.max(1)).min(count - 1),
        _ => frame % count,
    }
}

pub(super) fn image_frame(
    image: &SkinImageDef,
    a: &SkinQwilightAnimation,
    state: Option<&SkinDrawState>,
    elapsed: i32,
) -> usize {
    if let Some(state) = state {
        if a.mode == "key" {
            return state.qwilight_key_frames.get(&image.id).copied().unwrap_or(0);
        }
        if a.mode == "beat" || (a.mode == "gauge" && a.looping == 1) {
            return a.count.saturating_sub(1).saturating_sub(
                (state.quarter_note_elapsed_ms.unwrap_or(i32::MAX).max(0) as u64 * u64::from(a.fps)
                    / 1000) as usize,
            );
        }
        if a.mode == "laneTiming" {
            let lane = lane_index(a.lane);
            if let Some((age, side)) = state.qwilight_lane_timing[lane] {
                let amount = a
                    .count
                    .saturating_sub(1)
                    .saturating_sub((age.max(0) as u64 * u64::from(a.fps) / 1000) as usize);
                return match side {
                    -1 => a.count.saturating_sub(1) - amount,
                    1 => a.count.saturating_sub(1) + amount,
                    _ => a.count.saturating_sub(1),
                };
            }
            return a.count.saturating_sub(1);
        }
    }
    frame_index(a, elapsed)
}

fn lane_index(timer_index: usize) -> usize {
    match timer_index {
        10 => Lane::Scratch2.index(),
        11..=17 => Lane::Key8.index() + timer_index - 11,
        8..=9 => Lane::Key8.index() + timer_index - 8,
        _ => timer_index.min(LANE_COUNT - 1),
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct KeyAnimationRuntime {
    frames: HashMap<String, (f64, bool, i32)>,
    lane_timing: [Option<(i64, i8)>; LANE_COUNT],
    sequence: Option<u64>,
    now_us: Option<i64>,
}
impl KeyAnimationRuntime {
    pub(super) fn ingest(&mut self, events: &[SkinRuntimeEvent], now_us: i64) {
        if self.now_us.is_some_and(|previous| now_us < previous) {
            *self = Self::default();
        }
        self.now_us = Some(now_us);
        for event in events {
            if self.sequence.is_some_and(|sequence| event.sequence <= sequence) {
                continue;
            }
            self.sequence = Some(event.sequence);
            if let SkinRuntimeEventKind::Judgement(judgement) = &event.kind
                && judgement.affects_score
                && !matches!(judgement.judge, Judge::Poor | Judge::EmptyPoor)
            {
                self.lane_timing[judgement.lane.index()] =
                    Some((judgement.time.0, judgement.delta.0.signum() as i8));
            }
        }
    }
    pub(super) fn advance(&mut self, document: &SkinDocument, state: &mut SkinDrawState, now: i32) {
        state.qwilight_lane_timing = self.lane_timing.map(|value| {
            value.map(|(start, side)| {
                (
                    ((self.now_us.unwrap_or(start) - start) / 1000).clamp(0, i64::from(i32::MAX))
                        as i32,
                    side,
                )
            })
        });
        state.qwilight_key_frames.clear();
        for image in &document.image {
            let Some(a) = image.qwilight_animation.as_ref().filter(|a| a.mode == "key") else {
                continue;
            };
            let lane = lane_index(a.lane);
            let held = state.keyon_ms[lane].is_some();
            let max = a.count.saturating_sub(1) as f64;
            let (value, previous, last) =
                self.frames.entry(image.id.clone()).or_insert((0.0, false, now));
            if now < *last {
                *value = 0.0;
                *previous = false;
            }
            let delta = f64::from(now.saturating_sub(*last).max(0)) * f64::from(a.fps) / 1000.0;
            if held != *previous && (a.looping == 2 || (!held && matches!(a.looping, 1 | 3))) {
                *value = 0.0;
            }
            let active =
                if a.looping == 2 { !held && state.keyoff_ms[lane].is_some() } else { held };
            let target = if active { max } else { 0.0 };
            *value = if a.looping == 3 && held && max > 0.0 {
                if *value + delta > max {
                    1.0 + (*value + delta - 1.0) % max
                } else {
                    *value + delta
                }
            } else if target > *value {
                (*value + delta).min(target)
            } else {
                (*value - delta).max(target)
            };
            state.qwilight_key_frames.insert(image.id.clone(), value.floor() as usize);
            *previous = held;
            *last = now;
        }
    }
}

pub(super) fn transform(
    a: &SkinQwilightAnimation,
    state: &SkinDrawState,
    frame: &mut ResolvedSkinFrame,
    uv: &mut TextureRegion,
) -> bool {
    let (start, length) = match a.mode.as_str() {
        "long" => {
            let elapsed = state.hold_ms[lane_index(a.lane)].unwrap_or(0).max(0) as u64;
            return (elapsed * u64::from(a.fps) / 1000) % (a.loop_frame.max(1) as u64)
                < a.count as u64;
        }
        "auto" => return state.autoplay || state.auto_note_lanes[lane_index(a.lane)],
        "clear" => return state.failed_ms.is_none() && state.gauge >= state.gauge_border,
        "gauge" => {
            // BMZ assist/easy/normal/hard/ex-hard/hazard -> Qwilight Lowest/Lower/Default/Higher/Highest/Failed.
            let variant = match state.gauge_type {
                0 => 4,
                1 => 0,
                2 => 1,
                3 => 2,
                4 => 5,
                _ => 3,
            };
            if a.index != variant {
                return false;
            }
            (0.0, (state.gauge / state.gauge_max.max(1.0)).clamp(0.0, 1.0))
        }
        "progress" => (0.0, state.play_progress.clamp(0.0, 1.0)),
        "ratio" => {
            let counts = &state.judge_counts;
            let values = [
                counts.pgreat,
                counts.great,
                counts.good,
                counts.bad,
                counts.poor,
                counts.empty_poor,
            ];
            let total = values.iter().map(|v| u64::from(*v)).sum::<u64>().max(1) as f32;
            let Some(value) = values.get(a.index) else {
                return false;
            };
            (
                values[..a.index].iter().map(|v| *v as f32).sum::<f32>() / total,
                *value as f32 / total,
            )
        }
        _ => return true,
    };
    let end = (start + length).min(1.0);
    match a.system {
        0 => {
            frame.y += (frame.h as f32 * start).round() as i32;
            frame.h = (frame.h as f32 * length).round() as i32;
            uv.y += uv.height * (1.0 - end);
            uv.height *= length;
        }
        1 => {
            frame.y += (frame.h as f32 * (1.0 - end)).round() as i32;
            frame.h = (frame.h as f32 * length).round() as i32;
            uv.y += uv.height * start;
            uv.height *= length;
        }
        2 => {
            frame.x += (frame.w as f32 * (1.0 - end)).round() as i32;
            frame.w = (frame.w as f32 * length).round() as i32;
            uv.x += uv.width * (1.0 - end);
            uv.width *= length;
        }
        _ => {
            frame.x += (frame.w as f32 * start).round() as i32;
            frame.w = (frame.w as f32 * length).round() as i32;
            uv.x += uv.width * start;
            uv.width *= length;
        }
    }
    frame.w > 0 && frame.h > 0
}

pub(super) fn value_elapsed(value: &SkinValueDef, state: &SkinDrawState) -> i32 {
    let judge = skin_timer_elapsed_ms(value.timer, state).unwrap_or(i32::MAX).max(0);
    let Some(animation) = &value.qwilight_animation else {
        return judge;
    };
    if animation.looping <= 0 {
        return judge;
    }
    state
        .hold_ms
        .iter()
        .flatten()
        .map(|elapsed| elapsed.rem_euclid(animation.looping))
        .min()
        .map_or(judge, |elapsed| elapsed.min(judge))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qwilight_padded_atlas_never_plays_unused_cells() {
        let animation = SkinQwilightAnimation { count: 11, cycle_ms: 1100, ..Default::default() };
        for (elapsed, expected) in [(0, 0), (1000, 10), (1099, 10), (1100, 0), (1200, 1)] {
            assert_eq!(frame_index(&animation, elapsed), expected);
        }
    }

    #[test]
    fn qwilight_gauge_variants_and_all_fill_directions_preserve_the_source_edge() {
        for (gauge_type, variant) in [4, 0, 1, 2, 5, 3].into_iter().enumerate() {
            let state = SkinDrawState {
                gauge_type: gauge_type as i32,
                gauge: 25.0,
                gauge_max: 100.0,
                ..Default::default()
            };
            for (system, bounds, region) in [
                (0, (10, 20, 100, 50), (0.0, 0.75, 1.0, 0.25)),
                (1, (10, 170, 100, 50), (0.0, 0.0, 1.0, 0.25)),
                (2, (85, 20, 25, 200), (0.75, 0.0, 0.25, 1.0)),
                (3, (10, 20, 25, 200), (0.0, 0.0, 0.25, 1.0)),
            ] {
                let animation = SkinQwilightAnimation {
                    mode: "gauge".into(),
                    index: variant,
                    system,
                    ..Default::default()
                };
                let mut frame =
                    ResolvedSkinFrame { x: 10, y: 20, w: 100, h: 200, ..Default::default() };
                let mut uv = TextureRegion::default();
                assert!(transform(&animation, &state, &mut frame, &mut uv));
                assert_eq!((frame.x, frame.y, frame.w, frame.h), bounds);
                assert_eq!((uv.x, uv.y, uv.width, uv.height), region);
                let wrong = SkinQwilightAnimation { index: (variant + 1) % 6, ..animation };
                assert!(!transform(&wrong, &state, &mut frame, &mut uv));
            }
        }
    }

    #[test]
    fn qwilight_long_bomb_retrigger_period_can_leave_a_gap() {
        let animation = SkinQwilightAnimation {
            mode: "long".into(),
            count: 3,
            fps: 10,
            loop_frame: 5,
            lane: 1,
            ..Default::default()
        };
        let mut state = SkinDrawState::default();
        for (elapsed, visible, frame) in
            [(0, true, 0), (200, true, 2), (300, false, 2), (499, false, 2), (500, true, 0)]
        {
            state.hold_ms[Lane::Key1.index()] = Some(elapsed);
            assert_eq!(
                transform(
                    &animation,
                    &state,
                    &mut ResolvedSkinFrame::default(),
                    &mut TextureRegion::default()
                ),
                visible
            );
            assert_eq!(frame_index(&animation, elapsed), frame);
        }
    }

    #[test]
    fn qwilight_key_animation_modes_release_loop_and_reset_on_retry() {
        for (mode, held_frame, released_frame) in [(0, 2, 1), (1, 2, 0), (2, 0, 1), (3, 1, 0)] {
            let document: SkinDocument = serde_json::from_value(serde_json::json!({"image":[{"id":"key","src":"0","qwilightAnimation":{"mode":"key","count":3,"fps":10,"lane":1,"looping":mode}}]})).unwrap();
            let mut state = SkinDrawState::default();
            let mut runtime = KeyAnimationRuntime::default();
            runtime.advance(&document, &mut state, 0);
            runtime.advance(&document, &mut state, 1000);
            assert_eq!(state.qwilight_key_frames["key"], 0, "idle mode {mode}");
            state.keyon_ms[1] = Some(0);
            runtime.advance(&document, &mut state, 1000);
            runtime.advance(&document, &mut state, 1300);
            assert_eq!(state.qwilight_key_frames["key"], held_frame, "held mode {mode}");
            state.keyon_ms[1] = None;
            state.keyoff_ms[1] = Some(100);
            runtime.advance(&document, &mut state, 1400);
            assert_eq!(state.qwilight_key_frames["key"], released_frame, "released mode {mode}");
            state.keyoff_ms[1] = None;
            runtime.advance(&document, &mut state, 0);
            assert_eq!(state.qwilight_key_frames["key"], 0, "retry mode {mode}");
        }
    }

    #[test]
    fn qwilight_lane_timing_uses_raw_pgreat_delta_and_forgets_previous_attempt() {
        use bmz_core::time::TimeUs;
        let image: SkinImageDef =
            serde_json::from_value(serde_json::json!({"id":"meter","src":"0"})).unwrap();
        let animation = SkinQwilightAnimation {
            mode: "laneTiming".into(),
            count: 4,
            fps: 10,
            lane: 1,
            ..Default::default()
        };
        let document: SkinDocument = serde_json::from_str("{}").unwrap();
        let mut runtime = KeyAnimationRuntime::default();
        let mut state = SkinDrawState::default();
        for (sequence, delta, expected) in [(1, -1000, 0), (2, 1000, 6), (3, 0, 3)] {
            let event = SkinRuntimeEvent {
                sequence,
                kind: SkinRuntimeEventKind::Judgement(bmz_gameplay::judge::model::JudgementEvent {
                    note_id: None,
                    lane: Lane::Key1,
                    judge: Judge::PGreat,
                    side: TimingSide::Fast,
                    delta: TimeUs(delta),
                    time: TimeUs(1_000_000),
                    affects_score: true,
                }),
            };
            runtime.ingest(&[event], 1_000_000);
            runtime.advance(&document, &mut state, 1000);
            assert_eq!(image_frame(&image, &animation, Some(&state), 0), expected);
            runtime.ingest(&[], 1_400_000);
            runtime.advance(&document, &mut state, 1400);
            assert_eq!(image_frame(&image, &animation, Some(&state), 0), 3);
            runtime.ingest(&[], 0);
            runtime.advance(&document, &mut state, 0);
            assert!(state.qwilight_lane_timing.iter().all(Option::is_none));
        }
    }
}
