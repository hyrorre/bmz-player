use crate::manifest::{
    PackageKind, PackageManifest, checked_path, managed_path, reject_link, safe_relative,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Operation {
    path: String,
    existed: bool,
    install: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    directory: String,
    operations: Vec<Operation>,
    complete: bool,
}

pub fn active_path(root: &Path) -> PathBuf {
    root.join(crate::WORK_DIR).join("active.json")
}

pub fn new_work_dir(root: &Path) -> Result<PathBuf> {
    let base = checked_path(root, crate::WORK_DIR)?;
    fs::create_dir_all(&base)?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
    let path = base.join(format!("job-{stamp}-{}", std::process::id()));
    fs::create_dir(&path)?;
    Ok(path)
}

pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    reject_link(path)?;
    let tmp = path.with_extension("new");
    reject_link(&tmp)?;
    let mut file = File::create(&tmp)?;
    serde_json::to_writer(&mut file, value)?;
    file.flush()?;
    file.sync_all()?;
    drop(file);
    durable_rename(&tmp, path)
}

fn durable_rename(source_path: &Path, path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        let source: Vec<u16> = source_path.as_os_str().encode_wide().chain(Some(0)).collect();
        let dest: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            MoveFileExW(
                source.as_ptr(),
                dest.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(source_path, path)?;
        File::open(source_path.parent().context("missing source parent")?)?.sync_all()?;
        File::open(path.parent().context("missing parent")?)?.sync_all()?;
    }
    Ok(())
}

pub fn preflight(root: &Path, work: &Path) -> Result<()> {
    ensure!(
        work.parent() == Some(root.join(crate::WORK_DIR).as_path()),
        "work directory must be inside installation"
    );
    reject_link(&root.join(crate::WORK_DIR))?;
    reject_link(work)?;
    let old = PackageManifest::read(root)?;
    let new = PackageManifest::read(&work.join("stage"))?;
    ensure!(
        old.kind == PackageKind::Portable && new.kind == PackageKind::Portable,
        "portable update required"
    );
    ensure!(old.target == new.target, "update target mismatch");
    ensure!(
        crate::manifest::version(&new.version)? > crate::manifest::version(&old.version)?,
        "update must be newer"
    );
    for entry in &new.files {
        let previous = old.files.iter().find(|f| f.path.eq_ignore_ascii_case(&entry.path));
        if let Some(previous) = previous {
            ensure!(previous.path == entry.path, "case-only path changes require a bridge package");
        } else {
            ensure!(
                !checked_path(root, &entry.path)?.exists(),
                "new package file conflicts with a user file: {}",
                entry.path
            );
        }
    }
    for entry in old.files.iter().chain(&new.files) {
        let path = checked_path(root, &entry.path)?;
        ensure!(!path.exists() || path.is_file(), "update target is not a file");
    }
    // A real write probe detects read-only media/ACLs before the app exits.
    let probe = work.join("write-probe");
    let file = File::options().write(true).create_new(true).open(&probe)?;
    file.sync_all()?;
    drop(file);
    fs::remove_file(probe)?;
    Ok(())
}

pub fn apply(root: &Path, work: &Path) -> Result<()> {
    apply_with_checkpoint(root, work, |_| Ok(()))
}

