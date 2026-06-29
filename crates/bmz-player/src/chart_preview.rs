use bmz_audio::command::{AudioEngineCommand, AudioEngineHandle};
use bmz_audio::sample::DecodedSample;
use bmz_core::ids::SoundId;

/// 選曲プレビュー用の予約 SoundId。システム音 (100_000 帯) および chart キー音と衝突しない。
pub const CHART_PREVIEW_SOUND_ID: SoundId = SoundId(100_050);
const CHART_PREVIEW_ATTACK_FADE_MS: u32 = 10;

/// 選曲画面で `#PREVIEW` を system audio engine 上にループ再生する。
pub struct SelectChartPreview {
    engine: AudioEngineHandle,
}

impl SelectChartPreview {
    pub fn new(engine: AudioEngineHandle) -> Self {
        engine.reserve_sample_slot(CHART_PREVIEW_SOUND_ID);
        Self { engine }
    }

    pub fn stop(&self) {
        self.engine.stop_sound(CHART_PREVIEW_SOUND_ID);
    }

    pub fn set_volume(&self, volume: f32) {
        self.engine.set_sound_volume(CHART_PREVIEW_SOUND_ID, volume);
    }

    pub fn play_sample(&self, sample: DecodedSample, volume: f32) -> bool {
        let output_sample_rate = self.engine.output_sample_rate();
        let sample = sample.resampled_to(output_sample_rate);
        let fade_in_frames = attack_fade_frames(output_sample_rate);
        self.engine.push_commands(vec![
            AudioEngineCommand::StopSound { id: CHART_PREVIEW_SOUND_ID },
            AudioEngineCommand::InsertPreparedSample { id: CHART_PREVIEW_SOUND_ID, sample },
            AudioEngineCommand::PlayNowWithFadeIn {
                sound_id: CHART_PREVIEW_SOUND_ID,
                volume,
                loop_playback: true,
                fade_in_frames,
            },
        ])
    }
}

fn attack_fade_frames(sample_rate: u32) -> u32 {
    if sample_rate == 0 {
        return 0;
    }
    (sample_rate / 1_000).saturating_mul(CHART_PREVIEW_ATTACK_FADE_MS).max(1)
}

#[cfg(test)]
mod tests {
    use bmz_audio::engine::AudioEngine;

    use super::*;

    #[test]
    fn chart_preview_sound_id_avoids_system_and_chart_ranges() {
        const { assert!(CHART_PREVIEW_SOUND_ID.0 >= 100_050) };
        const { assert!((CHART_PREVIEW_SOUND_ID.0 as usize) < 10_000_000) };
    }

    #[test]
    fn stop_is_noop_without_loaded_sample() {
        let engine = AudioEngineHandle::new(AudioEngine::default());
        let preview = SelectChartPreview::new(engine);
        preview.stop();
    }

    #[test]
    fn set_volume_updates_looping_preview_voice() {
        let engine = AudioEngineHandle::new(AudioEngine::new(1_000));
        let mut processor = engine.processor();
        let preview = SelectChartPreview::new(engine);
        assert!(preview.play_sample(
            DecodedSample { channels: 1, sample_rate: 1_000, frames: vec![1.0, 1.0] },
            1.0,
        ));

        let mut output = vec![0.0; 2];
        processor.render_stereo(0, &mut output);
        preview.set_volume(0.25);
        let mut output = vec![0.0; 2];
        processor.render_stereo(10, &mut output);

        assert_eq!(output, vec![0.25, 0.25]);
    }

    #[test]
    fn play_sample_uses_short_attack_fade() {
        let engine = AudioEngineHandle::new(AudioEngine::new(1_000));
        let mut processor = engine.processor();
        let preview = SelectChartPreview::new(engine);
        assert!(preview.play_sample(
            DecodedSample { channels: 1, sample_rate: 1_000, frames: vec![1.0, 1.0] },
            1.0,
        ));

        let mut output = vec![0.0; 2];
        processor.render_stereo(0, &mut output);
        assert_eq!(output, vec![0.0, 0.0]);

        let mut output = vec![0.0; 2];
        processor.render_stereo(10, &mut output);
        assert_eq!(output, vec![1.0, 1.0]);
    }

    #[test]
    fn play_sample_prepares_sample_at_engine_output_rate() {
        let engine = AudioEngineHandle::new(AudioEngine::new(2_000));
        let mut processor = engine.processor();
        let preview = SelectChartPreview::new(engine.clone());

        assert!(preview.play_sample(
            DecodedSample { channels: 1, sample_rate: 1_000, frames: vec![0.0, 1.0] },
            1.0,
        ));
        processor.apply_pending_commands_for_tests();

        let (_, samples) = engine.clone_sample_bank().unwrap();
        let sample = samples.get(CHART_PREVIEW_SOUND_ID).unwrap();
        assert_eq!(sample.sample_rate, 2_000);
        assert_eq!(sample.frames, vec![0.0, 0.5, 1.0, 1.0]);
    }
}
