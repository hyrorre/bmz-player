use bmz_core::latency::LatencyHistogram;
use std::cell::RefCell;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Profile {
    last_advance: Option<Instant>,
    window_start: Option<Instant>,
    judged: Option<Instant>,
    advance: LatencyHistogram,
    event_age: LatencyHistogram,
    collect_judge: LatencyHistogram,
    judge_enqueue: LatencyHistogram,
}
thread_local! { static PROFILE: RefCell<Profile> = RefCell::new(Profile::default()); }

#[cfg(test)]
pub(super) fn max_collect_judge_us() -> u64 {
    PROFILE.with_borrow(|p| p.collect_judge.summary().max)
}

#[cfg(test)]
pub(super) fn judgement_sample_counts() -> (u64, u64) {
    PROFILE.with_borrow(|p| (p.collect_judge.summary().count, p.judge_enqueue.summary().count))
}

pub fn begin_advance() {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    PROFILE.with_borrow_mut(|p| {
        let now = Instant::now();
        if let Some(last) = p.last_advance.replace(now) {
            p.advance.record(now.duration_since(last).as_micros() as u64);
        }
        let start = *p.window_start.get_or_insert(now);
        if now.duration_since(start) >= Duration::from_secs(5) {
            tracing::debug!(window_ms = now.duration_since(start).as_millis() as u64,
                advance_interval_us = ?p.advance.summary(), input_event_age_us = ?p.event_age.summary(),
                collect_to_judgement_us = ?p.collect_judge.summary(),
                judgement_to_enqueue_us = ?p.judge_enqueue.summary(), "gameplay latency");
            *p = Profile { last_advance: Some(now), window_start: Some(now), ..Default::default() };
        }
    });
}

pub fn collected(event_age_us: impl Iterator<Item = u64>) {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    PROFILE.with_borrow_mut(|p| {
        for age in event_age_us {
            p.event_age.record(age);
        }
    });
}

pub fn judged(collected: Instant) {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    PROFILE.with_borrow_mut(|p| {
        let now = Instant::now();
        p.collect_judge.record(now.duration_since(collected).as_micros() as u64);
        p.judged.get_or_insert(now);
    });
}

/// Called only after the audio command queue accepted the pending sounds.
pub fn audio_enqueued() {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    PROFILE.with_borrow_mut(|p| {
        if let Some(judged) = p.judged.take() {
            p.judge_enqueue.record(judged.elapsed().as_micros() as u64);
        }
    });
}
