//! LR2 select data of the selected chart: same-folder difficulties for the level bar
//! (number 45..49, option 70..79 / 500..565, bargraph 105..109) and the clear-option
//! history (option 118..149).

use bmz_render::scene::SelectSameFolderDifficulty;

use super::*;

/// Cleared whenever the select list is reloaded, which also happens after a play
/// updates the scores.
#[derive(Debug, Default)]
pub(super) struct Lr2SelectCache {
    same_folder: HashMap<String, CachedQuery<[SelectSameFolderDifficulty; 5]>>,
    option_history: HashMap<i64, CachedQuery<u32>>,
}

#[derive(Debug)]
struct CachedQuery<T> {
    value: Option<T>,
    queried_at: Instant,
}

impl<T: Copy> CachedQuery<T> {
    fn cached(&self, now: Instant) -> Option<Option<T>> {
        // Busy/unavailable DBs must not be queried and logged every frame.
        (self.value.is_some() || now.duration_since(self.queried_at) < Duration::from_secs(1))
            .then_some(self.value)
    }
}

impl WinitApp {
    /// Same-folder difficulties of the selected chart, cached per folder.
    pub(super) fn selected_same_folder_difficulties(
        &self,
    ) -> Option<[SelectSameFolderDifficulty; 5]> {
        let folder = &self.selected_chart_row()?.chart.as_ref()?.folder_path;
        if let Some(slots) = self
            .select
            .lr2_select_cache
            .borrow()
            .same_folder
            .get(folder)
            .and_then(|cached| cached.cached(Instant::now()))
        {
            return slots;
        }
        let slots = crate::screens::select_model::same_folder_difficulties(
            &self.boot.library_db,
            &self.boot.score_db,
            folder,
            self.boot.profile_config.play.ln_mode_policy,
            self.boot.profile_config.play.rule_mode,
        )
        .inspect_err(|error| tracing::warn!(%error, folder, "failed to load same-folder charts"))
        .ok();
        self.select
            .lr2_select_cache
            .borrow_mut()
            .same_folder
            .insert(folder.clone(), CachedQuery { value: slots, queried_at: Instant::now() });
        slots
    }

    /// LR2 clear-option history of the selected chart, cached per chart.
    pub(super) fn selected_lr2_option_history(&self) -> Option<u32> {
        let chart = self.selected_chart_row()?.chart.as_ref()?;
        if let Some(bits) = self
            .select
            .lr2_select_cache
            .borrow()
            .option_history
            .get(&chart.chart_id)
            .and_then(|cached| cached.cached(Instant::now()))
        {
            return bits;
        }
        let bits = crate::screens::select_model::lr2_clear_option_history(
            &self.boot.score_db,
            chart,
            self.boot.profile_config.play.ln_mode_policy,
            self.boot.profile_config.play.rule_mode,
        )
        .inspect_err(|error| tracing::warn!(%error, "failed to load clear option history"))
        .ok();
        self.select
            .lr2_select_cache
            .borrow_mut()
            .option_history
            .insert(chart.chart_id, CachedQuery { value: bits, queried_at: Instant::now() });
        bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_select_queries_retry_after_a_second_without_expiring_successes() {
        let now = Instant::now();
        let failed = CachedQuery::<u32> { value: None, queried_at: now };
        for frame in 0..240 {
            assert_eq!(failed.cached(now + Duration::from_micros(frame * 4_166)), Some(None));
        }
        assert_eq!(failed.cached(now + Duration::from_secs(1)), None);
        let successful = CachedQuery { value: Some(0), queried_at: now };
        assert_eq!(successful.cached(now + Duration::from_secs(60)), Some(Some(0)));
    }
}
