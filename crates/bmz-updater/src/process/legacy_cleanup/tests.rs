use super::*;
use crate::process::instance_guard;

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
    assert_eq!(guard.len(), 1);
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
    let updater = lock_file(root, UPDATE_LOCK).unwrap();
    updater.try_lock().unwrap();
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
    fs::write(root.join(".bmz-update/user-file"), b"user file").unwrap();
    fs::create_dir_all(root.join(MOVED_BACKUP).parent().unwrap()).unwrap();
    fs::write(root.join(MOVED_BACKUP), b"existing backup").unwrap();
    drop(instance_guard(root).unwrap());
    assert!(root.join(".bmz-update/job-unfinished").is_dir());
    assert_eq!(fs::read(root.join(".bmz-update/user-file")).unwrap(), b"user file");
    assert_eq!(fs::read(root.join(OLD_BACKUP)).unwrap(), b"previous file");
    assert_eq!(fs::read(root.join(MOVED_BACKUP)).unwrap(), b"existing backup");
}
