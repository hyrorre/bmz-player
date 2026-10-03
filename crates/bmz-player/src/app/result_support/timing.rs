use super::*;

pub(super) fn skin_duration_ms(ms: i32) -> Duration {
    Duration::from_millis(ms.max(0) as u64)
}

pub(super) fn result_input_duration_for_document(document: Option<&SkinDocument>) -> Duration {
    document.map(|document| skin_duration_ms(document.input)).unwrap_or_default()
}

pub(super) fn result_panel_supported(document: &SkinDocument) -> bool {
    document.result_panel_default.is_some()
        && document.destination.iter().flat_map(destination_entry_values).any(|destination| {
            destination.draw.contains("result_panel(")
                || destination.draw.starts_with("bmz:lua_draw_callback:")
        })
}

/// Result IR の行スクロールを現在の skin 表示で受け付けるか。
///
/// WMII / Luxe Flat のように panel gate を持つ skin は IR panel (1) のみ、
/// mz-select のように常時 IR を表示する skin は type 8 slider があれば有効にする。
pub(super) fn result_ir_scroll_supported(document: &SkinDocument, result_panel: i32) -> bool {
    document.slider.iter().any(|slider| slider.slider_type == 8)
        && (!result_panel_supported(document) || result_panel == 1)
}

#[cfg(test)]
pub(super) fn result_scene_duration_for_document(document: Option<&SkinDocument>) -> Duration {
    document
        .map(|document| skin_duration_ms(document.scene))
        .unwrap_or(FALLBACK_RESULT_SCENE_DURATION)
}

pub(super) fn result_auto_exit_duration_for_document(
    document: Option<&SkinDocument>,
    is_course_intermediate: bool,
    course_intermediate_auto_advance: bool,
) -> Option<Duration> {
    if is_course_intermediate {
        if !course_intermediate_auto_advance {
            return None;
        }
        return Some(
            document
                .and_then(|document| (document.scene > 0).then(|| skin_duration_ms(document.scene)))
                .unwrap_or(FALLBACK_RESULT_SCENE_DURATION),
        );
    }

    match document {
        Some(document) if document.scene > 0 => Some(skin_duration_ms(document.scene)),
        Some(_) => None,
        None => Some(FALLBACK_RESULT_SCENE_DURATION),
    }
}

pub(super) fn decide_fadeout_scene_elapsed(
    fadeout_started_elapsed: Duration,
    fadeout_elapsed: Duration,
    scene_duration: Duration,
    fadeout_duration: Duration,
    timing: DecideFadeoutSceneTiming,
) -> Duration {
    let direct_elapsed = fadeout_started_elapsed.saturating_add(fadeout_elapsed);
    let tail_elapsed = match timing {
        DecideFadeoutSceneTiming::DirectOnly => direct_elapsed,
        DecideFadeoutSceneTiming::TailStart(tail_start) if fadeout_duration > Duration::ZERO => {
            let tail_start = tail_start.min(scene_duration);
            let tail_duration = scene_duration.saturating_sub(tail_start);
            if tail_duration > Duration::ZERO {
                let scaled = scale_duration(
                    fadeout_elapsed.min(fadeout_duration),
                    tail_duration,
                    fadeout_duration,
                );
                tail_start.saturating_add(scaled).min(scene_duration)
            } else {
                scene_duration
            }
        }
        DecideFadeoutSceneTiming::TailStart(_) => scene_duration,
        DecideFadeoutSceneTiming::DefaultTail => {
            let tail_start = scene_duration.checked_sub(fadeout_duration).unwrap_or_default();
            tail_start.saturating_add(fadeout_elapsed).min(scene_duration)
        }
    };
    direct_elapsed.max(tail_elapsed)
}

