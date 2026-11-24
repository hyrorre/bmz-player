use crate::{
    LEGACY_HELPER, LEGACY_INSTANCE_LOCK, LEGACY_MANIFEST, LEGACY_UPDATE_LOCK, LEGACY_WORK_DIR,
    manifest::checked_path, transaction::pending_update,
};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File},
    io,
    os::windows::{ffi::OsStrExt, fs::MetadataExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{
        ERROR_ACCESS_DENIED, ERROR_DELETE_PENDING, ERROR_LOCK_VIOLATION, ERROR_SHARING_VIOLATION,
        GENERIC_READ,
    },
    Storage::FileSystem::{
        DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_INFO, FILE_FLAG_OPEN_REPARSE_POINT,
        FileDispositionInfo, MOVEFILE_WRITE_THROUGH, MoveFileExW, SetFileInformationByHandle,
    },
};

pub(super) fn retry_legacy_open(open: impl Fn() -> Result<Option<File>>) -> Result<Option<File>> {
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        let result = open();
        if !result
            .as_ref()
            .err()
            .and_then(|error| error.downcast_ref())
            .is_some_and(legacy_lock_busy)
            || Instant::now() >= deadline
        {
            return result;
        }
        // Legacy handles can be temporarily unavailable while another launch closes them.
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn legacy_lock_busy(error: &io::Error) -> bool {
    // An open during delete-on-close can report access denied on Windows.
    matches!(
        error.raw_os_error().map(|code| code as u32),
        Some(
            ERROR_ACCESS_DENIED
                | ERROR_DELETE_PENDING
                | ERROR_SHARING_VIOLATION
                | ERROR_LOCK_VIOLATION
        )
    )
}

pub(super) fn cleanup(root: &Path) -> Result<()> {
    if legacy_layout_present(root)? || pending_update(root)?.is_some() {
        return Ok(());
    }
    let mut paths = Vec::new();
    for name in [LEGACY_UPDATE_LOCK, LEGACY_INSTANCE_LOCK] {
        let path = checked_path(root, name)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) => validate_lock(&path, &metadata)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error).with_context(|| format!("stat {}", path.display())),
        }
        paths.push(path);
    }
    let work = checked_path(root, LEGACY_WORK_DIR)?;
    let (jobs, empty_work) = completed_jobs(root, &work)?;
    if jobs.is_empty() && !empty_work && paths.is_empty() {
        return Ok(());
    }

    // Hold both old handles only for deletion. The caller's shared modern lock covers job moves.
    let mut legacy_locks = Vec::new();
    for path in paths {
        let file = match File::options()
            .access_mode(GENERIC_READ | DELETE)
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) if legacy_lock_busy(&error) => return Ok(()),
            Err(error) => return Err(error).with_context(|| format!("open {}", path.display())),
        };
        validate_lock(&path, &file.metadata()?)?;
        legacy_locks.push((path, file));
    }
    if legacy_layout_present(root)? || pending_update(root)?.is_some() {
        return Ok(());
    }

    mark_locks_for_removal(&legacy_locks, set_delete_disposition)?;
    drop(legacy_locks);

    let remove_work = empty_work || !jobs.is_empty();
    for (path, backup) in jobs {
        fs::create_dir_all(backup.parent().context("missing legacy backup directory")?)?;
        if let Err(error) = move_job(&path, &backup)
            && !backup.try_exists()?
        {
            return Err(error)
                .with_context(|| format!("move {} to {}", path.display(), backup.display()));
        }
    }
    if remove_work {
        match fs::remove_dir(&work) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
                ) => {}
            Err(error) => return Err(error).with_context(|| format!("remove {}", work.display())),
        }
    }
    Ok(())
}

fn move_job(source: &Path, backup: &Path) -> io::Result<()> {
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let backup: Vec<u16> = backup.as_os_str().encode_wide().chain(Some(0)).collect();
    // A concurrent launch may have created the destination after the scan.
    if unsafe { MoveFileExW(source.as_ptr(), backup.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn legacy_layout_present(root: &Path) -> Result<bool> {
    Ok(checked_path(root, LEGACY_MANIFEST)?.try_exists()?
        || checked_path(root, LEGACY_HELPER)?.try_exists()?)
}

fn validate_lock(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    ensure!(
        metadata.is_file()
            && metadata.len() == 0
            && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
        "legacy lock is not an empty regular file: {}",
        path.display()
    );
    ensure!(!metadata.permissions().readonly(), "legacy lock is read-only: {}", path.display());
    Ok(())
}

fn mark_locks_for_removal(
    locks: &[(PathBuf, File)],
    mut set_disposition: impl FnMut(&File, bool) -> io::Result<()>,
) -> Result<()> {
    for (index, (path, file)) in locks.iter().enumerate() {
        if let Err(error) = set_disposition(file, true) {
            for (previous_path, previous) in &locks[..index] {
                if let Err(rollback) = set_disposition(previous, false) {
                    return Err(error).context(format!(
                        "could not mark {} for removal, and could not cancel removal of {}: {rollback}",
                        path.display(), previous_path.display()
                    ));
                }
            }
            return Err(error).with_context(|| format!("mark {} for removal", path.display()));
        }
    }
    Ok(())
}

fn set_delete_disposition(file: &File, delete: bool) -> io::Result<()> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: delete };
    // Change the disposition of the same exclusively opened file.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            (&raw const disposition).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn completed_jobs(root: &Path, work: &Path) -> Result<(Vec<(PathBuf, PathBuf)>, bool)> {
    let mut entries = match fs::read_dir(work) {
        Ok(entries) => entries.peekable(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), false)),
        Err(error) => return Err(error).with_context(|| format!("read {}", work.display())),
    };
    let empty = entries.peek().is_none();
    let mut jobs = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|name| name.starts_with("job-")) else { continue };
        let path = entry.path();
        if !entry.file_type()?.is_dir() || !checked_path(&path, "result.json")?.is_file() {
            continue;
        }
        let backup = checked_path(root, &format!("updater/legacy-backups/{name}"))?;
        if !backup.try_exists()? {
            jobs.push((path, backup));
        }
    }
    Ok((jobs, empty))
}

#[cfg(test)]
mod tests;
