/// 譜面時刻でキー音を先読みする経路を使うか。
/// `auto_keysound` 設定に加え、フルオートも同じ経路にする
/// (判定バッチ遅延の catch_up でアタックが欠けるのを避ける)。
pub(super) fn uses_scheduled_auto_keysounds(session: &GameSession) -> bool {
    session.audio_mix.auto_keysound
        || session.autoplay.as_ref().is_some_and(AutoplayController::is_full)
}

pub fn schedule_keysounds(session: &mut GameSession, audio: &mut dyn AudioScheduler) {
    let mut pending = std::mem::take(&mut session.pending_keysounds);
    if uses_scheduled_auto_keysounds(session) {
        session.pending_keysound_volumes.clear();
        let allow_fallback = session.audio_mix.auto_keysound_fallback;
        let allow_mine = session.audio_mix.auto_keysound_mine;
        // フルオートは設定OFFでも判定キー音を抑止し、schedule-ahead 側だけ鳴らす。
        let full_auto = session.autoplay.as_ref().is_some_and(AutoplayController::is_full);
        pending.retain(|event| match event.trigger {
            KeySoundTrigger::NoteJudged | KeySoundTrigger::Miss => false,
            KeySoundTrigger::Fallback => allow_fallback && !full_auto,
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
        // Judgement / press keysounds must start from the sample head. Using
        // catch_up:true seeks into the WAV when the command reaches the audio
        // callback a few ms late, which guts short kick/bass attacks and
        // sounds like a pitch change. A few ms late from-head is preferable
        // and keeps key-press latency low (no extra buffering).
        for sound_id in note.sounds() {
            audio.schedule(ScheduledSound {
                start_frame: session.audio_clock.time_to_output_frame(event.time),
                sample_offset_frames: 0,
                sound_id,
                volume,
                pan: 0.0,
                loop_playback: false,
                fade_in_frames: 0,
                catch_up: false,
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
