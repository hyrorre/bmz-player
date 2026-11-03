use super::integration_tests::{chart_identity, identified_request};
use super::test_support::{HttpFixture, archive, wait};
use super::*;
use crate::config::app_config::{AppConfig, PathEntry};
use crate::song_archive::{self, test_support as fixtures};
use crate::storage::library_db::LibraryDatabase;

fn register(root: &Path, result: &ChartDownloadResult) -> Vec<ChartDownloadIdentity> {
    let db_path = root.join("library.db");
    crate::storage::migration::migrate_library_db(&db_path).unwrap();
    let mut db = LibraryDatabase::open(&db_path).unwrap();
    let roots = [PathEntry {
        path: result.root_dir.to_string_lossy().into_owned(),
        enabled: true,
        recursive: true,
    }];
    db.set_configured_song_roots(&roots).unwrap();
    let mut scan = AppConfig::default().scan;
    scan.use_everything = result.scan_path.is_file();
    let targets = [PathEntry {
        path: result.scan_path.to_string_lossy().into_owned(),
        enabled: true,
        recursive: true,
    }];
    let report = crate::storage::scan::scan_song_roots(&mut db, &targets, &scan, 1, true).unwrap();
    assert!(report.summary.imported > 0, "{report:?}");
    assert!(report.failures.is_empty(), "{report:?}");
    db.register_partial_song_roots(&targets).unwrap();
    assert_eq!(db.reconcile_configured_song_roots(&roots).unwrap(), 0);
    downloaded_chart_identities(&db, &result.scan_path).unwrap()
}

#[tokio::test]
async fn every_http_format_registers_after_extracting_or_preserving_original_bytes() {
    for format in fixtures::FORMATS {
        for keep_archive in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let body = fixtures::archive(format);
            let reference = root.path().join(format!("reference.{}", format.extension()));
            std::fs::write(&reference, &body).unwrap();
            let index = song_archive::inspect(&reference, &Default::default()).unwrap();
            let first = index.entries.iter().find(|entry| entry.is_chart()).unwrap();
            let chart =
                song_archive::read_chart(&reference, &first.name, &Default::default()).unwrap();
            let server = HttpFixture::new(body.clone(), Duration::ZERO, 200);
            let mut request = identified_request(
                root.path(),
                &format!("{}/{{md5}}", server.url),
                &chart.bytes,
                1,
            );
            request.keep_archive = keep_archive;
            let result = download_chart(request).await.unwrap();
            assert!(result.is_present());
            assert_eq!(result.scan_path.parent().unwrap(), result.root_dir);
            assert_eq!(std::fs::read_dir(&result.root_dir).unwrap().count(), 1);
            if keep_archive {
                assert_eq!(std::fs::read(&result.scan_path).unwrap(), body);
                assert_eq!(
                    bmz_song_resource::ArchiveFormat::from_path(&result.scan_path),
                    Some(format)
                );
            } else {
                assert_eq!(std::fs::read(result.scan_path.join(&first.name)).unwrap(), chart.bytes);
            }
            let registered = register(root.path(), &result);
            assert!(registered.iter().any(|identity| identity.overlaps(&result.identity)));
        }
    }
}

#[test]
fn mixed_storage_modes_do_not_share_tasks_or_archive_results() {
    let root = tempfile::tempdir().unwrap();
    let body = archive(&[("song/chart.bms", fixtures::CHART)]);
    let server = HttpFixture::new(body, Duration::ZERO, 200);
    let request =
        identified_request(root.path(), &format!("{}/{{md5}}", server.url), fixtures::CHART, 1);
    let mut kept = request.clone();
    kept.keep_archive = true;
    let mut manager = ChartDownloadManager::default();
    let (extracted, merged) = manager.submit(request);
    assert!(!merged);
    let (preserved, merged) = manager.submit(kept);
    assert!(!merged);
    assert_ne!(extracted, preserved);
    wait(&mut manager, 2, |m| m.tasks().iter().all(|task| task.state() == DownloadState::Scanning));
    let scans = manager.take_scan_requests();
    assert_eq!(scans.len(), 2);
    assert!(manager.task(extracted).unwrap().result.as_ref().unwrap().scan_path.is_dir());
    assert!(manager.task(preserved).unwrap().result.as_ref().unwrap().scan_path.is_file());
    for (id, result) in scans {
        let identities = register(root.path(), &result);
        manager.finish_scan(id, &identities, None);
        assert_eq!(manager.task(id).unwrap().state(), DownloadState::Completed);
    }
}

