use super::lock_file;
use crate::{
    INSTANCE_LOCK, LEGACY_HELPER, LEGACY_INSTANCE_LOCK, LEGACY_MANIFEST, LEGACY_UPDATE_LOCK,
    LEGACY_WORK_DIR, UPDATE_LOCK, manifest::checked_path, transaction::pending_update,
};
use anyhow::{Context, Result};
use std::{
    fs::{self, File},
    io,
    os::windows::{fs::MetadataExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
};
use windows_sys::Win32::{
    Foundation::{ERROR_LOCK_VIOLATION, ERROR_SHARING_VIOLATION, GENERIC_READ},
    Storage::FileSystem::{
        DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_INFO, FILE_FLAG_OPEN_REPARSE_POINT,
        FileDispositionInfo, SetFileInformationByHandle,
    },
};

pub(super) fn cleanup(root: &Path) -> Result<()> {
    let work = checked_path(root, LEGACY_WORK_DIR)?;
    let instance = checked_path(root, LEGACY_INSTANCE_LOCK)?;
    let update = checked_path(root, LEGACY_UPDATE_LOCK)?;
    if !work.try_exists()? && !instance.try_exists()? && !update.try_exists()? {
        return Ok(());
    }

    let update_guard = lock_file(root, UPDATE_LOCK)?;
    if update_guard.try_lock().is_err() {
        return Ok(());
    }
    let instance_guard = lock_file(root, INSTANCE_LOCK)?;
    if instance_guard.try_lock().is_err()
        || pending_update(root)?.is_some()
        || checked_path(root, LEGACY_MANIFEST)?.try_exists()?
        || checked_path(root, LEGACY_HELPER)?.try_exists()?
    {
        return Ok(());
    }

    // Take both handles before moving anything. A byte-range lock alone still allows open handles.
    let mut legacy_locks = Vec::new();
    for path in [update, instance] {
        let file = match File::options()
            .access_mode(GENERIC_READ | DELETE)
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error)
                if error.raw_os_error().is_some_and(|code| {
                    matches!(code as u32, ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
                }) =>
            {
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.len() != 0
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Ok(());
        }
        legacy_locks.push(file);
    }

    if work.try_exists()? {
        move_completed_jobs(root, &work)?;
    }
    for file in legacy_locks {
        let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
        // Mark the same exclusively opened file for deletion when its handle closes.
        if unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileDispositionInfo,
                (&raw const disposition).cast(),
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error().into());
        }
    }
    Ok(())
}

fn move_completed_jobs(root: &Path, work: &Path) -> Result<()> {
    for entry in fs::read_dir(work)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|name| name.starts_with("job-")) else { continue };
        let path = entry.path();
        if !entry.file_type()?.is_dir() || !checked_path(&path, "result.json")?.is_file() {
            continue;
        }
        let backup = checked_path(root, &format!("updater/legacy-backups/{name}"))?;
        if !backup.try_exists()? {
            fs::create_dir_all(backup.parent().context("missing legacy backup directory")?)?;
            fs::rename(path, backup)?;
        }
    }
    if fs::read_dir(work)?.next().is_none() {
        fs::remove_dir(work)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
