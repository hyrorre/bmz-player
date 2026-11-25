use super::*;
use crate::{
    INSTANCE_LOCK, MANIFEST, UPDATE_LOCK,
    manifest::{PackageKind, PackageManifest},
    process::{instance_guard, lock_file, update_guards},
};
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::{
    Foundation::ERROR_ACCESS_DENIED, Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS,
};

const OLD_BACKUP: &str = ".bmz-update/job-finished/backup/0";
const MOVED_BACKUP: &str = "updater/legacy-backups/job-finished/backup/0";

fn fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    write_fixture(temp.path());
    temp
}

fn write_fixture(root: &Path) {
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
    // The kept lock was reported by the launch that moved the backups, not on every start.
    let second = instance_guard(root).unwrap();
    assert!(second.cleanup_error.is_none());
    assert!(!root.join(UPDATE_LOCK).exists());
    let (updates, instances) = update_guards(root).unwrap();
    assert!(matches!(instances[0].try_lock(), Err(std::fs::TryLockError::WouldBlock)));
    for update in &updates {
        update.unlock().unwrap();
    }
    assert_eq!(fs::read(legacy_update).unwrap(), b"unexpected lock contents");
    assert!(!root.join(LEGACY_INSTANCE_LOCK).exists());
    assert_eq!(fs::read(root.join(MOVED_BACKUP)).unwrap(), b"previous file");
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
fn readonly_and_nonempty_locks_are_preserved_while_backups_move() {
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
        let open_lock = File::open(&path).unwrap();
        assert!(instance_guard(root).unwrap().cleanup_error.is_none());
        assert_legacy_kept(root);
        drop(open_lock);

        let guard = instance_guard(root).unwrap();
        assert!(
            format!("{:#}", guard.cleanup_error.as_ref().unwrap()).contains(LEGACY_INSTANCE_LOCK)
        );
        assert!(!root.join(LEGACY_UPDATE_LOCK).exists());
        assert!(!root.join(LEGACY_WORK_DIR).exists());
        assert_eq!(fs::read(root.join(MOVED_BACKUP)).unwrap(), b"previous file");
        drop(guard);
        assert!(instance_guard(root).unwrap().cleanup_error.is_none());
        // The helper joins the kept lock without recreating the removed legacy update lock.
        let (updates, instances) = update_guards(root).unwrap();
        assert_eq!((updates.len(), instances.len()), (1, 2));
        drop((updates, instances));
        assert!(!root.join(LEGACY_UPDATE_LOCK).exists());
        if readonly {
            assert!(fs::metadata(&path).unwrap().permissions().readonly());
            fs::set_permissions(&path, permissions).unwrap();
        } else {
            assert_eq!(fs::read(path).unwrap(), b"not an empty lock");
        }
    }
}

#[test]
fn every_kept_lock_is_named_in_the_launch_that_moves_backups() {
    let temp = fixture();
    let root = temp.path();
    fs::write(root.join(LEGACY_UPDATE_LOCK), b"not an empty lock").unwrap();
    let instance = root.join(LEGACY_INSTANCE_LOCK);
    let permissions = fs::metadata(&instance).unwrap().permissions();
    let mut read_only = permissions.clone();
    read_only.set_readonly(true);
    fs::set_permissions(&instance, read_only).unwrap();

    let first = instance_guard(root).unwrap().cleanup_error.map(|error| format!("{error:#}"));
    let second = instance_guard(root).unwrap().cleanup_error.is_none();
    fs::set_permissions(&instance, permissions).unwrap();

    let first = first.unwrap();
    assert!(first.contains(LEGACY_UPDATE_LOCK) && first.contains("not empty"), "{first}");
    assert!(first.contains(LEGACY_INSTANCE_LOCK) && first.contains("read-only"), "{first}");
    assert!(second);
    assert_eq!(fs::read(root.join(MOVED_BACKUP)).unwrap(), b"previous file");
}

