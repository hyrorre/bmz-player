//! Gameplay owns mutable session state. The window thread sends commands and
//! consumes detached observations; it never holds a gameplay lock while drawing.
use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::screens::{
    play_finish::FinishSessionSnapshot,
    play_session::AppliedArrange,
    play_snapshot::{
        BgaFrameCatalog, PlayRenderSnapshotCache, PlayfieldProjection, ProjectionClock,
    },
    result_model::ResultGraphCollector,
};
use anyhow::{Result, anyhow};
use bmz_audio::command::AudioEngineHandle;
use bmz_core::time::TimeUs;
use bmz_gameplay::runtime::GameplayRuntime;
use bmz_gameplay::session::{FrameOutput, GameSession, PlayState, SkinRuntimeEvent};
use bmz_render::snapshot::RenderSnapshot;

mod observation;
pub use observation::PlaySessionObservation;

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
const COMMAND_CAPACITY: usize = 64;
const PRESENTATION_HISTORY_CAPACITY: usize = 8192;
const SAFETY_WAKE: Duration = Duration::from_millis(2);

pub struct RuntimeRenderConfig {
    #[cfg(test)]
    pub probe: Option<Arc<tests::RuntimeProbe>>,
    pub effects: Option<crate::system_sound_manager::GameplaySoundOutput>,
    pub best_ex_score: Option<u32>,
    pub best_ghost: Option<Vec<u8>>,
    pub target_ex_score: Option<u32>,
    pub target: String,
    pub resolved_target_name: Option<String>,
    pub applied_arrange: AppliedArrange,
    pub source_ln_profile: crate::ln_policy::ChartLnProfile,
    pub skin_attempt: bmz_render::snapshot::SkinAttemptState,
    pub score_key: crate::storage::score_db::ScoreKey,
    pub practice_mode: bool,
    pub score_save_disabled: bool,
    pub bga_frames: BgaFrameCatalog,
    pub cache: PlayRenderSnapshotCache,
}

#[derive(Clone)]
pub struct RuntimeResult {
    pub snapshot: FinishSessionSnapshot,
    pub graph: ResultGraphCollector,
    pub settled_at: TimeUs,
    pub play_duration_ms: u64,
}

struct Publication {
    generation: u64,
    session: PlaySessionObservation,
    frame: Arc<FrameOutput<RenderSnapshot>>,
    projection: Arc<PlayfieldProjection>,
    result: Option<Arc<RuntimeResult>>,
}

type EditSession = Box<dyn FnOnce(&mut GameSession) + Send>;
struct Worker {
    commands: mpsc::SyncSender<EditSession>,
    latest: Arc<Mutex<Option<Publication>>>,
    stop: Arc<AtomicBool>,
    snapshot_requested: Arc<AtomicBool>,
    event_ack: Arc<AtomicU64>,
    thread: JoinHandle<()>,
}

/// Prepared sessions stay local until READY. During play this is a command
/// endpoint plus immutable observations, with no access to the live GameSession.
pub struct GameplayClient {
    pub session: PlaySessionObservation,
    final_notes_processed: Arc<AtomicBool>,
    local: Option<Box<GameplayRuntime>>,
    worker: Option<Worker>,
    generation: u64,
    latest_frame: Option<Arc<FrameOutput<RenderSnapshot>>>,
    projection: Option<Arc<PlayfieldProjection>>,
    projection_clock: ProjectionClock,
    snapshot_diagnostics: Box<SnapshotDiagnostics>,
    pub result: Option<Arc<RuntimeResult>>,
}

impl GameplayClient {
    pub fn new(session: GameSession) -> Self {
        let final_notes_processed = Arc::new(AtomicBool::new(
            session.judge.is_exhausted(&session.chart)
                && bmz_gameplay::session::conditional::next_evaluation(&session).is_none(),
        ));
        Self {
            session: PlaySessionObservation::from_session(&session),
            final_notes_processed,
            local: Some(Box::new(GameplayRuntime::new(session))),
            worker: None,
            generation: NEXT_GENERATION.fetch_add(1, Ordering::Relaxed),
            latest_frame: None,
            projection: None,
            projection_clock: ProjectionClock::default(),
            snapshot_diagnostics: Box::default(),
            result: None,
        }
    }

