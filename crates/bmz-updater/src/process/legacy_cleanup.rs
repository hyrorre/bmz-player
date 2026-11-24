use crate::{
    LEGACY_HELPER, LEGACY_INSTANCE_LOCK, LEGACY_MANIFEST, LEGACY_UPDATE_LOCK, LEGACY_WORK_DIR,
    manifest::checked_path,
    transaction::pending_update,
    windows_path::{gone_or_deleting, verbatim_path},
};
use anyhow::{Context, Result, anyhow};
use std::{
    fs::{self, File},
    io,
    os::windows::{fs::MetadataExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{ERROR_LOCK_VIOLATION, ERROR_SHARING_VIOLATION, GENERIC_READ},
    Storage::FileSystem::{
        DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_INFO, FILE_FLAG_OPEN_REPARSE_POINT,
        FileDispositionInfo, MOVEFILE_WRITE_THROUGH, MoveFileExW, SetFileInformationByHandle,
    },
};

pub(super) fn retry_legacy_open(
    path: &Path,
    open: impl Fn() -> Result<Option<File>>,
) -> Result<Option<File>> {
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        let result = open();
        if !result
            .as_ref()
            .err()
            .and_then(|error| error.downcast_ref())
            .is_some_and(|error| legacy_lock_busy(path, error))
            || Instant::now() >= deadline
        {
            return result;
        }
        // Legacy handles can be temporarily unavailable while another launch closes them.
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn legacy_lock_busy(path: &Path, error: &io::Error) -> bool {
    matches!(
        error.raw_os_error().map(|code| code as u32),
        Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
    ) || gone_or_deleting(path, error)
}

pub(super) fn cleanup(root: &Path) -> Result<()> {
    if legacy_layout_present(root)? || pending_update(root)?.is_some() {
        return Ok(());
    }
    let mut paths = Vec::new();
    let mut warning = None;
    for name in [LEGACY_UPDATE_LOCK, LEGACY_INSTANCE_LOCK] {
        // The names are fixed and symlink_metadata does not follow a legacy lock reparse point.
        let path = root.join(name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) if gone_or_deleting(&path, &error) => return Ok(()),
            Err(error) => return Err(error).with_context(|| format!("stat {}", path.display())),
        };
        let defect = lock_defect(&metadata);
        if let Some(defect) = defect {
            warning.get_or_insert_with(|| lock_warning(&path, defect));
        }
        if regular_lock(&metadata) {
            paths.push((path, defect.is_none()));
        }
    }
    let work = checked_path(root, LEGACY_WORK_DIR)?;
    let (jobs, empty_work) = completed_jobs(root, &work)?;
    if jobs.is_empty() && !empty_work && paths.iter().all(|(_, removable)| !removable) {
        return warning.map_or(Ok(()), Err);
    }

    // Hold both old handles only for deletion. The caller's shared modern lock covers job moves.
    let mut removable_locks = Vec::new();
    let mut retained_locks = Vec::new();
    for (path, removable) in paths {
        let file = match File::options()
            .access_mode(if removable { GENERIC_READ | DELETE } else { GENERIC_READ })
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) if legacy_lock_busy(&path, &error) => return Ok(()),
            Err(error) => return Err(error).with_context(|| format!("open {}", path.display())),
        };
        if let Some(defect) = lock_defect(&file.metadata()?) {
            warning.get_or_insert_with(|| lock_warning(&path, defect));
            retained_locks.push(file);
        } else if removable {
            removable_locks.push((path, file));
        } else {
            retained_locks.push(file);
        }
    }
    if legacy_layout_present(root)? || pending_update(root)?.is_some() {
        return Ok(());
    }

    mark_locks_for_removal(&removable_locks, set_delete_disposition)?;
    drop(removable_locks);
    drop(retained_locks);

    finish_jobs(&work, jobs, empty_work)?;
    warning.map_or(Ok(()), Err)
}

fn finish_jobs(work: &Path, jobs: Vec<(PathBuf, PathBuf)>, empty_work: bool) -> Result<()> {
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
        match fs::remove_dir(work) {
            Ok(()) => {}
            Err(error)
                if error.kind() == io::ErrorKind::DirectoryNotEmpty
                    || gone_or_deleting(work, &error) => {}
            Err(error) => return Err(error).with_context(|| format!("remove {}", work.display())),
        }
    }
    Ok(())
}

fn move_job(source: &Path, backup: &Path) -> io::Result<()> {
    let source = verbatim_path(source)?;
    let backup = verbatim_path(backup)?;
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

fn regular_lock(metadata: &fs::Metadata) -> bool {
    metadata.is_file() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
}

fn lock_defect(metadata: &fs::Metadata) -> Option<&'static str> {
    if !regular_lock(metadata) {
        Some("not a regular file or is a reparse point")
    } else if metadata.len() != 0 {
        Some("not empty")
    } else if metadata.permissions().readonly() {
        Some("read-only")
    } else {
        None
    }
}

fn lock_warning(path: &Path, defect: &str) -> anyhow::Error {
    anyhow!(
        "legacy lock {} is {defect} and was kept; close BMZ and its updater before inspecting the file",
        path.display()
    )
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
        Err(error) if gone_or_deleting(work, &error) => return Ok((Vec::new(), false)),
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
