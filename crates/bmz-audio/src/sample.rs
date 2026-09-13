use std::collections::HashSet;
use std::sync::Arc;

use bmz_core::ids::SoundId;

/// Generous per-source headroom (+18 dBFS), not a clipping ceiling for a mix.
/// Vorbis overshoot above unity is valid; values beyond this are treated as
/// corrupt PCM. Do not apply this threshold to the sum of layered voices.
pub const MAX_PCM_AMPLITUDE: f32 = 8.0;

pub(crate) fn safe_pcm(value: f32) -> f32 {
    if value.is_finite() && value.abs() <= MAX_PCM_AMPLITUDE { value } else { 0.0 }
}

#[derive(Debug, Clone)]
pub struct DecodedSample {
    pub channels: u16,
    pub sample_rate: u32,
    pub frames: Vec<f32>,
}

/// 1 つのデコード済み PCM 内の再生範囲。
///
/// BMSON の `sound_channel` は同じ音声ファイルを複数の `c=true` ノートへ
/// 分割できるため、`SoundId` ごとに PCM を複製せず共有元と frame 範囲だけを持つ。
#[derive(Debug, Clone)]
pub struct SampleRegion {
    source: Arc<DecodedSample>,
    start_frame: usize,
    end_frame: usize,
}

impl SampleRegion {
    fn new(source: Arc<DecodedSample>, start_frame: usize, end_frame: usize) -> Self {
        let source_frames = source.frame_count();
        let start_frame = start_frame.min(source_frames);
        let end_frame = end_frame.clamp(start_frame, source_frames);
        Self { source, start_frame, end_frame }
    }

    pub fn sample_rate(&self) -> u32 {
        self.source.sample_rate
    }

    pub fn frame_count(&self) -> usize {
        self.end_frame - self.start_frame
    }

    pub fn sample_stereo(&self, frame: usize) -> (f32, f32) {
        if frame >= self.frame_count() {
            return (0.0, 0.0);
        }
        self.source.sample_stereo(self.start_frame + frame)
    }

    /// Mixes consecutive frames from this region into an interleaved stereo buffer.
    ///
    /// Keeping the channel dispatch and region lookup outside the frame loop makes
    /// offline analyzers substantially cheaper without changing sample order.
    pub(crate) fn mix_stereo_into(
        &self,
        source_frame: usize,
        gain: f32,
        output: &mut [f32],
    ) -> usize {
        debug_assert_eq!(output.len() % 2, 0);
        let frame_count = output.len() / 2;
        let mixed_frames = frame_count.min(self.frame_count().saturating_sub(source_frame));
        if mixed_frames == 0 {
            return 0;
        }

        let absolute_start = self.start_frame + source_frame;
        match self.source.channels {
            0 => 0,
            1 => {
                let source = &self.source.frames[absolute_start..absolute_start + mixed_frames];
                for (output, sample) in output.as_chunks_mut::<2>().0.iter_mut().zip(source) {
                    let sample = safe_pcm(*sample) * gain;
                    output[0] += sample;
                    output[1] += sample;
                }
                mixed_frames
            }
            2 => {
                let source_start = absolute_start * 2;
                let source = &self.source.frames[source_start..source_start + mixed_frames * 2];
                for (output, source) in
                    output.as_chunks_mut::<2>().0.iter_mut().zip(source.as_chunks::<2>().0)
                {
                    output[0] += safe_pcm(source[0]) * gain;
                    output[1] += safe_pcm(source[1]) * gain;
                }
                mixed_frames
            }
            channels => {
                let channels = channels as usize;
                for (frame, output) in
                    output.as_chunks_mut::<2>().0.iter_mut().take(mixed_frames).enumerate()
                {
                    let source = (absolute_start + frame) * channels;
                    output[0] += safe_pcm(self.source.frames[source]) * gain;
                    output[1] += safe_pcm(self.source.frames[source + 1]) * gain;
                }
                mixed_frames
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

    /// 同じデコード済みPCMを参照しているかを返す。
    #[cfg(test)]
    pub(crate) fn shares_source_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.source, &other.source)
    }

    #[cfg(test)]
    pub(crate) fn source_frames_ptr(&self) -> *const f32 {
        self.source.frames.as_ptr()
    }
}

#[derive(Debug, Clone, Default)]
pub struct SampleBank {
    samples: Vec<Option<SampleRegion>>,
}

impl SampleBank {
    pub fn reserve_slot(&mut self, id: SoundId) {
        let index = id.0 as usize;
        if self.samples.len() <= index {
            self.samples.resize_with(index + 1, || None);
        }
    }

