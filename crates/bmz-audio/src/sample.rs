use bmz_core::ids::SoundId;

#[derive(Debug, Clone)]
pub struct DecodedSample {
    pub channels: u16,
    pub sample_rate: u32,
    pub frames: Vec<f32>,
}

#[derive(Debug, Default)]
pub struct SampleBank {
    samples: Vec<Option<DecodedSample>>,
}

impl SampleBank {
    pub fn insert(&mut self, id: SoundId, sample: DecodedSample) {
        let index = id.0 as usize;
        if self.samples.len() <= index {
            self.samples.resize_with(index + 1, || None);
        }
        self.samples[index] = Some(sample);
    }

    pub fn get(&self, id: SoundId) -> Option<&DecodedSample> {
        self.samples.get(id.0 as usize)?.as_ref()
    }
}

impl DecodedSample {
    pub fn frame_count(&self) -> usize {
        if self.channels == 0 { 0 } else { self.frames.len() / self.channels as usize }
    }

    pub fn sample_stereo(&self, frame: usize) -> (f32, f32) {
        match self.channels {
            0 => (0.0, 0.0),
            1 => {
                let value = self.frames.get(frame).copied().unwrap_or(0.0);
                (value, value)
            }
            _ => {
                let index = frame * self.channels as usize;
                (
                    self.frames.get(index).copied().unwrap_or(0.0),
                    self.frames.get(index + 1).copied().unwrap_or(0.0),
                )
            }
        }
    }

    pub fn sample_stereo_linear(&self, position: f64) -> (f32, f32) {
        let frame = position.floor().max(0.0) as usize;
        let frac = (position - frame as f64) as f32;
        let (left_a, right_a) = self.sample_stereo(frame);
        let (left_b, right_b) = self.sample_stereo(frame + 1);
        (lerp(left_a, left_b, frac), lerp(right_a, right_b, frac))
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_bank_returns_inserted_sample() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(2),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5] },
        );

        assert_eq!(bank.get(SoundId(2)).unwrap().sample_stereo(0), (0.5, 0.5));
        assert!(bank.get(SoundId(1)).is_none());
    }

    #[test]
    fn sample_stereo_linear_interpolates_between_frames() {
        let sample = DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.0, 1.0] };

        assert_eq!(sample.sample_stereo_linear(0.5), (0.5, 0.5));
    }
}
