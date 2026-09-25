use crate::{
    manifest::{PackageManifest, checked_path},
    transaction,
};

static COMMITTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn committed() -> bool {
    COMMITTED.load(std::sync::atomic::Ordering::Relaxed)
}
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestartContext {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub resource_dir: PathBuf,
    pub profile: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub root: PathBuf,
    pub work: PathBuf,
    pub restart: RestartContext,
}

fn lock_file(root: &Path, name: &str) -> Result<File> {
    let path = checked_path(root, name)?;
    fs::create_dir_all(path.parent().context("missing lock directory")?)?;
    Ok(File::options().read(true).write(true).create(true).truncate(false).open(path)?)
}

/// All packaged BMZ processes hold this until shutdown (including Viewer/CLI).
pub fn instance_guard(root: &Path) -> Result<Vec<File>> {
    let legacy = PackageManifest::read(root)?.manifest_path() == crate::LEGACY_MANIFEST;
    let mut guards = Vec::new();
    for name in [crate::INSTANCE_LOCK, crate::LEGACY_INSTANCE_LOCK] {
        let path = checked_path(root, name)?;
        let exists = path.try_exists()?;
        if (legacy && name == crate::INSTANCE_LOCK
            || !legacy && name == crate::LEGACY_INSTANCE_LOCK)
            && !exists
        {
            continue;
        }
        // Packages ship an empty instance lock, permitting read-only launches with data overrides.
        let file = if exists { File::open(path)? } else { lock_file(root, name)? };
        file.try_lock_shared().context("BMZ is being updated")?;
        guards.push(file);
    }
    ensure!(
        transaction::pending_update(root)?.is_none(),
        "interrupted update: run bmz-updater --recover INSTALL_DIR before starting BMZ"
    );
    Ok(guards)
}

// Hold both generations during migration. Never delete legacy locks: replacing a lock file
// can split the lock identity while another process still has the original file open.
fn update_guards(root: &Path) -> Result<(Vec<File>, Vec<File>)> {
    let legacy = checked_path(root, crate::LEGACY_MANIFEST)?.try_exists()?
        || checked_path(root, crate::LEGACY_INSTANCE_LOCK)?.try_exists()?
        || checked_path(root, &format!("{}/active.json", crate::LEGACY_WORK_DIR))?.try_exists()?;
    let mut updates = vec![lock_file(root, crate::UPDATE_LOCK)?];
    if legacy || checked_path(root, crate::LEGACY_UPDATE_LOCK)?.try_exists()? {
        updates.push(lock_file(root, crate::LEGACY_UPDATE_LOCK)?);
    }
    for lock in &updates {
        lock.try_lock().context("another updater is running")?;
    }
    let mut instances = vec![lock_file(root, crate::INSTANCE_LOCK)?];
    if legacy {
        instances.push(lock_file(root, crate::LEGACY_INSTANCE_LOCK)?);
    }
    Ok((updates, instances))
}

pub struct Handoff {
    child: Option<Child>,
}

impl Handoff {
    /// Only commit when the UI has rechecked that it is still safe to shut down.
    pub fn commit(mut self) -> Result<()> {
        let child = self.child.as_mut().context("missing updater process")?;
        child.stdin.take().context("missing updater pipe")?.write_all(b"APPLY\n")?;
        self.child.take();
        // Detach. The updater waits for the shared instance locks to be released.
        Ok(())
    }
}

impl Drop for Handoff {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn start(request: &Request) -> Result<Handoff> {
    transaction::preflight(&request.root, &request.work)?;
    let exe = checked_path(&request.work, "helper.exe")?;
    let package = PackageManifest::read(&request.root)?;
    fs::copy(checked_path(&request.root, package.helper_path())?, &exe)?;
    let request_path = request.work.join("request.json");
    transaction::atomic_json(&request_path, request)?;
    let mut command = Command::new(exe);
    command
        .arg("--apply")
        .arg(request_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().context("failed to start update helper")?;
    let stdout = child.stdout.take().context("missing updater stdout")?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
        let _ = tx.send(result);
    });
    match rx.recv_timeout(Duration::from_secs(30)) {
        Ok(Ok(line)) if line.trim() == "READY" => Ok(Handoff { child: Some(child) }),
        Ok(Ok(line)) if line.starts_with("ERROR ") => {
            let _ = child.wait();
            bail!("{}", line.trim_start_matches("ERROR ").trim());
        }
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            bail!("updater did not become ready; BMZ has not been closed")
        }
    }
}

