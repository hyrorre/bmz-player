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

                let sample_frame = voice.sample_position.floor() as usize;
                if sample_frame >= sample.frame_count() {
                    alive = false;
                    break;
                }

                let (mut left, mut right) = sample.sample_stereo_linear(voice.sample_position);
                left *= voice.sound.volume * pan_left(voice.sound.pan);
                right *= voice.sound.volume * pan_right(voice.sound.pan);

                output[out_frame * 2] += left;
                output[out_frame * 2 + 1] += right;
                voice.sample_position += sample.sample_rate as f64 / self.output_sample_rate as f64;
            }

            if alive && voice.sample_position.floor() < sample.frame_count() as f64 {
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
        }]);
        let mut output = vec![0.0; 8];

        mixer.mix_stereo(&bank, 0, &mut output);

        assert_eq!(output, vec![0.25, 0.25, 0.375, 0.375, 0.5, 0.5, 0.625, 0.625]);
        assert!(!mixer.voices.is_empty());
    }
}