    pub fn prepared_session(&self) -> Option<&GameSession> {
        self.local.as_ref().map(|runtime| &runtime.session)
    }

    pub fn configure_prepared<R>(&mut self, edit: impl FnOnce(&mut GameSession) -> R) -> R {
        let runtime =
            self.local.as_mut().expect("prepared session cannot be edited after runtime start");
        let result = edit(&mut runtime.session);
        self.session = PlaySessionObservation::from_session(&runtime.session);
        self.final_notes_processed.store(
            runtime.session.judge.is_exhausted(&runtime.session.chart)
                && bmz_gameplay::session::conditional::next_evaluation(&runtime.session).is_none(),
            Ordering::Release,
        );
        result
    }

    pub fn edit(&mut self, edit: impl FnOnce(&mut GameSession) + Send + 'static) -> bool {
        if let Some(runtime) = &mut self.local {
            edit(&mut runtime.session);
            self.session = PlaySessionObservation::from_session(&runtime.session);
            self.final_notes_processed.store(
                runtime.session.judge.is_exhausted(&runtime.session.chart)
                    && bmz_gameplay::session::conditional::next_evaluation(&runtime.session)
                        .is_none(),
                Ordering::Release,
            );
            return true;
        }
        let Some(worker) = &self.worker else {
            return false;
        };
        match worker.commands.try_send(Box::new(edit)) {
            Ok(()) => {
                worker.thread.thread().unpark();
                true
            }
            Err(_) => {
                tracing::error!(generation = self.generation, "gameplay control queue unavailable");
                false
            }
        }
    }