pub fn run_request(path: &Path) -> Result<()> {
    ensure!(path.metadata()?.len() <= 64 * 1024, "invalid updater request");
    let request: Request = serde_json::from_reader(File::open(path)?)?;
    let root = request.root.canonicalize()?;
    let work = request.work.canonicalize()?;
    let (_update_locks, instances) = update_guards(&root)?;
    transaction::preflight(&root, &work)?;
    println!("READY");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    ensure!(answer.trim() == "APPLY", "update was canceled before shutdown");
    COMMITTED.store(true, std::sync::atomic::Ordering::Relaxed);
    let started = Instant::now();
    for instance in &instances {
        while instance.try_lock().is_err() {
            ensure!(
                started.elapsed() < Duration::from_secs(60),
                "another BMZ process is still running; close it and retry"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let result = transaction::apply(&root, &work);
    drop(instances);
    let log = match &result {
        Ok(()) => "Update completed.\n".to_owned(),
        Err(e) => format!("{e:#}\n"),
    };
    fs::write(work.join("update.log"), log)?;
    // Only launch if the installation is coherent; never run after failed rollback.
    if transaction::pending_update(&root)?.is_none() {
        restart(&root, &request.restart)?;
    }
    result
}

fn restart(root: &Path, context: &RestartContext) -> Result<()> {
    ensure!(
        !context.profile.is_empty()
            && context.profile.len() <= 64
            && context.profile.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "invalid restart profile"
    );
    Command::new(root.join("bmz-player.exe"))
        .current_dir(root)
        .arg("--profile")
        .arg(&context.profile)
        .env("BMZ_DATA_DIR", &context.data_dir)
        .env("BMZ_CACHE_DIR", &context.cache_dir)
        .env("BMZ_LOGS_DIR", &context.logs_dir)
        .env("BMZ_RESOURCE_DIR", &context.resource_dir)
        .spawn()
        .context("updated BMZ could not be restarted")?;
    Ok(())
}

pub fn recover(root: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    let (_updates, instances) = update_guards(&root)?;
    for instance in &instances {
        instance.try_lock().context("close all BMZ processes before recovery")?;
    }
    transaction::recover(&root)
}

pub fn available_space(root: &Path) -> Result<u64> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let root: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut available = 0;
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                root.as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(available)
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        Ok(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_running_instances_block_exclusive_update_lock() {
        let temp = tempfile::tempdir().unwrap();
        crate::manifest::write_test_package(
            temp.path(),
            "0.5.0",
            &[("bmz-player.exe", b"app"), (crate::HELPER, b"helper")],
        );
        let first = instance_guard(temp.path()).unwrap();
        let second = instance_guard(temp.path()).unwrap();
        let updater = lock_file(temp.path(), crate::INSTANCE_LOCK).unwrap();
        assert!(updater.try_lock().is_err());
        drop(first);
        assert!(updater.try_lock().is_err());
        drop(second);
        updater.try_lock().unwrap();
        assert!(instance_guard(temp.path()).is_err());
    }

    #[test]
    fn bridge_updates_wait_for_legacy_instances_and_guard_both_lock_generations() {
        let temp = tempfile::tempdir().unwrap();
        crate::manifest::write_test_package(
            temp.path(),
            "0.4.0",
            &[("bmz-player.exe", b"app"), (crate::LEGACY_HELPER, b"helper")],
        );
        let running = instance_guard(temp.path()).unwrap();
        let (_updates, locks) = update_guards(temp.path()).unwrap();
        assert_eq!(locks.len(), 2);
        assert!(locks[1].try_lock().is_err());
        drop(running);
        for lock in &locks {
            lock.try_lock().unwrap();
        }
        assert!(instance_guard(temp.path()).is_err());
        assert!(update_guards(temp.path()).is_err());
    }
}
