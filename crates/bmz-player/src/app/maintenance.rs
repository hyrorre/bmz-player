use super::*;

fn select_maintenance_allowed(
    first_frame_startup_completed: bool,
    deferred_boot_pending: bool,
    view_state: AppViewState,
) -> bool {
    first_frame_startup_completed
        && !deferred_boot_pending
        && matches!(view_state, AppViewState::Select)
}

impl WinitApp {
    /// DB・filesystem・maintenance network処理を開始/反映してよい静かなシーン。
    ///
    /// 初回描画前とCLI直接起動待ちは見かけ上Selectでも直後にDecide/Playへ遷移するため、
    /// maintenanceを開始しない。コースの中間Resultを含むResultも次のSelectまで保留する。
    pub(super) fn select_maintenance_allowed(&self) -> bool {
        self.jobs.profile_change.is_none()
            && select_maintenance_allowed(
                self.first_frame_startup_completed,
                self.deferred_boot.is_some(),
                self.view_state(),
            )
    }

    /// network worker群へSelect実行許可を同期する。
    pub(super) fn sync_select_maintenance_gate(&self) {
        if !self.select_maintenance_allowed() {
            if let Some(progress) = &self.jobs.update_progress {
                progress.paused.store(true, Ordering::Relaxed);
                progress.cancel.store(true, Ordering::Relaxed);
            }
            crate::update::sparkle::pause();
        }
        self.jobs.maintenance_select_tx.send_if_modified(|allowed| {
            let next = self.select_maintenance_allowed();
            if *allowed == next {
                false
            } else {
                *allowed = next;
                true
            }
        });
    }

    /// Select中だけmaintenance workerの完了をDB/UIへ反映し、queued jobを開始する。
    pub(super) fn poll_select_maintenance(&mut self) {
        self.poll_profile_change();
        self.sync_select_maintenance_gate();
        if !self.select_maintenance_allowed() {
            return;
        }

        // Play中にIR設定が変わった場合、旧identityのqueued fetchを始める前に
        // cache世代を切り替える。
        self.reconcile_rian_table_identity();

        // 完了済み結果を先に反映する。channelはSelect外ではpollしないため、
        // workerがPlay中に完了してもDB保存・select model再構築はここまで遅延される。
        self.poll_pending_table_fetch();
        self.poll_pending_rian_table_fetch();
        self.poll_pending_chart_download();
        self.poll_pending_song_scan();
        self.poll_pending_replay_import();
        self.poll_pending_update_check();
        self.poll_pending_update_download();
        self.poll_update_handoff();
        self.poll_sparkle_update();
        self.poll_pending_rival_sync();
        self.poll_pending_course_link_repair();

        self.start_startup_course_link_repair_after_first_frame();
        self.start_startup_table_fetch_after_first_frame();
        self.start_startup_rival_sync_after_first_frame();
        self.start_queued_table_fetch_if_idle();
        self.start_queued_rian_table_fetch_if_idle();
        self.maybe_start_periodic_rian_table_fetch();
        self.start_queued_song_scan_if_idle();
        self.start_queued_update_check_if_idle();
    }

    pub(super) fn start_queued_song_scan_if_idle(&mut self) {
        if self.jobs.pending_song_scan.is_some() {
            return;
        }
        if let Some((roots, force, label)) = self.jobs.queued_song_scans.pop_front() {
            self.spawn_song_scan(roots, force, label);
        }
    }

    pub(super) fn start_queued_update_check_if_idle(&mut self) {
        if self.jobs.pending_update_check.is_some() {
            return;
        }
        if let Some((label, report_up_to_date)) = self.jobs.queued_update_check.take() {
            self.spawn_update_check(label, report_up_to_date);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maintenance_policy_allows_only_stable_select() {
        assert!(select_maintenance_allowed(true, false, AppViewState::Select));
        assert!(!select_maintenance_allowed(false, false, AppViewState::Select));
        assert!(!select_maintenance_allowed(true, true, AppViewState::Select));
        assert!(!select_maintenance_allowed(true, false, AppViewState::Decide));
        assert!(!select_maintenance_allowed(true, false, AppViewState::Play));
        assert!(!select_maintenance_allowed(true, false, AppViewState::Result));
    }
}
