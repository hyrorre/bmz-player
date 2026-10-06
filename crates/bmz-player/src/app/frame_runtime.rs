use std::time::{Duration, Instant};

use bmz_render::renderer::{RenderFrameTimings, RenderSurfaceStatus, WgpuPresentMode};

use crate::i18n::{FluentArgs, Localizer};

pub(super) struct FrameRuntime {
    last_present: Option<Instant>,
    present_window: Instant,
    present_intervals: bmz_core::latency::LatencyHistogram,
    pacer: FramePacer,
    skip_next_pace: bool,
    fps: SkinFpsCounter,
    pacing_state: Option<FramePacingState>,
    pending_wake: Option<PendingFrameWake>,
    current_pacing_timings: FramePacingTimings,
    consecutive_deadline_misses: u32,
    cadence: Box<FrameCadence>,
    select_profiler: SceneFrameProfiler,
    decide_profiler: SceneFrameProfiler,
    play_profiler: SceneFrameProfiler,
    result_profiler: SceneFrameProfiler,
}

pub(super) enum FrameSchedule {
    Start,
    WaitUntil(Instant),
}

impl FrameRuntime {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            last_present: None,
            present_window: now,
            present_intervals: Default::default(),
            pacer: FramePacer::default(),
            skip_next_pace: false,
            fps: SkinFpsCounter::new(now),
            pacing_state: None,
            pending_wake: None,
            current_pacing_timings: FramePacingTimings::default(),
            consecutive_deadline_misses: 0,
            cadence: Box::default(),
            select_profiler: SceneFrameProfiler::default(),
            decide_profiler: SceneFrameProfiler::default(),
            play_profiler: SceneFrameProfiler::default(),
            result_profiler: SceneFrameProfiler::default(),
        }
    }

    pub(super) fn request_immediate_frame(&mut self) {
        self.skip_next_pace = true;
    }

    pub(super) fn begin_scheduled_frame(
        &mut self,
        now: Instant,
        pacing_state: FramePacingState,
    ) -> FrameSchedule {
        self.sync_pacing_state(now, pacing_state);
        let fps = pacing_state.effective_frame_limit;
        let skip_wait = self.skip_next_pace;
        if let Some(deadline) = self.pacer.next_deadline(now, fps, skip_wait) {
            return FrameSchedule::WaitUntil(deadline);
        }

        self.skip_next_pace = false;
        self.pacer.record_frame_started(now, fps, skip_wait);
        self.cadence.record(now, fps);
        self.current_pacing_timings = self
            .pending_wake
            .take()
            .map(|wake| wake.finish(now, pacing_state))
            .unwrap_or_else(|| FramePacingTimings::without_wait(pacing_state, now));
        FrameSchedule::Start
    }

    fn sync_pacing_state(&mut self, now: Instant, pacing_state: FramePacingState) {
        if self.pacing_state == Some(pacing_state) {
            return;
        }
        let previous = self.pacing_state.replace(pacing_state);
        self.fps.reset(now);
        self.pending_wake = None;
        self.consecutive_deadline_misses = 0;
        *self.cadence = FrameCadence::default();
        self.pacer = FramePacer::default();
        self.select_profiler = SceneFrameProfiler::default();
        self.decide_profiler = SceneFrameProfiler::default();
        self.play_profiler = SceneFrameProfiler::default();
        self.result_profiler = SceneFrameProfiler::default();
        if let Some(previous) = previous {
            tracing::info!(
                previous_focused = previous.focused,
                focused = pacing_state.focused,
                previous_effective_frame_limit = previous.effective_frame_limit,
                effective_frame_limit = pacing_state.effective_frame_limit,
                previous_present_mode = ?previous.present_mode,
                present_mode = ?pacing_state.present_mode,
                previous_window_mode = ?previous.window_mode,
                window_mode = ?pacing_state.window_mode,
                "frame pacing state changed; FPS sample reset"
            );
        }
    }

    pub(super) fn record_wait_wake(
        &mut self,
        wait_started_at: Instant,
        scheduled_deadline: Instant,
        actual_wake_at: Instant,
        effective_frame_limit: u32,
    ) {
        let frame_budget = frame_budget_or_zero(effective_frame_limit);
        let wake_lateness = actual_wake_at.saturating_duration_since(scheduled_deadline);
        let missed_by_one_budget = !frame_budget.is_zero() && wake_lateness >= frame_budget;
        if missed_by_one_budget {
            self.consecutive_deadline_misses = self.consecutive_deadline_misses.saturating_add(1);
        } else {
            self.consecutive_deadline_misses = 0;
        }
        self.pending_wake = Some(PendingFrameWake {
            wait_started_at,
            scheduled_deadline,
            actual_wake_at,
            consecutive_deadline_misses: self.consecutive_deadline_misses,
        });
    }

    pub(super) fn current_pacing_timings(&self) -> FramePacingTimings {
        self.current_pacing_timings
    }

    pub(super) fn record_surface_status(
        &mut self,
        now: Instant,
        status: Option<RenderSurfaceStatus>,
    ) {
        if status == Some(RenderSurfaceStatus::Rendered) {
            if tracing::enabled!(tracing::Level::DEBUG) {
                if let Some(last) = self.last_present.replace(now) {
                    self.present_intervals
                        .record(now.saturating_duration_since(last).as_micros() as u64);
                }
                if now.duration_since(self.present_window) >= Duration::from_secs(5) {
                    tracing::debug!(present_interval_us = ?self.present_intervals.summary(), "presentation latency");
                    self.present_window = now;
                    self.present_intervals = Default::default();
                }
            }
            self.fps.record_presented_frame(now);
        }
    }

    pub(super) fn next_deadline(&self, now: Instant, fps: u32) -> Option<Instant> {
        self.pacer.next_deadline(now, fps, self.skip_next_pace)
    }

    pub(super) fn current_fps(&self) -> u32 {
        self.fps.current()
    }

    pub(super) fn overlay_text(&self, show_fps: bool, text: Localizer) -> String {
        fps_overlay_text(show_fps, self.current_fps(), text)
    }

    pub(super) fn record_profile(
        &mut self,
        sample: SceneFrameProfileSample,
        app_loop: AppLoopFrameTimings,
    ) {
        let profiler = match sample.kind {
            FrameProfileKind::Select => &mut self.select_profiler,
            FrameProfileKind::Decide => &mut self.decide_profiler,
            FrameProfileKind::Play => &mut self.play_profiler,
            FrameProfileKind::Result => &mut self.result_profiler,
        };
        profiler.record(
            sample.kind,
            sample.video_us,
            sample.video_profile,
            sample.snapshot_us,
            sample.render_us,
            sample.render_timings,
            app_loop,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FramePacingState {
    pub(super) focused: bool,
    pub(super) effective_frame_limit: u32,
    pub(super) present_mode: WgpuPresentMode,
    pub(super) window_mode: FrameWindowMode,
}

pub(super) enum FrameWait {
    SleepUntil(Instant),
    FinishAt(Instant),
}

impl FramePacingState {
    /// winit already uses a high-resolution Windows timer. Wake a little early
    /// to absorb its measured scheduling jitter, then finish only the bounded
    /// tail on the rendering thread. Never spin for FIFO, background or unlimited.
    pub(super) fn wait_plan(self, now: Instant, deadline: Instant, windows: bool) -> FrameWait {
        let guard = if windows && self.focused && self.present_mode == WgpuPresentMode::Immediate {
            (frame_budget_or_zero(self.effective_frame_limit) / 8).min(Duration::from_micros(600))
        } else {
            Duration::ZERO
        };
        if guard.is_zero() {
            return FrameWait::SleepUntil(deadline);
        }
        let wake = deadline - guard;
        if now < wake { FrameWait::SleepUntil(wake) } else { FrameWait::FinishAt(deadline) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FrameWindowMode {
    Windowed,
    BorderlessFullscreen,
    ExclusiveFullscreen,
}

#[derive(Debug, Clone, Copy)]
struct PendingFrameWake {
    wait_started_at: Instant,
    scheduled_deadline: Instant,
    actual_wake_at: Instant,
    consecutive_deadline_misses: u32,
}

impl PendingFrameWake {
    fn finish(self, redraw_started_at: Instant, state: FramePacingState) -> FramePacingTimings {
        FramePacingTimings {
            effective_frame_limit: state.effective_frame_limit,
            frame_budget_us: duration_us_saturating(frame_budget_or_zero(
                state.effective_frame_limit,
            )),
            wait_wake_sampled: true,
            scheduled_wait_us: duration_us_saturating(
                self.scheduled_deadline.saturating_duration_since(self.wait_started_at),
            ),
            wake_lateness_us: duration_us_saturating(
                self.actual_wake_at.saturating_duration_since(self.scheduled_deadline),
            ),
            redraw_after_wake_us: duration_us_saturating(
                redraw_started_at.saturating_duration_since(self.actual_wake_at),
            ),
            consecutive_deadline_misses: self.consecutive_deadline_misses,
            scheduled_deadline: Some(self.scheduled_deadline),
            actual_wake_at: Some(self.actual_wake_at),
            redraw_started_at: Some(redraw_started_at),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct FramePacingTimings {
    pub(super) effective_frame_limit: u32,
    pub(super) frame_budget_us: u64,
    pub(super) wait_wake_sampled: bool,
    pub(super) scheduled_wait_us: u64,
    pub(super) wake_lateness_us: u64,
    pub(super) redraw_after_wake_us: u64,
    pub(super) consecutive_deadline_misses: u32,
    pub(super) scheduled_deadline: Option<Instant>,
    pub(super) actual_wake_at: Option<Instant>,
    pub(super) redraw_started_at: Option<Instant>,
}

impl FramePacingTimings {
    fn without_wait(state: FramePacingState, redraw_started_at: Instant) -> Self {
        Self {
            effective_frame_limit: state.effective_frame_limit,
            frame_budget_us: duration_us_saturating(frame_budget_or_zero(
                state.effective_frame_limit,
            )),
            redraw_started_at: Some(redraw_started_at),
            ..Self::default()
        }
    }
}

/// 正常に present できたフレームだけを実経過時間で正規化し、1 秒ごとに
/// skin の NUMBER_CURRENT_FPS (20) と右上表示へ渡す。
struct SkinFpsCounter {
    window_started_at: Instant,
    frames: u32,
    current: u32,
}

impl SkinFpsCounter {
    fn new(now: Instant) -> Self {
        Self { window_started_at: now, frames: 0, current: 0 }
    }

    fn record_presented_frame(&mut self, now: Instant) {
        self.frames = self.frames.saturating_add(1);
        let elapsed = now.duration_since(self.window_started_at);
        if elapsed >= Duration::from_secs(1) {
            self.current = normalized_fps(self.frames, elapsed);
            self.frames = 0;
            self.window_started_at = now;
        }
    }

    fn reset(&mut self, now: Instant) {
        self.window_started_at = now;
        self.frames = 0;
        self.current = 0;
    }

    fn current(&self) -> u32 {
        self.current
    }
}

fn normalized_fps(presented_frames: u32, elapsed: Duration) -> u32 {
    if elapsed.is_zero() {
        return 0;
    }
    (f64::from(presented_frames) / elapsed.as_secs_f64()).round().clamp(0.0, f64::from(u32::MAX))
        as u32
}

#[derive(Debug, Default)]
struct FramePacer {
    next_frame_at: Option<Instant>,
    fps: Option<u32>,
}

/// CPU frame cadence is distinct from the monitor's displayed frame cadence.
/// TRACE permits offline correlation without synchronous file IO in this loop.
#[derive(Default)]
struct FrameCadence {
    previous: Option<Instant>,
    started: Option<Instant>,
    intervals: bmz_core::latency::LatencyHistogram,
    error: bmz_core::latency::LatencyHistogram,
}

impl FrameCadence {
    fn record(&mut self, now: Instant, fps: u32) {
        if !tracing::enabled!(target: "bmz_player::frame_pacing", tracing::Level::DEBUG) {
            return;
        }
        let started = *self.started.get_or_insert(now);
        if let Some(previous) = self.previous.replace(now) {
            let interval_us = duration_us_saturating(now.duration_since(previous));
            let error_us = interval_us.abs_diff(duration_us_saturating(frame_budget_or_zero(fps)));
            self.intervals.record(interval_us);
            self.error.record(error_us);
            tracing::trace!(target: "bmz_player::frame_pacing", fps, interval_us, error_us, "frame cadence sample");
        }
        if now.duration_since(started) >= Duration::from_secs(5) {
            tracing::debug!(target: "bmz_player::frame_pacing", fps, interval_us = ?self.intervals.summary(), error_us = ?self.error.summary(), "CPU frame cadence");
            self.intervals = Default::default();
            self.error = Default::default();
            self.started = Some(now);
        }
    }
}

impl FramePacer {
    fn delay(&self, now: Instant, fps: u32, skip_wait: bool) -> Duration {
        if skip_wait || fps == 0 || self.fps != Some(fps) {
            return Duration::ZERO;
        }
        self.next_frame_at
            .and_then(|deadline| deadline.checked_duration_since(now))
            .unwrap_or_default()
    }

    fn record_frame_started(&mut self, now: Instant, fps: u32, immediate: bool) {
        if fps == 0 {
            self.next_frame_at = None;
            self.fps = None;
            return;
        }

        let budget = frame_budget(fps);
        let fps_changed = self.fps != Some(fps);
        let next_frame_at = if fps_changed {
            now + budget
        } else if let Some(previous_deadline) = self.next_frame_at {
            if immediate && previous_deadline > now {
                // A skin upload needs a fresh image, not a new periodic clock.
                previous_deadline
            } else {
                let scheduled = previous_deadline + budget;
                // Preserve phase for ordinary wake jitter, but don't follow a
                // late frame with a near-immediate catch-up frame. That creates
                // long/short pairs even when the average FPS looks correct.
                if scheduled.saturating_duration_since(now) >= budget / 2 {
                    scheduled
                } else {
                    now + budget
                }
            }
        } else {
            now + budget
        };
        self.next_frame_at = Some(next_frame_at);
        self.fps = Some(fps);
    }

    fn next_deadline(&self, now: Instant, fps: u32, skip_wait: bool) -> Option<Instant> {
        let delay = self.delay(now, fps, skip_wait);
        if delay.is_zero() { None } else { now.checked_add(delay) }
    }
}

fn frame_budget(fps: u32) -> Duration {
    debug_assert!(fps > 0);
    Duration::from_secs_f64(1.0 / f64::from(fps)).max(Duration::from_nanos(1))
}

fn frame_budget_or_zero(fps: u32) -> Duration {
    if fps == 0 { Duration::ZERO } else { frame_budget(fps) }
}

fn duration_us_saturating(duration: Duration) -> u64 {
    duration.as_micros().min(u128::from(u64::MAX)) as u64
}

fn fps_overlay_text(show_fps: bool, current_fps: u32, text: Localizer) -> String {
    if !show_fps || current_fps == 0 {
        return String::new();
    }
    let mut args = FluentArgs::new();
    args.set("fps", i64::from(current_fps));
    text.format("fps-overlay", &args)
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct SkinVideoFrameProfile {
    pub(super) poll_us: u128,
    pub(super) upload_us: u128,
    pub(super) opened: u32,
    pub(super) active_sources: u32,
    pub(super) visible_sources: u32,
    pub(super) uploaded_frames: u32,
}

#[derive(Debug, Default)]
struct SceneFrameProfiler {
    frames: u32,
    video_us: u128,
    video_poll_us: u128,
    video_upload_us: u128,
    video_opened: u128,
    video_active_sources: u128,
    video_visible_sources: u128,
    video_uploaded_frames: u128,
    snapshot_us: u128,
    render_us: u128,
    plan_us: u128,
    draw_us: u128,
    text_us: u128,
    geometry_us: u128,
    upload_us: u128,
    submit_us: u128,
    surface_us: u128,
    bind_us: u128,
    encode_us: u128,
    queue_us: u128,
    present_us: u128,
    commands: u128,
    steps: u128,
    rect_steps: u128,
    image_steps: u128,
    text_steps: u128,
    rect_instances: u128,
    image_instances: u128,
    text_instances: u128,
    total_redraw_us: u128,
    input_us: u128,
    background_us: u128,
    transition_us: u128,
    egui_us: u128,
    consume_active_play_us: u128,
    post_scene_us: u128,
    wait_wake_samples: u128,
    scheduled_wait_us: u128,
    wake_lateness_us: u128,
    redraw_after_wake_us: u128,
    maximum_consecutive_deadline_misses: u32,
    effective_frame_limit: u32,
    frame_budget_us: u64,
    last_scheduled_deadline: Option<Instant>,
    last_actual_wake_at: Option<Instant>,
    last_redraw_started_at: Option<Instant>,
    total_redraw_samples_us: Vec<u64>,
    render_samples_us: Vec<u64>,
    surface_samples_us: Vec<u64>,
    queue_samples_us: Vec<u64>,
    present_samples_us: Vec<u64>,
    wake_lateness_samples_us: Vec<u64>,
    redraw_after_wake_samples_us: Vec<u64>,
}

const FRAME_PROFILE_SAMPLE_CAPACITY: usize = 120;

#[derive(Debug, Clone, Copy)]
pub(super) struct AppLoopFrameTimings {
    pub(super) total_redraw_us: u64,
    pub(super) input_us: u64,
    pub(super) background_us: u64,
    pub(super) transition_us: u64,
    pub(super) egui_us: u64,
    pub(super) consume_active_play_us: u64,
    pub(super) post_scene_us: u64,
    pub(super) pacing: FramePacingTimings,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SceneFrameProfileSample {
    pub(super) kind: FrameProfileKind,
    pub(super) video_us: u128,
    pub(super) video_profile: SkinVideoFrameProfile,
    pub(super) snapshot_us: u128,
    pub(super) render_us: u128,
    pub(super) render_timings: Option<RenderFrameTimings>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FrameProfileKind {
    Select,
    Decide,
    Play,
    Result,
}

impl SceneFrameProfiler {
    const LOG_EVERY_FRAMES: u32 = 120;

    fn record(
        &mut self,
        profile: FrameProfileKind,
        video_us: u128,
        video_profile: SkinVideoFrameProfile,
        snapshot_us: u128,
        render_us: u128,
        timings: Option<RenderFrameTimings>,
        app_loop: AppLoopFrameTimings,
    ) {
        self.frames += 1;
        self.video_us += video_us;
        self.video_poll_us += video_profile.poll_us;
        self.video_upload_us += video_profile.upload_us;
        self.video_opened += video_profile.opened as u128;
        self.video_active_sources += video_profile.active_sources as u128;
        self.video_visible_sources += video_profile.visible_sources as u128;
        self.video_uploaded_frames += video_profile.uploaded_frames as u128;
        self.snapshot_us += snapshot_us;
        self.render_us += render_us;
        if let Some(timings) = timings {
            self.plan_us += timings.plan_us;
            self.draw_us += timings.draw_us;
            self.text_us += timings.text_us;
            self.geometry_us += timings.geometry_us;
            self.upload_us += timings.upload_us;
            self.submit_us += timings.submit_us;
            self.surface_us += timings.surface_us;
            self.bind_us += timings.bind_us;
            self.encode_us += timings.encode_us;
            self.queue_us += timings.queue_us;
            self.present_us += timings.present_us;
            self.commands += timings.commands as u128;
            self.steps += timings.steps as u128;
            self.rect_steps += timings.rect_steps as u128;
            self.image_steps += timings.image_steps as u128;
            self.text_steps += timings.text_steps as u128;
            self.rect_instances += timings.rect_instances as u128;
            self.image_instances += timings.image_instances as u128;
            self.text_instances += timings.text_instances as u128;
            push_profile_sample(&mut self.surface_samples_us, timings.surface_us);
            push_profile_sample(&mut self.queue_samples_us, timings.queue_us);
            push_profile_sample(&mut self.present_samples_us, timings.present_us);
        }
        self.total_redraw_us += u128::from(app_loop.total_redraw_us);
        self.input_us += u128::from(app_loop.input_us);
        self.background_us += u128::from(app_loop.background_us);
        self.transition_us += u128::from(app_loop.transition_us);
        self.egui_us += u128::from(app_loop.egui_us);
        self.consume_active_play_us += u128::from(app_loop.consume_active_play_us);
        self.post_scene_us += u128::from(app_loop.post_scene_us);
        self.effective_frame_limit = app_loop.pacing.effective_frame_limit;
        self.frame_budget_us = app_loop.pacing.frame_budget_us;
        self.last_redraw_started_at = app_loop.pacing.redraw_started_at;
        if app_loop.pacing.wait_wake_sampled {
            self.wait_wake_samples += 1;
            self.scheduled_wait_us += u128::from(app_loop.pacing.scheduled_wait_us);
            self.wake_lateness_us += u128::from(app_loop.pacing.wake_lateness_us);
            self.redraw_after_wake_us += u128::from(app_loop.pacing.redraw_after_wake_us);
            self.maximum_consecutive_deadline_misses = self
                .maximum_consecutive_deadline_misses
                .max(app_loop.pacing.consecutive_deadline_misses);
            self.last_scheduled_deadline = app_loop.pacing.scheduled_deadline;
            self.last_actual_wake_at = app_loop.pacing.actual_wake_at;
            push_profile_sample(
                &mut self.wake_lateness_samples_us,
                u128::from(app_loop.pacing.wake_lateness_us),
            );
            push_profile_sample(
                &mut self.redraw_after_wake_samples_us,
                u128::from(app_loop.pacing.redraw_after_wake_us),
            );
        }
        if self.total_redraw_samples_us.len() < FRAME_PROFILE_SAMPLE_CAPACITY {
            self.total_redraw_samples_us.push(app_loop.total_redraw_us);
        }
        push_profile_sample(&mut self.render_samples_us, render_us);
        if self.frames >= Self::LOG_EVERY_FRAMES {
            self.log_and_reset(profile);
        }
    }

    fn log_and_reset(&mut self, profile: FrameProfileKind) {
        let frames = self.frames.max(1) as u128;
        let commands = (self.commands / frames) as u64;
        let steps = (self.steps / frames) as u64;
        let rect_steps = (self.rect_steps / frames) as u64;
        let image_steps = (self.image_steps / frames) as u64;
        let text_steps = (self.text_steps / frames) as u64;
        let rect_instances = (self.rect_instances / frames) as u64;
        let image_instances = (self.image_instances / frames) as u64;
        let text_instances = (self.text_instances / frames) as u64;
        let video_ms = fmt_profile_ms(self.video_us, frames);
        let video_poll_ms = fmt_profile_ms(self.video_poll_us, frames);
        let video_upload_ms = fmt_profile_ms(self.video_upload_us, frames);
        let video_opened = self.video_opened as u64;
        let video_active_sources = (self.video_active_sources / frames) as u64;
        let video_visible_sources = (self.video_visible_sources / frames) as u64;
        let video_uploaded_frames = self.video_uploaded_frames as u64;
        let video_upload_frame_ms =
            fmt_profile_ms(self.video_upload_us, self.video_uploaded_frames.max(1));
        let snapshot_ms = fmt_profile_ms(self.snapshot_us, frames);
        let render_ms = fmt_profile_ms(self.render_us, frames);
        let app_cpu_outside_render_ms =
            fmt_profile_ms(self.total_redraw_us.saturating_sub(self.render_us), frames);
        let plan_ms = fmt_profile_ms(self.plan_us, frames);
        let draw_ms = fmt_profile_ms(self.draw_us, frames);
        let text_ms = fmt_profile_ms(self.text_us, frames);
        let geometry_ms = fmt_profile_ms(self.geometry_us, frames);
        let upload_ms = fmt_profile_ms(self.upload_us, frames);
        let submit_ms = fmt_profile_ms(self.submit_us, frames);
        let surface_ms = fmt_profile_ms(self.surface_us, frames);
        let bind_ms = fmt_profile_ms(self.bind_us, frames);
        let encode_ms = fmt_profile_ms(self.encode_us, frames);
        let queue_ms = fmt_profile_ms(self.queue_us, frames);
        let present_ms = fmt_profile_ms(self.present_us, frames);
        let total_redraw_ms = fmt_profile_ms(self.total_redraw_us, frames);
        let input_ms = fmt_profile_ms(self.input_us, frames);
        let background_ms = fmt_profile_ms(self.background_us, frames);
        let transition_ms = fmt_profile_ms(self.transition_us, frames);
        let egui_ms = fmt_profile_ms(self.egui_us, frames);
        let consume_active_play_ms = fmt_profile_ms(self.consume_active_play_us, frames);
        let post_scene_ms = fmt_profile_ms(self.post_scene_us, frames);
        let wait_wake_samples = self.wait_wake_samples;
        let scheduled_wait_ms = fmt_profile_ms(self.scheduled_wait_us, wait_wake_samples.max(1));
        let wake_lateness_ms = fmt_profile_ms(self.wake_lateness_us, wait_wake_samples.max(1));
        let redraw_after_wake_ms =
            fmt_profile_ms(self.redraw_after_wake_us, wait_wake_samples.max(1));
        let total_redraw_percentiles = frame_duration_percentiles(&self.total_redraw_samples_us);
        let render_percentiles = frame_duration_percentiles(&self.render_samples_us);
        let surface_percentiles = frame_duration_percentiles(&self.surface_samples_us);
        let queue_percentiles = frame_duration_percentiles(&self.queue_samples_us);
        let present_percentiles = frame_duration_percentiles(&self.present_samples_us);
        let wake_lateness_percentiles = frame_duration_percentiles(&self.wake_lateness_samples_us);
        let redraw_after_wake_percentiles =
            frame_duration_percentiles(&self.redraw_after_wake_samples_us);
        macro_rules! log_frame_profile {
            ($target:literal, $message:literal) => {
                tracing::debug!(
                    target: $target,
                    frames = self.frames,
                    video_ms,
                    video_poll_ms,
                    video_upload_ms,
                    video_upload_frame_ms,
                    video_opened,
                    video_active_sources,
                    video_visible_sources,
                    video_uploaded_frames,
                    snapshot_ms,
                    render_ms,
                    app_cpu_outside_render_ms,
                    render_p95_ms = render_percentiles
                        .map(|value| fmt_profile_us_ms(value.p95_us)),
                    render_p99_ms = render_percentiles
                        .map(|value| fmt_profile_us_ms(value.p99_us)),
                    render_max_ms = render_percentiles
                        .map(|value| fmt_profile_us_ms(value.max_us)),
                    plan_ms,
                    draw_ms,
                    text_ms,
                    geometry_ms,
                    upload_ms,
                    submit_ms,
                    surface_ms,
                    surface_p95_ms = surface_percentiles
                        .map(|value| fmt_profile_us_ms(value.p95_us)),
                    surface_p99_ms = surface_percentiles
                        .map(|value| fmt_profile_us_ms(value.p99_us)),
                    surface_max_ms = surface_percentiles
                        .map(|value| fmt_profile_us_ms(value.max_us)),
                    bind_ms,
                    encode_ms,
                    queue_ms,
                    queue_p95_ms = queue_percentiles
                        .map(|value| fmt_profile_us_ms(value.p95_us)),
                    queue_p99_ms = queue_percentiles
                        .map(|value| fmt_profile_us_ms(value.p99_us)),
                    queue_max_ms = queue_percentiles
                        .map(|value| fmt_profile_us_ms(value.max_us)),
                    present_ms,
                    present_p95_ms = present_percentiles
                        .map(|value| fmt_profile_us_ms(value.p95_us)),
                    present_p99_ms = present_percentiles
                        .map(|value| fmt_profile_us_ms(value.p99_us)),
                    present_max_ms = present_percentiles
                        .map(|value| fmt_profile_us_ms(value.max_us)),
                    total_redraw_ms,
                    total_redraw_p95_ms = total_redraw_percentiles
                        .map(|value| fmt_profile_us_ms(value.p95_us)),
                    total_redraw_p99_ms = total_redraw_percentiles
                        .map(|value| fmt_profile_us_ms(value.p99_us)),
                    total_redraw_max_ms = total_redraw_percentiles
                        .map(|value| fmt_profile_us_ms(value.max_us)),
                    input_ms,
                    background_ms,
                    transition_ms,
                    egui_ms,
                    consume_active_play_ms,
                    post_scene_ms,
                    effective_frame_limit = self.effective_frame_limit,
                    frame_budget_ms = fmt_profile_us_ms(self.frame_budget_us),
                    wait_wake_samples,
                    scheduled_wait_ms,
                    wake_lateness_ms,
                    wake_lateness_p95_ms = wake_lateness_percentiles
                        .map(|value| fmt_profile_us_ms(value.p95_us)),
                    wake_lateness_p99_ms = wake_lateness_percentiles
                        .map(|value| fmt_profile_us_ms(value.p99_us)),
                    wake_lateness_max_ms = wake_lateness_percentiles
                        .map(|value| fmt_profile_us_ms(value.max_us)),
                    redraw_after_wake_ms,
                    redraw_after_wake_p95_ms = redraw_after_wake_percentiles
                        .map(|value| fmt_profile_us_ms(value.p95_us)),
                    redraw_after_wake_p99_ms = redraw_after_wake_percentiles
                        .map(|value| fmt_profile_us_ms(value.p99_us)),
                    redraw_after_wake_max_ms = redraw_after_wake_percentiles
                        .map(|value| fmt_profile_us_ms(value.max_us)),
                    maximum_consecutive_deadline_misses =
                        self.maximum_consecutive_deadline_misses,
                    last_scheduled_deadline = ?self.last_scheduled_deadline,
                    last_actual_wake_at = ?self.last_actual_wake_at,
                    last_redraw_started_at = ?self.last_redraw_started_at,
                    commands,
                    steps,
                    rect_steps,
                    image_steps,
                    text_steps,
                    rect_instances,
                    image_instances,
                    text_instances,
                    $message
                );
            };
        }
        match profile {
            FrameProfileKind::Select => {
                log_frame_profile!("bmz_player::select_profile", "select frame profile");
            }
            FrameProfileKind::Decide => {
                log_frame_profile!("bmz_player::decide_profile", "decide frame profile");
            }
            FrameProfileKind::Play => {
                log_frame_profile!("bmz_player::play_profile", "play frame profile");
            }
            FrameProfileKind::Result => {
                log_frame_profile!("bmz_player::result_profile", "result frame profile");
            }
        }
        *self = Self::default();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameDurationPercentiles {
    p95_us: u64,
    p99_us: u64,
    max_us: u64,
}

fn fmt_profile_ms(total_us: u128, frames: u128) -> String {
    format!("{:.3}", total_us as f64 / frames as f64 / 1000.0)
}

fn fmt_profile_us_ms(us: u64) -> String {
    format!("{:.3}", us as f64 / 1000.0)
}

fn push_profile_sample(samples: &mut Vec<u64>, value_us: u128) {
    if samples.len() < FRAME_PROFILE_SAMPLE_CAPACITY {
        samples.push(value_us.min(u128::from(u64::MAX)) as u64);
    }
}

fn frame_duration_percentiles(samples: &[u64]) -> Option<FrameDurationPercentiles> {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let len = sorted.len();
    (len > 0).then(|| FrameDurationPercentiles {
        p95_us: sorted[(len * 95).div_ceil(100).saturating_sub(1)],
        p99_us: sorted[(len * 99).div_ceil(100).saturating_sub(1)],
        max_us: *sorted.last().expect("non-empty sample list"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::AppLocale;

    #[test]
    fn frame_duration_percentiles_use_nearest_rank() {
        let samples = [1_000, 2_000, 3_000, 4_000, 5_000, 6_000, 7_000, 8_000, 9_000, 10_000];

        assert_eq!(
            frame_duration_percentiles(&samples),
            Some(FrameDurationPercentiles { p95_us: 10_000, p99_us: 10_000, max_us: 10_000 })
        );
        assert_eq!(frame_duration_percentiles(&[]), None);
    }

    #[test]
    fn skin_fps_updates_only_after_each_one_second_window() {
        let started_at = Instant::now();
        let mut fps = SkinFpsCounter::new(started_at);

        for elapsed_ms in [0, 250, 500, 750] {
            fps.record_presented_frame(started_at + Duration::from_millis(elapsed_ms));
            assert_eq!(fps.current(), 0);
        }

        fps.record_presented_frame(started_at + Duration::from_secs(1));
        assert_eq!(fps.current(), 5);
        assert_eq!(fps_overlay_text(true, fps.current(), Localizer::new(AppLocale::Ja)), "FPS 5");
        fps.record_presented_frame(started_at + Duration::from_millis(1_250));
        assert_eq!(fps.current(), 5);
        fps.record_presented_frame(started_at + Duration::from_secs(2));
        assert_eq!(fps.current(), 2);
    }

    #[test]
    fn skin_fps_normalizes_presented_frames_by_elapsed_time() {
        assert_eq!(normalized_fps(240, Duration::from_secs(1)), 240);
        assert_eq!(normalized_fps(240, Duration::from_millis(1_200)), 200);
        assert_eq!(normalized_fps(120, Duration::from_millis(500)), 240);
    }

    #[test]
    fn skin_fps_reset_does_not_carry_previous_frames() {
        let started_at = Instant::now();
        let mut fps = SkinFpsCounter::new(started_at);
        for _ in 0..120 {
            fps.record_presented_frame(started_at + Duration::from_millis(500));
        }
        fps.reset(started_at + Duration::from_millis(500));
        for _ in 0..239 {
            fps.record_presented_frame(started_at + Duration::from_millis(1_499));
        }
        fps.record_presented_frame(started_at + Duration::from_millis(1_500));
        assert_eq!(fps.current(), 240);
    }

    #[test]
    fn failed_surface_frame_is_not_counted_as_presented() {
        let started_at = Instant::now();
        let mut runtime = FrameRuntime::new(started_at);
        runtime.record_surface_status(
            started_at + Duration::from_secs(1),
            Some(RenderSurfaceStatus::TimedOut),
        );
        runtime.record_surface_status(
            started_at + Duration::from_secs(1),
            Some(RenderSurfaceStatus::Rendered),
        );
        assert_eq!(runtime.current_fps(), 1);
    }

    #[test]
    fn pacing_state_changes_reset_the_fps_sample() {
        let now = Instant::now();
        let mut runtime = FrameRuntime::new(now);
        let initial = test_pacing_state(240);
        runtime.sync_pacing_state(now, initial);

        let changed_states = [
            FramePacingState { focused: false, ..initial },
            FramePacingState { focused: false, effective_frame_limit: 60, ..initial },
            FramePacingState {
                focused: false,
                effective_frame_limit: 60,
                present_mode: WgpuPresentMode::Immediate,
                ..initial
            },
            FramePacingState {
                focused: false,
                effective_frame_limit: 60,
                present_mode: WgpuPresentMode::Immediate,
                window_mode: FrameWindowMode::ExclusiveFullscreen,
            },
        ];
        for state in changed_states {
            runtime.fps.frames = 100;
            runtime.fps.current = 240;
            runtime.sync_pacing_state(now, state);
            assert_eq!(runtime.fps.frames, 0);
            assert_eq!(runtime.current_fps(), 0);
        }
    }

    #[test]
    fn frame_pacer_waits_for_every_light_frame_without_alternating() {
        let started_at = Instant::now();
        let budget = frame_budget(120);
        let work = Duration::from_micros(500);
        let mut pacer = FramePacer::default();

        assert_eq!(pacer.delay(started_at, 120, false), Duration::ZERO);
        pacer.record_frame_started(started_at, 120, false);

        let mut previous_frame = started_at;
        for _ in 0..4 {
            let redraw_arrived_at = previous_frame + work;
            let delay = pacer.delay(redraw_arrived_at, 120, false);
            assert_eq!(delay, budget - work);

            let frame_started = redraw_arrived_at + delay;
            assert_eq!(frame_started.duration_since(previous_frame), budget);
            pacer.record_frame_started(frame_started, 120, false);
            previous_frame = frame_started;
        }
    }

    #[test]
    fn immediate_wait_has_a_bounded_tail_only_on_focused_windows() {
        for fps in [120, 240] {
            let now = Instant::now();
            let deadline = now + frame_budget(fps);
            let state = FramePacingState {
                present_mode: WgpuPresentMode::Immediate,
                ..test_pacing_state(fps)
            };
            let FrameWait::SleepUntil(wake) = state.wait_plan(now, deadline, true) else {
                panic!("long wait must sleep")
            };
            let tail = deadline.duration_since(wake);
            assert!(tail <= Duration::from_micros(600));
            assert!(tail <= frame_budget(fps) / 8);
            assert!(
                matches!(state.wait_plan(wake, deadline, true), FrameWait::FinishAt(value) if value == deadline)
            );
            for (state, windows) in [
                (state, false),
                (FramePacingState { focused: false, ..state }, true),
                (FramePacingState { effective_frame_limit: 0, ..state }, true),
                (FramePacingState { present_mode: WgpuPresentMode::Fifo, ..state }, true),
                (FramePacingState { present_mode: WgpuPresentMode::Mailbox, ..state }, true),
            ] {
                assert!(
                    matches!(state.wait_plan(wake, deadline, windows), FrameWait::SleepUntil(value) if value == deadline)
                );
            }
        }
    }

    #[test]
    fn immediate_frame_preserves_period_and_stall_does_not_catch_up_in_a_burst() {
        for fps in [120, 240] {
            for stall_ms in [16, 33, 100, 250] {
                let now = Instant::now();
                let budget = frame_budget(fps);
                let mut pacer = FramePacer::default();
                pacer.record_frame_started(now, fps, false);
                for micros in [100, 200, 300] {
                    pacer.record_frame_started(now + Duration::from_micros(micros), fps, true);
                    assert_eq!(pacer.next_frame_at, Some(now + budget));
                }
                let resumed = now + Duration::from_millis(stall_ms);
                pacer.record_frame_started(resumed, fps, false);
                assert_eq!(pacer.next_frame_at, Some(resumed + budget));
            }
        }
    }

    #[test]
    fn frame_pacer_exposes_the_next_deadline_without_blocking() {
        let started_at = Instant::now();
        let budget = frame_budget(120);
        let work = Duration::from_micros(500);
        let mut pacer = FramePacer::default();
        pacer.record_frame_started(started_at, 120, false);

        assert_eq!(pacer.next_deadline(started_at + work, 120, false), Some(started_at + budget));
        assert_eq!(pacer.next_deadline(started_at + budget, 120, false), None);
        assert_eq!(pacer.next_deadline(started_at + work, 120, true), None);
        assert_eq!(pacer.next_deadline(started_at + work, 0, false), None);
    }

    #[test]
    fn frame_runtime_records_wait_wake_and_redraw_timing() {
        let started_at = Instant::now();
        let pacing = test_pacing_state(240);
        let mut runtime = FrameRuntime::new(started_at);
        runtime.sync_pacing_state(started_at, pacing);
        let deadline = started_at + Duration::from_millis(4);
        let actual_wake = deadline + Duration::from_millis(1);
        runtime.record_wait_wake(started_at, deadline, actual_wake, 240);

        assert!(matches!(
            runtime.begin_scheduled_frame(actual_wake + Duration::from_micros(250), pacing),
            FrameSchedule::Start
        ));
        let timings = runtime.current_pacing_timings();
        assert!(timings.wait_wake_sampled);
        assert_eq!(timings.effective_frame_limit, 240);
        assert_eq!(timings.frame_budget_us, 4_166);
        assert_eq!(timings.scheduled_wait_us, 4_000);
        assert_eq!(timings.wake_lateness_us, 1_000);
        assert_eq!(timings.redraw_after_wake_us, 250);
        assert_eq!(timings.consecutive_deadline_misses, 0);
        assert_eq!(timings.scheduled_deadline, Some(deadline));
        assert_eq!(timings.actual_wake_at, Some(actual_wake));
    }

    #[test]
    fn frame_runtime_counts_only_full_budget_wake_misses_as_consecutive() {
        let started_at = Instant::now();
        let deadline = started_at + Duration::from_millis(4);
        let mut runtime = FrameRuntime::new(started_at);

        runtime.record_wait_wake(started_at, deadline, deadline + Duration::from_millis(5), 240);
        assert_eq!(runtime.consecutive_deadline_misses, 1);
        runtime.record_wait_wake(started_at, deadline, deadline + Duration::from_millis(10), 240);
        assert_eq!(runtime.consecutive_deadline_misses, 2);
        runtime.record_wait_wake(started_at, deadline, deadline + Duration::from_millis(1), 240);
        assert_eq!(runtime.consecutive_deadline_misses, 0);
    }

    #[test]
    fn frame_runtime_waits_until_deadline_and_honors_immediate_request() {
        let started_at = Instant::now();
        let work = Duration::from_micros(500);
        let mut runtime = FrameRuntime::new(started_at);

        let pacing = test_pacing_state(120);
        assert!(matches!(runtime.begin_scheduled_frame(started_at, pacing), FrameSchedule::Start));
        assert!(matches!(
            runtime.begin_scheduled_frame(started_at + work, pacing),
            FrameSchedule::WaitUntil(deadline) if deadline == started_at + frame_budget(120)
        ));

        runtime.request_immediate_frame();
        assert!(matches!(
            runtime.begin_scheduled_frame(started_at + work, pacing),
            FrameSchedule::Start
        ));
        assert_eq!(runtime.current_fps(), 0);
    }

    fn test_pacing_state(effective_frame_limit: u32) -> FramePacingState {
        FramePacingState {
            focused: true,
            effective_frame_limit,
            present_mode: WgpuPresentMode::Fifo,
            window_mode: FrameWindowMode::Windowed,
        }
    }

    #[test]
    fn frame_pacer_rebases_after_a_missed_deadline() {
        let started_at = Instant::now();
        let budget = frame_budget(120);
        let work = Duration::from_micros(500);
        let mut pacer = FramePacer::default();
        pacer.record_frame_started(started_at, 120, false);

        let late_frame = started_at + budget + budget;
        assert_eq!(pacer.delay(late_frame, 120, false), Duration::ZERO);
        pacer.record_frame_started(late_frame, 120, false);

        let next_redraw = late_frame + work;
        assert_eq!(pacer.delay(next_redraw, 120, false), budget - work);
    }

    #[test]
    fn frame_pacer_rebases_when_fps_changes_but_keeps_phase_for_immediate_frames() {
        let started_at = Instant::now();
        let work = Duration::from_micros(500);
        let mut pacer = FramePacer::default();
        pacer.record_frame_started(started_at, 120, false);

        let fps_changed_at = started_at + work;
        assert_eq!(pacer.delay(fps_changed_at, 60, false), Duration::ZERO);
        pacer.record_frame_started(fps_changed_at, 60, false);
        assert_eq!(pacer.delay(fps_changed_at + work, 60, false), frame_budget(60) - work);

        let skipped_at = fps_changed_at + Duration::from_millis(2);
        assert_eq!(pacer.delay(skipped_at, 60, true), Duration::ZERO);
        pacer.record_frame_started(skipped_at, 60, true);
        assert_eq!(
            pacer.next_deadline(skipped_at + work, 60, false),
            Some(fps_changed_at + frame_budget(60))
        );

        pacer.record_frame_started(skipped_at + work, 0, false);
        assert_eq!(pacer.delay(skipped_at + work, 0, false), Duration::ZERO);
        assert_eq!(pacer.delay(skipped_at + work, 120, false), Duration::ZERO);
    }

    #[test]
    fn fps_overlay_text_uses_skin_fps_value() {
        let text = Localizer::new(AppLocale::Ja);
        assert_eq!(fps_overlay_text(true, 237, text), "FPS 237");
        assert_eq!(fps_overlay_text(false, 237, text), "");
        assert_eq!(fps_overlay_text(true, 0, text), "");
    }
}