    pub fn insert(&mut self, id: SoundId, sample: DecodedSample) {
        let source = Arc::new(sample);
        self.insert_shared_region(id, source.clone(), 0, source.frame_count());
    }

    /// 共有PCMの指定範囲を `SoundId` に登録する。
    pub fn insert_shared_region(
        &mut self,
        id: SoundId,
        source: Arc<DecodedSample>,
        start_frame: usize,
        end_frame: usize,
    ) {
        let index = id.0 as usize;
        self.reserve_slot(id);
        self.samples[index] = Some(SampleRegion::new(source, start_frame, end_frame));
    }

    pub fn get(&self, id: SoundId) -> Option<&SampleRegion> {
        self.samples.get(id.0 as usize)?.as_ref()
    }

    /// 登録済みの再生region数を返す。
    pub fn region_count(&self) -> usize {
        self.samples.iter().flatten().count()
    }

    /// regionが参照しているデコード済みPCM source数を返す。
    pub fn source_count(&self) -> usize {
        self.samples
            .iter()
            .flatten()
            .map(|region| Arc::as_ptr(&region.source))
            .collect::<HashSet<_>>()
            .len()
    }

    /// 保持中の全サンプルを `target_rate` へリサンプルする。出力レート変更時に
    /// 呼ばれ、ミキサー側でのリアルタイムリサンプルを不要にする。
    pub fn resample_all_to(&mut self, target_rate: u32) {
        let mut resampled_by_source =
            std::collections::HashMap::<*const DecodedSample, Arc<DecodedSample>>::new();
        for region in self.samples.iter_mut().flatten() {
            if region.source.sample_rate == target_rate {
                continue;
            }
            let source = region.source.clone();
            let source_key = Arc::as_ptr(&source);
            let resampled = resampled_by_source
                .entry(source_key)
                .or_insert_with(|| Arc::new(source.resampled_to(target_rate)))
                .clone();
            if resampled.sample_rate != source.sample_rate {
                let old_frame_count = source.frame_count();
                let new_frame_count = resampled.frame_count();
                region.start_frame =
                    scale_region_boundary(region.start_frame, old_frame_count, new_frame_count);
                region.end_frame =
                    scale_region_boundary(region.end_frame, old_frame_count, new_frame_count)
                        .max(region.start_frame);
            }
            region.source = resampled;
        }
    }
}

fn scale_region_boundary(boundary: usize, old_frame_count: usize, new_frame_count: usize) -> usize {
    if old_frame_count == 0 {
        return 0;
    }
    (boundary as u128 * new_frame_count as u128 / old_frame_count as u128)
        .min(new_frame_count as u128) as usize
}

impl DecodedSample {
    /// Repair before gain/resampling so invalid values cannot spread to neighbors.
    /// One diagnostic per decode, never one per PCM value.
    pub(crate) fn sanitize_pcm(&mut self, path: &std::path::Path) {
        let mut replaced = 0usize;
        let mut peak = 0.0f32;
        let mut non_finite = 0usize;
        for value in &mut self.frames {
            if value.is_finite() {
                peak = peak.max(value.abs());
            } else {
                non_finite += 1;
            }
            let safe = safe_pcm(*value);
            if !value.is_finite() || *value != safe {
                replaced += 1;
                *value = safe;
            }
        }
        if replaced != 0 {
            tracing::warn!(path = %path.display(), peak, replaced, non_finite,
                threshold = MAX_PCM_AMPLITUDE, action = "replace with silence",
                "abnormal decoded PCM amplitude");
        }
    }

    pub fn frame_count(&self) -> usize {
        if self.channels == 0 { 0 } else { self.frames.len() / self.channels as usize }
    }

