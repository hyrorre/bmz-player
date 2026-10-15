//! App-owned queue. Only running transfers own a worker; queued requests are data.
use super::*;
use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
    mpsc,
};

pub type DownloadTaskId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DownloadState {
    Queued,
    Resolving,
    Downloading,
    Extracting,
    Scanning,
    Completed,
    Failed,
    Cancelled,
}

impl DownloadState {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Default)]
pub struct DownloadControl {
    cancelled: AtomicBool,
    notify: tokio::sync::Notify,
    state: AtomicU8,
    bytes: AtomicU64,
    total: AtomicU64,
}

impl DownloadControl {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_one();
    }
    pub fn check(&self) -> Result<()> {
        anyhow::ensure!(!self.is_cancelled(), "download cancelled");
        Ok(())
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub fn set_state(&self, state: DownloadState) {
        self.state.store(state as u8, Ordering::Relaxed);
    }
    pub fn state(&self) -> DownloadState {
        match self.state.load(Ordering::Relaxed) {
            0 => DownloadState::Queued,
            1 => DownloadState::Resolving,
            2 => DownloadState::Downloading,
            3 => DownloadState::Extracting,
            4 => DownloadState::Scanning,
            5 => DownloadState::Completed,
            6 => DownloadState::Failed,
            _ => DownloadState::Cancelled,
        }
    }
    pub fn bytes(&self) -> (u64, Option<u64>) {
        let total = self.total.load(Ordering::Relaxed);
        (self.bytes.load(Ordering::Relaxed), (total > 0).then_some(total))
    }
    pub(super) fn progress(&self, bytes: u64, total: Option<u64>) {
        self.bytes.store(bytes, Ordering::Relaxed);
        self.total.store(total.unwrap_or(0), Ordering::Relaxed);
    }
    pub(super) async fn cancelled(&self) {
        if !self.is_cancelled() {
            self.notify.notified().await;
        }
    }
}

#[derive(Default)]
pub struct DownloadServices {
    pub(super) extraction: Mutex<()>,
    pub(super) archives: Mutex<std::collections::HashMap<String, Arc<Mutex<ArchiveResult>>>>,
}

pub(super) type ArchiveResult = Option<std::result::Result<ChartDownloadResult, String>>;

pub struct DownloadTask {
    pub id: DownloadTaskId,
    pub requests: Vec<ChartDownloadRequest>,
    pub control: Arc<DownloadControl>,
    pub error: Option<String>,
    pub result: Option<ChartDownloadResult>,
    worker: Option<mpsc::Receiver<Result<ChartDownloadResult>>>,
    scan_pending: bool,
}

impl DownloadTask {
    pub fn state(&self) -> DownloadState {
        self.control.state()
    }
    pub fn source_name(&self) -> &'static str {
        match self.requests[0].action {
            MissingChartAction::Ipfs { .. } => "IPFS",
            _ => "HTTP",
        }
    }
}

#[derive(Default)]
pub struct ChartDownloadManager {
    tasks: Vec<DownloadTask>,
    queues: [VecDeque<usize>; 2],
    active: Vec<usize>,
    services: Arc<DownloadServices>,
    next_id: u64,
}

impl ChartDownloadManager {
    pub fn tasks(&self) -> &[DownloadTask] {
        &self.tasks
    }
    pub fn task(&self, id: DownloadTaskId) -> Option<&DownloadTask> {
        self.tasks.iter().find(|task| task.id == id)
    }

    /// Returns (task id, merged). Scope includes the exact API configuration and
    /// destination. Credentials/query parameters are never stripped for sharing.
    pub fn submit(&mut self, request: ChartDownloadRequest) -> (DownloadTaskId, bool) {
        if let Some(task) = self.tasks.iter_mut().find(|task| {
            !matches!(task.state(), DownloadState::Failed | DownloadState::Cancelled)
                && task.requests.iter().any(|other| requests_share_transfer(other, &request))
        }) {
            task.requests.push(request);
            if task.state() == DownloadState::Completed {
                task.control.set_state(DownloadState::Scanning);
                task.scan_pending = true;
            }
            return (task.id, true);
        }
        self.next_id += 1;
        let id = self.next_id;
        tracing::info!(task_id = id, identity = ?request.identity, "queued chart download");
        self.queues[usize::from(matches!(request.action, MissingChartAction::Ipfs { .. }))]
            .push_back(self.tasks.len());
        self.tasks.push(DownloadTask {
            id,
            requests: vec![request],
            control: Arc::default(),
            error: None,
            result: None,
            worker: None,
            scan_pending: false,
        });
        (id, false)
    }

