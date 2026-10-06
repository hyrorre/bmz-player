use super::*;

pub(super) fn current_poor_bga_frame(
    session: &GameSession,
    cache: &PlayRenderSnapshotCache,
    render_now: TimeUs,
    recent_judgements: &[JudgementEvent],
    bga_frames: &BgaFrameCatalog,
) -> Option<DisplayBgaFrame> {
    if session.hide_misslayer_on_good && recent_judgements == session.recent_judgements {
        poor_bga_from_events(
            cache,
            render_now,
            session
                .recent_display_judgements
                .iter()
                .map(|event| (&event.judgement, event.display_time)),
            bga_frames,
            session.poor_bga_duration_us,
            true,
        )
    } else {
        // Keep the raw-history contract for OFF and standalone snapshots.
        poor_bga_from_events(
            cache,
            render_now,
            recent_judgements.iter().map(|event| (event, event.time)),
            bga_frames,
            session.poor_bga_duration_us,
            session.hide_misslayer_on_good,
        )
    }
}

fn poor_bga_from_events<'a>(
    cache: &PlayRenderSnapshotCache,
    render_now: TimeUs,
    events: impl DoubleEndedIterator<Item = (&'a JudgementEvent, TimeUs)>,
    bga_frames: &BgaFrameCatalog,
    duration_us: i64,
    hide_on_good: bool,
) -> Option<DisplayBgaFrame> {
    if duration_us <= 0 {
        return None;
    }

    for (event, display_time) in events.rev() {
        if render_now < display_time {
            continue;
        }
        if hide_on_good && matches!(event.judge, Judge::PGreat | Judge::Great | Judge::Good) {
            return None;
        }
        if matches!(event.judge, Judge::Bad | Judge::Poor)
            && render_now.0 < display_time.0.saturating_add(duration_us)
        {
            return current_bga_frame(cache, event.time, BgaEventKind::Poor, bga_frames);
        }
    }
    None
}

pub(super) fn note_display_duration_ms(
    session: &GameSession,
    now_bpm: f32,
    scroll_multiplier: f32,
) -> i32 {
    let lane_cover = if session.lane_cover_visible {
        crate::config::play::clamp_lane_cover_for_lift(session.lane_cover, session.lift)
    } else {
        0.0
    };
    let now_bpm = effective_bpm_for_playback_rate(
        f64::from(now_bpm),
        session.audio_clock.playback_rate_percent(),
    ) as f32;
    display_duration_ms_for_bpm_hispeed(
        now_bpm,
        session.hispeed,
        lane_cover,
        session.lift,
        scroll_multiplier,
    )
    .round()
    .clamp(0.0, i32::MAX as f32) as i32
}

/// beatoraja Practice は譜面の BPM を再生速度倍してから LaneRenderer を初期化する。
/// BMZ は譜面時刻を速度倍で進めるため、レーン表示の計算時だけ同じ実効 BPM に変換する。
pub(crate) fn effective_bpm_for_playback_rate(bpm: f64, playback_rate_percent: u16) -> f64 {
    let playback_rate_percent =
        bmz_audio::clock::clamp_playback_rate_percent(playback_rate_percent);
    bpm * f64::from(playback_rate_percent) / 100.0
}

pub(crate) fn display_duration_ms_for_bpm_hispeed(
    now_bpm: f32,
    hispeed: f32,
    lane_cover: f32,
    lift: f32,
    scroll_multiplier: f32,
) -> f32 {
    let visible_max = crate::config::play::visible_lane_fraction(lane_cover, lift);
    if scroll_multiplier <= 0.0 {
        return 0.0;
    }
    let now_bpm = positive_bpm_or_default(f64::from(now_bpm)) as f32;
    BEATORAJA_DURATION_BPM_FACTOR_MS
        / now_bpm
        / hispeed.max(crate::config::play::HISPEED_MIN)
        / scroll_multiplier
        * visible_max
}

