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
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[cfg(windows)]
mod legacy_cleanup;

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

// Windows can take an exclusive byte-range lock on a read-only handle, so locks are opened
// read-only. That keeps a lock file with the read-only attribute usable.
fn lock_file(root: &Path, name: &str) -> Result<File> {
    let path = checked_path(root, name)?;
    match File::open(&path) {
        Ok(file) => return Ok(file),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    fs::create_dir_all(path.parent().context("missing lock directory")?)?;
    create_lock(&path)
}

/// Creates a missing lock file, or opens the one that another process created first.
fn create_lock(path: &Path) -> Result<File> {
    let create = || match File::options().read(true).write(true).create_new(true).open(path) {
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => File::open(path),
        result => result,
    };
    let result = create();
    #[cfg(windows)]
    if let Err(error) = &result
        && missing_permission(path, error)
    {
        // A name waiting for deletion can disappear before the native query. Try once more,
        // then report missing permission without the I/O error that callers retry.
        return match create() {
            Err(error) if missing_permission(path, &error) => {
                bail!("could not create {}: {error}", path.display())
            }
            result => Ok(result?),
        };
    }
    Ok(result?)
}

/// Win32 also reports a name waiting for deletion as access denied. Only that state is transient.
#[cfg(windows)]
fn missing_permission(path: &Path, error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED as i32)
        && !crate::windows_path::delete_pending(path)
}

/// A directory or reparse point cannot serve as a lock, so no BMZ process coordinates on it.
fn regular_file(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        metadata.is_file() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
    }
    #[cfg(not(windows))]
    {
        metadata.is_file()
    }
}

/// All packaged BMZ processes hold this until shutdown (including Viewer/CLI).
pub struct InstanceGuard {
    _locks: Vec<File>,
    pub cleanup_error: Option<anyhow::Error>,
}

pub fn instance_guard(root: &Path) -> Result<InstanceGuard> {
    let package = PackageManifest::read(root)?;
    let legacy = package.manifest_path() == crate::LEGACY_MANIFEST;
    let mut guards = Vec::new();
    let instance = checked_path(root, crate::INSTANCE_LOCK)?;
    let exists = instance.try_exists()?;
    if !legacy || exists {
        // Packages ship an empty instance lock, permitting read-only launches with data overrides.
        let file = lock_file(root, crate::INSTANCE_LOCK)?;
        file.try_lock_shared().context("BMZ is being updated")?;
        guards.push(file);
    }

    #[cfg(windows)]
    let cleanup_error = if !legacy
        && package.kind == crate::manifest::PackageKind::Portable
        && package.version == env!("CARGO_PKG_VERSION")
    {
        // The shared modern lock protects against apply/recovery without blocking other launches.
        legacy_cleanup::cleanup(root).err()
    } else {
        None
    };
    #[cfg(not(windows))]
    let cleanup_error = None;

    let old_instance = open_legacy_lock(root, crate::LEGACY_INSTANCE_LOCK, legacy)?;
    if let Some(file) = old_instance {
        file.try_lock_shared().context("BMZ is being updated")?;
        guards.push(file);
    }
    ensure!(
        transaction::pending_update(root)?.is_none(),
        "interrupted update: run bmz-updater --recover INSTALL_DIR before starting BMZ"
    );
    Ok(InstanceGuard { _locks: guards, cleanup_error })
}

fn open_legacy_lock(root: &Path, name: &str, create: bool) -> Result<Option<File>> {
    let open = || -> Result<Option<File>> {
        crate::manifest::reject_link(root)?;
        let path = root.join(name);
        match fs::symlink_metadata(&path) {
            // Keep such a path in place, but do not fail startup or updates on it.
            Ok(metadata) if !regular_file(&metadata) => return Ok(None),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        match File::open(&path) {
            Ok(file) => Ok(Some(file)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if create {
                    create_lock(&path).map(Some)
                } else {
                    Ok(None)
                }
            }
            Err(error) => Err(error.into()),
        }
    };
    #[cfg(windows)]
    let result = legacy_cleanup::retry_legacy_open(&root.join(name), open);
    #[cfg(not(windows))]
    let result = open();
    result.with_context(|| format!("could not open {name}"))
}

fn update_guards(root: &Path) -> Result<(Vec<File>, Vec<File>)> {
    let update = lock_file(root, crate::UPDATE_LOCK)?;
    update.try_lock().context("another updater is running")?;
    let legacy = checked_path(root, crate::LEGACY_MANIFEST)?.try_exists()?
        || transaction::pending_update_in(root, crate::LEGACY_WORK_DIR)?.is_some();
    // Existing legacy locks are always joined. Every grouped helper also holds the modern update
    // lock, so legacy locks are created only while a protocol 1 helper may use the installation.
    let old_instance = open_legacy_lock(root, crate::LEGACY_INSTANCE_LOCK, legacy)?;
    let mut updates = vec![update];
    if let Some(lock) = open_legacy_lock(root, crate::LEGACY_UPDATE_LOCK, legacy)? {
        lock.try_lock().context("another updater is running")?;
        updates.push(lock);
    }
    let mut instances = vec![lock_file(root, crate::INSTANCE_LOCK)?];
    instances.extend(old_instance);
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

pub(crate) fn verify_player_startup(root: &Path, work: &Path, version: &str) -> Result<()> {
    let cwd = checked_path(work, "startup-probe")?;
    fs::create_dir(&cwd)?;
    let output = checked_path(work, "startup-version.txt")?;
    let stdout = File::options().write(true).create_new(true).open(&output)?;
    let mut command = Command::new(checked_path(root, "bmz-player.exe")?);
    command
        .current_dir(cwd)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let system = std::env::var_os("SystemRoot").context("SystemRoot is missing")?;
        let system = Path::new(&system);
        command.env("PATH", std::env::join_paths([system.join("System32"), system.to_path_buf()])?);
        command.creation_flags(0x08000000);
    }
    // --version exits before the player's startup guard and any data migration.
    let mut child =
        command.spawn().context("SxS loader check could not start the updated player")?;
    // The first start of a new executable can wait for antivirus scans of every bundled DLL,
    // which takes several seconds on Windows 8.1 VMs and slow disks.
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            child.wait().context("could not stop the timed-out SxS loader check")?;
            bail!("SxS loader check timed out");
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    ensure!(status.success(), "SxS loader check failed: {status}");
    let mut version_text = String::new();
    File::open(output)?.take(4096).read_to_string(&mut version_text)?;
    ensure!(
        version_text.starts_with(&format!("bmz-player {version} (")),
        "unexpected player version in the SxS loader check"
    );
    Ok(())
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
