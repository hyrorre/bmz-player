pub fn schedule_keysounds(session: &mut GameSession, audio: &mut dyn AudioScheduler) {
    let mut pending = std::mem::take(&mut session.pending_keysounds);
    if session.audio_mix.auto_keysound {
        session.pending_keysound_volumes.clear();
        let allow_fallback = session.audio_mix.auto_keysound_fallback;
        let allow_mine = session.audio_mix.auto_keysound_mine;
        pending.retain(|event| match event.trigger {
            KeySoundTrigger::NoteJudged => false,
            KeySoundTrigger::Fallback => allow_fallback,
            KeySoundTrigger::Mine => allow_mine,
        });
    }
    for event in pending {
        let note_id = event.note_id;
        let Some(note) = session.chart.note_by_id(note_id) else {
            continue;
        };

        let chart_volume = bmz_chart::volume::chart_channel_volume_factor(
            bmz_chart::volume::chart_volume_at_time(&session.chart.key_volume_events, event.time),
        );
        let volume = (session.audio_mix.master_volume
            * session.audio_mix.effective_normalization_gain()
            * session.audio_mix.key_volume
            * chart_volume)
            .clamp(0.0, 1.0);
        for sound_id in note.sounds() {
            audio.schedule(ScheduledSound {
                start_frame: session.audio_clock.time_to_output_frame(event.time),
                sample_offset_frames: 0,
                sound_id,
                volume,
                pan: 0.0,
                loop_playback: false,
                fade_in_frames: 0,
                catch_up: true,
                restart_policy: RestartPolicy::StopSameSound,
            });
        }
    }
}

fn plays_keysound(judge: Judge) -> bool {
    matches!(judge, Judge::PGreat | Judge::Great | Judge::Good | Judge::Bad)
}

pub(super) fn counts_for_input_offset_auto_adjust(judge: Judge) -> bool {
    plays_keysound(judge)
}
use super::*;