#[test]
fn read_only_modern_locks_remain_usable_by_launches_and_the_helper() {
    let temp = fixture();
    let root = temp.path();
    let paths = [INSTANCE_LOCK, UPDATE_LOCK].map(|name| {
        drop(lock_file(root, name).unwrap());
        root.join(name)
    });
    let original = paths.each_ref().map(|path| fs::metadata(path).unwrap().permissions());
    for (path, permissions) in paths.iter().zip(&original) {
        let mut read_only = permissions.clone();
        read_only.set_readonly(true);
        fs::set_permissions(path, read_only).unwrap();
    }
    let launch = instance_guard(root).map(drop);
    let helper = update_guards(root).and_then(|(_updates, instances)| {
        instances[0].try_lock()?;
        instances[0].unlock()?;
        Ok(())
    });
    for (path, permissions) in paths.iter().zip(original) {
        assert!(fs::metadata(path).unwrap().permissions().readonly());
        fs::set_permissions(path, permissions).unwrap();
    }
    launch.unwrap();
    helper.unwrap();
}

#[test]
fn creating_a_lock_without_permission_is_reported_without_retrying() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("no-new-files");
    fs::create_dir(&directory).unwrap();
    let change_acl = |args: [&str; 2]| {
        let output =
            std::process::Command::new("icacls").arg(&directory).args(args).output().unwrap();
        assert!(output.status.success(), "{output:?}");
    };
    change_acl(["/deny", "*S-1-1-0:(WD)"]);
    let result = crate::process::create_lock(&directory.join("instance.lock"));
    change_acl(["/remove:d", "*S-1-1-0"]);

    let error = result.unwrap_err();
    assert!(format!("{error:#}").contains("could not create"), "{error:#}");
    // retry_legacy_open only retries I/O errors that report a busy or deleting lock.
    assert!(error.downcast_ref::<io::Error>().is_none());
}

#[test]
fn the_helper_can_exclusively_lock_both_read_only_legacy_files() {
    let temp = fixture();
    let root = temp.path();
    let paths = [root.join(LEGACY_INSTANCE_LOCK), root.join(LEGACY_UPDATE_LOCK)];
    let original = paths.each_ref().map(|path| fs::metadata(path).unwrap().permissions());
    for (path, permissions) in paths.iter().zip(&original) {
        let mut read_only = permissions.clone();
        read_only.set_readonly(true);
        fs::set_permissions(path, read_only).unwrap();
    }
    let result = update_guards(root).and_then(|(updates, instances)| {
        assert_eq!(updates.len(), 2);
        assert_eq!(instances.len(), 2);
        instances[1].try_lock()?;
        instances[1].unlock()?;
        Ok(())
    });
    for (path, permissions) in paths.iter().zip(original) {
        assert!(fs::metadata(path).unwrap().permissions().readonly());
        fs::set_permissions(path, permissions).unwrap();
    }
    result.unwrap();
}

#[test]
fn cleanup_reports_a_delete_permission_denial() {
    let temp = fixture();
    let root = temp.path();
    let path = root.join(LEGACY_UPDATE_LOCK);
    let change_acl = |args: [&str; 2]| {
        let output = std::process::Command::new("icacls").arg(&path).args(args).output().unwrap();
        assert!(output.status.success(), "{output:?}");
    };
    change_acl(["/deny", "*S-1-1-0:(D)"]);
    let guard = instance_guard(root);
    let attempts = std::cell::Cell::new(0);
    let result = retry_legacy_open(&path, || {
        attempts.set(attempts.get() + 1);
        Ok(Some(File::options().access_mode(GENERIC_READ | DELETE).open(&path)?))
    });
    change_acl(["/remove:d", "*S-1-1-0"]);

    let error = guard.unwrap().cleanup_error.unwrap();
    assert!(format!("{error:#}").contains(LEGACY_UPDATE_LOCK));
    assert_eq!(error.downcast_ref::<io::Error>().unwrap().kind(), io::ErrorKind::PermissionDenied);
    assert!(result.is_err());
    assert_eq!(attempts.get(), 1);
    assert_legacy_kept(root);
}

