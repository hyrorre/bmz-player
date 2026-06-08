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

    /// 保持中の全サンプルを `target_rate` へリサンプルする。出力レート変更時に
    /// 呼ばれ、ミキサー側でのリアルタイムリサンプルを不要にする。
    pub fn resample_all_to(&mut self, target_rate: u32) {
        for slot in self.samples.iter_mut().flatten() {
            if slot.sample_rate != target_rate {
                *slot = slot.resampled_to(target_rate);
            }
        }
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

    pub fn apply_gain(&mut self, gain: f32) {
        if gain == 1.0 {
            return;
        }
        for frame in &mut self.frames {
            *frame *= gain;
        }
    }

    /// `target_rate` へ線形補間でリサンプルした新しいサンプルを返す。
    /// 既に同レート、または無効なサンプルはそのまま複製する。
    /// beatoraja 同様、再生時ではなく読込時に出力レートへ揃えることで、
    /// オーディオコールバックでの逐次リサンプルコストを無くす。
    pub fn resampled_to(&self, target_rate: u32) -> DecodedSample {
        let channels = self.channels as usize;
        if target_rate == 0 || self.sample_rate == 0 || channels == 0 {
            return self.clone();
        }
        let src_frames = self.frame_count();
        if self.sample_rate == target_rate || src_frames == 0 {
            return DecodedSample {
                channels: self.channels,
                sample_rate: target_rate,
                frames: self.frames.clone(),
            };
        }

        // 出力フレーム数 = 入力フレーム数 * target / src
        let dst_frames =
            (src_frames as u64 * target_rate as u64 / self.sample_rate as u64).max(1) as usize;
        // src を進める歩幅(出力1フレームあたりの入力フレーム数)。
        let step = self.sample_rate as f64 / target_rate as f64;
        let mut frames = Vec::with_capacity(dst_frames * channels);
        for i in 0..dst_frames {
            let pos = i as f64 * step;
            let idx = pos.floor() as usize;
            let frac = (pos - idx as f64) as f32;
            let base = idx * channels;
            let next = base + channels;
            for c in 0..channels {
                let a = self.frames.get(base + c).copied().unwrap_or(0.0);
                let b = self.frames.get(next + c).copied().unwrap_or(a);
                frames.push(a + (b - a) * frac);
            }
        }
        DecodedSample { channels: self.channels, sample_rate: target_rate, frames }
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

    #[test]
    fn resampled_to_upsamples_with_linear_interpolation() {
        let sample = DecodedSample { channels: 1, sample_rate: 24_000, frames: vec![0.0, 1.0] };

        let resampled = sample.resampled_to(48_000);

        assert_eq!(resampled.sample_rate, 48_000);
        assert_eq!(resampled.channels, 1);
        assert_eq!(resampled.frames, vec![0.0, 0.5, 1.0, 1.0]);
    }

    #[test]
    fn resampled_to_same_rate_keeps_frames() {
        let sample = DecodedSample { channels: 2, sample_rate: 48_000, frames: vec![0.1, 0.2] };

        let resampled = sample.resampled_to(48_000);

        assert_eq!(resampled.sample_rate, 48_000);
        assert_eq!(resampled.frames, vec![0.1, 0.2]);
    }

    #[test]
    fn resample_all_to_converts_each_sample() {
        let mut bank = SampleBank::default();
        bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 24_000, frames: vec![0.0, 1.0] },
        );

        bank.resample_all_to(48_000);

        let sample = bank.get(SoundId(1)).unwrap();
        assert_eq!(sample.sample_rate, 48_000);
        assert_eq!(sample.frames, vec![0.0, 0.5, 1.0, 1.0]);
    }
}
