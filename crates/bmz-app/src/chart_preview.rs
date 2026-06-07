use bmz_audio::backend::cpal::SharedAudioEngine;
use bmz_audio::sample::DecodedSample;
use bmz_core::ids::SoundId;

/// 選曲プレビュー用の予約 SoundId。システム音 (100_000 帯) および chart キー音と衝突しない。
pub const CHART_PREVIEW_SOUND_ID: SoundId = SoundId(100_050);

/// 選曲画面で `#PREVIEW` を system audio engine 上にループ再生する。
pub struct SelectChartPreview {
    engine: SharedAudioEngine,
}

impl SelectChartPreview {
    pub fn new(engine: SharedAudioEngine) -> Self {
        Self { engine }
    }

    pub fn stop(&self) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.stop_sound(CHART_PREVIEW_SOUND_ID);
        }
    }

    pub fn set_volume(&self, volume: f32) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.set_sound_volume(CHART_PREVIEW_SOUND_ID, volume);
        }
    }

    pub fn play_sample(&self, sample: DecodedSample, volume: f32) -> bool {
        let Ok(mut engine) = self.engine.lock() else {
            return false;
        };
        engine.stop_sound(CHART_PREVIEW_SOUND_ID);
        engine.insert_sample(CHART_PREVIEW_SOUND_ID, sample);
        engine.play_now(CHART_PREVIEW_SOUND_ID, volume, true);
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use bmz_audio::engine::AudioEngine;

    use super::*;

    #[test]
    fn chart_preview_sound_id_avoids_system_and_chart_ranges() {
        const { assert!(CHART_PREVIEW_SOUND_ID.0 >= 100_050) };
        const { assert!((CHART_PREVIEW_SOUND_ID.0 as usize) < 10_000_000) };
    }

    #[test]
    fn stop_is_noop_without_loaded_sample() {
        let engine: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));
        let preview = SelectChartPreview::new(engine);
        preview.stop();
    }

    #[test]
    fn set_volume_updates_looping_preview_voice() {
        let engine: SharedAudioEngine = Arc::new(Mutex::new(AudioEngine::default()));
        let preview = SelectChartPreview::new(Arc::clone(&engine));
        assert!(preview.play_sample(
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0] },
            1.0,
        ));

        {
            let mut guard = engine.lock().unwrap();
            let mut output = vec![0.0; 2];
            guard.render_stereo(0, &mut output);
        }
        preview.set_volume(0.25);
        let mut output = vec![0.0; 2];
        engine.lock().unwrap().render_stereo(1, &mut output);

        assert_eq!(output, vec![0.25, 0.25]);
    }
}
