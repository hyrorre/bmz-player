use std::sync::Arc;

use bmz_audio::clock::AudioClock;
use bmz_audio::queue::{AudioScheduler, ScheduledSound};
use bmz_chart::model::PlayableChart;
use bmz_core::input::{InputEvent, InputKind};
use bmz_core::judge::Judge;
use bmz_core::time::TimeUs;

use crate::autoplay::AutoplayController;
use crate::gauge::GaugeState;
use crate::input::system::InputSystem;
use crate::input::translator::{InputTimestampAnchor, InputTimingContext};
use crate::judge::engine::JudgeEngine;
use crate::judge::model::{JudgeOutcome, JudgementEvent};
use crate::replay::{ReplayPlayer, ReplayRecorder};
use crate::score::ScoreState;

pub const AUDIO_SCHEDULE_AHEAD_US: i64 = 100_000;
pub const SESSION_END_MARGIN_US: i64 = 500_000;
pub const JUDGEMENT_DISPLAY_US: i64 = 800_000;
pub const INPUT_DISPLAY_US: i64 = 160_000;

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
pub struct PlayAudioMix {
    pub master_volume: f32,
    pub key_volume: f32,
    pub bgm_volume: f32,
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
    pub recent_inputs: Vec<InputEvent>,
    pub recent_judgements: Vec<JudgementEvent>,
    pub bgm_scheduler: BgmScheduler,
    pub offsets: PlayOffsets,
    pub audio_mix: PlayAudioMix,
    pub hispeed: f32,
    pub lift: f32,
    pub lane_cover: f32,
    pub input_timestamp_anchor: Option<InputTimestampAnchor>,
    pub state: PlayState,
}

