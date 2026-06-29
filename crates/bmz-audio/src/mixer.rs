use crate::queue::{RestartPolicy, ScheduledSound};
use crate::sample::SampleBank;

#[derive(Debug, Clone)]
pub struct ActiveVoice {
    pub sound: ScheduledSound,
    pub sample_position: f64,
    pub played_output_frames: u64,
    pub next_output_frame: u64,
    pub started: bool,
    pub stop_at_frame: Option<u64>,
}

#[derive(Debug)]
pub struct MixerState {
    pub output_sample_rate: u32,
    pub voices: Vec<ActiveVoice>,
    /// 全 voice 合成後に掛けるマスターゲイン。リザルト退出時のフェードアウト等、
    /// 出力全体を一括で絞るために使う。既定は 1.0(素通し)。
    pub master_gain: f32,
}

impl Default for MixerState {
    fn default() -> Self {
        Self { output_sample_rate: 48_000, voices: Vec::new(), master_gain: 1.0 }
    }
}

impl MixerState {
    pub fn new(output_sample_rate: u32) -> Self {
        Self { output_sample_rate, voices: Vec::new(), master_gain: 1.0 }
    }

    pub fn push_scheduled(&mut self, sounds: impl IntoIterator<Item = ScheduledSound>) {
        for sound in sounds {
            if sound.restart_policy == RestartPolicy::StopSameSound {
                stop_same_sound_at(&mut self.voices, sound.sound_id, sound.start_frame);
            }
            self.voices.push(ActiveVoice {
                next_output_frame: sound.start_frame,
                sound,
                sample_position: 0.0,
                played_output_frames: 0,
                started: false,
                stop_at_frame: None,
            });
        }
    }

    pub fn set_volume_for_sound(&mut self, id: bmz_core::ids::SoundId, volume: f32) {
        for voice in &mut self.voices {
            if voice.sound.sound_id == id {
                voice.sound.volume = volume;
            }
        }
    }

    pub fn mix_stereo(
        &mut self,
        sample_bank: &SampleBank,
        output_start_frame: u64,
        output: &mut [f32],
    ) {
        debug_assert_eq!(output.len() % 2, 0);

        let frame_count = output.len() / 2;
        // オーディオコールバックから毎回呼ばれるホットパス。`retain_mut` で
        // voice 配列を in-place 更新し、毎コールバックのヒープ確保
        // (旧 `Vec::with_capacity` + voices 差し替え)を排してリアルタイム安全にする。
        let output_sample_rate = self.output_sample_rate;

        self.voices.retain_mut(|voice| {
            let Some(sample) = sample_bank.get(voice.sound.sound_id) else {
                return false;
            };
            if voice.stop_at_frame.is_some_and(|stop_frame| output_start_frame >= stop_frame) {
                return false;
            }

            let mut alive = true;
            let sample_frames = sample.frame_count();
            // 読込時リサンプル後はサンプルが出力レートと一致するため、補間不要の
            // ファストパス(整数インデックスで直接読み出し)で再生できる。
            let native_rate = sample.sample_rate == output_sample_rate;
            let step = sample.sample_rate as f64 / output_sample_rate as f64;

            if voice.started {
                if output_start_frame > voice.next_output_frame {
                    let missed_frames = output_start_frame - voice.next_output_frame;
                    if !advance_voice_position(voice, missed_frames, step, sample_frames) {
                        return false;
                    }
                    voice.next_output_frame = output_start_frame;
                }
            } else if voice.sound.catch_up && output_start_frame > voice.sound.start_frame {
                let missed_frames = output_start_frame - voice.sound.start_frame;
                if !advance_voice_position(voice, missed_frames, step, sample_frames) {
                    return false;
                }
                voice.started = true;
                voice.next_output_frame = output_start_frame;
            }

            for out_frame in 0..frame_count {
                let absolute_frame = output_start_frame + out_frame as u64;
                if voice.stop_at_frame.is_some_and(|stop_frame| absolute_frame >= stop_frame) {
                    alive = false;
                    break;
                }
                if !voice.started && absolute_frame < voice.sound.start_frame {
                    continue;
                }
                if !voice.started {
                    voice.started = true;
                    voice.next_output_frame = absolute_frame;
                }

                let sample_frame = voice.sample_position.floor() as usize;
                if sample_frame >= sample_frames {
                    if voice.sound.loop_playback && sample_frames > 0 {
                        // サンプル末尾まで到達したらループ。先頭からの相対位置を保つ。
                        let frames_f = sample_frames as f64;
                        let wrapped = voice.sample_position.rem_euclid(frames_f);
                        voice.sample_position = wrapped;
                    } else {
                        alive = false;
                        break;
                    }
                }

                let (mut left, mut right) = if native_rate {
                    sample.sample_stereo(voice.sample_position as usize)
                } else {
                    sample.sample_stereo_linear(voice.sample_position)
                };
                let fade_gain =
                    fade_in_gain(voice.sound.fade_in_frames, voice.played_output_frames);
                left *= voice.sound.volume * fade_gain * pan_left(voice.sound.pan);
                right *= voice.sound.volume * fade_gain * pan_right(voice.sound.pan);

                output[out_frame * 2] += left;
                output[out_frame * 2 + 1] += right;
                voice.sample_position += step;
                voice.played_output_frames = voice.played_output_frames.saturating_add(1);
                voice.next_output_frame = absolute_frame.saturating_add(1);
            }

            // ループ voice はサンプル末尾を超えても破棄しない。
            let still_playing =
                voice.sound.loop_playback || voice.sample_position.floor() < sample_frames as f64;
            alive && still_playing
        });

        // マスターゲインは全 voice 合成後に一括適用する。素通し時は走査を省く。
        if self.master_gain != 1.0 {
            for sample in output.iter_mut() {
                *sample *= self.master_gain;
            }
        }
    }
}

