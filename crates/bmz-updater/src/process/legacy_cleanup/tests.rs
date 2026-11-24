use super::*;
use crate::{
    INSTANCE_LOCK, MANIFEST, UPDATE_LOCK,
    manifest::{PackageKind, PackageManifest},
    process::{instance_guard, lock_file, update_guards},
};

const OLD_BACKUP: &str = ".bmz-update/job-finished/backup/0";
const MOVED_BACKUP: &str = "updater/legacy-backups/job-finished/backup/0";

fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    crate::manifest::write_test_package(
        root,
        env!("CARGO_PKG_VERSION"),
        &[("bmz-player.exe", b"app"), (crate::HELPER, b"helper")],
    );
    fs::write(root.join(LEGACY_INSTANCE_LOCK), b"").unwrap();
    fs::write(root.join(LEGACY_UPDATE_LOCK), b"").unwrap();
    fs::create_dir_all(root.join(OLD_BACKUP).parent().unwrap()).unwrap();
    fs::write(root.join(OLD_BACKUP), b"previous file").unwrap();
    fs::write(root.join(".bmz-update/job-finished/result.json"), b"{}").unwrap();
    fs::write(root.join(".bmz-update/job-finished/update.log"), b"finished").unwrap();
    temp
}

fn assert_legacy_kept(root: &Path) {
    assert!(root.join(LEGACY_INSTANCE_LOCK).is_file());
    assert!(root.join(LEGACY_UPDATE_LOCK).is_file());
    assert_eq!(fs::read(root.join(OLD_BACKUP)).unwrap(), b"previous file");
    assert!(!root.join(MOVED_BACKUP).exists());
}

#[test]
fn grouped_startup_moves_completed_backups_and_retires_idle_locks() {
    let temp = fixture();
    let root = temp.path();
    let guard = instance_guard(root).unwrap();
    assert_eq!(guard._locks.len(), 1);
    assert!(guard.cleanup_error.is_none());
    assert!(root.join(INSTANCE_LOCK).is_file());
    assert!(!root.join(LEGACY_INSTANCE_LOCK).exists());
    assert!(!root.join(LEGACY_UPDATE_LOCK).exists());
    assert!(!root.join(LEGACY_WORK_DIR).exists());
    assert_eq!(fs::read(root.join(MOVED_BACKUP)).unwrap(), b"previous file");
    assert_eq!(
        fs::read(root.join("updater/legacy-backups/job-finished/update.log")).unwrap(),
        b"finished"
    );
    assert!(instance_guard(root).is_ok());
}

#[test]
fn either_open_legacy_lock_defers_all_cleanup() {
    for name in [LEGACY_INSTANCE_LOCK, LEGACY_UPDATE_LOCK] {
        let temp = fixture();
        let root = temp.path();
        // An old process may have opened the file without taking a byte-range lock yet.
        let old_handle = File::open(root.join(name)).unwrap();
        drop(instance_guard(root).unwrap());
        assert_legacy_kept(root);
        drop(old_handle);
        drop(instance_guard(root).unwrap());
        assert!(root.join(MOVED_BACKUP).is_file());
    }
}

#[test]
fn active_transaction_in_either_layout_prevents_cleanup() {
    for path in [".bmz-update/active.json", "updater/active.json"] {
        let temp = fixture();
        let root = temp.path();
        fs::write(root.join(path), b"unfinished journal").unwrap();
        assert!(instance_guard(root).is_err());
        assert_legacy_kept(root);
        assert_eq!(fs::read(root.join(path)).unwrap(), b"unfinished journal");
    }
}

#[test]
fn startup_retries_after_the_updating_helper_releases_its_lock() {
    let temp = fixture();
    let root = temp.path();
    let updater = update_guards(root).unwrap();
    drop(instance_guard(root).unwrap());
    assert_legacy_kept(root);
    drop(updater);
    drop(instance_guard(root).unwrap());
    assert!(root.join(MOVED_BACKUP).is_file());
}

