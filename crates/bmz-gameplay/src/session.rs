use std::sync::Arc;

use bmz_audio::clock::AudioClock;
use bmz_audio::queue::{AudioScheduler, ScheduledSound};
use bmz_chart::model::PlayableChart;
use bmz_core::time::TimeUs;

use crate::autoplay::AutoplayController;
use crate::gauge::GaugeState;
use crate::input::system::InputSystem;
use crate::input::translator::InputTimingContext;
use crate::judge::engine::JudgeEngine;
use crate::judge::model::{JudgeOutcome, JudgementEvent};
use crate::replay::{ReplayPlayer, ReplayRecorder};
use crate::score::ScoreState;

pub const AUDIO_SCHEDULE_AHEAD_US: i64 = 100_000;
pub const SESSION_END_MARGIN_US: i64 = 500_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Ready,
    Playing,
    Finished,
    Failed,
}

#[derive(Debug, Clone, Copy)]
pub struct PlayOffsets {
    pub input_offset_us: i64,
    pub visual_offset_us: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct FrameTimes {
    pub audio_now: TimeUs,
    pub render_now: TimeUs,
    pub audio_schedule_until: TimeUs,
}

#[derive(Debug, Clone, Default)]
pub struct BgmScheduler {
    pub next_index: usize,
}

pub struct GameSession {
    pub chart: Arc<PlayableChart>,
    pub audio_clock: AudioClock,
    pub input_system: InputSystem,
    pub judge: JudgeEngine,
    pub score: ScoreState,
    pub gauge: GaugeState,
    pub replay_recorder: ReplayRecorder,
    pub replay_player: Option<ReplayPlayer>,
    pub autoplay: Option<AutoplayController>,
    pub bgm_scheduler: BgmScheduler,
    pub offsets: PlayOffsets,
    pub state: PlayState,
}

pub struct FrameOutput<TSnapshot> {
    pub render_snapshot: TSnapshot,
    pub state: PlayState,
}

#[derive(Debug, Clone)]
pub struct SessionFrame {
    pub times: FrameTimes,
    pub judgements: Vec<JudgementEvent>,
    pub state: PlayState,
}

impl BgmScheduler {
    pub fn schedule_until(
        &mut self,
        chart: &PlayableChart,
        clock: &AudioClock,
        until: TimeUs,
        audio: &mut dyn AudioScheduler,
    ) {
        while let Some(event) = chart.bgm_events.get(self.next_index) {
            if event.time > until {
                break;
            }

            audio.schedule(ScheduledSound {
                start_frame: clock.time_to_output_frame(event.time),
                sound_id: event.sound,
                volume: 1.0,
                pan: 0.0,
            });

            self.next_index += 1;
        }
    }

    pub fn is_done(&self, chart: &PlayableChart) -> bool {
        self.next_index >= chart.bgm_events.len()
    }
}

pub fn compute_frame_times(session: &GameSession) -> FrameTimes {
    let audio_now = session.audio_clock.now();
    let render_now = TimeUs(audio_now.0 + session.offsets.visual_offset_us);
    let audio_schedule_until = TimeUs(audio_now.0 + AUDIO_SCHEDULE_AHEAD_US);
    FrameTimes { audio_now, render_now, audio_schedule_until }
}

pub fn apply_judge_outcome(
    session: &mut GameSession,
    outcome: JudgeOutcome,
) -> Vec<JudgementEvent> {
    let mut events = Vec::with_capacity(outcome.events.len());
    for event in outcome.events {
        session.score.apply(&event);
        session.gauge.apply_judge(event.judge, 1.0);
        events.push(event);
    }
    events
}

pub fn process_human_inputs(session: &mut GameSession) -> Vec<JudgementEvent> {
    let ctx = InputTimingContext { audio_clock: &session.audio_clock, offsets: session.offsets };
    let inputs = session.input_system.collect_game_inputs(&ctx);
    let mut judgements = Vec::new();
    for input in inputs {
        session.replay_recorder.record(input);
        let outcome = session.judge.process_input(&session.chart, input);
        judgements.extend(apply_judge_outcome(session, outcome));
    }
    judgements
}

pub fn process_replay_inputs(session: &mut GameSession, audio_now: TimeUs) -> Vec<JudgementEvent> {
    let Some(player) = &mut session.replay_player else {
        return Vec::new();
    };

    let mut judgements = Vec::new();
    for input in player.poll_until(audio_now) {
        let outcome = session.judge.process_input(&session.chart, input);
        judgements.extend(apply_judge_outcome(session, outcome));
    }
    judgements
}

pub fn process_autoplay_inputs(
    session: &mut GameSession,
    audio_now: TimeUs,
) -> Vec<JudgementEvent> {
    let Some(auto) = &mut session.autoplay else {
        return Vec::new();
    };

    let mut judgements = Vec::new();
    for input in auto.poll_until(&session.chart, audio_now) {
        let outcome = session.judge.process_input(&session.chart, input);
        judgements.extend(apply_judge_outcome(session, outcome));
    }
    judgements
}

pub fn process_misses(session: &mut GameSession, audio_now: TimeUs) -> Vec<JudgementEvent> {
    let outcome = session.judge.process_misses(&session.chart, audio_now);
    apply_judge_outcome(session, outcome)
}

pub fn advance_session_frame(
    session: &mut GameSession,
    audio: &mut dyn AudioScheduler,
) -> SessionFrame {
    if session.state == PlayState::Ready {
        session.state = PlayState::Playing;
    }

    let times = compute_frame_times(session);
    let mut judgements = Vec::new();

    if session.state == PlayState::Playing {
        session.bgm_scheduler.schedule_until(
            &session.chart,
            &session.audio_clock,
            times.audio_schedule_until,
            audio,
        );

        judgements.extend(process_human_inputs(session));
        judgements.extend(process_replay_inputs(session, times.audio_now));
        judgements.extend(process_autoplay_inputs(session, times.audio_now));
        judgements.extend(process_misses(session, times.audio_now));

        if should_finish(session, times.audio_now) {
            session.state = PlayState::Finished;
        }
    }

    SessionFrame { times, judgements, state: session.state }
}

pub fn should_finish(session: &GameSession, audio_now: TimeUs) -> bool {
    session.judge.is_exhausted(&session.chart)
        && session.bgm_scheduler.is_done(&session.chart)
        && audio_now.0 > session.chart.end_time.0 + SESSION_END_MARGIN_US
}