    pub fn cancel(&mut self, id: DownloadTaskId) {
        if let Some(task) = self.tasks.iter_mut().find(|task| task.id == id)
            && !task.state().terminal()
        {
            task.control.cancel();
            // Committed files and ongoing DB scans must never be deleted.
            if task.worker.is_none() {
                task.control.set_state(DownloadState::Cancelled);
            }
        }
    }
    pub fn cancel_queued(&mut self) {
        for task in &mut self.tasks {
            if task.state() == DownloadState::Queued {
                task.control.cancel();
                task.control.set_state(DownloadState::Cancelled);
            }
        }
    }
    pub fn retry(&mut self, id: DownloadTaskId) {
        if let Some((index, task)) =
            self.tasks.iter_mut().enumerate().find(|(_, task)| task.id == id)
            && matches!(task.state(), DownloadState::Failed | DownloadState::Cancelled)
            && task.worker.is_none()
        {
            task.control = Arc::default();
            task.error = None;
            if task.result.is_some() {
                task.control.set_state(DownloadState::Scanning);
                task.scan_pending = true;
            } else {
                self.queues[usize::from(task.source_name() == "IPFS")].push_back(index);
            }
        }
    }

    pub fn poll(&mut self, http_parallelism: usize) {
        let mut active = Vec::new();
        for index in std::mem::take(&mut self.active) {
            let task = &mut self.tasks[index];
            let Some(worker) = &task.worker else {
                continue;
            };
            let result = match worker.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => {
                    active.push(index);
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err(anyhow::anyhow!("download worker disconnected"))
                }
            };
            task.worker = None;
            match result {
                Ok(result) => {
                    task.result = Some(result);
                    task.scan_pending = true;
                    task.control.set_state(DownloadState::Scanning);
                }
                Err(error) => {
                    task.error = Some(format!("{error:#}"));
                    task.control.set_state(if task.control.is_cancelled() {
                        DownloadState::Cancelled
                    } else {
                        DownloadState::Failed
                    });
                    tracing::warn!(task_id = task.id, identity = ?task.requests[0].identity, source = task.source_name(), error = ?task.error, "chart download ended");
                }
            }
        }
        self.active = active;
        for queue in 0..2 {
            let limit = if queue == 0 { http_parallelism.clamp(1, 5) } else { 1 };
            let running = self
                .active
                .iter()
                .filter(|&&i| usize::from(self.tasks[i].source_name() == "IPFS") == queue)
                .count();
            let mut slots = limit.saturating_sub(running);
            while slots > 0 {
                let Some(index) = self.queues[queue].pop_front() else {
                    break;
                };
                let task = &mut self.tasks[index];
                if task.state() != DownloadState::Queued {
                    continue;
                }
                slots -= 1;
                task.control.set_state(DownloadState::Resolving);
                let control = Arc::clone(&task.control);
                let services = Arc::clone(&self.services);
                let request = task.requests[0].clone();
                let (tx, rx) = mpsc::channel();
                match std::thread::Builder::new().name(format!("chart-download-{}", task.id)).spawn(
                    move || {
                        let result = (|| {
                            let runtime = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()?;
                            runtime.block_on(download_chart_controlled(request, control, services))
                        })();
                        let _ = tx.send(result);
                    },
                ) {
                    Ok(_) => {
                        task.worker = Some(rx);
                        self.active.push(index);
                    }
                    Err(error) => {
                        task.error = Some(error.to_string());
                        task.control.set_state(DownloadState::Failed);
                    }
                }
            }
        }
    }

    pub fn take_scan_requests(&mut self) -> Vec<(DownloadTaskId, ChartDownloadResult)> {
        self.tasks
            .iter_mut()
            .filter_map(|task| {
                if !std::mem::take(&mut task.scan_pending) {
                    return None;
                }
                task.result.clone().map(|result| (task.id, result))
            })
            .collect()
    }

    pub fn finish_scan(
        &mut self,
        id: DownloadTaskId,
        registered: &[ChartDownloadIdentity],
        error: Option<&str>,
    ) {
        let Some(task) = self.tasks.iter_mut().find(|task| task.id == id) else {
            return;
        };
        if task.state() == DownloadState::Cancelled {
            return;
        }
        let success = error.is_none()
            && task.requests.iter().all(|r| {
                registered.iter().any(|identity| {
                    r.identity.sha256.is_some_and(|sha| identity.sha256 == Some(sha))
                        && r.identity.md5.is_none_or(|md5| identity.md5 == Some(md5))
                        || r.identity.sha256.is_none()
                            && r.identity.md5.is_some()
                            && r.identity.md5 == identity.md5
                })
            });
        task.error = (!success).then(|| {
            error.unwrap_or("requested chart not found in the library after scanning").to_string()
        });
        task.control.set_state(if success {
            DownloadState::Completed
        } else {
            DownloadState::Failed
        });
        tracing::info!(task_id = id, state = ?task.state(), error = ?task.error, "chart registration finished");
    }
}

