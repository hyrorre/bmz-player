use super::*;
use bmz_render::skin::SkinLuaEffect;

impl WinitApp {
    /// Apply what the shown skin's Lua `act` / `customEvents` requested while
    /// the renderer prepared this frame.
    pub(super) fn apply_skin_lua_effects(&mut self, scene_kind: AppSceneKind) {
        let scene = skin_runtime_scene(scene_kind);
        for effect in self.renderer.take_skin_lua_effects(scene) {
            match effect {
                SkinLuaEffect::Event { id, arg1, arg2: _ } => {
                    self.execute_skin_lua_event(scene, id, arg1)
                }
                SkinLuaEffect::Audio(_) | SkinLuaEffect::SetVolume { .. } => {}
            }
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
}

pub(super) fn skin_runtime_scene(scene_kind: AppSceneKind) -> SkinRuntimeScene {
    match scene_kind {
        AppSceneKind::Select => SkinRuntimeScene::Select,
        AppSceneKind::Decide => SkinRuntimeScene::Decide,
        AppSceneKind::Play => SkinRuntimeScene::Play,
        AppSceneKind::Result => SkinRuntimeScene::Result,
    }
}
