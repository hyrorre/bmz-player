use anyhow::{Result, anyhow};
use bmz_audio::backend::cpal::SharedAudioEngine;
use bmz_audio::queue::AudioScheduler;
use bmz_gameplay::session::{FrameOutput, GameSession, advance_session_frame};
use bmz_render::snapshot::RenderSnapshot;

use crate::audio::RunningPlaySession;
use crate::screens::play_snapshot::build_render_snapshot;

pub fn advance_play_screen(
    session: &mut GameSession,
    audio: &mut dyn AudioScheduler,
) -> FrameOutput<RenderSnapshot> {
    let frame = advance_session_frame(session, audio);
    let render_snapshot = build_render_snapshot(session, frame.times.render_now, &frame.judgements);
    FrameOutput { render_snapshot, state: frame.state }
}

pub fn advance_play_screen_with_shared_audio(
    session: &mut GameSession,
    audio: &SharedAudioEngine,
) -> Result<FrameOutput<RenderSnapshot>> {
    let mut audio = audio.lock().map_err(|_| anyhow!("audio engine lock poisoned"))?;
    Ok(advance_play_screen(session, &mut *audio))
}

pub fn advance_running_play_session(
    running: &mut RunningPlaySession,
) -> Result<FrameOutput<RenderSnapshot>> {
    advance_play_screen_with_shared_audio(&mut running.session, &running.audio.engine)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::Mutex;

    use bmz_audio::backend::cpal::SharedAudioEngine;
    use bmz_audio::engine::AudioEngine;
    use bmz_audio::queue::{AudioScheduler, ScheduledSound};
    use bmz_chart::hash::compute_chart_identity;
    use bmz_chart::model::{ChartMetadata, NoteEvent, NoteKind, PlayableChart};
    use bmz_core::ids::NoteId;
    use bmz_core::lane::Lane;
    use bmz_core::time::{ChartTick, TimeUs};

    use crate::config::profile_config::ProfileConfig;
    use crate::screens::play_session::{PlaySessionOptions, build_game_session};

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
    fn advance_play_screen_returns_snapshot() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        let mut audio = TestAudio::default();

        let frame = advance_play_screen(&mut session, &mut audio);

        assert_eq!(frame.render_snapshot.time, TimeUs(0));
        assert_eq!(frame.render_snapshot.visible_notes[Lane::Key1.index()].len(), 1);
    }

    #[test]
    fn advance_play_screen_with_shared_audio_locks_engine() {
        let profile = ProfileConfig::new_default("default", "Default", 1);
        let mut session =
            build_game_session(Arc::new(chart()), &profile, PlaySessionOptions::default());
        let audio: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));

        let frame = advance_play_screen_with_shared_audio(&mut session, &audio).unwrap();

        assert_eq!(frame.render_snapshot.visible_notes[Lane::Key1.index()].len(), 1);
    }

    fn chart() -> PlayableChart {
        let note = NoteEvent {
            id: NoteId(1),
            lane: Lane::Key1,
            kind: NoteKind::Tap,
            tick: ChartTick(0),
            time: TimeUs(1_000_000),
            sound: None,
        };
        let mut lane_notes = std::array::from_fn(|_| Vec::new());
        lane_notes[Lane::Key1.index()].push(note);

        PlayableChart {
            identity: compute_chart_identity(b"play-loop"),
            metadata: ChartMetadata {
                title: "play-loop".to_string(),
                initial_bpm: 120.0,
                total: Some(160.0),
                ..Default::default()
            },
            lane_notes,
            long_notes: Vec::new(),
            bgm_events: Vec::new(),
            timing_events: Vec::new(),
            bar_lines: Vec::new(),
            sounds: Vec::new(),
            total_notes: 1,
            end_time: TimeUs(1_000_000),
        }
    }
}