fn stop_same_sound_at(
    voices: &mut [ActiveVoice],
    sound_id: bmz_core::ids::SoundId,
    stop_frame: u64,
) {
    for voice in voices {
        if voice.sound.sound_id == sound_id {
            voice.stop_at_frame =
                Some(voice.stop_at_frame.map_or(stop_frame, |existing| existing.min(stop_frame)));
        }
    }
}

fn advance_voice_position(
    voice: &mut ActiveVoice,
    frames: u64,
    step: f64,
    sample_frames: usize,
) -> bool {
    if frames == 0 {
        return true;
    }
    if sample_frames == 0 {
        return false;
    }

    voice.sample_position += frames as f64 * step;
    voice.played_output_frames = voice.played_output_frames.saturating_add(frames);
    if voice.sound.loop_playback {
        voice.sample_position = voice.sample_position.rem_euclid(sample_frames as f64);
        true
    } else {
        voice.sample_position.floor() < sample_frames as f64
    }
}

fn fade_in_gain(fade_in_frames: u32, played_output_frames: u64) -> f32 {
    if fade_in_frames == 0 {
        return 1.0;
    }
    (played_output_frames as f32 / fade_in_frames as f32).clamp(0.0, 1.0)
}

fn pan_left(pan: f32) -> f32 {
    if pan <= 0.0 { 1.0 } else { 1.0 - pan.clamp(0.0, 1.0) }
}

fn pan_right(pan: f32) -> f32 {
    if pan >= 0.0 { 1.0 } else { 1.0 + pan.clamp(-1.0, 0.0) }
}

#[cfg(test)]
mod tests {
    use bmz_core::ids::SoundId;

    use crate::queue::ScheduledSound;
    use crate::sample::{DecodedSample, SampleBank};

    use super::*;