#[test]
fn kept_archive_is_shared_but_each_requested_hash_is_verified() {
    let root = tempfile::tempdir().unwrap();
    let second = b"#TITLE Second\n#BPM 180\n#00011:01\n";
    let body = archive(&[("song/first.bms", fixtures::CHART), ("song/second.bms", second)]);
    let server = HttpFixture::with_handler(
        move |path| {
            if path == "/archive" {
                (200, "application/octet-stream".into(), body.clone())
            } else {
                (200, "application/json".into(), br#"{"downloadURL":"/archive"}"#.to_vec())
            }
        },
        Duration::ZERO,
    );
    let mut manager = ChartDownloadManager::default();
    for (id, bytes) in [(1, fixtures::CHART), (2, second.as_slice())] {
        let mut request =
            identified_request(root.path(), &format!("{}/{{md5}}", server.url), bytes, id);
        request.keep_archive = true;
        manager.submit(request);
    }
    wait(&mut manager, 2, |m| m.tasks().iter().all(|task| task.state() == DownloadState::Scanning));
    let scans = manager.take_scan_requests();
    assert_eq!(scans[0].1.scan_path, scans[1].1.scan_path);
    let registered = register(root.path(), &scans[0].1);
    for (id, result) in scans {
        assert!(registered.iter().any(|identity| identity.overlaps(&result.identity)));
        manager.finish_scan(id, &registered, None);
        assert_eq!(manager.task(id).unwrap().state(), DownloadState::Completed);
    }
    assert!(registered.iter().any(|identity| identity.overlaps(&chart_identity(second))));
}

#[test]
fn kept_archive_retries_scanning_then_refetches_if_the_file_is_removed() {
    let root = tempfile::tempdir().unwrap();
    let body = archive(&[("song/chart.bms", fixtures::CHART)]);
    let server = HttpFixture::new(body, Duration::ZERO, 200);
    let mut request =
        identified_request(root.path(), &format!("{}/{{md5}}", server.url), fixtures::CHART, 1);
    request.keep_archive = true;
    let mut manager = ChartDownloadManager::default();
    let (id, _) = manager.submit(request);
    wait(&mut manager, 1, |m| m.task(id).unwrap().state() == DownloadState::Scanning);
    let original = manager.take_scan_requests().pop().unwrap().1;
    manager.finish_scan(id, &[], Some("fixture DB error"));
    manager.retry(id);
    assert_eq!(manager.task(id).unwrap().state(), DownloadState::Scanning);
    assert_eq!(manager.take_scan_requests()[0].1.scan_path, original.scan_path);
    manager.finish_scan(id, &[], None); // Missing target: retry must refetch, then reject unchanged bytes.
    manager.retry(id);
    wait(&mut manager, 1, |m| m.task(id).unwrap().state() == DownloadState::Failed);
    assert!(manager.task(id).unwrap().archive_unchanged);
    assert_eq!(std::fs::read_dir(&original.root_dir).unwrap().count(), 1);
    std::fs::remove_file(&original.scan_path).unwrap();
    manager.retry(id);
    wait(&mut manager, 1, |m| m.task(id).unwrap().state() == DownloadState::Scanning);
    let result = manager.take_scan_requests().pop().unwrap().1;
    assert!(result.scan_path.is_file());
    assert_ne!(result.scan_path, original.scan_path);
    let registered = register(root.path(), &result);
    manager.finish_scan(id, &registered, None);
    assert_eq!(manager.task(id).unwrap().state(), DownloadState::Completed);
}

#[tokio::test]
async fn invalid_kept_archives_do_not_publish_files_or_remove_existing_songs() {
    let mut corrupt_gzip = archive(&[("chart.bms", fixtures::CHART)]);
    let footer = corrupt_gzip.len() - 8;
    corrupt_gzip[footer] ^= 1;
    for body in [b"PK\x03\x04broken archive".to_vec(), corrupt_gzip, b"not an archive".to_vec()] {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("songs/http");
        std::fs::create_dir_all(&destination).unwrap();
        let previous = destination.join("keep.bms");
        std::fs::write(&previous, b"KEEP").unwrap();
        let server = HttpFixture::new(body, Duration::ZERO, 200);
        let mut request =
            identified_request(root.path(), &format!("{}/{{md5}}", server.url), fixtures::CHART, 1);
        request.keep_archive = true;
        assert!(download_chart(request).await.is_err());
        assert_eq!(std::fs::read(&previous).unwrap(), b"KEEP");
        assert_eq!(std::fs::read_dir(&destination).unwrap().count(), 1);
    }
}
