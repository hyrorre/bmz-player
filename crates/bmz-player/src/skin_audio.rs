use std::collections::{HashMap, HashSet};

use bmz_audio::command::{AudioEngineCommand, AudioEngineHandle};
use bmz_core::ids::SoundId;
use bmz_render::skin::{SkinAudioActionDef, SkinAudioActionKind, SkinCustomEventDef, SkinDocument};

use crate::skin_loader::DecodedSkinAudio;

/// system sound / chart preview の予約範囲より後ろに置く、小さい固定レンジ。
const SKIN_AUDIO_SOUND_BASE: u32 = 100_100;

/// Lua skin の音声命令を、共有 system audio engine 上で実行する Result 専用ランタイム。
pub struct SkinAudioRuntime {
    engine: AudioEngineHandle,
    sound_ids: HashMap<String, SoundId>,
    scene_audio: Vec<SkinAudioActionDef>,
    custom_events: Vec<SkinCustomEventDef>,
    fired_once_events: HashSet<i32>,
}

impl SkinAudioRuntime {
    pub fn install(
        engine: AudioEngineHandle,
        document: &SkinDocument,
        audio_assets: Vec<DecodedSkinAudio>,
    ) -> Self {
        let mut sound_ids = HashMap::new();
        let mut commands = Vec::with_capacity(audio_assets.len());
        for (index, audio) in audio_assets.into_iter().enumerate() {
            let id = SoundId(SKIN_AUDIO_SOUND_BASE + index as u32);
            sound_ids.insert(audio.path, id);
            commands.push(AudioEngineCommand::InsertSample { id, sample: audio.sample });
        }
        if !commands.is_empty() && !engine.push_commands(commands) {
            tracing::warn!("failed to enqueue decoded skin audio assets");
        }
        Self {
            engine,
            sound_ids,
            scene_audio: document.scene_audio.clone(),
            custom_events: document.custom_events.clone(),
            fired_once_events: HashSet::new(),
        }
    }

    pub fn reset(&mut self) {
        self.stop_all();
        self.fired_once_events.clear();
    }

    pub fn start_scene(&self, bgm_volume: f32, se_volume: f32) -> bool {
        self.run_actions(&self.scene_audio, bgm_volume, se_volume)
    }

    pub fn trigger_timer(&mut self, timer: i32, bgm_volume: f32, se_volume: f32) -> bool {
        let matching = self
            .custom_events
            .iter()
            .filter(|event| {
                event.timer == timer && (!event.once || !self.fired_once_events.contains(&event.id))
            })
            .cloned()
            .collect::<Vec<_>>();
        let actions = matching
            .iter()
            .flat_map(|event| event.audio_actions.iter().cloned())
            .collect::<Vec<_>>();
        let dispatched = self.run_actions(&actions, bgm_volume, se_volume);
        if dispatched {
            for event in matching.into_iter().filter(|event| event.once) {
                self.fired_once_events.insert(event.id);
            }
        }
        dispatched
    }

    pub fn stop_all(&self) -> bool {
        let commands = self
            .sound_ids
            .values()
            .copied()
            .map(|id| AudioEngineCommand::StopSound { id })
            .collect::<Vec<_>>();
        !commands.is_empty() && self.engine.push_commands(commands)
    }

    fn run_actions(&self, actions: &[SkinAudioActionDef], bgm_volume: f32, se_volume: f32) -> bool {
        let mut commands = Vec::new();
        for action in actions {
            let Some(&id) = self.sound_ids.get(&action.path) else { continue };
            match action.action {
                SkinAudioActionKind::Play => {
                    commands.push(AudioEngineCommand::PlayNow {
                        sound_id: id,
                        volume: normalized_volume(action.volume * se_volume),
                        loop_playback: false,
                    });
                }
                SkinAudioActionKind::Loop => {
                    commands.push(AudioEngineCommand::StopSound { id });
                    commands.push(AudioEngineCommand::PlayNow {
                        sound_id: id,
                        volume: normalized_volume(action.volume * bgm_volume),
                        loop_playback: true,
                    });
                }
                SkinAudioActionKind::Stop => {
                    commands.push(AudioEngineCommand::StopSound { id });
                }
            }
        }
        !commands.is_empty() && self.engine.push_commands(commands)
    }
}

impl Drop for SkinAudioRuntime {
    fn drop(&mut self) {
        self.stop_all();
    }
}

fn normalized_volume(volume: f32) -> f32 {
    if volume.is_finite() { volume.clamp(0.0, 1.0) } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use bmz_audio::command::AudioEngineHandle;
    use bmz_audio::engine::AudioEngine;
    use bmz_audio::sample::DecodedSample;

    use super::*;

    fn sample(value: f32, frames: usize) -> DecodedSample {
        DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![value; frames] }
    }

    fn render(processor: &mut bmz_audio::command::CommandedAudioEngine, frames: usize) -> Vec<f32> {
        let mut output = vec![0.0; frames * 2];
        assert!(processor.render_stereo(0, &mut output));
        output
    }

    #[test]
    fn scene_loop_and_once_timer_event_execute_declaratively() {
        let engine = AudioEngineHandle::new(AudioEngine::default());
        let mut processor = engine.processor();
        let document: SkinDocument = serde_json::from_str(
            r#"{
                "sceneAudio": [
                    { "action": "loop", "path": "bgm.ogg", "volume": 0.5 }
                ],
                "customEvents": [
                    {
                        "id": 1001,
                        "timer": 2,
                        "once": true,
                        "audioActions": [
                            { "action": "stop", "path": "bgm.ogg" },
                            { "action": "play", "path": "close.ogg", "volume": 0.25 }
                        ]
                    }
                ]
            }"#,
        )
        .unwrap();
        let assets = vec![
            DecodedSkinAudio { path: "bgm.ogg".to_string(), sample: sample(1.0, 16) },
            DecodedSkinAudio { path: "close.ogg".to_string(), sample: sample(0.5, 1) },
        ];
        let mut runtime = SkinAudioRuntime::install(engine, &document, assets);

        assert!(runtime.start_scene(0.5, 1.0));
        assert_eq!(render(&mut processor, 1), vec![0.25, 0.25]);

        assert!(runtime.trigger_timer(2, 0.5, 0.8));
        assert_eq!(render(&mut processor, 1), vec![0.1, 0.1]);
        assert!(!runtime.trigger_timer(2, 0.5, 0.8));
        assert_eq!(render(&mut processor, 1), vec![0.0, 0.0]);
    }
}
