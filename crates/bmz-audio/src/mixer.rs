use crate::queue::ScheduledSound;
use crate::sample::SampleBank;

#[derive(Debug, Clone)]
pub struct ActiveVoice {
    pub sound: ScheduledSound,
    pub sample_frame: usize,
}

#[derive(Debug, Default)]
pub struct MixerState {
    pub voices: Vec<ActiveVoice>,
}

impl MixerState {
    pub fn push_scheduled(&mut self, sounds: impl IntoIterator<Item = ScheduledSound>) {
        self.voices.extend(sounds.into_iter().map(|sound| ActiveVoice { sound, sample_frame: 0 }));
    }

    pub fn mix_stereo(
        &mut self,
        sample_bank: &SampleBank,
        output_start_frame: u64,
        output: &mut [f32],
    ) {
        debug_assert_eq!(output.len() % 2, 0);

        let frame_count = output.len() / 2;
        let mut retained = Vec::with_capacity(self.voices.len());

        for mut voice in self.voices.drain(..) {
            let Some(sample) = sample_bank.get(voice.sound.sound_id) else {
                continue;
            };

            let mut alive = true;
            for out_frame in 0..frame_count {
                let absolute_frame = output_start_frame + out_frame as u64;
                if absolute_frame < voice.sound.start_frame {
                    continue;
                }

                if voice.sample_frame >= sample.frame_count() {
                    alive = false;
                    break;
                }

                let (mut left, mut right) = sample.sample_stereo(voice.sample_frame);
                left *= voice.sound.volume * pan_left(voice.sound.pan);
                right *= voice.sound.volume * pan_right(voice.sound.pan);

                output[out_frame * 2] += left;
                output[out_frame * 2 + 1] += right;
                voice.sample_frame += 1;
            }

            if alive && voice.sample_frame < sample.frame_count() {
                retained.push(voice);
            }
        }

        self.voices = retained;
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
        }]);
        let mut output = vec![0.0; 6];

        mixer.mix_stereo(&bank, 9, &mut output);

        assert_eq!(output, vec![0.0, 0.0, 0.5, 0.5, 1.0, 1.0]);
        assert!(mixer.voices.is_empty());
    }
}