pub(crate) fn hispeed_for_green_number_values(
    target_green: f32,
    visible_max: f32,
    now_bpm: f64,
    scroll_multiplier: f32,
) -> f32 {
    let now_bpm = positive_bpm_or_default(now_bpm) as f32;
    BEATORAJA_DURATION_BPM_FACTOR_MS * visible_max.clamp(0.0, 1.0) * 0.6
        / (target_green.max(1.0) * now_bpm * scroll_multiplier.max(0.01))
}

fn positive_bpm_or_default(bpm: f64) -> f64 {
    if bpm.is_finite() && bpm > 0.0 { bpm } else { 1.0 }
}

pub(super) fn current_keybound_bga_frame(
    session: &GameSession,
    cache: &PlayRenderSnapshotCache,
    render_now: TimeUs,
    bga_frames: &BgaFrameCatalog,
) -> Option<DisplayBgaFrame> {
    let asset = bmz_chart::bga_keybound::keybound_bga_asset_at_time(
        &session.chart,
        render_now,
        session.lane_keyon_started_at,
    )?;
    let mut frame = bga_frames.get(&asset).copied()?;
    let tint = bga_tint_at_time(cache, BgaEventKind::Layer, render_now);
    frame.tint_r = tint.r;
    frame.tint_g = tint.g;
    frame.tint_b = tint.b;
    frame.tint_a = tint.a;
    Some(frame)
}

pub(super) fn current_bga_frame(
    cache: &PlayRenderSnapshotCache,
    render_now: TimeUs,
    kind: BgaEventKind,
    bga_frames: &BgaFrameCatalog,
) -> Option<DisplayBgaFrame> {
    let events = cache.bga_events.events(kind);
    let end = events.partition_point(|event| event.time <= render_now);
    let event = events[..end].last()?;
    let asset = event.asset?;
    let mut frame = bga_frames.get(&asset).copied()?;
    let tint = bga_tint_at_time(cache, kind, render_now);
    frame.tint_r = tint.r;
    frame.tint_g = tint.g;
    frame.tint_b = tint.b;
    frame.tint_a = tint.a;
    Some(frame)
}

pub(super) fn bga_tint_at_time(
    cache: &PlayRenderSnapshotCache,
    kind: BgaEventKind,
    render_now: TimeUs,
) -> bmz_chart::bga::BgaTint {
    let opacity = bga_opacity_at_time(cache, kind, render_now);
    let (alpha, red, green, blue) = bga_argb_at_time(cache, kind, render_now);
    bmz_chart::bga::BgaTint {
        r: red as f32 / 255.0,
        g: green as f32 / 255.0,
        b: blue as f32 / 255.0,
        a: (opacity as f32 / 255.0) * (alpha as f32 / 255.0),
    }
}

pub(super) fn bga_opacity_at_time(
    cache: &PlayRenderSnapshotCache,
    kind: BgaEventKind,
    render_now: TimeUs,
) -> u8 {
    let events = cache.bga_events.opacity_events(kind);
    let end = events.partition_point(|event| event.time <= render_now);
    events[..end].last().map_or(0xFF, |event| event.opacity)
}

pub(super) fn bga_argb_at_time(
    cache: &PlayRenderSnapshotCache,
    kind: BgaEventKind,
    render_now: TimeUs,
) -> (u8, u8, u8, u8) {
    let events = cache.bga_events.argb_events(kind);
    let end = events.partition_point(|event| event.time <= render_now);
    events[..end]
        .last()
        .map_or((0xFF, 0xFF, 0xFF, 0xFF), |event| (event.alpha, event.red, event.green, event.blue))
}

pub fn display_bga_frame(id: BgaAssetId, width: u32, height: u32) -> DisplayBgaFrame {
    DisplayBgaFrame::opaque(bga_texture_id(id), width.max(1) as f32, height.max(1) as f32)
}

pub fn display_video_bga_frame(id: BgaAssetId, width: u32, height: u32) -> DisplayBgaFrame {
    DisplayBgaFrame::opaque_video(bga_texture_id(id), width.max(1) as f32, height.max(1) as f32)
}

pub fn bga_texture_id(id: BgaAssetId) -> u32 {
    CHART_BGA_TEXTURE_BASE + id.0
}
