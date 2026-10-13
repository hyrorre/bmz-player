//! Skin settings embedded in LR2 select skins (buttons 170..190 / 220..229,
//! text 50 / 51 / 100..119, slider 7). They edit the same profile skin settings
//! as the F1 settings screen and reload the edited skin.

use bmz_render::scene::Lr2SkinSettingsSnapshot;

use super::*;
use crate::ui::{Lr2SkinTarget, lr2_cycle_skin_item, lr2_skin_items};

type SettingsKey = (Lr2SkinTarget, String, BTreeMap<String, String>, BTreeMap<String, String>);

#[derive(Default)]
pub(super) struct Lr2SkinSettingsRuntime {
    target: Lr2SkinTarget,
    scroll: usize,
    /// A customize change reloads the select skin being shown. Its install then
    /// keeps the open panel and scene timers instead of restarting the scene, so
    /// the next click still lands on the settings.
    keep_select_scene: bool,
    /// Rebuilt when the edited slot's skin path, options or files change, so
    /// edits from the F1 settings screen are shown too.
    cache: RefCell<Option<(SettingsKey, SceneSkinDefs, Lr2SkinSettingsSnapshot)>>,
}

impl WinitApp {
    /// The embedded skin settings of the active LR2 select skin.
    pub(super) fn lr2_skin_settings_snapshot(&self) -> Option<Lr2SkinSettingsSnapshot> {
        if !self.lr2_select_active() {
            return None;
        }
        let runtime = &self.select.lr2_skin_settings;
        self.refresh_lr2_skin_settings();
        let cache = runtime.cache.borrow();
        let (_, _, snapshot) = cache.as_ref()?;
        let mut snapshot = snapshot.clone();
        snapshot.visible_rows = self
            .renderer
            .select_skin_document()
            .and_then(|document| document.lr2_skin_settings_rows())
            .unwrap_or(0);
        snapshot.scroll = runtime.scroll.min(snapshot.max_scroll());
        Some(snapshot)
    }

    fn refresh_lr2_skin_settings(&self) {
        let runtime = &self.select.lr2_skin_settings;
        let skin = &self.boot.profile_config.skin;
        let target = runtime.target;
        let (path, options, files) = target.settings_key(skin);
        let key = (target, path, options, files);
        let mut cache = runtime.cache.borrow_mut();
        if cache.as_ref().is_some_and(|(cached, ..)| *cached == key) {
            return;
        }
        let defs = skin_defs_from_path(&self.boot.app_paths, &key.1, target.is_play());
        let items = lr2_skin_items(target, &defs, skin, &self.boot.app_paths)
            .into_iter()
            .map(|item| (item.name, item.value))
            .collect();
        let snapshot = Lr2SkinSettingsSnapshot {
            target_button: target.button(),
            skin_name: defs.name.clone(),
            skin_author: defs.author.clone(),
            items,
            scroll: 0,
            visible_rows: 0,
        };
        *cache = Some((key, defs, snapshot));
    }

    /// Buttons 170..185 choose the skin type to edit.
    pub(super) fn select_lr2_skin_target(&mut self, button: i32) -> bool {
        let Some(target) = Lr2SkinTarget::from_button(button) else {
            return false;
        };
        let runtime = &mut self.select.lr2_skin_settings;
        if runtime.target != target {
            runtime.target = target;
            runtime.scroll = 0;
        }
        true
    }

    /// Button 190 steps the edited type's skin through BMZ's default and the installed skins.
    pub(super) fn cycle_lr2_skin(&mut self, direction: i32) -> bool {
        let target = self.select.lr2_skin_settings.target;
        if !target.cycle_skin(
            &mut self.boot.profile_config.skin,
            &self.skin.skin_catalog,
            direction,
        ) {
            return false;
        }
        self.select.lr2_skin_settings.scroll = 0;
        self.apply_lr2_skin_settings_change(target);
        true
    }

    /// Buttons 220..229 step the value of the visible customize row.
    pub(super) fn cycle_lr2_skin_item(&mut self, row: usize, direction: i32) -> bool {
        self.refresh_lr2_skin_settings();
        let runtime = &self.select.lr2_skin_settings;
        let target = runtime.target;
        let Some(defs) = runtime.cache.borrow().as_ref().map(|(_, defs, _)| defs.clone()) else {
            return false;
        };
        let index = runtime.scroll + row;
        if !lr2_cycle_skin_item(
            target,
            &defs,
            &mut self.boot.profile_config.skin,
            &self.boot.app_paths,
            index,
            direction,
        ) {
            return false;
        }
        if target.reload_request().select {
            self.select.lr2_skin_settings.keep_select_scene = true;
        }
        self.apply_lr2_skin_settings_change(target);
        true
    }

    /// Whether this select skin install comes from a customize change made on the
    /// embedded skin settings, and so keeps the scene.
    pub(super) fn take_lr2_skin_settings_reload(&mut self) -> bool {
        std::mem::take(&mut self.select.lr2_skin_settings.keep_select_scene)
    }

    /// Slider 7 scrolls the customize list.
    pub(super) fn set_lr2_skin_settings_scroll(&mut self, value: f32) {
        let max = self.lr2_skin_settings_snapshot().map_or(0, |settings| settings.max_scroll());
        self.select.lr2_skin_settings.scroll =
            (value.clamp(0.0, 1.0) * max as f32).round() as usize;
    }

    /// The mouse wheel over the list moves it one row.
    pub(super) fn scroll_lr2_skin_settings(&mut self, rows: i32) {
        let max = self.lr2_skin_settings_snapshot().map_or(0, |settings| settings.max_scroll());
        let runtime = &mut self.select.lr2_skin_settings;
        runtime.scroll = runtime.scroll.min(max).saturating_add_signed(rows as isize).min(max);
    }

    fn apply_lr2_skin_settings_change(&mut self, target: Lr2SkinTarget) {
        self.boot.profile_config.updated_at = now_unix_seconds();
        self.reload_skins(target.reload_request());
    }
}
