use super::test_support::*;
use super::*;
use crate::config::app_config::{AppConfig, PathEntry};
use crate::storage::library_db::LibraryDatabase;

fn chart_identity(bytes: &[u8]) -> ChartDownloadIdentity {
    let imported = bmz_chart::import::import_chart_bytes_with_random_source(
        Path::new("chart.bms"),
        bytes,
        bmz_chart::import::BmsRandomSource::Seed(None),
        false,
    )
    .unwrap();
    ChartDownloadIdentity {
        sha256: Some(imported.chart.identity.file_sha256),
        md5: Some(imported.chart.identity.file_md5),
    }
}

fn identified_request(root: &Path, api: &str, bytes: &[u8], id: u64) -> ChartDownloadRequest {
    let identity = chart_identity(bytes);
    ChartDownloadRequest {
        request_id: id,
        title: "Same title".into(),
        data_dir: root.into(),
        action: MissingChartAction::Http {
            api_url: api.into(),
            md5: crate::storage::common::hash_to_hex(&identity.md5.unwrap()),
            sha256: crate::storage::common::hash_to_hex(&identity.sha256.unwrap()),
        },
        identity,
    }
}

#[test]
fn shared_http_archive_registers_multiple_chart_hashes_before_completion() {
    let first = b"#TITLE Same title\n#BPM 120\n#00011:01\n";
    let second = b"#TITLE Same title\n#BPM 180\n#00011:01\n";
    let body = archive(&[("song/first.bms", first), ("song/second.bms", second)]);
    let server = HttpFixture::with_handler(
        move |path| {
            if path == "/archive" {
                (200, "application/octet-stream".into(), body.clone())
            } else {
                (
                    200,
                    "application/json".into(),
                    br#"{"result":"success","data":{"song_url":"/archive"}}"#.to_vec(),
                )
            }
        },
        Duration::from_millis(10),
    );
    let root = tempfile::tempdir().unwrap();
    let api = format!("{}/{{md5}}", server.url);
    let mut manager = ChartDownloadManager::default();
    let (a, _) = manager.submit(identified_request(root.path(), &api, first, 1));
    let (b, _) = manager.submit(identified_request(root.path(), &api, second, 2));
    wait(&mut manager, 3, |m| m.tasks().iter().all(|t| t.state() == DownloadState::Scanning));
    let scans = manager.take_scan_requests();
    assert_eq!(scans.len(), 2);
    assert_eq!(scans[0].1.chart_dir, scans[1].1.chart_dir);
    let db_path = root.path().join("library.db");
    crate::storage::migration::migrate_library_db(&db_path).unwrap();
    let mut db = LibraryDatabase::open(&db_path).unwrap();
    assert!(
        !registered_download_identity(&db, &chart_identity(first), &scans[0].1.chart_dir).unwrap()
    );
    let mut config = AppConfig::default();
    config.scan.use_everything = false;
    crate::songs_cmd::ensure_download_song_root(
        &mut config.songs.roots,
        &scans[0].1.root_dir.to_string_lossy(),
    );
    db.set_configured_song_roots(&config.songs.roots).unwrap();
    let roots = vec![PathEntry {
        path: scans[0].1.chart_dir.to_string_lossy().into_owned(),
        enabled: true,
        recursive: true,
    }];
    let report =
        crate::storage::scan::scan_song_roots(&mut db, &roots, &config.scan, 1, true).unwrap();
    assert_eq!(report.summary.imported, 2);
    let registered = [chart_identity(first), chart_identity(second)];
    for (id, result) in &scans {
        assert!(registered_download_identity(&db, &result.identity, &result.chart_dir).unwrap());
        manager.finish_scan(*id, &registered, None);
    }
    assert_eq!(manager.task(a).unwrap().state(), DownloadState::Completed);
    assert_eq!(manager.task(b).unwrap().state(), DownloadState::Completed);
    let unknown = ChartDownloadIdentity { sha256: Some([99; 32]), md5: None };
    assert!(!registered_download_identity(&db, &unknown, &scans[0].1.chart_dir).unwrap());
    // A matching title is never evidence of a successful requested download.
    let (missing, _) = manager.submit(request(root.path(), &api, 99));
    wait(&mut manager, 3, |m| m.task(missing).unwrap().state() == DownloadState::Scanning);
    manager.finish_scan(missing, &registered, None);
    assert_eq!(manager.task(missing).unwrap().state(), DownloadState::Failed);
    assert_eq!(manager.task(a).unwrap().state(), DownloadState::Completed);
}