    pub fn start(&mut self, audio: AudioEngineHandle, config: RuntimeRenderConfig) -> Result<()> {
        if self.worker.is_some() {
            return Ok(());
        }
        let runtime = self.local.take().ok_or_else(|| anyhow!("gameplay is not prepared"))?;
        let times = bmz_gameplay::session::compute_frame_times(&runtime.session);
        let projection_pool = PlayfieldProjection::pool(&runtime.session, &config.cache);
        self.projection = Some(projection_pool[0].clone());
        let mut initial_frame = crate::screens::play_loop::frame_output_from_session_frame_cached(
            &runtime.session,
            bmz_gameplay::session::SessionFrame {
                times,
                judgements: Vec::new(),
                mine_hits: Vec::new(),
                keysound_volumes: Vec::new(),
                skin_events: Vec::new(),
                state: runtime.session.state,
            },
            config.best_ex_score,
            config.best_ghost.as_deref(),
            config.target_ex_score,
            &config.bga_frames,
            &config.cache,
            false,
        );
        initial_frame.render_snapshot.target.clone_from(&config.target);
        initial_frame.render_snapshot.resolved_target_name.clone_from(&config.resolved_target_name);
        self.latest_frame = Some(Arc::new(initial_frame));
        let latest = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (commands, receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
        let worker_latest = Arc::clone(&latest);
        let worker_stop = Arc::clone(&stop);
        let generation = self.generation;
        let snapshot_requested = Arc::new(AtomicBool::new(true));
        let request = snapshot_requested.clone();
        let event_ack = Arc::new(AtomicU64::new(0));
        let worker_ack = event_ack.clone();
        let worker_final_notes_processed = self.final_notes_processed.clone();
        let thread =
            thread::Builder::new().name(format!("bmz-gameplay-{generation}")).spawn(move || {
                run(
                    *runtime,
                    audio,
                    config,
                    receiver,
                    worker_latest,
                    generation,
                    WorkerSignals {
                        stop: worker_stop,
                        snapshot_requested: request,
                        event_ack: worker_ack,
                        final_notes_processed: worker_final_notes_processed,
                    },
                    projection_pool,
                )
            })?;
        self.worker =
            Some(Worker { commands, latest, stop, snapshot_requested, event_ack, thread });
        tracing::info!(
            generation,
            "gameplay runtime: dedicated thread; audio scheduling: gameplay runtime"
        );
        Ok(())
    }

    pub fn final_notes_processed(&self) -> bool {
        self.final_notes_processed.load(Ordering::Acquire)
    }

    pub fn poll(&mut self) -> Option<FrameOutput<RenderSnapshot>> {
        let worker = self.worker.as_ref()?;
        worker.snapshot_requested.store(true, Ordering::Release);
        // Rendering requests a publication at the next independent gameplay
        // wake; it must not add gameplay advances or alter HCN update cadence.
        let publication = worker.latest.try_lock().ok().and_then(|mut latest| latest.take());
        if let Some(publication) = publication {
            if publication.generation != self.generation {
                return None;
            }
            if let Some(event) = publication.frame.render_snapshot.skin_events.last() {
                worker.event_ack.store(event.sequence.saturating_add(1), Ordering::Release);
            }
            self.session = publication.session;
            self.result = publication.result;
            self.latest_frame = Some(publication.frame);
            self.projection = Some(publication.projection);
        }
        let mut frame = self.latest_frame.as_deref()?.clone();
        let published_time = frame.render_snapshot.time;
        let now = self.session.audio_clock.now();
        self.projection.as_ref()?.project(
            &mut frame.render_snapshot,
            now,
            &mut self.projection_clock,
        );
        self.snapshot_diagnostics.record(
            self.generation,
            now,
            frame.render_snapshot.time,
            published_time,
        );
        Some(frame)
    }

    pub fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    pub fn shutdown(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.stop.store(true, Ordering::Release);
            worker.thread.thread().unpark();
            // No device calls or renderer work runs in this thread. Never wait
            // on it in the window callback; its owned session is retired there.
            if worker.thread.is_finished() {
                let _ = worker.thread.join();
            }
        }
    }
}

#[derive(Default)]
struct SnapshotDiagnostics {
    started: Option<Instant>,
    previous: Option<TimeUs>,
    age: bmz_core::latency::LatencyHistogram,
    publication_age: bmz_core::latency::LatencyHistogram,
    step: bmz_core::latency::LatencyHistogram,
    repeats: u64,
}

impl SnapshotDiagnostics {
    fn record(
        &mut self,
        generation: u64,
        audio_now: TimeUs,
        snapshot_time: TimeUs,
        published_time: TimeUs,
    ) {
        if !tracing::enabled!(target: "bmz_player::frame_pacing", tracing::Level::DEBUG) {
            return;
        }
        let now = Instant::now();
        let started = *self.started.get_or_insert(now);
        let age_us = audio_now.0.saturating_sub(snapshot_time.0).max(0) as u64;
        self.age.record(age_us);
        let publication_age_us = audio_now.0.saturating_sub(published_time.0).max(0) as u64;
        self.publication_age.record(publication_age_us);
        if let Some(previous) = self.previous.replace(snapshot_time) {
            let step_us = snapshot_time.0.saturating_sub(previous.0);
            self.repeats += u64::from(step_us == 0);
            self.step.record(step_us.max(0) as u64);
            tracing::trace!(target: "bmz_player::frame_pacing", generation, age_us, step_us, publication_age_us, "snapshot cadence sample");
        }
        if now.duration_since(started) >= Duration::from_secs(5) {
            tracing::debug!(target: "bmz_player::frame_pacing", generation, age_us = ?self.age.summary(), step_us = ?self.step.summary(), publication_age_us = ?self.publication_age.summary(), repeats = self.repeats, "snapshot cadence");
            self.age = Default::default();
            self.publication_age = Default::default();
            self.step = Default::default();
            self.repeats = 0;
            self.started = Some(now);
        }
    }
}

