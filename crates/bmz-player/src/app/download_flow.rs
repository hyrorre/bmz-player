use super::*;
use crate::song_download::{BulkDownloadBatch, BulkDownloadScope, prepare_bulk_download};

impl WinitApp {
    pub(super) fn bulk_download_scope(&self) -> Option<BulkDownloadScope> {
        if !self.select_maintenance_allowed() {
            return None;
        }
        self.select
            .select_items
            .get(self.select.selected_index)
            .and_then(|item| match item {
                SelectItem::Folder { path, .. } => BulkDownloadScope::from_path(path),
                _ => None,
            })
            .or_else(|| {
                self.select.folder_stack.last().and_then(|path| BulkDownloadScope::from_path(path))
            })
    }

    pub(super) fn apply_download_command(&mut self, command: crate::ui::DownloadUiCommand) {
        use crate::ui::DownloadUiCommand;
        match command {
            DownloadUiCommand::Cancel(id) => self.jobs.chart_downloads.cancel(id),
            DownloadUiCommand::Retry(id) => self.jobs.chart_downloads.retry(id),
            DownloadUiCommand::CancelQueued => self.jobs.chart_downloads.cancel_queued(),
            DownloadUiCommand::Prepare(scope) => {
                if !self.select_maintenance_allowed() || self.jobs.pending_bulk_download.is_some() {
                    return;
                }
                let db_path = self.boot.app_paths.library_db.clone();
                let config = self.boot.app_config.downloads.clone();
                let data_dir = self.boot.app_paths.data_dir.clone();
                let (tx, rx) = mpsc::channel();
                match thread::Builder::new().name("chart-download-plan".into()).spawn(move || {
                    let result = LibraryDatabase::open_read_only(&db_path)
                        .and_then(|db| prepare_bulk_download(&db, &config, scope, &data_dir));
                    let _ = tx.send(result);
                }) {
                    Ok(_) => {
                        self.jobs.bulk_download = crate::song_download::BulkDownloadPreview {
                            preparing: true,
                            ..Default::default()
                        };
                        self.jobs.pending_bulk_download = Some(rx);
                    }
                    Err(error) => self.jobs.bulk_download.error = Some(error.to_string()),
                }
            }
            DownloadUiCommand::Confirm => {
                if !self.select_maintenance_allowed() {
                    return;
                }
                let Some(plan) = self.jobs.bulk_download.plan.take() else {
                    return;
                };
                let mut batch = BulkDownloadBatch {
                    scope: plan.scope,
                    counts: plan.counts,
                    targets: Vec::new(),
                };
                for mut request in plan.requests {
                    self.jobs.download_request_id += 1;
                    request.request_id = self.jobs.download_request_id;
                    let identity = request.identity.clone();
                    let (id, merged) = self.jobs.chart_downloads.submit(request);
                    batch.targets.push((id, identity));
                    if merged {
                        batch.counts.merged += 1;
                    } else {
                        batch.counts.queued += 1;
                    }
                }
                self.jobs.chart_downloads.batches.push(batch);
                self.jobs
                    .chart_downloads
                    .poll(self.boot.app_config.downloads.http_parallelism as usize);
            }
            DownloadUiCommand::Dismiss => {
                self.jobs.bulk_download.plan = None;
                self.jobs.bulk_download.error = None;
            }
        }
    }

    pub(super) fn poll_bulk_download_plan(&mut self) {
        let Some(rx) = &self.jobs.pending_bulk_download else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(anyhow::anyhow!("download planner disconnected"))
            }
        };
        self.jobs.pending_bulk_download = None;
        self.jobs.bulk_download.preparing = false;
        match result {
            Ok(plan) => self.jobs.bulk_download.plan = Some(plan),
            Err(error) => self.jobs.bulk_download.error = Some(format!("{error:#}")),
        }
    }

    /// Fills chart rows' download bars from the task list, as LR2oraja
    /// Endless Dream draws them with the songlist `graph`.
    pub(super) fn apply_select_download_progress(&self, rows: &mut [SelectRowSnapshot]) {
        if self.jobs.chart_downloads.tasks().is_empty() {
            return;
        }
        let now = Instant::now();
        for row in rows {
            let Some(SelectItem::Chart(chart)) = self.select.select_items.get(row.index as usize)
            else {
                continue;
            };
            row.download_progress = self
                .jobs
                .chart_downloads
                .chart_row_task(chart.score_sha256(), chart.score_md5(), now)
                .map(download_row_progress);
        }
    }
}

/// Endless Dream's bar: graph cells 0 (none), 1 (failed), 4 (in progress or
/// cancelled) and 6 (completed).
pub(super) fn download_row_progress(
    task: &crate::song_download::DownloadTask,
) -> bmz_render::scene::SelectRowDownloadProgress {
    use crate::song_download::DownloadState;
    let (fraction, lamp) = match task.state() {
        DownloadState::Queued | DownloadState::Resolving => (0.0, 0),
        DownloadState::Downloading => {
            let (bytes, total) = task.control.bytes();
            (total.map_or(0.0, |total| bytes as f32 / total as f32), 4)
        }
        DownloadState::Extracting | DownloadState::Scanning | DownloadState::Cancelled => (1.0, 4),
        DownloadState::Completed => (1.0, 6),
        DownloadState::Failed => (1.0, 1),
    };
    bmz_render::scene::SelectRowDownloadProgress { fraction: fraction.clamp(0.0, 1.0), lamp }
}
