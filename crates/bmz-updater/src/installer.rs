//! Retire known app-local DLLs after a successful Inno Setup upgrade to SxS.

use crate::{
    manifest::{PackageKind, PackageManifest, checked_path},
    transaction,
};
use anyhow::{Result, ensure};
use std::{fs, path::Path, time::SystemTime};

const BACKUP: &str = "updater/installer-dll-backup";
const PREVIOUS: &str = "updater/installer-dll-backup/previous-package.json";

pub fn prepare_dll_cleanup(root: &Path) -> Result<()> {
    let Some(old) = PackageManifest::read_optional(root)? else { return Ok(()) };
    if old.kind != PackageKind::Installer || old.uses_sxs() {
        return Ok(());
    }
    let previous = checked_path(root, PREVIOUS)?;
    fs::create_dir_all(checked_path(root, BACKUP)?)?;
    // Keep the inventory even if Setup or an individual rename fails and Setup has to be rerun.
    transaction::atomic_json(&previous, &old)
}

pub fn retire_dlls(root: &Path, expected_version: &str) -> Result<()> {
    let Some(old) = PackageManifest::read_file(&checked_path(root, PREVIOUS)?)? else {
        return Ok(());
    };
    ensure!(old.kind == PackageKind::Installer && !old.uses_sxs(), "invalid flat Setup snapshot");
    let new = PackageManifest::read(root)?;
    ensure!(
        new.kind == PackageKind::Installer
            && new.uses_sxs()
            && new.version == expected_version
            && new.target == old.target,
        "SxS Setup did not install the expected package"
    );
    ensure!(transaction::pending_update(root)?.is_none(), "unfinished portable update");
    // Setup can finish with a file skipped or pending replacement. Check the installed loader inputs.
    for entry in new
        .files
        .iter()
        .filter(|entry| entry.path == "bmz-player.exe" || entry.path.starts_with("dll/"))
    {
        ensure!(
            entry.matches(&checked_path(root, &entry.path)?, &mut || Ok(()))?,
            "installed SxS file did not verify: {}",
            entry.path
        );
    }

    let mut dlls = Vec::new();
    for entry in old.files.iter().filter(|entry| {
        !entry.path.contains('/') && entry.path.to_ascii_lowercase().ends_with(".dll")
    }) {
        let name = &entry.path;
        match checked_path(root, name).and_then(|path| {
            let matches = entry.matches(&path, &mut || Ok(()))?;
            Ok((path, matches))
        }) {
            Ok((path, true)) => dlls.push((name, path)),
            Ok((path, false)) if path.exists() => println!("Preserving modified root DLL: {name}"),
            Err(error) => eprintln!("Preserving root DLL {name}: {error:#}"),
            _ => {}
        }
    }
    if dlls.is_empty() {
        return Ok(());
    }

    let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_nanos();
    let backup = checked_path(root, &format!("{BACKUP}/{stamp}-{}", std::process::id()))?;
    fs::create_dir(&backup)?;
    let mut complete = true;
    for (name, path) in dlls {
        let destination = backup.join(name);
        match fs::rename(&path, &destination) {
            Ok(()) => println!("Retired root DLL to {}", destination.display()),
            Err(error) => {
                eprintln!("Kept root DLL {name}: {error}");
                complete = false;
            }
        }
    }
    ensure!(complete, "some DLLs could not be retired; close running applications and rerun Setup");
    Ok(())
}
