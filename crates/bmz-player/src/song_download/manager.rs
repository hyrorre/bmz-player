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

pub struct DownloadServices {
    #[cfg(test)]
    pub(super) timeout: Option<Duration>,
    pub(super) extraction: tokio::sync::Semaphore,
    pub(super) archives: Mutex<std::collections::HashMap<[u8; 32], Arc<SharedArchive>>>,
}

impl Default for DownloadServices {
    fn default() -> Self {
        Self {
            #[cfg(test)]
            timeout: None,
            extraction: tokio::sync::Semaphore::new(1),
            archives: Mutex::default(),
        }
    }
}

impl DownloadServices {
    fn evict_archive(&self, result: &ChartDownloadResult) {
        let Some(key) = result.archive_cache_key else {
            return;
        };
        let mut cache = self.archives.lock().unwrap_or_else(|error| error.into_inner());
        let same_result = cache.get(&key).is_some_and(|entry| {
            let entry = entry.result.lock().unwrap_or_else(|error| error.into_inner());
            matches!(entry.as_ref(), Some(Ok(current)) if current.chart_dir == result.chart_dir)
        });
        if same_result {
            cache.remove(&key);
        }
    }

    pub(super) fn timeout(&self) -> Duration {
        #[cfg(test)]
        if let Some(timeout) = self.timeout {
            return timeout;
        }
        Duration::from_secs(30 * 60)
    }
}

pub(super) type ArchiveResult = Option<std::result::Result<ChartDownloadResult, String>>;

/// One archive URL's extraction, shared by every task that resolves to it.
#[derive(Default)]
pub(super) struct SharedArchive {
    pub(super) result: Mutex<ArchiveResult>,
    /// Woken when the owner publishes a result or releases its claim.
    pub(super) ready: tokio::sync::Notify,
}

impl SharedArchive {
    pub(super) fn publish(&self, result: std::result::Result<ChartDownloadResult, String>) {
        *self.result.lock().unwrap_or_else(|error| error.into_inner()) = Some(result);
        self.ready.notify_waiters();
    }
}

pub struct DownloadTask {
    pub id: DownloadTaskId,
    pub requests: Vec<ChartDownloadRequest>,
    pub control: Arc<DownloadControl>,
    pub error: Option<String>,
    pub registration_missing: bool,
    /// The last retry fetched the same archive as the attempt that lacked the chart.
    pub archive_unchanged: bool,
    pub result: Option<ChartDownloadResult>,
    pub registered: HashSet<ChartDownloadIdentity>,
    worker: Option<mpsc::Receiver<Result<ChartDownloadResult>>>,
    /// Archive digest and folder of the attempt that lacked the requested chart.
    previous_archive: Option<([u8; 32], PathBuf)>,
    thread: Option<std::thread::JoinHandle<()>>,
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

    pub fn source_label(&self) -> String {
        let api = match &self.requests[0].action {
            MissingChartAction::Http { api_url, .. } | MissingChartAction::Ipfs { api_url, .. } => {
                api_url
            }
            _ => return self.source_name().into(),
        };
        let url = Url::parse(api).ok();
        match url.as_ref().and_then(Url::host_str) {
            Some("gingerrush.com") => "Ginger".into(),
            Some("bms.alvorna.com") => "Konmai".into(),
            Some("bms.wrigglebug.xyz") => "Wriggle".into(),
            Some(host) => format!("{} · {host}", self.source_name()),
            None => self.source_name().into(),
        }
    }
}

#[derive(Default)]
pub struct ChartDownloadManager {
    pub batches: Vec<BulkDownloadBatch>,
    tasks: Vec<DownloadTask>,
    queues: [VecDeque<usize>; 2],
    active: Vec<usize>,
    retired: Vec<usize>,
    scan_queue: VecDeque<usize>,
    failed_notices: VecDeque<DownloadTaskId>,
    transfers: std::collections::HashMap<super::transfer_key::TransferKey, usize>,
    services: Arc<DownloadServices>,
    next_id: u64,
}