#[test]
fn failed_http_task_does_not_stop_other_tasks_and_can_retry() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let fail = Arc::new(AtomicBool::new(true));
    let worker_fail = Arc::clone(&fail);
    let server = HttpFixture::with_handler(
        move |path| {
            if path.starts_with("/01") && worker_fail.load(Ordering::Relaxed) {
                (500, "text/plain".into(), b"temporary failure".to_vec())
            } else {
                (
                    200,
                    "application/octet-stream".into(),
                    archive(&[("chart.bms", b"#TITLE Fixture")]),
                )
            }
        },
        Duration::from_millis(5),
    );
    let root = tempfile::tempdir().unwrap();
    let api = format!("{}/{{md5}}", server.url);
    let mut manager = ChartDownloadManager::default();
    let (a, _) = manager.submit(request(root.path(), &api, 1));
    let (b, _) = manager.submit(request(root.path(), &api, 2));
    wait(&mut manager, 3, |m| {
        m.task(a).unwrap().state() == DownloadState::Failed
            && m.task(b).unwrap().state() == DownloadState::Scanning
    });
    assert!(manager.task(a).unwrap().error.as_ref().unwrap().contains("500"));
    fail.store(false, Ordering::Relaxed);
    manager.retry(a);
    wait(&mut manager, 3, |m| m.task(a).unwrap().state() == DownloadState::Scanning);
    assert_ne!(
        manager.task(a).unwrap().result.as_ref().unwrap().chart_dir,
        manager.task(b).unwrap().result.as_ref().unwrap().chart_dir
    );
    for entry in std::fs::read_dir(root.path().join("songs/http")).unwrap() {
        assert!(!entry.unwrap().file_name().to_string_lossy().starts_with(".bmz-"));
    }
}

