use super::*;

impl WinitApp {
    pub(in crate::app) fn spawn_table_fetch(&mut self, url: String) {
        self.spawn_table_fetches(vec![url], "table fetch".to_string());
    }

    pub(in crate::app) fn start_startup_course_link_repair_after_first_frame(&mut self) {
        if !self.select_maintenance_allowed()
            || !self.jobs.startup_course_link_repair
            || self.jobs.pending_course_link_repair.is_some()
        {
            return;
        }
        self.jobs.startup_course_link_repair = false;
        let library_db_path = self.boot.app_paths.library_db.clone();
        let (tx, rx) = mpsc::channel();
        let event_proxy = self.event_proxy.clone();
        match thread::Builder::new().name("course-link-repair".to_string()).spawn(move || {
            let result =
                crate::storage::migration::repair_course_entry_chart_links_once(&library_db_path);
            let _ = tx.send(result);
            let _ = event_proxy.send_event(AppUserEvent::CourseLinkRepair);
        }) {
            Ok(_) => {
                self.jobs.pending_course_link_repair = Some(rx);
                tracing::info!("started one-time course link repair worker");
            }
            Err(error) => {
                tracing::warn!(%error, "failed to start course link repair worker");
            }
        }
    }

    pub(in crate::app) fn poll_pending_course_link_repair(&mut self) {
        let Some(rx) = self.jobs.pending_course_link_repair.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(run)) => {
                tracing::info!(
                    already_completed = run.already_completed,
                    scanned_entries = run.scanned_entries,
                    repaired_entries = run.repaired_entries,
                    "course link repair worker complete"
                );
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "course link repair worker failed");
                // 次回起動ではmaintenance markerが無いため再試行される。
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.jobs.pending_course_link_repair = Some(rx);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("course link repair worker disconnected");
            }
        }
    }

    /// 起動直後の初回描画が完了してから、未取得の有効な表を取得する。
    pub(in crate::app) fn start_startup_table_fetch_after_first_frame(&mut self) {
        if !self.select_maintenance_allowed() {
            return;
        }
        let Some(urls) = self.jobs.table_fetch.startup_urls.take() else {
            return;
        };
        self.spawn_table_fetches(urls, "startup table fetch".to_string());
        self.spawn_rian_table_fetch(false);
    }

    pub(in crate::app) fn spawn_rian_table_fetch(&mut self, manual: bool) {
        if !self.select_maintenance_allowed() {
            self.jobs.table_fetch.rian_refresh_queued = true;
            self.jobs.table_fetch.rian_refresh_manual |= manual;
            tracing::debug!(manual, "queued rianIR table refresh until Select");
            return;
        }
        let Some(identity) = self.jobs.table_fetch.rian_identity.clone() else {
            return;
        };
        if self.jobs.table_fetch.pending_rian.is_some() {
            tracing::debug!("rianIR table fetch already in progress");
            return;
        }
        let now = Instant::now();
        let minimum_interval =
            if manual { RIAN_TABLE_MANUAL_REFRESH_COOLDOWN } else { RIAN_TABLE_REFRESH_INTERVAL };
        if self
            .jobs
            .table_fetch
            .rian_last_started_at
            .is_some_and(|started| now.duration_since(started) < minimum_interval)
        {
            return;
        }

        let generation = self.jobs.table_fetch.rian_generation;
        let fetched_at = now_unix_seconds();
        let (tx, rx) = mpsc::channel();
        let event_proxy = self.event_proxy.clone();
        let worker_identity = identity.clone();
        let worker_profile_root = self.boot.profile_paths.root_dir.clone();
        let mut maintenance_allowed = self.jobs.maintenance_select_tx.subscribe();
        thread::Builder::new()
            .name("rian-table-fetch".to_string())
            .spawn(move || {
                let result = match tokio::runtime::Runtime::new()
                    .context("failed to create tokio runtime")
                {
                    Err(error) => RianTableFetchOutcome::Completed(Err(error)),
                    Ok(runtime) => runtime.block_on(async {
                        tokio::select! {
                            biased;
                            _ = async {
                                while *maintenance_allowed.borrow() {
                                    if maintenance_allowed.changed().await.is_err() {
                                        break;
                                    }
                                }
                            } => RianTableFetchOutcome::Paused,
                            result = crate::ir::table::fetch_account_tables(
                                &worker_identity,
                                &worker_profile_root,
                                fetched_at,
                            ) => RianTableFetchOutcome::Completed(result),
                        }
                    }),
                };
                let _ = tx.send(RianTableFetchWorkerResult {
                    generation,
                    identity: worker_identity,
                    result,
                });
                let _ = event_proxy.send_event(AppUserEvent::TableFetch);
            })
            .expect("failed to spawn rianIR table fetch thread");
        self.jobs.table_fetch.pending_rian = Some(rx);
        self.jobs.table_fetch.rian_last_started_at = Some(now);
        self.jobs.table_fetch.rian_next_refresh_at = now.checked_add(RIAN_TABLE_REFRESH_INTERVAL);
        tracing::info!(
            provider = %identity.provider_key,
            manual,
            "started rianIR table fetch"
        );
    }

    pub(in crate::app) fn poll_pending_rian_table_fetch(&mut self) {
        let Some(rx) = self.jobs.table_fetch.pending_rian.take() else {
            return;
        };
        let provider_name = self
            .jobs
            .table_fetch
            .rian_identity
            .as_ref()
            .map_or("IR", RianTableIdentity::display_name);
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                self.jobs.table_fetch.pending_rian = Some(rx);
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                tracing::warn!("rianIR table fetch worker disconnected");
                self.show_left_overlay_toast(format!("{provider_name} TABLE: worker disconnected"));
                return;
            }
        };

        if result.generation != self.jobs.table_fetch.rian_generation
            || self.jobs.table_fetch.rian_identity.as_ref() != Some(&result.identity)
        {
            tracing::info!("ignored stale rianIR table fetch result");
            return;
        }

        let provider_name = result.identity.display_name();
        match result.result {
            RianTableFetchOutcome::Completed(Ok(tables)) => {
                match crate::ir::table::store_account_tables(
                    &mut self.boot.library_db,
                    &result.identity,
                    &tables,
                ) {
                    Ok((table_count, entry_count)) => {
                        tracing::info!(
                            tables = table_count,
                            entries = entry_count,
                            "rianIR table fetch complete"
                        );
                        self.refresh_difficulty_tables_and_select();
                        self.show_left_overlay_toast(format!(
                            "{provider_name} TABLE: {table_count} tables, {entry_count} entries"
                        ));
                    }
                    Err(error) => {
                        tracing::error!(%error, "failed to store rianIR tables");
                        self.show_left_overlay_toast(format!(
                            "{provider_name} TABLE: cache update failed"
                        ));
                    }
                }
            }
            RianTableFetchOutcome::Completed(Err(error)) => {
                // stale-while-revalidate: 既存キャッシュは消さず、そのまま選曲に残す。
                tracing::warn!(%error, "failed to fetch rianIR tables; keeping cached tables");
                self.show_left_overlay_toast(format!(
                    "{provider_name} TABLE: fetch failed (using cache)"
                ));
            }
            RianTableFetchOutcome::Paused => {
                self.jobs.table_fetch.rian_last_started_at = None;
                self.jobs.table_fetch.rian_next_refresh_at = None;
                self.jobs.table_fetch.rian_refresh_queued = true;
                tracing::debug!("paused rianIR table fetch outside Select");
            }
        }
    }

    pub(in crate::app) fn maybe_start_periodic_rian_table_fetch(&mut self) {
        if !self.select_maintenance_allowed() {
            return;
        }
        if self.jobs.table_fetch.pending_rian.is_some() {
            return;
        }
        if self
            .jobs
            .table_fetch
            .rian_next_refresh_at
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.spawn_rian_table_fetch(false);
        }
    }

    pub(in crate::app) fn start_queued_rian_table_fetch_if_idle(&mut self) {
        if self.jobs.table_fetch.pending_rian.is_some()
            || !self.jobs.table_fetch.rian_refresh_queued
        {
            return;
        }
        let manual = std::mem::take(&mut self.jobs.table_fetch.rian_refresh_manual);
        self.jobs.table_fetch.rian_refresh_queued = false;
        self.spawn_rian_table_fetch(manual);
    }

    pub(in crate::app) fn reconcile_rian_table_identity(&mut self) {
        let next = RianTableIdentity::from_ir_config(&self.boot.profile_config.ir);
        if next == self.jobs.table_fetch.rian_identity {
            return;
        }
        if !self.select_maintenance_allowed() {
            self.jobs.table_fetch.rian_refresh_queued = true;
            return;
        }

        let previous = self.jobs.table_fetch.rian_identity.take();
        self.jobs.table_fetch.rian_generation =
            self.jobs.table_fetch.rian_generation.wrapping_add(1);
        self.jobs.table_fetch.pending_rian = None;
        self.jobs.table_fetch.rian_last_started_at = None;
        self.jobs.table_fetch.rian_next_refresh_at = None;
        self.jobs.table_fetch.rian_refresh_queued = false;
        self.jobs.table_fetch.rian_refresh_manual = false;

        if let Some(previous) = &previous {
            match self
                .boot
                .library_db
                .delete_difficulty_tables_by_source_prefix(previous.source_prefix())
            {
                Ok(removed) => tracing::info!(
                    removed,
                    "removed rianIR account table cache after identity change"
                ),
                Err(error) => tracing::warn!(%error, "failed to remove old rianIR table cache"),
            }
            if let Err(error) =
                self.boot.library_db.delete_table_courses_by_source_prefix(previous.source_prefix())
            {
                tracing::warn!(%error, "failed to remove old rianIR course cache");
            }
            if self.select.folder_stack.iter().any(|path| path.contains(previous.source_prefix())) {
                self.select.folder_stack.clear();
                self.select.selected_index_stack.clear();
                self.select.selected_index = 0;
                self.reset_selected_replay_slot();
            }
        }

        self.jobs.table_fetch.rian_identity = next;
        self.refresh_difficulty_tables_and_select();
        if self.first_frame_startup_completed {
            self.spawn_rian_table_fetch(true);
        }
    }

    pub(in crate::app) fn refresh_difficulty_tables_and_select(&mut self) {
        match self.boot.library_db.list_difficulty_tables() {
            Ok(tables) => self.select.difficulty_tables = tables,
            Err(error) => tracing::warn!(%error, "failed to refresh difficulty table metadata"),
        }
        self.select.table_breadcrumb_cache.borrow_mut().clear();
        self.invalidate_select_folder_summaries();
        self.reload_select_items();
    }

    pub(in crate::app) fn spawn_table_fetches(&mut self, urls: Vec<String>, label: String) {
        let urls = self.jobs.table_fetch.filter_new_urls(urls);
        if urls.is_empty() {
            return;
        }
        if !self.select_maintenance_allowed() || self.jobs.table_fetch.pending.is_some() {
            self.jobs.table_fetch.queued_urls.extend(urls);
            tracing::debug!(
                queued = self.jobs.table_fetch.queued_urls.len(),
                %label,
                "queued table fetch until Select maintenance is available"
            );
            return;
        }
        let (tx, rx) = mpsc::channel();
        let fetch_urls = urls.clone();
        let progress_tx = tx.clone();
        let event_proxy = self.event_proxy.clone();
        let maintenance_allowed = self.jobs.maintenance_select_tx.subscribe();
        thread::Builder::new()
            .name("table-fetch".to_string())
            .spawn(move || {
                let result = (|| -> Result<crate::table_cmd::TableFetchDownloadBatchResult> {
                    let rt =
                        tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
                    rt.block_on(crate::table_cmd::download_table_urls_with_progress(
                        fetch_urls,
                        maintenance_allowed,
                        |outcome| {
                            let _ = progress_tx.send(TableFetchWorkerEvent::Downloaded(outcome));
                            let _ = event_proxy.send_event(AppUserEvent::TableFetch);
                        },
                    ))
                })();
                let _ = tx.send(TableFetchWorkerEvent::Finished(result));
                let _ = event_proxy.send_event(AppUserEvent::TableFetch);
            })
            .expect("failed to spawn table fetch thread");
        self.jobs.table_fetch.pending_urls = urls.iter().cloned().collect();
        self.jobs.table_fetch.progress = Some(TableFetchProgress {
            label: label.clone(),
            total: urls.len(),
            completed: 0,
            succeeded: 0,
            failed: 0,
            outcomes: Vec::with_capacity(urls.len()),
        });
        self.jobs.table_fetch.pending = Some(rx);
        tracing::info!(count = urls.len(), %label, "started table fetch");
    }

    pub(in crate::app) fn poll_pending_table_fetch(&mut self) {
        let Some(rx) = self.jobs.table_fetch.pending.take() else {
            return;
        };
        let mut keep_pending = true;
        loop {
            match rx.try_recv() {
                Ok(TableFetchWorkerEvent::Downloaded(downloaded)) => {
                    let outcome = match downloaded {
                        crate::table_cmd::TableFetchDownloadOutcome::Succeeded(table) => {
                            match crate::table_cmd::store_fetched_table(
                                &mut self.boot.library_db,
                                &table,
                            ) {
                                Ok(success) => TableFetchOutcome::Succeeded(success),
                                Err(error) => {
                                    TableFetchOutcome::Failed(crate::table_cmd::TableFetchFailure {
                                        url: table.source_url,
                                        error: format!(
                                            "failed to store difficulty table: {error:#}"
                                        ),
                                    })
                                }
                            }
                        }
                        crate::table_cmd::TableFetchDownloadOutcome::Failed(failure) => {
                            TableFetchOutcome::Failed(failure)
                        }
                    };
                    if let Some(progress) = &mut self.jobs.table_fetch.progress {
                        progress.completed =
                            progress.completed.saturating_add(1).min(progress.total);
                        match &outcome {
                            TableFetchOutcome::Succeeded(_) => progress.succeeded += 1,
                            TableFetchOutcome::Failed(_) => progress.failed += 1,
                        }
                        progress.outcomes.push(outcome.clone());
                    }
                    match &outcome {
                        TableFetchOutcome::Succeeded(success) => tracing::info!(
                            url = %success.url,
                            name = %success.name,
                            entries = success.entries,
                            courses = success.courses,
                            "difficulty table fetched"
                        ),
                        TableFetchOutcome::Failed(failure) => tracing::warn!(
                            url = %failure.url,
                            error = %failure.error,
                            "failed to fetch difficulty table"
                        ),
                    }
                }
                Ok(TableFetchWorkerEvent::Finished(Ok(batch))) => {
                    keep_pending = false;
                    let outcomes = self
                        .jobs
                        .table_fetch
                        .progress
                        .as_mut()
                        .map(|progress| std::mem::take(&mut progress.outcomes))
                        .unwrap_or_default();
                    let completed = batch.requested.saturating_sub(batch.remaining_urls.len());
                    if completed > 0 {
                        self.finish_table_fetch(TableFetchReport {
                            requested: completed,
                            outcomes,
                        });
                    }
                    if !batch.remaining_urls.is_empty() {
                        tracing::debug!(
                            remaining = batch.remaining_urls.len(),
                            "paused table fetch outside Select"
                        );
                        self.jobs.table_fetch.queued_urls.extend(batch.remaining_urls);
                    }
                    break;
                }
                Ok(TableFetchWorkerEvent::Finished(Err(error))) => {
                    keep_pending = false;
                    let label = self
                        .jobs
                        .table_fetch
                        .progress
                        .as_ref()
                        .map(|progress| progress.label.as_str())
                        .unwrap_or("table fetch");
                    tracing::error!(%label, %error, "table fetch worker failed");
                    self.show_left_overlay_toast("TABLE: fetch failed");
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    keep_pending = false;
                    tracing::warn!("table fetch worker disconnected");
                    self.show_left_overlay_toast("TABLE: worker disconnected");
                    break;
                }
            }
        }
        if keep_pending {
            self.jobs.table_fetch.pending = Some(rx);
            return;
        }

        self.jobs.table_fetch.pending_urls.clear();
        self.jobs.table_fetch.progress = None;
        self.start_queued_table_fetch_if_idle();
    }

    pub(in crate::app) fn start_queued_table_fetch_if_idle(&mut self) {
        if self.jobs.table_fetch.pending.is_some()
            || self.jobs.table_fetch.queued_urls.is_empty()
            || !self.select_maintenance_allowed()
        {
            return;
        }
        let queued = std::mem::take(&mut self.jobs.table_fetch.queued_urls);
        self.spawn_table_fetches(queued, "queued table fetch".to_string());
    }

    pub(in crate::app) fn finish_table_fetch(&mut self, report: TableFetchReport) {
        let succeeded = report.succeeded_count();
        let failed = report.failed_count();
        tracing::info!(requested = report.requested, succeeded, failed, "table fetch complete");
        if succeeded > 0 {
            match self.boot.library_db.list_difficulty_tables() {
                Ok(tables) => self.select.difficulty_tables = tables,
                Err(error) => {
                    tracing::warn!(%error, "failed to refresh difficulty table metadata")
                }
            }
            self.select.table_breadcrumb_cache.borrow_mut().clear();
            self.invalidate_select_folder_summaries();
            self.reload_select_items();
        }
        self.show_left_overlay_toast(format!("TABLE: {succeeded} succeeded, {failed} failed"));
    }
}
