use super::*;
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize};

pub struct HttpFixture {
    pub url: String,
    pub requests: Arc<AtomicUsize>,
    pub maximum: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl HttpFixture {
    pub fn new(body: Vec<u8>, delay: Duration, status: u16) -> Self {
        Self::with_handler(
            move |_| (status, "application/octet-stream".into(), body.clone()),
            delay,
        )
    }

    pub fn with_handler(
        handler: impl Fn(&str) -> (u16, String, Vec<u8>) + Send + Sync + 'static,
        delay: Duration,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicUsize::new(0));
        let handler = Arc::new(handler);
        let (worker_stop, worker_requests, worker_maximum) =
            (Arc::clone(&stop), Arc::clone(&requests), Arc::clone(&maximum));
        let thread = std::thread::spawn(move || {
            let mut workers = Vec::new();
            while !worker_stop.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                };
                let (requests, maximum, active, handler) = (
                    Arc::clone(&worker_requests),
                    Arc::clone(&worker_maximum),
                    Arc::clone(&active),
                    Arc::clone(&handler),
                );
                workers.push(std::thread::spawn(move || {
                    // Accepted sockets inherit the listener's nonblocking mode
                    // on Windows. The handler deliberately uses blocking I/O.
                    stream.set_nonblocking(false).unwrap();
                    stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                    let mut buffer = [0; 8192];
                    let mut length = 0;
                    while length < buffer.len() && !buffer[..length].windows(4).any(|window| window == b"\r\n\r\n") {
                        match stream.read(&mut buffer[length..]) {
                            Ok(0) | Err(_) => return,
                            Ok(count) => length += count,
                        }
                    }
                    requests.fetch_add(1, Ordering::Relaxed);
                    let count = active.fetch_add(1, Ordering::Relaxed) + 1;
                    maximum.fetch_max(count, Ordering::Relaxed);
                    let path = String::from_utf8_lossy(&buffer[..length]).split_whitespace().nth(1).unwrap_or("/").to_string();
                    let (status, content_type, body) = handler(&path);
                    std::thread::sleep(delay);
                    let _ = write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                    let _ = stream.write_all(&body);
                    let _ = stream.flush();
                    active.fetch_sub(1, Ordering::Relaxed);
                    // Gracefully close the write side and drain the peer. A
                    // Windows close with unread TCP bytes can otherwise reset
                    // the connection before hyper has consumed the response.
                    let _ = stream.shutdown(std::net::Shutdown::Write);
                    let _ = stream.read_to_end(&mut Vec::new());
                }));
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self { url, requests, maximum, stop, thread: Some(thread) }
    }
}

impl Drop for HttpFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

pub fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    for (name, bytes) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive.append_data(&mut header, name, *bytes).unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap()
}

pub fn request(root: &Path, api: &str, number: u8) -> ChartDownloadRequest {
    ChartDownloadRequest {
        request_id: number as u64,
        identity: ChartDownloadIdentity { sha256: Some([number; 32]), md5: Some([number; 16]) },
        action: MissingChartAction::Http {
            api_url: api.into(),
            md5: format!("{number:02x}").repeat(16),
            sha256: format!("{number:02x}").repeat(32),
        },
        title: "same title".into(),
        data_dir: root.into(),
    }
}

pub fn wait(
    manager: &mut ChartDownloadManager,
    parallel: usize,
    condition: impl Fn(&ChartDownloadManager) -> bool,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        manager.poll(parallel);
        if condition(manager) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "download fixture timed out: {:?}",
            manager
                .tasks()
                .iter()
                .map(|task| (task.id, task.state(), &task.error))
                .collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