#[derive(Debug, Clone)]
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
        volume: f32,
        audio: &mut dyn AudioScheduler,
    ) {
        while let Some(event) = chart.bgm_events.get(self.next_index) {
            if event.time > until {
                break;
            }

            audio.schedule(ScheduledSound {
                start_frame: clock.time_to_output_frame(event.time),
                sound_id: event.sound,
                volume: volume.clamp(0.0, 1.0),
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

pub fn schedule_keysounds(
    session: &GameSession,
    judgements: &[JudgementEvent],
    audio: &mut dyn AudioScheduler,
) {
    for event in judgements {
        if !plays_keysound(event.judge) {
            continue;
        }
        let Some(note_id) = event.note_id else {
            continue;
        };
        let Some(sound_id) = session.chart.note_by_id(note_id).and_then(|note| note.sound) else {
            continue;
        };

        audio.schedule(ScheduledSound {
            start_frame: session.audio_clock.time_to_output_frame(event.time),
            sound_id,
            volume: (session.audio_mix.master_volume * session.audio_mix.key_volume)
                .clamp(0.0, 1.0),
            pan: 0.0,
        });
    }
}

fn plays_keysound(judge: Judge) -> bool {
    matches!(judge, Judge::PGreat | Judge::Great | Judge::Good | Judge::Bad)
}

pub fn update_recent_judgements(session: &mut GameSession, events: &[JudgementEvent], now: TimeUs) {
    session.recent_judgements.extend(events.iter().cloned());
    session.recent_judgements.retain(|event| now.0 <= event.time.0 + JUDGEMENT_DISPLAY_US);
}

pub fn update_recent_inputs(session: &mut GameSession, inputs: &[InputEvent], now: TimeUs) {
    session
        .recent_inputs
        .extend(inputs.iter().copied().filter(|input| input.kind == InputKind::Press));
    session.recent_inputs.retain(|input| now.0 <= input.time.0 + INPUT_DISPLAY_US);
}

pub fn process_human_inputs(session: &mut GameSession) -> Vec<JudgementEvent> {
    let ctx = InputTimingContext {
        audio_clock: &session.audio_clock,
        offsets: session.offsets,
        timestamp_anchor: session.input_timestamp_anchor,
    };
    let inputs = session.input_system.collect_game_inputs(&ctx);
    update_recent_inputs(session, &inputs, session.audio_clock.now());
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
            session.audio_mix.master_volume * session.audio_mix.bgm_volume,
            audio,
        );

        judgements.extend(process_human_inputs(session));
        judgements.extend(process_replay_inputs(session, times.audio_now));
        judgements.extend(process_autoplay_inputs(session, times.audio_now));
        judgements.extend(process_misses(session, times.audio_now));
        schedule_keysounds(session, &judgements, audio);
        update_recent_judgements(session, &judgements, times.render_now);

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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;

    use bmz_chart::model::{ChartMetadata, NoteEvent, NoteKind, SoundAssetRef, SoundEvent};
    use bmz_core::chart::ChartIdentity;
    use bmz_core::ids::{NoteId, SoundId};
    use bmz_core::input::InputSource;
    use bmz_core::judge::TimingSide;
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};

    use crate::input::backend::NullInputBackend;
    use crate::input::binding::LaneBinding;
    use crate::input::system::InputSystem;
    use crate::input::translator::DefaultInputTranslator;
    use crate::judge::model::JudgeWindow;

    use super::*;

    #[derive(Default)]
    struct TestAudio {
        scheduled: Vec<ScheduledSound>,
    }

    impl AudioScheduler for TestAudio {
        fn schedule(&mut self, sound: ScheduledSound) {
            self.scheduled.push(sound);
        }
    }

    #[test]
    fn advance_session_frame_schedules_autoplay_keysounds() {
        let mut session = session_with_autoplay(chart_with_keysound());
        session.audio_mix.master_volume = 0.5;
        session.audio_mix.key_volume = 0.25;
        let mut audio = TestAudio::default();

        let frame = advance_session_frame(&mut session, &mut audio);

        assert_eq!(frame.judgements.len(), 1);
        assert_eq!(audio.scheduled.len(), 1);
        assert_eq!(audio.scheduled[0].sound_id, SoundId(7));
        assert_eq!(audio.scheduled[0].start_frame, 0);
        assert_eq!(audio.scheduled[0].volume, 0.125);
        assert_eq!(session.recent_judgements.len(), 1);
    }

    #[test]
    fn advance_session_frame_schedules_bgm_with_mix_volume() {
        let mut session = session_with_autoplay(chart_with_bgm());
        session.audio_mix.master_volume = 0.5;
        session.audio_mix.bgm_volume = 0.75;
        let mut audio = TestAudio::default();

        advance_session_frame(&mut session, &mut audio);

        assert_eq!(audio.scheduled.len(), 1);
        assert_eq!(audio.scheduled[0].sound_id, SoundId(3));
        assert_eq!(audio.scheduled[0].volume, 0.375);
    }

    #[test]
    fn update_recent_judgements_expires_old_events() {
        let mut session = session_with_autoplay(chart_with_keysound());
        let event = JudgementEvent {
            note_id: Some(NoteId(1)),
            lane: Lane::Key1,
            judge: Judge::PGreat,
            side: TimingSide::Slow,
            delta: TimeUs(0),
            time: TimeUs(0),
        };

        update_recent_judgements(&mut session, &[event], TimeUs(0));
        update_recent_judgements(&mut session, &[], TimeUs(JUDGEMENT_DISPLAY_US + 1));

        assert!(session.recent_judgements.is_empty());
    }

    #[test]
    fn update_recent_inputs_keeps_presses_and_expires_old_events() {
        let mut session = session_with_autoplay(chart_with_keysound());
        let inputs = [
            InputEvent {
                lane: Lane::Key1,
                kind: InputKind::Press,
                time: TimeUs(10_000),
                source: InputSource::Human,
            },
            InputEvent {
                lane: Lane::Key2,
                kind: InputKind::Release,
                time: TimeUs(20_000),
                source: InputSource::Human,
            },
        ];

        update_recent_inputs(&mut session, &inputs, TimeUs(10_000));
        assert_eq!(session.recent_inputs.len(), 1);
        assert_eq!(session.recent_inputs[0].lane, Lane::Key1);
        update_recent_inputs(&mut session, &[], TimeUs(10_000 + INPUT_DISPLAY_US + 1));

        assert!(session.recent_inputs.is_empty());
    }

    fn session_with_autoplay(chart: PlayableChart) -> GameSession {
        let chart = Arc::new(chart);
        GameSession {
            chart: Arc::clone(&chart),
            audio_clock: AudioClock {
                sample_rate: 48_000,
                start_output_frame: 0,
                chart_zero_time_us: 0,
                current_frame: Arc::new(AtomicU64::new(0)),
                running: false,
            },
            input_system: InputSystem {
                backend: Box::new(NullInputBackend),
                translator: Box::new(DefaultInputTranslator {
                    binding: LaneBinding { entries: Vec::new() },
                }),
            },
            judge: JudgeEngine::new(JudgeWindow {
                pgreat_us: 16_000,
                great_us: 40_000,
                good_us: 80_000,
                bad_us: 120_000,
                empty_poor_fast_us: 500_000,
                empty_poor_slow_us: 200_000,
            }),
            score: ScoreState::default(),
            gauge: GaugeState::new(bmz_core::clear::GaugeType::Normal, 160.0, chart.total_notes),
            replay_recorder: ReplayRecorder::default(),
            replay_player: None,
            autoplay: Some(AutoplayController::default()),
            recent_inputs: Vec::new(),
            recent_judgements: Vec::new(),
            bgm_scheduler: BgmScheduler::default(),
            offsets: PlayOffsets { input_offset_us: 0, visual_offset_us: 0 },
            audio_mix: PlayAudioMix { master_volume: 1.0, key_volume: 1.0, bgm_volume: 1.0 },
            hispeed: 2.0,
            lift: 0.0,
            lane_cover: 0.0,
            input_timestamp_anchor: None,
            state: PlayState::Ready,
        }
    }

    fn chart_with_keysound() -> PlayableChart {
        let note = NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(0),
            sound: Some(SoundId(7)),
        };
        let mut lane_notes = std::array::from_fn(|_| Vec::new());
        lane_notes[Lane::Key1.index()].push(note);

        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata::default(),
            lane_notes,
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            timing_events: Vec::new(),
            bar_lines: Vec::new(),
            sounds: vec![SoundAssetRef { id: SoundId(7), path: "sound.wav".into() }],
            total_notes: 1,
            end_time: TimeUs(0),
        }
    }

    fn chart_with_bgm() -> PlayableChart {
        PlayableChart {
            identity: ChartIdentity { file_md5: [0; 16], file_sha256: [0; 32] },
            metadata: ChartMetadata::default(),
            lane_notes: std::array::from_fn(|_| Vec::new()),
            long_notes: Vec::new(),
            bgm_events: vec![SoundEvent { tick: ChartTick(0), time: TimeUs(0), sound: SoundId(3) }],
            timing_events: Vec::new(),
            bar_lines: Vec::new(),
            sounds: vec![SoundAssetRef { id: SoundId(3), path: "bgm.wav".into() }],
            total_notes: 0,
            end_time: TimeUs(0),
        }
    }
}
