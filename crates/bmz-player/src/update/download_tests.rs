use super::*;
use crate::bootstrap::profile_tests::ProfileTestDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn server(
    body: &'static [u8],
    size: usize,
    stall: bool,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/update.exe", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        socket.read(&mut request).await.unwrap();
        socket
            .write_all(
                format!("HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        socket.write_all(body).await.unwrap();
        if stall {
            std::future::pending::<()>().await;
        }
    });
    (url, handle)
}

fn candidate(url: String, bytes: &[u8]) -> UpdateCandidate {
    UpdateCandidate {
        version: "0.5.0".into(),
        tag: "v0.5.0".into(),
        title: String::new(),
        html_url: String::new(),
        body: String::new(),
        published_at: None,
        prerelease: false,
        asset: Some(UpdateAsset {
            name: "update.exe".into(),
            download_url: url,
            size: bytes.len() as u64,
            sha256: Some(format!("{:x}", Sha256::digest(bytes))),
            kind: UpdateAssetKind::WindowsInstaller,
        }),
    }
}

#[tokio::test]
async fn canceled_download_removes_partial_and_preserves_other_attempts() {
    let data = ProfileTestDir::new();
    let version_dir = data.paths.cache_dir.join("updates/0.5.0");
    let previous = version_dir.join("previous-attempt");
    std::fs::create_dir_all(&previous).unwrap();
    std::fs::write(previous.join("keep.exe"), b"keep").unwrap();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    // Repeat cancellation to cover the accumulating-cache failure.
    for _ in 0..2 {
        let (url, server) = server(b"half", 8, true).await;
        let progress = Arc::new(DownloadProgress::default());
        let mut download = Box::pin(download_update_with_client(
            candidate(url, b"halfmore"),
            &data.paths.cache_dir,
            Arc::clone(&progress),
            &client,
        ));
        let canceled = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::select! {
                result = &mut download => panic!("download ended before cancellation: {result:?}"),
                _ = async {
                    while progress.received.load(Ordering::Relaxed) < 4 {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    progress.cancel.store(true, Ordering::Relaxed);
                } => {}
            }
        })
        .await;
        // Same cancellation mechanism as the app's select!: drop the pending future.
        drop(download);
        server.abort();
        canceled.unwrap();
        let attempts: Vec<_> =
            std::fs::read_dir(&version_dir).unwrap().map(|entry| entry.unwrap().path()).collect();
        assert_eq!(attempts, vec![previous.clone()]);
        assert_eq!(std::fs::read(previous.join("keep.exe")).unwrap(), b"keep");
    }
}

#[tokio::test]
async fn verified_download_remains_available_for_installation() {
    let data = ProfileTestDir::new();
    let (url, server) = server(b"complete", 8, false).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let downloaded = download_update_with_client(
        candidate(url, b"complete"),
        &data.paths.cache_dir,
        Arc::new(DownloadProgress::default()),
        &client,
    )
    .await
    .unwrap();
    server.await.unwrap();
    assert_eq!(std::fs::read(&downloaded.path).unwrap(), b"complete");
    assert!(!downloaded.path.with_extension("download").exists());
}

#[tokio::test]
async fn failed_download_verification_removes_its_attempt_directory() {
    let data = ProfileTestDir::new();
    let (url, server) = server(b"tampered", 8, false).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let result = download_update_with_client(
        candidate(url, b"complete"),
        &data.paths.cache_dir,
        Arc::new(DownloadProgress::default()),
        &client,
    )
    .await;
    server.await.unwrap();
    assert!(result.unwrap_err().to_string().contains("SHA256 mismatch"));
    assert_eq!(std::fs::read_dir(data.paths.cache_dir.join("updates/0.5.0")).unwrap().count(), 0);
}

#[test]
fn failed_preparation_removes_owned_stage_and_preserves_existing_backup() {
    let data = ProfileTestDir::new();
    let root = &data.paths.data_dir;
    std::fs::create_dir_all(root).unwrap();
    let previous = bmz_updater::transaction::new_work_dir(root).unwrap();
    std::fs::create_dir(previous.join("backup")).unwrap();
    let work = bmz_updater::transaction::new_work_dir(root).unwrap();
    std::fs::create_dir(work.join("stage")).unwrap();
    std::fs::write(work.join("stage/partial.dll"), b"partial").unwrap();
    {
        let _cleanup = DownloadCleanup { directories: vec![work.clone()] };
        // Preparation can fail or observe cancellation after writing some files.
    }
    assert!(!work.exists());
    assert!(previous.join("backup").is_dir());
}