    #[test]
    fn mix_stereo_adds_active_voice_samples() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.25, 0.5] },
        );
        let mut mixer = MixerState::default();
        mixer.push_scheduled([ScheduledSound {
            start_frame: 10,
            sound_id: SoundId(1),
            volume: 2.0,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: true,
            restart_policy: RestartPolicy::Overlap,
        }]);
        let mut output = vec![0.0; 6];

        mixer.mix_stereo(&bank, 9, &mut output);

        assert_eq!(output, vec![0.0, 0.0, 0.5, 0.5, 1.0, 1.0]);
        assert!(mixer.voices.is_empty());
    }

    #[test]
    fn master_gain_scales_mixed_output() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 0.5] },
        );
        let mut mixer = MixerState::default();
        mixer.master_gain = 0.25;
        mixer.push_scheduled([ScheduledSound {
            start_frame: 0,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: false,
            restart_policy: RestartPolicy::Overlap,
        }]);
        let mut output = vec![0.0; 4];

        mixer.mix_stereo(&bank, 0, &mut output);

        // master_gain は全 voice 合成後に一括で掛かる。
        assert_eq!(output, vec![0.25, 0.25, 0.125, 0.125]);
    }

    #[test]
    fn mix_stereo_loops_sample_when_loop_playback_set() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5, 0.25] },
        );
        let mut mixer = MixerState::default();
        mixer.push_scheduled([ScheduledSound {
            start_frame: 0,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: true,
            fade_in_frames: 0,
            catch_up: false,
            restart_policy: RestartPolicy::Overlap,
        }]);
        let mut output = vec![0.0; 12]; // 6 frames, 3 ループ分

        mixer.mix_stereo(&bank, 0, &mut output);

        // [0.5, 0.25, 0.5, 0.25, 0.5, 0.25] が左右に複製される。
        assert_eq!(output, vec![0.5, 0.5, 0.25, 0.25, 0.5, 0.5, 0.25, 0.25, 0.5, 0.5, 0.25, 0.25]);
        // ループ voice はサンプル末尾を超えても残る。
        assert!(!mixer.voices.is_empty());
    }

    #[test]
    fn fade_in_applies_once_to_looping_voice() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0] },
        );
        let mut mixer = MixerState::default();
        mixer.push_scheduled([ScheduledSound {
            start_frame: 0,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: true,
            fade_in_frames: 2,
            catch_up: false,
            restart_policy: RestartPolicy::Overlap,
        }]);
        let mut output = vec![0.0; 12];

        mixer.mix_stereo(&bank, 0, &mut output);

        assert_eq!(output, vec![0.0, 0.0, 0.5, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn mix_stereo_advances_by_sample_rate_ratio() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 24_000, frames: vec![0.25, 0.5, 0.75] },
        );
        let mut mixer = MixerState::new(48_000);
        mixer.push_scheduled([ScheduledSound {
            start_frame: 0,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: true,
            restart_policy: RestartPolicy::Overlap,
        }]);
        let mut output = vec![0.0; 8];

        mixer.mix_stereo(&bank, 0, &mut output);

        assert_eq!(output, vec![0.25, 0.25, 0.375, 0.375, 0.5, 0.5, 0.625, 0.625]);
        assert!(!mixer.voices.is_empty());
    }

    #[test]
    fn timeline_sound_catches_up_when_rendered_late() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.25, 0.5, 0.75, 1.0] },
        );
        let mut mixer = MixerState::default();
        mixer.push_scheduled([ScheduledSound {
            start_frame: 10,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: true,
            restart_policy: RestartPolicy::Overlap,
        }]);
        let mut output = vec![0.0; 4];

        mixer.mix_stereo(&bank, 12, &mut output);

        assert_eq!(output, vec![0.75, 0.75, 1.0, 1.0]);
        assert!(mixer.voices.is_empty());
    }

    #[test]
    fn immediate_sound_starts_from_head_when_first_render_is_late() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.25, 0.5] },
        );
        let mut mixer = MixerState::default();
        mixer.push_scheduled([ScheduledSound {
            start_frame: 0,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: false,
            restart_policy: RestartPolicy::Overlap,
        }]);
        let mut output = vec![0.0; 4];

        mixer.mix_stereo(&bank, 100, &mut output);

        assert_eq!(output, vec![0.25, 0.25, 0.5, 0.5]);
        assert!(mixer.voices.is_empty());
    }

    #[test]
    fn active_voice_advances_across_skipped_output_frames() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample {
                channels: 1,
                sample_rate: 48_000,
                frames: vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5],
            },
        );
        let mut mixer = MixerState::default();
        mixer.push_scheduled([ScheduledSound {
            start_frame: 100,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: false,
            restart_policy: RestartPolicy::Overlap,
        }]);

        let mut first = vec![0.0; 4];
        mixer.mix_stereo(&bank, 100, &mut first);
        let mut after_skip = vec![0.0; 4];
        mixer.mix_stereo(&bank, 104, &mut after_skip);

        assert_eq!(first, vec![0.0, 0.0, 0.1, 0.1]);
        assert_eq!(after_skip, vec![0.4, 0.4, 0.5, 0.5]);
        assert!(mixer.voices.is_empty());
    }

    #[test]
    fn overlap_policy_keeps_same_sound_voices_mixed() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0, 1.0] },
        );
        let mut mixer = MixerState::default();
        mixer.push_scheduled([
            scheduled_sound(0, 1, 1.0, RestartPolicy::Overlap),
            scheduled_sound(1, 1, 1.0, RestartPolicy::Overlap),
        ]);
        let mut output = vec![0.0; 8];

        mixer.mix_stereo(&bank, 0, &mut output);

        assert_eq!(output, vec![1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 1.0, 1.0]);
        assert!(mixer.voices.is_empty());
    }

    #[test]
    fn stop_same_sound_policy_restarts_at_later_frame() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 0.5, 0.25, 0.125] },
        );
        let mut mixer = MixerState::default();
        mixer.push_scheduled([
            scheduled_sound(0, 1, 1.0, RestartPolicy::StopSameSound),
            scheduled_sound(2, 1, 1.0, RestartPolicy::StopSameSound),
        ]);
        let mut output = vec![0.0; 12];

        mixer.mix_stereo(&bank, 0, &mut output);

        assert_eq!(output, vec![1.0, 1.0, 0.5, 0.5, 1.0, 1.0, 0.5, 0.5, 0.25, 0.25, 0.125, 0.125]);
        assert!(mixer.voices.is_empty());
    }

    #[test]
    fn stop_same_sound_policy_keeps_last_same_frame_sound() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0] },
        );
        let mut mixer = MixerState::default();
        mixer.push_scheduled([
            scheduled_sound(0, 1, 0.25, RestartPolicy::StopSameSound),
            scheduled_sound(0, 1, 1.0, RestartPolicy::StopSameSound),
        ]);
        let mut output = vec![0.0; 4];

        mixer.mix_stereo(&bank, 0, &mut output);

        assert_eq!(output, vec![1.0, 1.0, 1.0, 1.0]);
        assert!(mixer.voices.is_empty());
    }

    #[test]
    fn stop_same_sound_policy_keeps_different_sounds_mixed() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0, 1.0] },
        );
        bank.insert(
            SoundId(2),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.25, 0.25, 0.25] },
        );
        let mut mixer = MixerState::default();
        mixer.push_scheduled([
            scheduled_sound(0, 1, 1.0, RestartPolicy::StopSameSound),
            scheduled_sound(1, 2, 1.0, RestartPolicy::StopSameSound),
        ]);
        let mut output = vec![0.0; 8];

        mixer.mix_stereo(&bank, 0, &mut output);

        assert_eq!(output, vec![1.0, 1.0, 1.25, 1.25, 1.25, 1.25, 0.25, 0.25]);
        assert!(mixer.voices.is_empty());
    }

    fn scheduled_sound(
        start_frame: u64,
        sound_id: u32,
        volume: f32,
        restart_policy: RestartPolicy,
    ) -> ScheduledSound {
        ScheduledSound {
            start_frame,
            sound_id: SoundId(sound_id),
            volume,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: false,
            restart_policy,
        }
    }
}
