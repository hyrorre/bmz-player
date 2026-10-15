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
}
