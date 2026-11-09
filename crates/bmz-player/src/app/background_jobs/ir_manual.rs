use super::*;
use crate::ir::manual_job::{IrManualEnv, outcome_message};

impl WinitApp {
    fn ir_manual_env(&self) -> IrManualEnv {
        IrManualEnv {
            library_db: self.boot.app_paths.library_db.clone(),
            score_db: self.boot.profile_paths.score_db.clone(),
            network_db: self.boot.profile_paths.network_db.clone(),
            profile_root: self.boot.profile_paths.root_dir.clone(),
            logs_dir: self.boot.app_paths.logs_dir.clone(),
            ir_config: self.boot.profile_config.ir.clone(),
        }
    }

    /// UIが積んだ手動IR要求を開始し、完了した操作をtoastで通知する。
    pub(in crate::app) fn poll_ir_manual(&mut self) {
        for queued in self.jobs.ir_manual.drain_requests() {
            self.jobs.ir_manual.dispatch(self.ir_manual_env(), queued);
        }
        if let Some(outcome) = self.jobs.ir_manual.take_notice() {
            if outcome.imported_scores_need_refresh() {
                self.refresh_player_stats_snapshot();
                self.invalidate_select_folder_summaries();
                self.reload_select_items();
            }
            let message =
                outcome_message(Localizer::new(self.boot.profile_config.ui.locale()), &outcome);
            tracing::info!(op = ?outcome.op, status = ?outcome.status, "manual IR operation finished");
            self.show_left_overlay_toast(message);
        }
    }
}