impl ChartDownloadManager {
    pub fn tasks(&self) -> &[DownloadTask] {
        &self.tasks
    }
    pub fn task(&self, id: DownloadTaskId) -> Option<&DownloadTask> {
        id.checked_sub(1)
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| self.tasks.get(index))
            .filter(|task| task.id == id)
    }

    /// Returns (task id, merged). Scope includes the exact API configuration and
    /// destination. Credentials/query parameters are never stripped for sharing.
    pub fn submit(&mut self, request: ChartDownloadRequest) -> (DownloadTaskId, bool) {
        let keys = super::transfer_key::transfer_keys(&request);
        let existing =
            keys.iter().filter_map(|key| self.transfers.get(key).copied()).find(|index| {
                let task = &self.tasks[*index];
                // A cancel takes effect at the next poll; never merge into it.
                !matches!(task.state(), DownloadState::Failed | DownloadState::Cancelled)
                    && !task.control.is_cancelled()
                    && task.requests.iter().any(|other| requests_share_transfer(other, &request))
            });
        if let Some(index) = existing {
            for key in keys {
                self.transfers.insert(key, index);
            }
            let task = &mut self.tasks[index];
            tracing::info!(task_id = task.id, request_id = request.request_id, identity = ?request.identity, source = task.source_label(), "merged chart download request");
            task.requests.push(request);
            if task.state() == DownloadState::Completed {
                task.control.set_state(DownloadState::Scanning);
                task.scan_pending = true;
                self.scan_queue.push_back(index);
            }
            return (task.id, true);
        }
        self.next_id += 1;
        let id = self.next_id;
        let index = self.tasks.len();
        for key in keys {
            self.transfers.insert(key, index);
        }
        self.queues[usize::from(matches!(request.action, MissingChartAction::Ipfs { .. }))]
            .push_back(index);
        self.tasks.push(DownloadTask {
            id,
            requests: vec![request],
            control: Arc::default(),
            error: None,
            registration_missing: false,
            archive_unchanged: false,
            result: None,
            registered: HashSet::new(),
            worker: None,
            previous_archive: None,
            thread: None,
            scan_pending: false,
        });
        tracing::info!(task_id = id, request_id = self.tasks[index].requests[0].request_id, identity = ?self.tasks[index].requests[0].identity, source = self.tasks[index].source_label(), "queued chart download");
        (id, false)
    }

    pub fn cancel(&mut self, id: DownloadTaskId) {
        // Committed files and ongoing DB scans must never be deleted, so a
        // registering task can no longer be cancelled.
        if let Some(task) = self.tasks.iter_mut().find(|task| task.id == id)
            && !task.state().terminal()
            && task.state() != DownloadState::Scanning
        {
            task.control.cancel();
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
            if (task.registration_missing
                || task.result.as_ref().is_some_and(|result| !result.chart_dir.is_dir()))
                && let Some(result) = task.result.take()
            {
                self.services.evict_archive(&result);
                // The source may have updated the archive, so fetch it again,
                // but never extract the same bytes into a second folder.
                if task.registration_missing {
                    task.previous_archive = result
                        .archive_sha256
                        .filter(|_| result.chart_dir.is_dir())
                        .map(|digest| (digest, result.chart_dir.clone()));
                }
            }
            task.control = Arc::default();
            task.error = None;
            task.registration_missing = false;
            task.archive_unchanged = false;
            if task.result.is_some() {
                task.control.set_state(DownloadState::Scanning);
                task.scan_pending = true;
                self.scan_queue.push_back(index);
            } else {
                self.queues[usize::from(task.source_name() == "IPFS")].push_back(index);
            }
        }
    }

    pub fn poll(&mut self, http_parallelism: usize) {
        self.retired.retain(|index| {
            let handle = &mut self.tasks[*index].thread;
            if handle.is_none() {
                false
            } else if handle.as_ref().is_some_and(|handle| handle.is_finished()) {
                if handle.take().unwrap().join().is_err() {
                    tracing::warn!("download worker panicked");
                }
                false
            } else {
                true
            }
        });
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
            self.retired.push(index);
            match result {
                Ok(result) => {
                    task.previous_archive = None;
                    task.result = Some(result);
                    task.scan_pending = true;
                    self.scan_queue.push_back(index);
                    // The folder is committed and will be registered even if a
                    // cancel arrived too late to stop the transfer.
                    task.control.set_state(DownloadState::Scanning);
                }
                Err(error) => {
                    task.archive_unchanged = error.downcast_ref::<ArchiveUnchanged>().is_some();
                    task.error = Some(sanitized_download_error(&error));
                    task.control.set_state(if task.control.is_cancelled() {
                        DownloadState::Cancelled
                    } else {
                        DownloadState::Failed
                    });
                    if task.state() == DownloadState::Failed {
                        self.failed_notices.push_back(task.id);
                    }
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
                if task.thread.is_some() {
                    self.queues[queue].push_back(index);
                    break;
                }
                slots -= 1;
                task.control.set_state(DownloadState::Resolving);
                let control = Arc::clone(&task.control);
                let services = Arc::clone(&self.services);
                let request = task.requests[0].clone();
                // A deleted folder no longer duplicates anything; extract again.
                let previous_archive = task
                    .previous_archive
                    .as_ref()
                    .filter(|(_, dir)| dir.is_dir())
                    .map(|(digest, _)| *digest);
                let (tx, rx) = mpsc::channel();
                match std::thread::Builder::new().name(format!("chart-download-{}", task.id)).spawn(
                    move || {
                        let result = (|| {
                            let runtime = tokio::runtime::Builder::new_current_thread()
                                .enable_all()
                                .build()?;
                            runtime.block_on(download_chart_controlled(
                                request,
                                control,
                                services,
                                previous_archive,
                            ))
                        })();
                        let _ = tx.send(result);
                    },
                ) {
                    Ok(handle) => {
                        task.worker = Some(rx);
                        task.thread = Some(handle);
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
        let mut scans = Vec::new();
        while let Some(index) = self.scan_queue.pop_front() {
            let task = &mut self.tasks[index];
            if std::mem::take(&mut task.scan_pending)
                && let Some(result) = &task.result
            {
                scans.push((task.id, result.clone()));
            }
        }
        scans
    }

    pub fn take_failure_notices(&mut self) -> Vec<DownloadTaskId> {
        self.failed_notices.drain(..).collect()
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
        task.registered = task
            .requests
            .iter()
            .filter(|request| registered.iter().any(|chart| request.identity.overlaps(chart)))
            .map(|request| request.identity.clone())
            .collect();
        let success =
            error.is_none() && task.requests.iter().all(|r| task.registered.contains(&r.identity));
        task.registration_missing = error.is_none() && !success;
        if task.registration_missing
            && let Some(result) = &task.result
        {
            self.services.evict_archive(result);
        }
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

fn sanitized_download_error(error: &anyhow::Error) -> String {
    let mut message = format!("{error:#}");
    for cause in error.chain() {
        if let Some(request_error) = cause.downcast_ref::<reqwest::Error>()
            && let Some(url) = request_error.url()
        {
            let mut safe = url.clone();
            let _ = safe.set_username("");
            let _ = safe.set_password(None);
            safe.set_query(None);
            safe.set_fragment(None);
            message = message.replace(url.as_str(), safe.as_str());
        }
    }
    message
}

impl Drop for ChartDownloadManager {
    fn drop(&mut self) {
        for task in &self.tasks {
            if task.worker.is_some() {
                task.control.cancel();
            }
        }
        // Shutdown only: wait for cancellation cleanup before the process exits.
        for task in &mut self.tasks {
            if let Some(handle) = task.thread.take() {
                let _ = handle.join();
            }
        }
    }
}

fn requests_share_transfer(a: &ChartDownloadRequest, b: &ChartDownloadRequest) -> bool {
    if a.data_dir != b.data_dir {
        return false;
    }
    let same_hash = a.identity.overlaps(&b.identity);
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
                        (Ok(mut a), Ok(mut b)) => {
                            a.set_fragment(None);
                            b.set_fragment(None);
                            a == b
                        }
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
    fn large_submissions_remain_queued_without_spawning_workers() {
        let root = tempfile::tempdir().unwrap();
        let mut manager = ChartDownloadManager::default();
        for id in 1_u64..=10_000 {
            let mut sha = [0; 32];
            sha[..8].copy_from_slice(&id.to_le_bytes());
            let request = ChartDownloadRequest {
                request_id: id,
                identity: ChartDownloadIdentity { sha256: Some(sha), md5: None },
                action: MissingChartAction::Http {
                    api_url: "https://fixture.invalid/{sha256}".into(),
                    md5: String::new(),
                    sha256: crate::storage::common::hash_to_hex(&sha),
                },
                title: "Chart".into(),
                data_dir: root.path().into(),
            };
            let (task_id, merged) = manager.submit(request.clone());
            assert!(!merged);
            assert_eq!(manager.submit(request), (task_id, true));
        }
        assert_eq!(manager.tasks.len(), 10_000);
        assert!(manager.active.is_empty());
        assert_eq!(manager.queues[0].len(), 10_000);
        assert!(manager.take_scan_requests().is_empty());
    }

    #[test]
    fn partially_registered_shared_tasks_report_each_target_independently() {
        let root = tempfile::tempdir().unwrap();
        let mut manager = ChartDownloadManager::default();
        let first = request(root.path(), "https://fixture.invalid/{sha256}", 1);
        let mut second = first.clone();
        second.request_id = 2;
        second.identity.md5 = Some([2; 16]);
        let first_identity = first.identity.clone();
        let second_identity = second.identity.clone();
        let (id, _) = manager.submit(first);
        assert_eq!(manager.submit(second), (id, true));
        manager.tasks[0].control.set_state(DownloadState::Scanning);
        manager.finish_scan(id, std::slice::from_ref(&first_identity), None);
        let task = manager.task(id).unwrap();
        assert_eq!(task.state(), DownloadState::Failed);
        assert!(task.registered.contains(&first_identity));
        assert!(!task.registered.contains(&second_identity));
        let batch = BulkDownloadBatch {
            scope: BulkDownloadScope { source_url: "table".into(), level: None },
            counts: Default::default(),
            targets: vec![(id, first_identity), (id, second_identity)],
        };
        assert_eq!(batch.results(&manager), (1, 1));
    }

    #[tokio::test]
    async fn cancelling_a_shared_archive_owner_allows_the_waiter_to_take_over() {
        let root = tempfile::tempdir().unwrap();
        let server = HttpFixture::with_handler(
            move |path| {
                if path == "/archive" {
                    (
                        200,
                        "application/octet-stream".into(),
                        archive(&[("chart.bms", b"#TITLE Fixture")]),
                    )
                } else {
                    (200, "application/json".into(), br#"{"downloadURL":"/archive"}"#.to_vec())
                }
            },
            Duration::from_millis(5),
        );
        let mut manager = ChartDownloadManager::default();
        let services = Arc::clone(&manager.services);
        let permit = services.extraction.acquire().await.unwrap();
        manager.submit(request(root.path(), &format!("{}/{{md5}}", server.url), 1));
        manager.submit(request(root.path(), &format!("{}/{{md5}}", server.url), 2));
        wait(&mut manager, 3, |m| m.tasks.iter().any(|task| task.control.bytes().0 > 0));
        let owner = manager.tasks.iter().find(|task| task.control.bytes().0 > 0).unwrap().id;
        let waiter = manager.tasks.iter().find(|task| task.id != owner).unwrap().id;
        manager.cancel(owner);
        wait(&mut manager, 3, |m| {
            m.task(owner).unwrap().state() == DownloadState::Cancelled
                && m.task(waiter).unwrap().control.bytes().0 > 0
        });
        drop(permit);
        wait(&mut manager, 3, |m| m.task(waiter).unwrap().state() == DownloadState::Scanning);
        assert_eq!(manager.task(owner).unwrap().state(), DownloadState::Cancelled);
    }

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
            manager.finish_scan(id, &[], Some("temporary database error"));
            assert_eq!(manager.task(id).unwrap().state(), DownloadState::Failed);
            manager.retry(id);
            assert_eq!(manager.take_scan_requests().len(), 1);
            let identity = manager.task(id).unwrap().requests[0].identity.clone();
            manager.finish_scan(id, &[identity], None);
            assert_eq!(manager.task(id).unwrap().state(), DownloadState::Completed);
        }
    }

    fn finished_result(task: &DownloadTask, chart_dir: &Path) -> ChartDownloadResult {
        ChartDownloadResult {
            archive_cache_key: None,
            archive_sha256: None,
            request_id: task.requests[0].request_id,
            identity: task.requests[0].identity.clone(),
            source: ChartDownloadSource::Http,
            root_dir: chart_dir.parent().unwrap().into(),
            chart_dir: chart_dir.into(),
        }
    }

    #[test]
    fn requests_never_merge_into_a_task_with_a_pending_cancel() {
        let root = tempfile::tempdir().unwrap();
        let mut manager = ChartDownloadManager::default();
        let r = request(root.path(), "https://test.invalid/{md5}", 1);
        let (id, _) = manager.submit(r.clone());
        // Running transfers observe the cancel only at the next poll.
        manager.tasks[0].control.set_state(DownloadState::Downloading);
        manager.tasks[0].control.cancel();
        let (other, merged) = manager.submit(r);
        assert!(!merged);
        assert_ne!(other, id);
    }

    #[test]
    fn transfers_finishing_after_a_cancel_are_still_registered() {
        let root = tempfile::tempdir().unwrap();
        let chart_dir = root.path().join("songs/http/chart");
        std::fs::create_dir_all(&chart_dir).unwrap();
        let mut manager = ChartDownloadManager::default();
        let (id, _) = manager.submit(request(root.path(), "https://test.invalid/{md5}", 1));
        let (tx, rx) = mpsc::channel();
        tx.send(Ok(finished_result(&manager.tasks[0], &chart_dir))).unwrap();
        manager.tasks[0].worker = Some(rx);
        manager.tasks[0].control.set_state(DownloadState::Extracting);
        manager.queues[0].clear();
        manager.active.push(0);
        manager.cancel(id);
        manager.poll(1);
        assert_eq!(manager.task(id).unwrap().state(), DownloadState::Scanning);
        // Registration cannot be undone, so a cancel during the scan is ignored.
        manager.cancel(id);
        assert_eq!(manager.task(id).unwrap().state(), DownloadState::Scanning);
        assert_eq!(manager.take_scan_requests().len(), 1);
        let identity = manager.task(id).unwrap().requests[0].identity.clone();
        manager.finish_scan(id, std::slice::from_ref(&identity), None);
        let task = manager.task(id).unwrap();
        assert_eq!(task.state(), DownloadState::Completed);
        assert!(task.registered.contains(&identity));
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