    pub fn sample_stereo(&self, frame: usize) -> (f32, f32) {
        match self.channels {
            0 => (0.0, 0.0),
            1 => {
                let value = safe_pcm(self.frames.get(frame).copied().unwrap_or(0.0));
                (value, value)
            }
            _ => {
                let index = frame * self.channels as usize;
                (
                    safe_pcm(self.frames.get(index).copied().unwrap_or(0.0)),
                    safe_pcm(self.frames.get(index + 1).copied().unwrap_or(0.0)),
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
                let a = safe_pcm(self.frames.get(base + c).copied().unwrap_or(0.0));
                let b = safe_pcm(self.frames.get(next + c).copied().unwrap_or(a));
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
    fn pcm_safety_preserves_headroom_and_replaces_only_outliers() {
        let mut sample = DecodedSample {
            channels: 1,
            sample_rate: 48_000,
            frames: vec![
                1.1,
                -1.2,
                1.36,
                8.0,
                -8.0,
                6405.997,
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
            ],
        };
        sample.sanitize_pcm(std::path::Path::new("fixture.ogg"));
        assert_eq!(sample.frames, vec![1.1, -1.2, 1.36, 8.0, -8.0, 0.0, 0.0, 0.0, 0.0]);
    }

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
    fn sample_region_mixes_consecutive_frames_into_stereo_output() {
        let mut mono_bank = SampleBank::default();
        mono_bank.insert(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5, 1.0, -0.5] },
        );
        let mut mono_output = vec![0.25; 4];
        let mixed = mono_bank.get(SoundId(1)).unwrap().mix_stereo_into(1, 0.5, &mut mono_output);
        assert_eq!(mixed, 2);
        assert_eq!(mono_output, vec![0.75, 0.75, 0.0, 0.0]);

        let mut stereo_bank = SampleBank::default();
        stereo_bank.insert(
            SoundId(2),
            DecodedSample { channels: 2, sample_rate: 48_000, frames: vec![0.1, 0.2, 0.3, 0.4] },
        );
        let mut stereo_output = vec![1.0; 4];
        let mixed =
            stereo_bank.get(SoundId(2)).unwrap().mix_stereo_into(0, 2.0, &mut stereo_output);
        assert_eq!(mixed, 2);
        assert_eq!(stereo_output, vec![1.2, 1.4, 1.6, 1.8]);
    }

    #[test]
    fn reserve_slot_keeps_empty_slot_without_sample() {
        let mut bank = SampleBank::default();

        bank.reserve_slot(SoundId(2));

        assert!(bank.get(SoundId(2)).is_none());
        bank.insert(
            SoundId(2),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5] },
        );
        assert_eq!(bank.get(SoundId(2)).unwrap().sample_stereo(0), (0.5, 0.5));
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
        assert_eq!(sample.sample_rate(), 48_000);
        assert_eq!(
            (0..sample.frame_count())
                .map(|frame| sample.sample_stereo(frame).0)
                .collect::<Vec<_>>(),
            vec![0.0, 0.5, 1.0, 1.0]
        );
    }

    #[test]
    fn shared_regions_keep_one_source_and_follow_resampled_boundaries() {
        let source = Arc::new(DecodedSample {
            channels: 1,
            sample_rate: 24_000,
            frames: vec![0.0, 1.0, 2.0],
        });
        let mut bank = SampleBank::default();
        bank.insert_shared_region(SoundId(1), source.clone(), 0, 1);
        bank.insert_shared_region(SoundId(2), source, 1, 3);

        assert!(bank.get(SoundId(1)).unwrap().shares_source_with(bank.get(SoundId(2)).unwrap()));
        bank.resample_all_to(48_000);

        let first = bank.get(SoundId(1)).unwrap();
        let second = bank.get(SoundId(2)).unwrap();
        assert!(first.shares_source_with(second));
        assert_eq!(first.frame_count(), 2);
        assert_eq!(second.frame_count(), 4);
        assert_eq!(first.sample_stereo(0), (0.0, 0.0));
        assert_eq!(first.sample_stereo(1), (0.5, 0.5));
        assert_eq!(second.sample_stereo(0), (1.0, 1.0));
        assert_eq!(second.sample_stereo(3), (2.0, 2.0));
    }
}