impl Drop for ChartDownloadManager {
    fn drop(&mut self) {
        for task in &self.tasks {
            if task.worker.is_some() {
                task.control.cancel();
            }
        }
    }
}

fn requests_share_transfer(a: &ChartDownloadRequest, b: &ChartDownloadRequest) -> bool {
    if a.data_dir != b.data_dir {
        return false;
    }
    let same_hash = match (a.identity.sha256, b.identity.sha256) {
        (Some(a), Some(b)) => a == b,
        _ => a.identity.md5.is_some() && a.identity.md5 == b.identity.md5,
    };
    match (&a.action, &b.action) {
        (
            MissingChartAction::Http { api_url: a_url, md5: a_md5, sha256: a_sha },
            MissingChartAction::Http { api_url: b_url, md5: b_md5, sha256: b_sha },
        ) => {
            a_url == b_url
                && (same_hash
                    || match (
                        expand_http_api_url(a_url, a_md5, a_sha),
                        expand_http_api_url(b_url, b_md5, b_sha),
                    ) {
                        (Ok(a), Ok(b)) => a == b,
                        _ => false,
                    })
        }
        (
            MissingChartAction::Ipfs { api_url: a_url, cids: a_cids },
            MissingChartAction::Ipfs { api_url: b_url, cids: b_cids },
        ) => a_url == b_url && a_cids == b_cids,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;

    #[test]
    fn bounded_queue_accepts_more_work_and_waits_for_db_registration() {
        for parallel in [1, 3, 5] {
            let root = tempfile::tempdir().unwrap();
            let server = HttpFixture::new(
                archive(&[("song/chart.bms", b"#TITLE Test")]),
                Duration::from_millis(60),
                200,
            );
            let api = format!("{}/{{md5}}", server.url);
            let mut manager = ChartDownloadManager::default();
            for number in 1..=7 {
                manager.submit(request(root.path(), &api, number));
            }
            manager.poll(parallel);
            manager.submit(request(root.path(), &api, 8));
            wait(&mut manager, parallel, |m| {
                m.tasks().iter().all(|t| t.state() == DownloadState::Scanning)
            });
            assert_eq!(server.requests.load(Ordering::Relaxed), 8);
            assert!(server.maximum.load(Ordering::Relaxed) <= parallel);
            assert_eq!(manager.take_scan_requests().len(), 8);
            for task in &manager.tasks {
                assert!(task.result.as_ref().unwrap().chart_dir.join("song/chart.bms").is_file());
            }
            let id = manager.tasks[0].id;
            manager.finish_scan(id, &[], None);
            assert_eq!(manager.task(id).unwrap().state(), DownloadState::Failed);
            manager.retry(id);
            assert_eq!(manager.take_scan_requests().len(), 1);
            let identity = manager.task(id).unwrap().requests[0].identity.clone();
            manager.finish_scan(id, &[identity], None);
            assert_eq!(manager.task(id).unwrap().state(), DownloadState::Completed);
        }
    }

    #[test]
    fn duplicate_hashes_merge_but_source_conditions_do_not() {
        let root = tempfile::tempdir().unwrap();
        let mut manager = ChartDownloadManager::default();
        let r = request(root.path(), "https://test.invalid/{md5}", 1);
        let (id, merged) = manager.submit(r.clone());
        assert!(!merged);
        assert_eq!(manager.submit(r.clone()), (id, true));
        let mut other = r;
        other.action = MissingChartAction::Http {
            api_url: "https://test.invalid/{md5}?token=other".into(),
            md5: "01".repeat(16),
            sha256: "01".repeat(32),
        };
        assert!(!manager.submit(other).1);
        manager.cancel(id);
        assert_eq!(manager.task(id).unwrap().state(), DownloadState::Cancelled);
        manager.retry(id);
        assert_eq!(manager.task(id).unwrap().state(), DownloadState::Queued);
    }

    #[test]
    fn resolved_archive_is_shared_by_different_charts_and_batches() {
        use std::sync::atomic::AtomicUsize;
        let root = tempfile::tempdir().unwrap();
        let archives = Arc::new(AtomicUsize::new(0));
        let worker_archives = Arc::clone(&archives);
        let server = HttpFixture::with_handler(
            move |path| {
                if path == "/archive" {
                    worker_archives.fetch_add(1, Ordering::Relaxed);
                    (
                        200,
                        "application/octet-stream".into(),
                        archive(&[("a.bms", b"A"), ("b.bms", b"B")]),
                    )
                } else {
                    (200, "application/json".into(), br#"{"downloadURL":"/archive"}"#.to_vec())
                }
            },
            Duration::from_millis(20),
        );
        let api = format!("{}/meta/{{md5}}", server.url);
        let mut manager = ChartDownloadManager::default();
        manager.submit(request(root.path(), &api, 1));
        manager.submit(request(root.path(), &api, 2));
        wait(&mut manager, 3, |m| m.tasks().iter().all(|t| t.state() == DownloadState::Scanning));
        assert_eq!(archives.load(Ordering::Relaxed), 1);
        assert_eq!(
            manager.tasks()[0].result.as_ref().unwrap().chart_dir,
            manager.tasks()[1].result.as_ref().unwrap().chart_dir
        );
        manager.submit(request(root.path(), &api, 3));
        wait(&mut manager, 3, |m| m.tasks().iter().all(|t| t.state() == DownloadState::Scanning));
        assert_eq!(archives.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn queued_and_running_cancellation_clean_staging_and_allow_retry() {
        let root = tempfile::tempdir().unwrap();
        let server = HttpFixture::new(
            archive(&[("chart.bms", b"#TITLE Test")]),
            Duration::from_millis(100),
            200,
        );
        let api = format!("{}/{{md5}}", server.url);
        let mut manager = ChartDownloadManager::default();
        let (first, _) = manager.submit(request(root.path(), &api, 1));
        let (second, _) = manager.submit(request(root.path(), &api, 2));
        manager.poll(1);
        manager.cancel_queued();
        manager.cancel(first);
        wait(&mut manager, 1, |m| m.tasks().iter().all(|t| t.state() == DownloadState::Cancelled));
        assert_eq!(manager.task(second).unwrap().state(), DownloadState::Cancelled);
        if root.path().join("songs/http").exists() {
            assert_eq!(std::fs::read_dir(root.path().join("songs/http")).unwrap().count(), 0);
        }
        manager.retry(first);
        wait(&mut manager, 1, |m| m.task(first).unwrap().state() == DownloadState::Scanning);
        assert!(manager.task(first).unwrap().result.is_some());
    }
}