pub(super) fn scale_duration(
    value: Duration,
    numerator: Duration,
    denominator: Duration,
) -> Duration {
    if denominator == Duration::ZERO {
        return Duration::ZERO;
    }
    let micros = value
        .as_micros()
        .saturating_mul(numerator.as_micros())
        .checked_div(denominator.as_micros())
        .unwrap_or(0);
    Duration::from_micros(micros.min(u64::MAX as u128) as u64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DecideFadeoutSceneTiming {
    /// `timer=2` が fadeout を担う skin。scene 時刻を終端へ飛ばすと
    /// timer なしの終了演出まで同時に進み、暗転が即飽和する。
    DirectOnly,
    /// timer=2 が無い skin 向け。従来通り fadeout 中は scene 末尾へ寄せる。
    DefaultTail,
    /// m-select のように scene 末尾の黒フェードを fadeout として使う skin。
    TailStart(Duration),
}

pub(super) fn decide_fadeout_scene_timing(
    document: Option<&SkinDocument>,
) -> DecideFadeoutSceneTiming {
    let Some(document) = document else {
        return DecideFadeoutSceneTiming::DefaultTail;
    };
    if document_has_fadeout_timer_black(document) {
        return DecideFadeoutSceneTiming::DirectOnly;
    }
    decide_scene_fadeout_tail_start(Some(document))
        .map(skin_duration_ms)
        .map_or(DecideFadeoutSceneTiming::DefaultTail, DecideFadeoutSceneTiming::TailStart)
}

pub(super) fn decide_scene_fadeout_tail_start(document: Option<&SkinDocument>) -> Option<i32> {
    let document = document?;
    if document.scene <= 0 || document.w == 0 || document.h == 0 {
        return None;
    }
    if document_has_fadeout_timer_black(document) {
        return None;
    }
    document
        .destination
        .iter()
        .flat_map(destination_entry_values)
        .filter_map(|destination| {
            if destination.id != "-110" || destination.timer.is_some() {
                return None;
            }
            scene_black_fade_tail_start(destination.dst.iter().flat_map(dst_entry_frames), document)
        })
        .max()
}

pub(super) fn document_has_fadeout_timer_black(document: &SkinDocument) -> bool {
    document.destination.iter().flat_map(destination_entry_values).any(|destination| {
        destination.id == "-110"
            && destination.timer == Some(2)
            && black_fade_start(destination.dst.iter().flat_map(dst_entry_frames), document, 0)
                .is_some()
    })
}

pub(super) fn destination_entry_values(
    entry: &DestinationListEntry,
) -> &[bmz_render::skin::SkinDestinationDef] {
    match entry {
        DestinationListEntry::Single(destination) => std::slice::from_ref(destination),
        DestinationListEntry::Conditional { destinations, .. } => destinations.as_slice(),
    }
}

pub(super) fn dst_entry_frames(entry: &SkinDstEntry) -> &[SkinAnimationDef] {
    match entry {
        SkinDstEntry::Frame(frame) => std::slice::from_ref(frame),
        SkinDstEntry::Conditional { frames, .. } => frames.as_slice(),
    }
}

pub(super) fn scene_black_fade_tail_start<'a>(
    frames: impl Iterator<Item = &'a SkinAnimationDef>,
    document: &SkinDocument,
) -> Option<i32> {
    black_fade_start(frames, document, document.scene)
}

pub(super) fn black_fade_start<'a>(
    frames: impl Iterator<Item = &'a SkinAnimationDef>,
    document: &SkinDocument,
    min_end_time: i32,
) -> Option<i32> {
    let mut resolved = ResolvedTailFrame::default();
    let mut previous: Option<ResolvedTailFrame> = None;
    let mut start = None;
    for frame in frames {
        resolved.apply(frame);
        let Some(previous_frame) = previous else {
            previous = Some(resolved);
            continue;
        };
        if resolved.time >= min_end_time
            && previous_frame.time < resolved.time
            && previous_frame.a < resolved.a
            && previous_frame.is_fullscreen(document)
        {
            start = Some(previous_frame.time);
        }
        previous = Some(resolved);
    }
    start
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ResolvedTailFrame {
    time: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    a: i32,
}

impl Default for ResolvedTailFrame {
    fn default() -> Self {
        Self { time: 0, x: 0, y: 0, w: 0, h: 0, a: 255 }
    }
}

impl ResolvedTailFrame {
    fn apply(&mut self, frame: &SkinAnimationDef) {
        if let Some(time) = frame.time {
            self.time = time;
        }
        if let Some(x) = frame.x {
            self.x = x;
        }
        if let Some(y) = frame.y {
            self.y = y;
        }
        if let Some(w) = frame.w {
            self.w = w;
        }
        if let Some(h) = frame.h {
            self.h = h;
        }
        if let Some(a) = frame.a {
            self.a = a;
        }
    }

    fn is_fullscreen(self, document: &SkinDocument) -> bool {
        let width = document.w as i32;
        let height = document.h as i32;
        self.x <= width / 20
            && self.y <= height / 20
            && self.w >= width * 9 / 10
            && self.h >= height * 9 / 10
    }
}
