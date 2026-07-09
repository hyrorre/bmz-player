use std::cell::Cell;

use bmz_audio::command::{AudioEngineCommand, AudioEngineHandle};
use bmz_audio::sample::DecodedSample;
use bmz_core::ids::SoundId;

/// 選曲プレビュー用の予約 SoundId。システム音 (100_000 帯) および chart キー音と衝突しない。
pub const CHART_PREVIEW_SOUND_ID: SoundId = SoundId(100_050);
const CHART_PREVIEW_SOUND_IDS: [SoundId; 4] =
    [SoundId(100_050), SoundId(100_051), SoundId(100_052), SoundId(100_053)];
const CHART_PREVIEW_ATTACK_FADE_MS: u32 = 10;
const CHART_PREVIEW_RELEASE_FADE_MS: u32 = 10;
const VOLUME_EPSILON: f32 = 0.000_1;

/// 選曲画面で `#PREVIEW` を system audio engine 上にループ再生する。
pub struct SelectChartPreview {
    engine: AudioEngineHandle,
    active_slot: Cell<Option<usize>>,
    last_volume: Cell<Option<f32>>,
}

impl SelectChartPreview {
    pub fn new(engine: AudioEngineHandle) -> Self {
        let commands = CHART_PREVIEW_SOUND_IDS
            .iter()
            .copied()
            .map(|id| AudioEngineCommand::ReserveSampleSlot { id })
            .collect::<Vec<_>>();
        engine.push_commands(commands);
        Self { engine, active_slot: Cell::new(None), last_volume: Cell::new(None) }
    }

    pub fn stop(&self) {
        let commands = CHART_PREVIEW_SOUND_IDS
            .iter()
            .copied()
            .map(|id| AudioEngineCommand::StopSound { id })
            .collect::<Vec<_>>();
        if self.engine.push_commands(commands) {
            self.active_slot.set(None);
            self.last_volume.set(None);
        }
    }

    pub fn set_volume(&self, volume: f32) {
        let volume = normalize_volume(volume);
        if self.last_volume.get().is_some_and(|last| volume_matches(last, volume)) {
            return;
        }
        let Some(active_slot) = self.active_slot.get() else {
            return;
        };
        if self.engine.set_sound_volume(CHART_PREVIEW_SOUND_IDS[active_slot], volume) {
            self.last_volume.set(Some(volume));
        }
    }

    pub fn output_sample_rate(&self) -> u32 {
        self.engine.output_sample_rate()
    }

    pub fn play_sample(&self, sample: DecodedSample, volume: f32) -> bool {
        let output_sample_rate = self.engine.output_sample_rate();
        let sample = sample.resampled_to(output_sample_rate);
        let fade_in_frames = attack_fade_frames(output_sample_rate);
        let fade_out_frames = release_fade_frames(output_sample_rate);
        let next_slot = self.next_slot();
        let next_id = CHART_PREVIEW_SOUND_IDS[next_slot];
        let mut commands = Vec::with_capacity(4);
        if let Some(active_slot) = self.active_slot.get() {
            commands.push(AudioEngineCommand::StopSoundWithFadeOut {
                id: CHART_PREVIEW_SOUND_IDS[active_slot],
                fade_out_frames,
            });
        }
        commands.extend([
            AudioEngineCommand::StopSound { id: next_id },
            AudioEngineCommand::InsertPreparedSample { id: next_id, sample },
            AudioEngineCommand::PlayNowWithFadeIn {
                sound_id: next_id,
                volume: normalize_volume(volume),
                loop_playback: true,
                fade_in_frames,
            },
        ]);
        if self.engine.push_commands(commands) {
            self.active_slot.set(Some(next_slot));
            self.last_volume.set(Some(normalize_volume(volume)));
            true
        } else {
            false
        }
    }

    fn next_slot(&self) -> usize {
        self.active_slot.get().map(|slot| (slot + 1) % CHART_PREVIEW_SOUND_IDS.len()).unwrap_or(0)
    }
}

fn attack_fade_frames(sample_rate: u32) -> u32 {
    if sample_rate == 0 {
        return 0;
    }
    (sample_rate / 1_000).saturating_mul(CHART_PREVIEW_ATTACK_FADE_MS).max(1)
}

fn release_fade_frames(sample_rate: u32) -> u32 {
    if sample_rate == 0 {
        return 0;
    }
    (sample_rate / 1_000).saturating_mul(CHART_PREVIEW_RELEASE_FADE_MS).max(1)
}

fn normalize_volume(volume: f32) -> f32 {
    if volume.is_finite() { volume.clamp(0.0, 1.0) } else { 0.0 }
}

fn volume_matches(left: f32, right: f32) -> bool {
    (left - right).abs() <= VOLUME_EPSILON
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
    fn set_volume_skips_redundant_updates() {
        let engine = AudioEngineHandle::new(AudioEngine::new(1_000));
        let preview = SelectChartPreview::new(engine.clone());
        assert!(preview.play_sample(
            DecodedSample { channels: 1, sample_rate: 1_000, frames: vec![1.0, 1.0] },
            0.5,
        ));

        preview.set_volume(0.5);
        preview.set_volume(0.5 + VOLUME_EPSILON / 2.0);

        assert_eq!(engine.diagnostics().submitted, 7);
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
    fn play_sample_fades_out_previous_slot_while_new_slot_fades_in() {
        let engine = AudioEngineHandle::new(AudioEngine::new(1_000));
        let mut processor = engine.processor();
        let preview = SelectChartPreview::new(engine);
        assert!(preview.play_sample(
            DecodedSample { channels: 1, sample_rate: 1_000, frames: vec![1.0; 32] },
            1.0,
        ));
        let mut output = vec![0.0; 20];
        processor.render_stereo(0, &mut output);

        assert!(preview.play_sample(
            DecodedSample { channels: 1, sample_rate: 1_000, frames: vec![0.5; 32] },
            1.0,
        ));
        let mut output = vec![0.0; 22];
        processor.render_stereo(10, &mut output);

        assert!(output[0] > 0.9, "old preview should start release near its current level");
        assert!(
            output[20] >= 0.49 && output[20] <= 0.51,
            "new preview should finish attack after the short crossfade"
        );
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