#[test]
fn unrecognized_work_and_conflicting_backups_stay_in_place() {
    let temp = fixture();
    let root = temp.path();
    fs::create_dir(root.join(".bmz-update/job-unfinished")).unwrap();
    fs::write(root.join(".bmz-update/active.new"), b"uncommitted journal").unwrap();
    fs::write(root.join(".bmz-update/user-file"), b"user file").unwrap();
    fs::create_dir_all(root.join(MOVED_BACKUP).parent().unwrap()).unwrap();
    fs::write(root.join(MOVED_BACKUP), b"existing backup").unwrap();
    drop(instance_guard(root).unwrap());
    assert!(root.join(".bmz-update/job-unfinished").is_dir());
    assert_eq!(fs::read(root.join(".bmz-update/active.new")).unwrap(), b"uncommitted journal");
    assert_eq!(fs::read(root.join(".bmz-update/user-file")).unwrap(), b"user file");
    assert_eq!(fs::read(root.join(OLD_BACKUP)).unwrap(), b"previous file");
    assert_eq!(fs::read(root.join(MOVED_BACKUP)).unwrap(), b"existing backup");
    assert!(!root.join(LEGACY_INSTANCE_LOCK).exists());
    assert!(!root.join(LEGACY_UPDATE_LOCK).exists());
    assert!(!root.join(UPDATE_LOCK).exists());
    assert!(instance_guard(root).unwrap().cleanup_error.is_none());
    assert!(!root.join(UPDATE_LOCK).exists());
}

#[test]
fn cleanup_can_run_with_another_modern_shared_startup_guard() {
    let temp = fixture();
    let root = temp.path();
    let other_startup = lock_file(root, INSTANCE_LOCK).unwrap();
    other_startup.try_lock_shared().unwrap();
    let launching = instance_guard(root).unwrap();
    assert!(launching.cleanup_error.is_none());
    assert!(root.join(MOVED_BACKUP).is_file());
    assert!(!root.join(UPDATE_LOCK).exists());
    assert!(update_guards(root).is_ok());
    other_startup.unlock().unwrap();
}

#[test]
fn cleanup_error_leaves_modern_locks_available_to_other_launches_and_the_helper() {
    let temp = fixture();
    let root = temp.path();
    let legacy_update = root.join(LEGACY_UPDATE_LOCK);
    fs::write(&legacy_update, b"unexpected lock contents").unwrap();

    let first = instance_guard(root).unwrap();
    assert!(first.cleanup_error.is_some());
    let second = instance_guard(root).unwrap();
    assert!(second.cleanup_error.is_some());
    assert!(!root.join(UPDATE_LOCK).exists());
    let (updates, instances) = update_guards(root).unwrap();
    assert!(matches!(instances[0].try_lock(), Err(std::fs::TryLockError::WouldBlock)));
    for update in &updates {
        update.unlock().unwrap();
    }
    assert_eq!(fs::read(legacy_update).unwrap(), b"unexpected lock contents");
    assert_legacy_kept(root);
}

#[test]
fn delete_pending_legacy_lock_defers_a_second_cleanup() {
    let temp = fixture();
    let root = temp.path();
    let modern = lock_file(root, INSTANCE_LOCK).unwrap();
    modern.try_lock_shared().unwrap();
    let busy = File::options()
        .access_mode(GENERIC_READ | DELETE)
        .share_mode(0)
        .open(root.join(LEGACY_INSTANCE_LOCK))
        .unwrap();
    set_delete_disposition(&busy, true).unwrap();

    cleanup(root).unwrap();
    assert!(root.join(LEGACY_UPDATE_LOCK).is_file());
    assert!(root.join(OLD_BACKUP).is_file());
    assert!(!root.join(MOVED_BACKUP).exists());

    drop(busy);
    let next = instance_guard(root).unwrap();
    assert!(next.cleanup_error.is_none());
    assert!(root.join(MOVED_BACKUP).is_file());
    modern.unlock().unwrap();
}

#[test]
fn short_legacy_handle_contention_does_not_fail_launch_or_updater_preflight() {
    for helper in [false, true] {
        let temp = fixture();
        let root = temp.path();
        let busy =
            File::options().read(true).share_mode(0).open(root.join(LEGACY_INSTANCE_LOCK)).unwrap();
        std::thread::scope(|scope| {
            let (started, ready) = std::sync::mpsc::channel();
            let attempt = scope.spawn(move || {
                started.send(()).unwrap();
                if helper {
                    update_guards(root).unwrap();
                } else {
                    assert!(instance_guard(root).unwrap().cleanup_error.is_none());
                }
            });
            ready.recv().unwrap();
            std::thread::sleep(Duration::from_millis(30));
            drop(busy);
            attempt.join().unwrap();
        });
    }
}

