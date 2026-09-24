//! The renderer-independent owner of one chart's mutable gameplay state.
//! Audio callbacks consume commands only; all judgement stays on this owner.
use bmz_audio::command::AudioEngineHandle;
use bmz_audio::queue::ScheduledSoundQueue;
use bmz_core::ids::SoundId;

use crate::session::{GameSession, SessionFrame, advance_session_frame};

pub struct GameplayRuntime {
    pub session: GameSession,
    pub pending_audio: ScheduledSoundQueue,
    pub pending_keysound_volumes: Vec<(SoundId, f32)>,
}

impl GameplayRuntime {
    /// Bound the sleep by chart events in addition to the safety wake. Input
    /// arrivals wake the owner separately through InputBackend::set_waker.
    pub fn next_wake_after(&self, safety: std::time::Duration) -> std::time::Duration {
        let session = &self.session;
        if !session.audio_clock.running {
            return safety;
        }
        let now = session.audio_clock.now().0;
        let mut next_us = safety.as_micros().min(i64::MAX as u128) as i64;
        let rate = i64::from(session.audio_clock.playback_rate_percent().max(1));
        let mut consider = |deadline: i64| {
            if deadline > now {
                next_us = next_us.min(deadline.saturating_sub(now).saturating_mul(100) / rate);
            }
        };
        consider(0);
        if let Some(time) = crate::session::conditional::next_evaluation(session) {
            consider(time.0);
        }
        for lane in bmz_core::lane::Lane::ALL {
            let state = &session.judge.lanes[lane.index()];
            if let Some(note) = session.chart.notes_for_lane(lane).get(state.next_note_index) {
                consider(note.time.0);
                consider(
                    note.time.0.saturating_add(session.judge.windows.bad_slow_us).saturating_add(1),
                );
            }
            if let Some(long) = state.active_long {
                consider(session.chart.long_notes[long.pair_index].end_time.0);
            }
            if let Some(release) = session.lane_auto_release_at[lane.index()] {
                consider(release.0);
            }
        }
        if let Some(replay) = &session.replay_player
            && let Some(event) = replay.events.get(replay.next_index)
        {
            consider(event.time.0);
        }
        if let Some(bgm) = session.chart.bgm_events.get(session.bgm_scheduler.next_index) {
            consider(bgm.time.0.saturating_sub(crate::session::AUDIO_SCHEDULE_AHEAD_US));
        }
        std::time::Duration::from_micros(next_us.max(100) as u64).min(safety)
    }
    pub fn new(session: GameSession) -> Self {
        Self {
            session,
            pending_audio: ScheduledSoundQueue::new(),
            pending_keysound_volumes: Vec::new(),
        }
    }

    pub fn advance(&mut self, audio: &AudioEngineHandle) -> SessionFrame {
        if !self.session.audio_clock.running {
            // Viewer pause consumes no gameplay input and advances no deadlines.
            self.session.input_system.backend.drain_events();
            return SessionFrame {
                times: crate::session::compute_frame_times(&self.session),
                judgements: Vec::new(),
                mine_hits: Vec::new(),
                keysound_volumes: Vec::new(),
                skin_events: Vec::new(),
                state: self.session.state,
            };
        }
        if matches!(
            self.session.state,
            crate::session::PlayState::Finished | crate::session::PlayState::Failed
        ) {
            let now = self.session.audio_clock.now();
            crate::session::apply_auto_key_release(&mut self.session, now);
            crate::session::update_recent_inputs(&mut self.session, &[], now);
            crate::session::update_recent_judgements(&mut self.session, &[], now);
        }
        let frame = advance_session_frame(&mut self.session, &mut self.pending_audio);
        for &(id, volume) in &frame.keysound_volumes {
            if let Some((_, pending)) =
                self.pending_keysound_volumes.iter_mut().find(|(pending_id, _)| *pending_id == id)
            {
                *pending = volume;
            } else {
                self.pending_keysound_volumes.push((id, volume));
            }
        }
        self.flush_audio(audio);
        frame
    }

    pub fn flush_audio(&mut self, audio: &AudioEngineHandle) {
        if !self.pending_audio.is_empty() {
            self.pending_audio.retain(|sound| {
                if audio.schedule_sound(*sound) {
                    crate::session::latency::audio_enqueued();
                    false
                } else {
                    true
                }
            });
        }
        self.pending_keysound_volumes.retain(|&(id, volume)| !audio.set_sound_volume(id, volume));
    }
}
