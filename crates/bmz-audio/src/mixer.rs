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
    pub fade_out: Option<ActiveVoiceFadeOut>,
}

#[derive(Debug, Clone, Copy)]
pub struct ActiveVoiceFadeOut {
    pub started_output_frames: u64,
    pub frames: u32,
}

#[derive(Debug)]
pub struct MixerState {
    pub output_sample_rate: u32,
    /// Gameplay timeline speed. The play output owns a dedicated mixer, so this
    /// changes chart BGM/keysounds without affecting system UI sounds.
    pub playback_rate: f64,
    pub voices: Vec<ActiveVoice>,
    /// 全 voice 合成後に掛けるマスターゲイン。リザルト退出時のフェードアウト等、
    /// 出力全体を一括で絞るために使う。既定は 1.0(素通し)。
    pub master_gain: f32,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            output_sample_rate: 48_000,
            playback_rate: 1.0,
            voices: Vec::new(),
            master_gain: 1.0,
        }
    }
}

impl MixerState {
    pub fn new(output_sample_rate: u32) -> Self {
        Self { output_sample_rate, playback_rate: 1.0, voices: Vec::new(), master_gain: 1.0 }
    }

    pub fn push_scheduled(&mut self, sounds: impl IntoIterator<Item = ScheduledSound>) {
        for sound in sounds {
            match sound.restart_policy {
                RestartPolicy::StopSameSound => {
                    stop_same_sound_at(&mut self.voices, sound.sound_id, sound.start_frame);
                }
                RestartPolicy::LimitSameSound(max_voices) => {
                    if max_voices == 0 {
                        continue;
                    }
                    limit_same_sound(
                        &mut self.voices,
                        sound.sound_id,
                        max_voices,
                        voice_steal_fade_out_frames(self.output_sample_rate),
                    );
                }
                RestartPolicy::Overlap => {}
            }
            self.voices.push(ActiveVoice {
                next_output_frame: sound.start_frame,
                sample_position: sound.sample_offset_frames as f64,
                sound,
                played_output_frames: 0,
                started: false,
                stop_at_frame: None,
                fade_out: None,
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

    pub fn shift_output_frames(&mut self, frames: u64) {
        if frames == 0 {
            return;
        }
        for voice in &mut self.voices {
            voice.sound.start_frame = voice.sound.start_frame.saturating_add(frames);
            voice.next_output_frame = voice.next_output_frame.saturating_add(frames);
            if let Some(stop_at_frame) = voice.stop_at_frame.as_mut() {
                *stop_at_frame = stop_at_frame.saturating_add(frames);
            }
        }
    }

    pub fn apply_playback_rate_change(&mut self, change: crate::clock::PlaybackRateChange) {
        self.playback_rate = f64::from(change.new_rate_percent) / 100.0;
        for voice in &mut self.voices {
            if !voice.started {
                voice.sound.start_frame =
                    crate::queue::retime_output_frame(voice.sound.start_frame, change);
                voice.next_output_frame = voice.sound.start_frame;
            }
            if let Some(stop_at_frame) = voice.stop_at_frame.as_mut() {
                *stop_at_frame = crate::queue::retime_output_frame(*stop_at_frame, change);
            }
        }
    }

    pub fn stop_sound_with_fade_out(&mut self, id: bmz_core::ids::SoundId, fade_out_frames: u32) {
        if fade_out_frames == 0 {
            self.voices.retain(|voice| voice.sound.sound_id != id);
            return;
        }
        for voice in &mut self.voices {
            if voice.sound.sound_id == id && voice.fade_out.is_none() {
                voice.fade_out = Some(ActiveVoiceFadeOut {
                    started_output_frames: voice.played_output_frames,
                    frames: fade_out_frames,
                });
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
        let playback_rate = self.playback_rate;

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
            let native_rate = sample.sample_rate() == output_sample_rate && playback_rate == 1.0;
            let step = sample.sample_rate() as f64 / output_sample_rate as f64 * playback_rate;

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
                let Some(fade_out_gain) = fade_out_gain(voice.fade_out, voice.played_output_frames)
                else {
                    alive = false;
                    break;
                };

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
                let voice_gain = voice.sound.volume * fade_gain * fade_out_gain;
                left *= voice_gain * pan_left(voice.sound.pan);
                right *= voice_gain * pan_right(voice.sound.pan);

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

/// 同時発音数の上限で押し出した voice を止めるまでのフェード時間(10ms)。
/// 波形途中で 0 にするとクリックノイズが出るため、短く絞ってから止める。
fn voice_steal_fade_out_frames(output_sample_rate: u32) -> u32 {
    (output_sample_rate / 100).max(1)
}

fn limit_same_sound(
    voices: &mut Vec<ActiveVoice>,
    sound_id: bmz_core::ids::SoundId,
    max_voices: usize,
    fade_out_frames: u32,
) {
    // voice は古い順に並んでいる。`swap_remove` は順序を崩すため使わない。
    let is_active =
        |voice: &ActiveVoice| voice.sound.sound_id == sound_id && voice.fade_out.is_none();
    let mut active_count = voices.iter().filter(|voice| is_active(voice)).count();
    while active_count >= max_voices {
        let Some(index) = voices.iter().position(is_active) else {
            break;
        };
        if voices[index].started {
            // 最古の音はフェードアウト後に mix_stereo が破棄する。
            voices[index].fade_out = Some(ActiveVoiceFadeOut {
                started_output_frames: voices[index].played_output_frames,
                frames: fade_out_frames,
            });
        } else {
            // まだ鳴っていない voice は即時に破棄してもノイズにならない。
            voices.remove(index);
        }
        active_count -= 1;
    }

    // フェード中の voice も含めた総数が際限なく増えないよう、上限の 2 倍で古いものから破棄する。
    let hard_limit = max_voices.saturating_mul(2);
    let mut same_sound_count =
        voices.iter().filter(|voice| voice.sound.sound_id == sound_id).count();
    while same_sound_count >= hard_limit {
        let Some(index) = voices.iter().position(|voice| voice.sound.sound_id == sound_id) else {
            break;
        };
        voices.remove(index);
        same_sound_count -= 1;
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

fn fade_out_gain(fade_out: Option<ActiveVoiceFadeOut>, played_output_frames: u64) -> Option<f32> {
    let Some(fade_out) = fade_out else {
        return Some(1.0);
    };
    if fade_out.frames == 0 {
        return None;
    }
    let elapsed = played_output_frames.saturating_sub(fade_out.started_output_frames);
    if elapsed >= u64::from(fade_out.frames) {
        return None;
    }
    Some((1.0 - elapsed as f32 / fade_out.frames as f32).clamp(0.0, 1.0))
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
            sample_offset_frames: 0,
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
        let mut mixer = MixerState { master_gain: 0.25, ..MixerState::default() };
        mixer.push_scheduled([ScheduledSound {
            start_frame: 0,
            sample_offset_frames: 0,
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
            sample_offset_frames: 0,
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
            sample_offset_frames: 0,
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
    fn stop_sound_with_fade_out_ramps_active_voice() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0, 1.0, 1.0] },
        );
        let mut mixer = MixerState::default();
        mixer.push_scheduled([ScheduledSound {
            start_frame: 0,
            sample_offset_frames: 0,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: true,
            fade_in_frames: 0,
            catch_up: false,
            restart_policy: RestartPolicy::Overlap,
        }]);
        let mut output = vec![0.0; 2];
        mixer.mix_stereo(&bank, 0, &mut output);

        mixer.stop_sound_with_fade_out(SoundId(1), 2);
        let mut output = vec![0.0; 6];
        mixer.mix_stereo(&bank, 1, &mut output);

        assert_eq!(output, vec![1.0, 1.0, 0.5, 0.5, 0.0, 0.0]);
        assert!(mixer.voices.is_empty());
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
            sample_offset_frames: 0,
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
            sample_offset_frames: 0,
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
    fn sample_offset_and_late_catch_up_are_combined() {
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
            start_frame: 10,
            sample_offset_frames: 2,
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

        assert_eq!(output, vec![0.4, 0.4, 0.5, 0.5]);
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
            sample_offset_frames: 0,
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
            sample_offset_frames: 0,
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
    fn limit_same_sound_policy_keeps_latest_voices_up_to_limit() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0, 1.0] },
        );
        let mut mixer = MixerState::default();
        mixer.push_scheduled(
            (0..5).map(|_| scheduled_sound(0, 1, 1.0, RestartPolicy::LimitSameSound(3))),
        );

        assert_eq!(mixer.voices.len(), 3);

        let mut output = vec![0.0; 2];
        mixer.mix_stereo(&bank, 0, &mut output);

        assert_eq!(output, vec![3.0, 3.0]);
    }

    #[test]
    fn limit_same_sound_policy_fades_out_oldest_started_voice() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0; 2_000] },
        );
        let mut mixer = MixerState::default();
        let fade_frames = voice_steal_fade_out_frames(mixer.output_sample_rate) as usize;
        for _ in 0..3 {
            mixer.push_scheduled([scheduled_sound(0, 1, 1.0, RestartPolicy::LimitSameSound(3))]);
            mixer.mix_stereo(&bank, 0, &mut [0.0; 2]);
        }

        mixer.push_scheduled([scheduled_sound(0, 1, 1.0, RestartPolicy::LimitSameSound(3))]);

        // 押し出された最古の voice は即時に消えず、フェードアウトしながら残る。
        assert_eq!(mixer.voices.len(), 4);
        assert!(mixer.voices[0].fade_out.is_some());
        assert!(mixer.voices[1..].iter().all(|voice| voice.fade_out.is_none()));

        let mut output = vec![0.0; (fade_frames + 1) * 2];
        mixer.mix_stereo(&bank, 0, &mut output);

        assert_eq!(output[0], 4.0);
        let midpoint = output[fade_frames];
        assert!(midpoint > 3.0 && midpoint < 4.0, "midpoint gain was {midpoint}");
        assert_eq!(output[fade_frames * 2], 3.0);
        assert_eq!(mixer.voices.len(), 3);
    }

    #[test]
    fn limit_same_sound_policy_steals_in_start_order_around_other_sounds() {
        let mut bank = SampleBank::default();
        for id in [1, 2] {
            bank.insert(
                SoundId(id),
                DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0; 2_000] },
            );
        }
        let mut mixer = MixerState::default();
        // 音量を voice の識別子に使う。[A1, X, A2, A3] の並びを作る。
        for (sound_id, volume, policy) in [
            (1, 0.1, RestartPolicy::LimitSameSound(3)),
            (2, 1.0, RestartPolicy::Overlap),
            (1, 0.2, RestartPolicy::LimitSameSound(3)),
            (1, 0.3, RestartPolicy::LimitSameSound(3)),
            (1, 0.4, RestartPolicy::LimitSameSound(3)),
            (1, 0.5, RestartPolicy::LimitSameSound(3)),
        ] {
            mixer.push_scheduled([scheduled_sound(0, sound_id, volume, policy)]);
            mixer.mix_stereo(&bank, 0, &mut [0.0; 2]);
        }

        let fading = mixer
            .voices
            .iter()
            .filter(|voice| voice.sound.sound_id == SoundId(1) && voice.fade_out.is_some())
            .map(|voice| voice.sound.volume)
            .collect::<Vec<_>>();
        let active = mixer
            .voices
            .iter()
            .filter(|voice| voice.sound.sound_id == SoundId(1) && voice.fade_out.is_none())
            .map(|voice| voice.sound.volume)
            .collect::<Vec<_>>();
        assert_eq!(fading, vec![0.1, 0.2]);
        assert_eq!(active, vec![0.3, 0.4, 0.5]);
    }

    #[test]
    fn limit_same_sound_policy_caps_fading_voices() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0; 2_000] },
        );
        let mut mixer = MixerState::default();
        for _ in 0..10 {
            mixer.push_scheduled([scheduled_sound(0, 1, 1.0, RestartPolicy::LimitSameSound(3))]);
            mixer.mix_stereo(&bank, 0, &mut [0.0; 2]);
        }

        assert_eq!(mixer.voices.len(), 6);
        assert_eq!(mixer.voices.iter().filter(|voice| voice.fade_out.is_none()).count(), 3);
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
            sample_offset_frames: 0,
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
