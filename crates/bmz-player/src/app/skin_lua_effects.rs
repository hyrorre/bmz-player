use super::*;
use crate::config::profile_config::AudioMixConfig;
use crate::skin_audio::SkinAudioRuntime;
use bmz_render::skin::{SkinAudioActionDef, SkinLuaEffect, SkinVolumeChannel};

/// Lua `audio_*` of the Select / Decide / Play skins. The Result skin uses
/// `result_skin_audio`, which also holds its declarative sounds.
#[derive(Default)]
pub(super) struct SkinLuaAudio {
    select: Option<SkinAudioRuntime>,
    decide: Option<SkinAudioRuntime>,
    play: Option<SkinAudioRuntime>,
    /// The scene whose skin last applied effects; its loops stop when it changes.
    scene: Option<SkinRuntimeScene>,
}

impl SkinLuaAudio {
    fn slot(&mut self, scene: SkinRuntimeScene) -> Option<&mut Option<SkinAudioRuntime>> {
        match scene {
            SkinRuntimeScene::Select => Some(&mut self.select),
            SkinRuntimeScene::Decide => Some(&mut self.decide),
            SkinRuntimeScene::Play => Some(&mut self.play),
            SkinRuntimeScene::Result => None,
        }
    }

    /// The scene's skin was replaced; the sounds its Lua VM started stop.
    pub(super) fn reset(&mut self, scene: SkinRuntimeScene) {
        if let Some(slot) = self.slot(scene) {
            *slot = None;
        }
    }
}

impl WinitApp {
    /// Apply what the shown skin's Lua `act` / `customEvents` requested while
    /// the renderer prepared this frame.
    pub(super) fn apply_skin_lua_effects(&mut self, scene_kind: AppSceneKind) {
        let scene = skin_runtime_scene(scene_kind);
        self.track_skin_lua_audio_scene(scene);
        let mut audio_started = false;
        for effect in self.renderer.take_skin_lua_effects(scene) {
            match effect {
                SkinLuaEffect::Event { id, arg1, arg2: _ } => {
                    self.execute_skin_lua_event(scene, id, arg1)
                }
                SkinLuaEffect::Audio(action) => {
                    audio_started |= self.run_skin_lua_audio(scene, &action);
                }
                SkinLuaEffect::SetVolume { channel, value } => self.set_skin_volume(channel, value),
            }
        }
        if audio_started {
            self.start_audio_output_stream();
        }
    }

    /// beatoraja's system / key / BGM volume from a skin (Lua `set_volume_*` or
    /// the Select volume sliders 17..=19). It is saved with the profile like the
    /// other Select settings.
    pub(super) fn set_skin_volume(&mut self, channel: SkinVolumeChannel, value: f32) {
        if set_audio_mix_volume(&mut self.boot.profile_config.audio_mix, channel, value) {
            self.sync_realtime_profile_settings();
            tracing::info!(?channel, value, "skin changed the volume");
        }
    }

    /// A `customEvents` action given as a built-in event ID, executed like the
    /// same event from a click in that scene.
    fn execute_skin_lua_event(&mut self, scene: SkinRuntimeScene, id: i32, arg1: i32) {
        match scene {
            SkinRuntimeScene::Select => self.execute_select_skin_event(id, arg1),
            SkinRuntimeScene::Result => self.execute_result_skin_event(id),
            SkinRuntimeScene::Decide | SkinRuntimeScene::Play => {
                tracing::debug!(id, ?scene, "Lua custom event action has no built-in event here");
            }
        }
    }

    fn run_skin_lua_audio(&mut self, scene: SkinRuntimeScene, action: &SkinAudioActionDef) -> bool {
        let (bgm_volume, se_volume) = self.result_skin_audio_volumes();
        let runtime = match self.audio.skin_lua_audio.slot(scene) {
            Some(slot) => {
                let Some(system_audio) = &self.audio.system_audio else { return false };
                slot.get_or_insert_with(|| SkinAudioRuntime::for_lua(system_audio.engine(), scene))
            }
            None => match self.result.result_skin_audio.as_mut() {
                Some(runtime) => runtime,
                None => return false,
            },
        };
        runtime.run_lua_action(action, bgm_volume, se_volume)
    }

    /// Loops a skin started end with its scene; one-shot sounds play out.
    fn track_skin_lua_audio_scene(&mut self, scene: SkinRuntimeScene) {
        let Some(previous) = self.audio.skin_lua_audio.scene.replace(scene) else { return };
        if previous == scene {
            return;
        }
        let runtime = match self.audio.skin_lua_audio.slot(previous) {
            Some(slot) => slot.as_mut(),
            None => self.result.result_skin_audio.as_mut(),
        };
        if let Some(runtime) = runtime {
            runtime.stop_loops();
        }
    }
}

pub(super) fn skin_runtime_scene(scene_kind: AppSceneKind) -> SkinRuntimeScene {
    match scene_kind {
        AppSceneKind::Select => SkinRuntimeScene::Select,
        AppSceneKind::Decide => SkinRuntimeScene::Decide,
        AppSceneKind::Play => SkinRuntimeScene::Play,
        AppSceneKind::Result => SkinRuntimeScene::Result,
    }
}

pub(super) fn skin_runtime_scene_for_kind(kind: SkinKind) -> SkinRuntimeScene {
    match kind {
        SkinKind::Select => SkinRuntimeScene::Select,
        SkinKind::Decide => SkinRuntimeScene::Decide,
        SkinKind::Play => SkinRuntimeScene::Play,
        SkinKind::Result => SkinRuntimeScene::Result,
    }
}

/// beatoraja's system volume is BMZ's master volume. Returns whether it changed.
pub(super) fn set_audio_mix_volume(
    mix: &mut AudioMixConfig,
    channel: SkinVolumeChannel,
    value: f32,
) -> bool {
    let value = volume_f32_to_unit(value);
    let volume = match channel {
        SkinVolumeChannel::System => &mut mix.master_volume,
        SkinVolumeChannel::Key => &mut mix.key_volume,
        SkinVolumeChannel::Bgm => &mut mix.bgm_volume,
    };
    std::mem::replace(volume, value) != value
}