fn apply_with_checkpoint(
    root: &Path,
    work: &Path,
    mut checkpoint: impl FnMut(usize) -> Result<()>,
) -> Result<()> {
    ensure!(!active_path(root).exists(), "unfinished update; recover it first");
    preflight(root, work)?;
    let old = PackageManifest::read(root)?;
    let new = PackageManifest::read(&work.join("stage"))?;
    // Recheck the prepared payload immediately before mutation, without blocking the UI handshake.
    new.verify_files(&work.join("stage"), || Ok(()))?;
    let mut paths: BTreeSet<String> =
        old.files.iter().chain(&new.files).map(|f| f.path.clone()).collect();
    // The package manifest is the last file committed, and is rolled back like the executable.
    paths.remove(crate::MANIFEST);
    let mut operations = Vec::new();
    for path in paths.into_iter().chain(Some(crate::MANIFEST.to_owned())) {
        let target = checked_path(root, &path)?;
        operations.push(Operation {
            install: path == crate::MANIFEST || new.files.iter().any(|f| f.path == path),
            existed: target.exists(),
            path,
        });
    }
    fs::create_dir(work.join("backup"))?;
    let mut journal = Journal {
        schema: 1,
        directory: work.file_name().context("missing work name")?.to_string_lossy().into_owned(),
        operations,
        complete: false,
    };
    atomic_json(&active_path(root), &journal)?;
    let result = (|| -> Result<()> {
        for (i, op) in journal.operations.iter().enumerate() {
            checkpoint(i * 2)?;
            let target = checked_path(root, &op.path)?;
            if op.existed {
                durable_rename(&target, &work.join("backup").join(i.to_string()))?;
            }
            checkpoint(i * 2 + 1)?;
            if op.install {
                fs::create_dir_all(target.parent().context("missing parent")?)?;
                durable_rename(&checked_path(&work.join("stage"), &op.path)?, &target)?;
            }
        }
        journal.complete = true;
        atomic_json(&active_path(root), &journal)?;
        Ok(())
    })();
    if let Err(error) = result {
        recover(root).context(
            "update failed and rollback could not finish; run bmz-updater --recover INSTALL_DIR",
        )?;
        return Err(error.context("update failed; previous version restored"));
    }
    finish(root, work, &journal)?;
    Ok(())
}

fn finish(root: &Path, work: &Path, journal: &Journal) -> Result<()> {
    // Retain backups, including user modifications to bundled resources. Never erase them implicitly.
    atomic_json(&work.join("result.json"), journal)?;
    fs::remove_file(active_path(root))?;
    Ok(())
}

