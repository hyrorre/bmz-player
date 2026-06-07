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

    /// 指定 sound_id のスケジュール済み音および再生中 voice をすべて停止する。
    /// BGM ループの停止等に使う。
    pub fn stop_sound(&mut self, id: bmz_core::ids::SoundId) {
        self.queue.retain(|sound| sound.sound_id != id);
        self.mixer.voices.retain(|voice| voice.sound.sound_id != id);
    }

    /// 指定 sound_id のスケジュール済み音および再生中 voice の音量を更新する。
    pub fn set_sound_volume(&mut self, id: bmz_core::ids::SoundId, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        self.queue.set_volume_for_sound(id, volume);
        self.mixer.set_volume_for_sound(id, volume);
    }

    /// `start_frame = 0`(=即時再生)で sound_id を 1 ショット再生する。
    /// 必要なら `loop_playback = true` でループ再生も可能。
    pub fn play_now(&mut self, sound_id: bmz_core::ids::SoundId, volume: f32, loop_playback: bool) {
        self.schedule(ScheduledSound {
            start_frame: 0,
            sound_id,
            volume: volume.clamp(0.0, 1.0),
            pan: 0.0,
            loop_playback,
        });
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
            loop_playback: false,
        });
        let mut output = vec![1.0; 8];

        engine.render_stereo(0, &mut output);

        assert_eq!(output, vec![0.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.25, 0.25]);
    }

    #[test]
    fn stop_sound_drops_queue_and_voices_for_id() {
        let mut engine = AudioEngine::default();
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5, 0.5, 0.5, 0.5] },
        );
        engine.play_now(SoundId(1), 1.0, true);
        // 1 サンプル分 render してループ voice が生きていることを確認。
        let mut output = vec![0.0; 4];
        engine.render_stereo(0, &mut output);
        assert!(!engine.is_idle());

        engine.stop_sound(SoundId(1));

        // stop 後はキューも voice も空。
        assert!(engine.is_idle());
    }

    #[test]
    fn set_sound_volume_updates_queue_and_active_voice() {
        let mut engine = AudioEngine::default();
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0] },
        );
        engine.schedule(ScheduledSound {
            start_frame: 2,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: false,
        });
        engine.set_sound_volume(SoundId(1), 0.25);

        let mut output = vec![0.0; 8];
        engine.render_stereo(0, &mut output);

        assert_eq!(output, vec![0.0, 0.0, 0.0, 0.0, 0.25, 0.25, 0.25, 0.25]);

        engine.play_now(SoundId(1), 1.0, true);
        let mut output = vec![0.0; 2];
        engine.render_stereo(4, &mut output);
        engine.set_sound_volume(SoundId(1), 0.5);
        let mut output = vec![0.0; 2];
        engine.render_stereo(5, &mut output);

        assert_eq!(output, vec![0.5, 0.5]);
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
            loop_playback: false,
        });
        // スケジュール済みの音が残っている間はアイドルではない。
        assert!(!engine.is_idle());

        // 1 フレームだけのサンプルを鳴らし切るとアイドルへ戻る。
        let mut output = vec![0.0; 4];
        engine.render_stereo(0, &mut output);
        assert!(engine.is_idle());
    }
}