#[test]
fn a_backup_directory_created_after_the_scan_is_not_replaced() {
    let temp = fixture();
    let root = temp.path();
    let work = root.join(LEGACY_WORK_DIR);
    let (jobs, empty_work) = completed_jobs(root, &work).unwrap();
    let [(_, backup)] = jobs.as_slice() else { panic!("expected one completed job") };
    fs::create_dir_all(backup).unwrap();
    fs::write(backup.join("sentinel"), b"existing directory").unwrap();
    finish_jobs(&work, jobs, empty_work).unwrap();
    assert_eq!(
        fs::read(root.join("updater/legacy-backups/job-finished/sentinel")).unwrap(),
        b"existing directory"
    );
    assert_eq!(fs::read(root.join(OLD_BACKUP)).unwrap(), b"previous file");
}

#[test]
fn a_concurrent_cleaner_can_move_a_job_and_leave_the_old_directory_delete_pending() {
    let temp = fixture();
    let root = temp.path();
    let work = root.join(LEGACY_WORK_DIR);
    let (jobs, empty_work) = completed_jobs(root, &work).unwrap();
    let [(source, backup)] = jobs.as_slice() else { panic!("expected one completed job") };
    fs::create_dir_all(backup.parent().unwrap()).unwrap();
    fs::rename(source, backup).unwrap();
    let directory = File::options()
        .access_mode(DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(&work)
        .unwrap();
    set_delete_disposition(&directory, true).unwrap();
    assert_eq!(fs::remove_dir(&work).unwrap_err().kind(), io::ErrorKind::PermissionDenied);
    finish_jobs(&work, jobs, empty_work).unwrap();
    assert!(instance_guard(root).unwrap().cleanup_error.is_none());
    assert!(update_guards(root).is_ok());
    drop(directory);
    assert!(!work.exists());
    assert_eq!(fs::read(root.join(MOVED_BACKUP)).unwrap(), b"previous file");
}

#[test]
fn completed_jobs_move_when_the_destination_exceeds_win32_max_path() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("a".repeat(110)).join("b".repeat(110));
    assert!(root.join(MOVED_BACKUP).as_os_str().encode_wide().count() > 260);
    fs::create_dir_all(&root).unwrap();
    write_fixture(&root.canonicalize().unwrap());
    assert!(instance_guard(&root).unwrap().cleanup_error.is_none());
    assert!(!root.join(LEGACY_WORK_DIR).exists());
    assert_eq!(fs::read(root.join(MOVED_BACKUP)).unwrap(), b"previous file");
}

#[test]
fn legacy_lock_reparse_point_is_preserved_while_backups_move() {
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
    assert!(!root.join(LEGACY_INSTANCE_LOCK).exists());
    assert!(!root.join(LEGACY_WORK_DIR).exists());
    assert_eq!(fs::read(root.join(MOVED_BACKUP)).unwrap(), b"previous file");
    drop(guard);
    let (updates, _) = update_guards(root).unwrap();
    assert_eq!(updates.len(), 1);
    drop(updates);
    assert!(path.is_dir());
    assert_eq!(fs::read(target.path().join("sentinel")).unwrap(), b"outside");
}

#[test]
fn a_directory_at_the_legacy_instance_lock_does_not_block_startup_or_updates() {
    let temp = fixture();
    let root = temp.path();
    let path = root.join(LEGACY_INSTANCE_LOCK);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    fs::write(path.join("user-file"), b"kept").unwrap();

    let guard = instance_guard(root).unwrap();
    let error = format!("{:#}", guard.cleanup_error.as_ref().unwrap());
    assert!(error.contains(LEGACY_INSTANCE_LOCK), "{error}");
    assert_eq!(guard._locks.len(), 1);
    drop(guard);
    let (updates, instances) = update_guards(root).unwrap();
    assert_eq!((updates.len(), instances.len()), (1, 1));
    drop((updates, instances));
    assert_eq!(fs::read(path.join("user-file")).unwrap(), b"kept");
    assert!(!root.join(LEGACY_UPDATE_LOCK).exists());
    assert_eq!(fs::read(root.join(MOVED_BACKUP)).unwrap(), b"previous file");
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