#[test]
fn old_layout_markers_and_other_package_types_skip_cleanup() {
    for marker in [LEGACY_MANIFEST, LEGACY_HELPER] {
        let temp = fixture();
        let root = temp.path();
        fs::write(root.join(marker), b"legacy marker").unwrap();
        assert!(instance_guard(root).unwrap().cleanup_error.is_none());
        assert_legacy_kept(root);
        assert!(!root.join(UPDATE_LOCK).exists());
    }
    for (kind, version) in
        [(PackageKind::Installer, env!("CARGO_PKG_VERSION")), (PackageKind::Portable, "0.0.1")]
    {
        let temp = fixture();
        let root = temp.path();
        let mut package = PackageManifest::read(root).unwrap();
        package.kind = kind;
        package.version = version.into();
        crate::transaction::atomic_json(&root.join(MANIFEST), &package).unwrap();
        assert!(instance_guard(root).unwrap().cleanup_error.is_none());
        assert_legacy_kept(root);
    }
}

#[test]
fn readonly_and_nonempty_locks_are_preserved_and_reported() {
    for readonly in [false, true] {
        let temp = fixture();
        let root = temp.path();
        let path = root.join(LEGACY_INSTANCE_LOCK);
        let permissions = fs::metadata(&path).unwrap().permissions();
        if readonly {
            let mut read_only = permissions.clone();
            read_only.set_readonly(true);
            fs::set_permissions(&path, read_only).unwrap();
        } else {
            fs::write(&path, b"not an empty lock").unwrap();
        }
        let guard = instance_guard(root).unwrap();
        assert!(
            format!("{:#}", guard.cleanup_error.as_ref().unwrap()).contains(LEGACY_INSTANCE_LOCK)
        );
        assert_legacy_kept(root);
        drop(guard);
        if readonly {
            assert!(fs::metadata(&path).unwrap().permissions().readonly());
            fs::set_permissions(&path, permissions).unwrap();
        } else {
            assert_eq!(fs::read(path).unwrap(), b"not an empty lock");
        }
    }
}

#[test]
fn a_backup_directory_created_after_the_scan_is_not_replaced() {
    let temp = fixture();
    let root = temp.path();
    let (jobs, _) = completed_jobs(root, &root.join(LEGACY_WORK_DIR)).unwrap();
    let [(source, backup)] = jobs.as_slice() else { panic!("expected one completed job") };
    fs::create_dir_all(backup).unwrap();
    assert!(move_job(source, backup).is_err());
    assert!(backup.is_dir());
    assert_eq!(fs::read(root.join(OLD_BACKUP)).unwrap(), b"previous file");
}

#[test]
fn legacy_lock_reparse_point_is_preserved_and_reported() {
    let temp = fixture();
    let root = temp.path();
    let path = root.join(LEGACY_UPDATE_LOCK);
    let target = tempfile::tempdir().unwrap();
    fs::write(target.path().join("sentinel"), b"outside").unwrap();
    fs::remove_file(&path).unwrap();
    assert!(
        std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&path)
            .arg(target.path())
            .output()
            .unwrap()
            .status
            .success()
    );
    let guard = instance_guard(root).unwrap();
    let error = format!("{:#}", guard.cleanup_error.as_ref().unwrap());
    assert!(error.contains("reparse point") || error.contains("symlink"), "{error}");
    assert!(path.is_dir());
    assert!(root.join(LEGACY_INSTANCE_LOCK).is_file());
    assert!(root.join(OLD_BACKUP).is_file());
    assert!(!root.join(MOVED_BACKUP).exists());
    assert_eq!(fs::read(target.path().join("sentinel")).unwrap(), b"outside");
}

#[test]
fn failed_second_removal_cancels_the_first_using_its_open_handle() {
    let temp = fixture();
    let root = temp.path();
    let locks: Vec<_> = [LEGACY_UPDATE_LOCK, LEGACY_INSTANCE_LOCK]
        .into_iter()
        .map(|name| {
            let path = root.join(name);
            let file = File::options()
                .access_mode(GENERIC_READ | DELETE)
                .share_mode(0)
                .open(&path)
                .unwrap();
            (path, file)
        })
        .collect();
    let mut calls = 0;
    let result = mark_locks_for_removal(&locks, |file, delete| {
        calls += 1;
        if calls == 2 {
            Err(io::Error::from_raw_os_error(ERROR_ACCESS_DENIED as i32))
        } else {
            set_delete_disposition(file, delete)
        }
    });
    assert!(result.is_err());
    assert_eq!(calls, 3);
    drop(locks);
    assert_legacy_kept(root);
}
