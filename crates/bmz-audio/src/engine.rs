use crate::mixer::MixerState;
use crate::queue::{AudioScheduler, ScheduledSound, ScheduledSoundQueue};
use crate::sample::{DecodedSample, SampleBank};

#[derive(Debug, Default)]
pub struct AudioEngine {
    pub queue: ScheduledSoundQueue,
    pub mixer: MixerState,
    pub samples: SampleBank,
}

impl AudioEngine {
    pub fn new(output_sample_rate: u32) -> Self {
        Self {
            queue: ScheduledSoundQueue::new(),
            mixer: MixerState::new(output_sample_rate),
            samples: SampleBank::default(),
        }
    }

    pub fn insert_sample(&mut self, id: bmz_core::ids::SoundId, sample: DecodedSample) {
        self.samples.insert(id, sample);
    }

    /// 再生待ちのスケジュール音も鳴っているボイスも無い、つまり出力を
    /// ドレインし終えた状態かどうか。リザルト遷移後の余韻再生の終了判定に使う。
    pub fn is_idle(&self) -> bool {
        self.queue.is_empty() && self.mixer.voices.is_empty()
    }

    pub fn render_stereo(&mut self, output_start_frame: u64, output: &mut [f32]) {
        output.fill(0.0);
        let frame_count = output.len() / 2;
        let output_end_frame = output_start_frame + frame_count.saturating_sub(1) as u64;
        let due = self.queue.drain_until_frame(output_end_frame);
        self.mixer.push_scheduled(due);
        self.mixer.mix_stereo(&self.samples, output_start_frame, output);
    }
}

impl AudioScheduler for AudioEngine {
    fn schedule(&mut self, sound: ScheduledSound) {
        self.queue.schedule(sound);
    }
}

#[cfg(test)]
mod tests {
    use bmz_core::ids::SoundId;

    use super::*;

    #[test]
    fn audio_engine_renders_queued_samples() {
        let mut engine = AudioEngine::default();
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5, 0.25] },
        );
        engine.schedule(ScheduledSound {
            start_frame: 2,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
        });
        let mut output = vec![1.0; 8];

        engine.render_stereo(0, &mut output);

        assert_eq!(output, vec![0.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.25, 0.25]);
    }

    #[test]
    fn audio_engine_is_idle_until_drained() {
        let mut engine = AudioEngine::default();
        assert!(engine.is_idle());

        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5] },
        );
        engine.schedule(ScheduledSound {
            start_frame: 0,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
        });
        // スケジュール済みの音が残っている間はアイドルではない。
        assert!(!engine.is_idle());

        // 1 フレームだけのサンプルを鳴らし切るとアイドルへ戻る。
        let mut output = vec![0.0; 4];
        engine.render_stereo(0, &mut output);
        assert!(engine.is_idle());
    }
}
