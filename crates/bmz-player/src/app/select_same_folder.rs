//! Same-folder difficulties of the selected chart for LR2's level bar
//! (number 45..49, option 70..79 / 500..565, bargraph 105..109).

use bmz_render::scene::SelectSameFolderDifficulty;

use super::*;

impl WinitApp {
    /// Cached per folder; the cache is cleared whenever the select list is reloaded,
    /// which also happens after a play updates the scores.
    pub(super) fn selected_same_folder_difficulties(
        &self,
    ) -> Option<[SelectSameFolderDifficulty; 5]> {
        let folder = &self.selected_chart_row()?.chart.as_ref()?.folder_path;
        if let Some(slots) = self.select.same_folder_cache.borrow().get(folder) {
            return Some(*slots);
        }
        let slots = crate::screens::select_model::same_folder_difficulties(
            &self.boot.library_db,
            &self.boot.score_db,
            folder,
            self.boot.profile_config.play.ln_mode_policy,
            self.boot.profile_config.play.rule_mode,
        )
        .inspect_err(|error| tracing::warn!(%error, folder, "failed to load same-folder charts"))
        .ok()?;
        self.select.same_folder_cache.borrow_mut().insert(folder.clone(), slots);
        Some(slots)
    }
}