/// Idempotent rollback of an interrupted transaction. Caller holds the exclusive instance lock.
pub fn recover(root: &Path) -> Result<()> {
    let path = checked_path(root, ".bmz-update/active.json")?;
    if !path.exists() {
        return Ok(());
    }
    ensure!(path.metadata()?.len() <= 32 * 1024 * 1024, "invalid journal size");
    let journal: Journal = serde_json::from_reader(File::open(path)?)?;
    ensure!(
        journal.schema == 1 && journal.operations.len() <= 200_001,
        "unsupported update journal"
    );
    safe_relative(&journal.directory)?;
    ensure!(
        journal.directory.starts_with("job-") && !journal.directory.contains('/'),
        "invalid journal directory"
    );
    let work = checked_path(root, &format!("{}/{}", crate::WORK_DIR, journal.directory))?;
    if !journal.complete {
        for (i, op) in journal.operations.iter().enumerate().rev() {
            managed_path(&op.path)?;
            let target = checked_path(root, &op.path)?;
            let backup = checked_path(&work, &format!("backup/{i}"))?;
            if backup.exists() {
                if target.exists() {
                    fs::remove_file(&target)?;
                }
                fs::create_dir_all(target.parent().context("missing parent")?)?;
                durable_rename(&backup, &target)?;
            } else if !op.existed
                && op.install
                && !checked_path(&work.join("stage"), &op.path)?.exists()
                && target.exists()
            {
                fs::remove_file(target)?;
            }
        }
    }
    finish(root, &work, &journal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{PackageFile, hash_file};

    fn package(root: &Path, version: &str, files: &[(&str, &[u8])]) {
        fs::create_dir_all(root).unwrap();
        let mut entries = Vec::new();
        for (name, bytes) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, bytes).unwrap();
            entries.push(PackageFile {
                path: (*name).into(),
                size: bytes.len() as u64,
                sha256: hash_file(&path).unwrap(),
            });
        }
        atomic_json(
            &root.join(crate::MANIFEST),
            &PackageManifest {
                schema: 1,
                kind: PackageKind::Portable,
                target: "windows-x64".into(),
                version: version.into(),
                min_updater_protocol: 1,
                files: entries,
            },
        )
        .unwrap();
    }

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        package(
            temp.path(),
            "0.4.0",
            &[
                ("bmz-player.exe", b"old app"),
                ("bmz-updater.exe", b"old updater"),
                ("old.dll", b"old library"),
                ("resources/skin.txt", b"stock"),
            ],
        );
        fs::write(temp.path().join("resources/skin.txt"), b"customized").unwrap();
        fs::create_dir(temp.path().join("data")).unwrap();
        fs::write(temp.path().join("data/score.db"), b"scores").unwrap();
        fs::write(temp.path().join("resources/user.txt"), b"extra").unwrap();
        let work = new_work_dir(temp.path()).unwrap();
        package(
            &work.join("stage"),
            "0.5.0",
            &[
                ("bmz-player.exe", b"new app"),
                ("bmz-updater.exe", b"new updater"),
                ("new.dll", b"new library"),
                ("resources/skin.txt", b"new stock"),
            ],
        );
        (temp, work)
    }

    fn assert_original(root: &Path) {
        assert_eq!(fs::read(root.join("bmz-player.exe")).unwrap(), b"old app");
        assert_eq!(fs::read(root.join("bmz-updater.exe")).unwrap(), b"old updater");
        assert_eq!(fs::read(root.join("old.dll")).unwrap(), b"old library");
        assert!(!root.join("new.dll").exists());
        assert_eq!(fs::read(root.join("resources/skin.txt")).unwrap(), b"customized");
        assert_eq!(PackageManifest::read(root).unwrap().version, "0.4.0");
        assert_user_data(root);
    }

    fn assert_user_data(root: &Path) {
        assert_eq!(fs::read(root.join("data/score.db")).unwrap(), b"scores");
        assert_eq!(fs::read(root.join("resources/user.txt")).unwrap(), b"extra");
    }

    #[test]
    fn updates_helper_removes_obsolete_dll_and_preserves_user_files_and_backups() {
        let (temp, work) = fixture();
        apply(temp.path(), &work).unwrap();
        assert_eq!(fs::read(temp.path().join("bmz-updater.exe")).unwrap(), b"new updater");
        assert!(!temp.path().join("old.dll").exists());
        assert!(temp.path().join("new.dll").exists());
        assert_user_data(temp.path());
        let backup_bytes: Vec<_> = fs::read_dir(work.join("backup"))
            .unwrap()
            .map(|e| fs::read(e.unwrap().path()).unwrap())
            .collect();
        assert!(backup_bytes.contains(&b"customized".to_vec()));
        assert!(!active_path(temp.path()).exists());
    }

    #[test]
    fn every_failed_file_move_rolls_back_including_updater() {
        for fail_at in 0..12 {
            let (temp, work) = fixture();
            let result = apply_with_checkpoint(temp.path(), &work, |step| {
                ensure!(step != fail_at, "injected file lock/IO failure");
                Ok(())
            });
            assert!(result.is_err(), "failure point {fail_at}");
            assert_original(temp.path());
            recover(temp.path()).unwrap();
            assert_original(temp.path());
        }
    }

    #[test]
    fn interrupted_process_can_recover_at_every_move_boundary() {
        for stop_at in 0..12 {
            let (temp, work) = fixture();
            let result = std::panic::catch_unwind(|| {
                apply_with_checkpoint(temp.path(), &work, |step| {
                    assert_ne!(step, stop_at, "simulated process termination");
                    Ok(())
                })
            });
            assert!(result.is_err());
            assert!(active_path(temp.path()).exists());
            recover(temp.path()).unwrap();
            assert_original(temp.path());
        }
    }

    #[test]
    fn tampered_staging_and_new_user_file_collisions_fail_before_mutation() {
        let (temp, work) = fixture();
        fs::write(work.join("stage/bmz-updater.exe"), b"tampered").unwrap();
        assert!(apply(temp.path(), &work).is_err());
        assert_original(temp.path());
        let (temp, work) = fixture();
        fs::write(temp.path().join("new.dll"), b"user library").unwrap();
        assert!(apply(temp.path(), &work).is_err());
        assert_eq!(fs::read(temp.path().join("new.dll")).unwrap(), b"user library");
        assert!(!active_path(temp.path()).exists());
    }

    #[cfg(windows)]
    #[test]
    fn a_real_windows_file_lock_restores_already_replaced_files() {
        use std::os::windows::fs::OpenOptionsExt;
        let (temp, work) = fixture();
        let _locked = File::options()
            .read(true)
            .share_mode(1 | 2)
            .open(temp.path().join("resources/skin.txt"))
            .unwrap();
        assert!(apply(temp.path(), &work).is_err());
        assert_original(temp.path());
    }
}
