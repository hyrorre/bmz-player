use crate::queue::ScheduledSound;
use crate::sample::SampleBank;

#[derive(Debug, Clone)]
pub struct ActiveVoice {
    pub sound: ScheduledSound,
    pub sample_position: f64,
}

#[derive(Debug)]
pub struct MixerState {
    pub output_sample_rate: u32,
    pub voices: Vec<ActiveVoice>,
}

impl Default for MixerState {
    fn default() -> Self {
        Self { output_sample_rate: 48_000, voices: Vec::new() }
    }
}

impl MixerState {
    pub fn new(output_sample_rate: u32) -> Self {
        Self { output_sample_rate, voices: Vec::new() }
    }

    pub fn push_scheduled(&mut self, sounds: impl IntoIterator<Item = ScheduledSound>) {
        self.voices
            .extend(sounds.into_iter().map(|sound| ActiveVoice { sound, sample_position: 0.0 }));
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

            let mut alive = true;
            let sample_frames = sample.frame_count();
            // 読込時リサンプル後はサンプルが出力レートと一致するため、補間不要の
            // ファストパス(整数インデックスで直接読み出し)で再生できる。
            let native_rate = sample.sample_rate == output_sample_rate;
            let step = sample.sample_rate as f64 / output_sample_rate as f64;
            for out_frame in 0..frame_count {
                let absolute_frame = output_start_frame + out_frame as u64;
                if absolute_frame < voice.sound.start_frame {
                    continue;
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
                left *= voice.sound.volume * pan_left(voice.sound.pan);
                right *= voice.sound.volume * pan_right(voice.sound.pan);

                output[out_frame * 2] += left;
                output[out_frame * 2 + 1] += right;
                voice.sample_position += step;
            }

            // ループ voice はサンプル末尾を超えても破棄しない。
            let still_playing =
                voice.sound.loop_playback || voice.sample_position.floor() < sample_frames as f64;
            alive && still_playing
        });
    }
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
        }]);
        let mut output = vec![0.0; 6];

        mixer.mix_stereo(&bank, 9, &mut output);

        assert_eq!(output, vec![0.0, 0.0, 0.5, 0.5, 1.0, 1.0]);
        assert!(mixer.voices.is_empty());
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
        }]);
        let mut output = vec![0.0; 12]; // 6 frames, 3 ループ分

        mixer.mix_stereo(&bank, 0, &mut output);

        // [0.5, 0.25, 0.5, 0.25, 0.5, 0.25] が左右に複製される。
        assert_eq!(output, vec![0.5, 0.5, 0.25, 0.25, 0.5, 0.5, 0.25, 0.25, 0.5, 0.5, 0.25, 0.25]);
        // ループ voice はサンプル末尾を超えても残る。
        assert!(!mixer.voices.is_empty());
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
        }]);
        let mut output = vec![0.0; 8];

        mixer.mix_stereo(&bank, 0, &mut output);

        assert_eq!(output, vec![0.25, 0.25, 0.375, 0.375, 0.5, 0.5, 0.625, 0.625]);
        assert!(!mixer.voices.is_empty());
    }
}