impl Drop for GameplayClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct WorkerSignals {
    stop: Arc<AtomicBool>,
    snapshot_requested: Arc<AtomicBool>,
    event_ack: Arc<AtomicU64>,
    final_notes_processed: Arc<AtomicBool>,
}

fn run(
    mut runtime: GameplayRuntime,
    audio: AudioEngineHandle,
    mut config: RuntimeRenderConfig,
    commands: mpsc::Receiver<EditSession>,
    latest: Arc<Mutex<Option<Publication>>>,
    generation: u64,
    signals: WorkerSignals,
    mut projection_pool: [Arc<PlayfieldProjection>; 3],
) {
    let WorkerSignals { stop, snapshot_requested, event_ack, final_notes_processed } = signals;
    let audio = audio.for_play(stop.clone());
    if let Some(effects) = &mut config.effects {
        effects.bind_play(stop.clone());
    }
    let mut graph = ResultGraphCollector::for_runtime(&runtime.session.chart);
    let mut history = VecDeque::<SkinRuntimeEvent>::with_capacity(PRESENTATION_HISTORY_CAPACITY);
    let mut result = None;
    let mut terminal_result = false;
    let mut last_time = TimeUs(i64::MIN);
    let mut last_publication = Instant::now() - Duration::from_secs(1);
    runtime.session.input_system.backend.set_waker(Some(thread::current()));
    while !stop.load(Ordering::Acquire) {
        let iteration_started = Instant::now();
        let previous_state = runtime.session.state;
        for edit in commands.try_iter() {
            edit(&mut runtime.session);
        }
        if !runtime.session.audio_clock.running && runtime.session.audio_clock.now() < last_time {
            // A pause request can cross an already completed gameplay wake.
            // Freeze at the last observed instant, never rewind the same play.
            runtime.session.audio_clock.pause_at(last_time);
        }
        if stop.load(Ordering::Acquire) {
            break;
        }
        let previous_chart = runtime.session.chart.clone();
        let mut frame = runtime.advance(&audio);
        if !Arc::ptr_eq(&previous_chart, &runtime.session.chart) {
            config.cache = config.cache.for_updated_chart(&runtime.session.chart);
        }
        final_notes_processed.store(
            runtime.session.judge.is_exhausted(&runtime.session.chart)
                && bmz_gameplay::session::conditional::next_evaluation(&runtime.session).is_none(),
            Ordering::Release,
        );
        if let Some(effects) = &config.effects {
            if runtime.session.guide_se_enabled {
                for event in &frame.judgements {
                    effects.play(crate::system_sound::guide_se_for_judge(event.judge));
                }
            }
            let mix = runtime.session.audio_mix;
            if (!mix.auto_keysound || mix.auto_keysound_mine)
                && frame.mine_hits.iter().any(|hit| hit.sound.is_none())
            {
                effects.play(crate::system_sound::SoundType::Landmine);
            }
            if previous_state != PlayState::Failed && frame.state == PlayState::Failed {
                effects.play(crate::system_sound::SoundType::PlayStop);
            }
        }
        crate::screens::play_loop::log_audio_scheduling_latency(&audio);
        debug_assert!(
            frame.times.audio_now >= last_time,
            "gameplay clock moved backwards within a generation"
        );
        last_time = frame.times.audio_now;
        graph.record_runtime_frame(&runtime.session, &frame);
        #[cfg(test)]
        if let Some(probe) = &config.probe {
            probe.record(&runtime.session, &audio);
        }
        let terminal = matches!(runtime.session.state, PlayState::Finished | PlayState::Failed);
        if (result.is_none()
            && bmz_gameplay::session::result_is_settled(&runtime.session, last_time))
            || (terminal && !terminal_result)
        {
            result = Some(Arc::new(RuntimeResult {
                snapshot: FinishSessionSnapshot::from_session(
                    &runtime.session,
                    config.source_ln_profile,
                    &config.applied_arrange,
                ),
                graph: graph.clone(),
                settled_at: last_time,
                play_duration_ms: (runtime.session.audio_clock.elapsed_since(TimeUs(0)).0.max(0)
                    / 1000) as u64,
            }));
            terminal_result = terminal;
        }
        for event in frame.skin_events.drain(..) {
            if history.len() == PRESENTATION_HISTORY_CAPACITY {
                history.pop_front();
            }
            history.push_back(event);
        }

        // Acknowledgement means the renderer owns a detached event copy.
        // Its timers use event timestamps, so expired animations are not replayed
        // as new effects after a stall; stateful observers still see every event.
        let ack = event_ack.load(Ordering::Acquire);
        while history.front().is_some_and(|event| event.sequence < ack) {
            history.pop_front();
        }
        let publish = snapshot_requested.swap(false, Ordering::AcqRel)
            || last_publication.elapsed() >= Duration::from_millis(8)
            || previous_state != frame.state;
        if !publish {
            let remaining =
                runtime.next_wake_after(SAFETY_WAKE.saturating_sub(iteration_started.elapsed()));
            if !remaining.is_zero() {
                thread::park_timeout(remaining);
            }
            continue;
        }
        last_publication = Instant::now();
        // The consumer and exchange slot can each hold one buffer. Never modify
        // an observed buffer and never wait for the rendering thread to release it.
        let Some(projection) = projection_pool.iter_mut().find(|slot| Arc::strong_count(slot) == 1)
        else {
            thread::park_timeout(SAFETY_WAKE.saturating_sub(iteration_started.elapsed()));
            continue;
        };
        Arc::get_mut(projection)
            .expect("unobserved projection")
            .update(&runtime.session, last_time);
        let mut frame = crate::screens::play_loop::frame_output_from_session_frame_cached(
            &runtime.session,
            frame,
            config.best_ex_score,
            config.best_ghost.as_deref(),
            config.target_ex_score,
            &config.bga_frames,
            &config.cache,
            false,
        );
        let snapshot = &mut frame.render_snapshot;
        crate::screens::play_loop::apply_play_arrange_to_snapshot(
            snapshot,
            &config.applied_arrange,
        );
        snapshot.target.clone_from(&config.target);
        snapshot.resolved_target_name.clone_from(&config.resolved_target_name);
        snapshot.skin_attempt = config.skin_attempt;
        snapshot.rule_mode_index =
            crate::skin_extension::rule_mode_index(config.score_key.rule_mode);
        snapshot.ln_score_policy_index =
            Some(crate::skin_extension::ln_score_policy_index(config.score_key.ln_policy));
        snapshot.practice_mode = config.practice_mode;
        snapshot.score_save_enabled = !snapshot.autoplay
            && !snapshot.replay_playback
            && !config.practice_mode
            && !config.score_save_disabled
            && runtime.session.assist.score_update_enabled();
        crate::screens::play_snapshot::refresh_play_skin_visuals(snapshot, &runtime.session);
        frame.render_snapshot.skin_events = history.iter().cloned().collect();
        let publication = Publication {
            generation,
            session: PlaySessionObservation::from_session(&runtime.session),
            frame: Arc::new(frame),
            projection: projection.clone(),
            result: result.clone(),
        };
        // The lock protects only the pointer exchange, never computation or GPU
        // work. try_lock on both ends means neither thread waits for the other.
        let old =
            if let Ok(mut slot) = latest.try_lock() { slot.replace(publication) } else { None };
        drop(old);
        let remaining =
            runtime.next_wake_after(SAFETY_WAKE.saturating_sub(iteration_started.elapsed()));
        if !remaining.is_zero() {
            thread::park_timeout(remaining);
        }
    }
}

#[cfg(test)]
mod tests;