#[test]
fn invalid_http_payloads_and_404_leave_no_staging_files() {
    let payloads = [
        (404, "text/plain", b"not found".to_vec()),
        (200, "application/json", b"{invalid".to_vec()),
        (200, "application/json", br#"{"result":"fail","data":{"song_url":"/archive"}}"#.to_vec()),
        (200, "text/plain", br#"{"unknown":true}"#.to_vec()),
        (200, "application/octet-stream", vec![0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c, 0, 0]),
    ];
    for (status, content_type, body) in payloads {
        let root = tempfile::tempdir().unwrap();
        let server = HttpFixture::with_handler(
            move |_| (status, content_type.into(), body.clone()),
            Duration::ZERO,
        );
        let mut manager = ChartDownloadManager::default();
        let (id, _) = manager.submit(request(root.path(), &format!("{}/{{md5}}", server.url), 1));
        wait(&mut manager, 1, |m| m.task(id).unwrap().state() == DownloadState::Failed);
        assert_eq!(std::fs::read_dir(root.path().join("songs/http")).unwrap().count(), 0);
    }
}

#[test]
fn tar_path_traversal_and_cross_platform_escapes_are_rejected() {
    for path in [
        "../outside.bms",
        "a/../../outside.bms",
        "..\\outside.bms",
        "/outside.bms",
        "C:\\outside.bms",
        "file:stream",
    ] {
        assert!(validate_archive_path(path).is_err());
    }
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("bad.tar.gz");
    let mut header = tar::Header::new_gnu();
    let name = b"../outside.bms";
    header.as_mut_bytes()[..name.len()].copy_from_slice(name);
    header.set_size(1);
    header.set_mode(0o644);
    header.set_cksum();
    let encoder = flate2::write::GzEncoder::new(
        File::create(&input).unwrap(),
        flate2::Compression::default(),
    );
    let mut builder = tar::Builder::new(encoder);
    builder.append(&header, &b"X"[..]).unwrap();
    builder.into_inner().unwrap().finish().unwrap();
    assert!(extract_tar_gz(&input, &root.path().join("staging")).is_err());
    assert!(!root.path().join("outside.bms").exists());
}

#[test]
fn unknown_content_length_reports_bytes_without_a_percentage() {
    let body = archive(&[("chart.bms", b"#TITLE Fixture")]);
    let size = body.len();
    let server = HttpFixture::raw(
        move |_| {
            let mut response = b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n".to_vec();
            response.extend_from_slice(&body);
            response
        },
        Duration::ZERO,
    );
    let root = tempfile::tempdir().unwrap();
    let mut manager = ChartDownloadManager::default();
    let (id, _) = manager.submit(request(root.path(), &format!("{}/{{md5}}", server.url), 1));
    wait(&mut manager, 1, |m| m.task(id).unwrap().state() == DownloadState::Scanning);
    assert_eq!(manager.task(id).unwrap().control.bytes(), (size as u64, None));
}

#[test]
fn oversized_headers_and_metadata_are_rejected_without_archive_files() {
    for metadata in [false, true] {
        let size = if metadata { MAX_METADATA_BYTES + 1 } else { MAX_DOWNLOAD_BYTES + 1 };
        let content_type = if metadata { "application/json" } else { "application/octet-stream" };
        let server = HttpFixture::raw(
            move |_| {
                format!("HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n").into_bytes()
            },
            Duration::ZERO,
        );
        let root = tempfile::tempdir().unwrap();
        let mut manager = ChartDownloadManager::default();
        let (id, _) = manager.submit(request(root.path(), &format!("{}/{{md5}}", server.url), 1));
        wait(&mut manager, 1, |m| m.task(id).unwrap().state() == DownloadState::Failed);
        assert_eq!(std::fs::read_dir(root.path().join("songs/http")).unwrap().count(), 0);
    }
}

#[tokio::test]
async fn timed_out_and_cancelled_requests_clean_only_their_own_staging() {
    let server = HttpFixture::new(
        archive(&[("chart.bms", b"#TITLE Fixture")]),
        Duration::from_millis(150),
        200,
    );
    let root = tempfile::tempdir().unwrap();
    let existing = root.path().join("songs/http/existing");
    std::fs::create_dir_all(&existing).unwrap();
    std::fs::write(existing.join("keep.bms"), b"KEEP").unwrap();
    let services =
        DownloadServices { timeout: Some(Duration::from_millis(25)), ..Default::default() };
    let result = download_chart_controlled(
        request(root.path(), &format!("{}/{{md5}}", server.url), 1),
        Arc::default(),
        Arc::new(services),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(std::fs::read_dir(root.path().join("songs/http")).unwrap().count(), 1);
    let mut manager = ChartDownloadManager::default();
    let (id, _) = manager.submit(request(root.path(), &format!("{}/{{md5}}", server.url), 2));
    manager.poll(1);
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while server.requests.load(Ordering::Relaxed) < 2 && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    manager.cancel(id);
    wait(&mut manager, 1, |m| m.task(id).unwrap().state() == DownloadState::Cancelled);
    assert_eq!(std::fs::read(existing.join("keep.bms")).unwrap(), b"KEEP");
    assert_eq!(std::fs::read_dir(root.path().join("songs/http")).unwrap().count(), 1);
}

#[test]
fn cancellation_interrupts_archive_reader_and_stops_before_committing() {
    let control = DownloadControl::default();
    let mut reader =
        CancelReader { reader: std::io::Cursor::new(vec![1; 1024]), control: &control };
    assert_eq!(reader.read(&mut [0; 128]).unwrap(), 128);
    control.cancel();
    assert!(reader.read(&mut [0; 128]).is_err());
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("archive.tar.gz");
    std::fs::write(&input, archive(&[("chart.bms", b"#TITLE Test")])).unwrap();
    assert!(extract_archive_controlled(&input, &root.path().join("staging"), &control).is_err());
    assert!(!root.path().join("staging/chart.bms").exists());
}

#[test]
fn redirected_metadata_resolves_relative_urls_from_the_final_response() {
    let body = archive(&[("chart.bms", b"#TITLE Redirect")]);
    let server = HttpFixture::raw(
        move |path| {
            if path.starts_with("/entry/") {
                b"HTTP/1.1 302 Found\r\nLocation: /meta/path/index\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
            } else {
                let (kind, data) = if path == "/meta/path/index" {
                    ("application/octet-stream", br#"{"downloadURL":"archive"}"#.to_vec())
                } else {
                    assert_eq!(path, "/meta/path/archive");
                    ("application/octet-stream", body.clone())
                };
                let mut response = format!("HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", data.len()).into_bytes();
                response.extend(data);
                response
            }
        },
        Duration::ZERO,
    );
    let root = tempfile::tempdir().unwrap();
    let mut manager = ChartDownloadManager::default();
    let (id, _) = manager.submit(request(root.path(), &format!("{}/entry/{{md5}}", server.url), 1));
    wait(&mut manager, 1, |m| m.task(id).unwrap().state() == DownloadState::Scanning);
    assert!(
        manager.task(id).unwrap().result.as_ref().unwrap().chart_dir.join("chart.bms").is_file()
    );
    assert_eq!(server.requests.load(Ordering::Relaxed), 3);
}

#[test]
fn declared_oversized_tar_file_is_rejected_before_reading_its_contents() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("oversized.tar.gz");
    let mut header = tar::Header::new_gnu();
    header.set_path("large.bin").unwrap();
    header.set_size(MAX_EXTRACTED_FILE_BYTES + 1);
    header.set_mode(0o644);
    header.set_cksum();
    let mut encoder =
        flate2::write::GzEncoder::new(File::create(&path).unwrap(), flate2::Compression::default());
    encoder.write_all(header.as_bytes()).unwrap();
    encoder.finish().unwrap();
    let error = extract_tar_gz(&path, &root.path().join("output")).unwrap_err();
    assert!(format!("{error:#}").contains("単一ファイル"));
    assert!(!root.path().join("output/large.bin").exists());
}
