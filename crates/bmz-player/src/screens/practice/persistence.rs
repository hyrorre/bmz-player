//! Practice-only recovery and durable replacement of per-chart settings.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::PracticeProperty;

pub(super) fn load(path: &Path) -> Option<PracticeProperty> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "failed to read practice config; using chart defaults");
            return None;
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(property) => Some(property),
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "invalid practice config; using chart defaults");
            None
        }
    }
}

pub(super) fn save(path: &Path, property: &PracticeProperty) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(property).context("serialize practice property")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create practice dir: {}", parent.display()))?;
    }
    save_after_preserving_invalid(path, &bytes, preserve_invalid)
}

// Keep the backup/replace boundary explicit: a failed backup must prevent even
// creating the replacement file. The callback also permits deterministic I/O failure tests.
fn save_after_preserving_invalid(
    path: &Path,
    bytes: &[u8],
    preserve: impl FnOnce(&Path, &[u8]) -> Result<()>,
) -> Result<()> {
    match fs::read(path) {
        Ok(existing) => {
            if serde_json::from_slice::<PracticeProperty>(&existing).is_err() {
                preserve(path, &existing)?;
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!("read practice config before saving: {}", path.display())
            });
        }
    }
    atomic_replace(path, bytes, |temporary, destination| fs::rename(temporary, destination))
}

fn create_sibling(path: &Path, kind: &str, extension: &str) -> Result<(PathBuf, File)> {
    let filename = path.file_name().context("practice config has no filename")?;
    loop {
        let mut random = [0; 16];
        getrandom::getrandom(&mut random).map_err(|error| {
            anyhow::anyhow!("generate practice {kind} UUID for {}: {error}", path.display())
        })?;
        let id = uuid::Builder::from_random_bytes(random).into_uuid();
        let mut name = filename.to_os_string();
        name.push(format!(".{kind}-{id}{extension}"));
        let sibling = path.with_file_name(name);
        match OpenOptions::new().write(true).create_new(true).open(&sibling) {
            Ok(file) => return Ok((sibling, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("create practice {kind}: {}", sibling.display()));
            }
        }
    }
}

fn preserve_invalid(path: &Path, bytes: &[u8]) -> Result<()> {
    let (backup, mut file) = create_sibling(path, "corrupt", ".bak")?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .with_context(|| format!("preserve invalid practice config: {}", backup.display()))?;
    tracing::warn!(path = %path.display(), backup = %backup.display(), "preserved invalid practice config before saving");
    Ok(())
}

fn atomic_replace(
    path: &Path,
    bytes: &[u8],
    replace: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> Result<()> {
    let (temporary, mut file) = create_sibling(path, "tmp", "")?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    // Windows needs the temporary handle closed before rename.
    drop(file);
    let result = written
        .with_context(|| format!("write practice temporary config: {}", temporary.display()))
        .and_then(|()| {
            replace(&temporary, path)
                .with_context(|| format!("replace practice config: {}", path.display()))
        });
    if result.is_err() {
        // This is the create_new file owned by this call, never the original or backup.
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (crate::bootstrap::profile_tests::ProfileTestDir, PathBuf) {
        let data = crate::bootstrap::profile_tests::ProfileTestDir::new();
        let paths = crate::paths::resolve_profile_paths(&data.paths, "default").unwrap();
        let path = super::super::practice_property_path(&paths, &[0; 32]);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        (data, path)
    }

    fn siblings(path: &Path) -> Vec<PathBuf> {
        let mut paths = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        paths.sort();
        paths
    }

    #[test]
    fn invalid_practice_bytes_are_backed_up_exactly_before_automatic_save() {
        for original in [b"null".as_slice(), b"{\"start_time_ms\":", b"[]", b"\xff\xfe\x80"] {
            let (_data, path) = fixture();
            fs::write(&path, original).unwrap();
            assert!(load(&path).is_none());
            assert_eq!(fs::read(&path).unwrap(), original);
            assert_eq!(siblings(&path), vec![path.clone()], "load must not create a backup");
            let saved = PracticeProperty { start_gauge: 77, ..Default::default() };
            save(&path, &saved).unwrap();
            assert_eq!(load(&path).unwrap(), saved);
            let backups =
                siblings(&path).into_iter().filter(|entry| entry != &path).collect::<Vec<_>>();
            assert_eq!(backups.len(), 1);
            let backup = &backups[0];
            let prefix = format!("{}.corrupt-", path.file_name().unwrap().to_string_lossy());
            let name = backup.file_name().unwrap().to_str().unwrap();
            let id = name.strip_prefix(&prefix).unwrap().strip_suffix(".bak").unwrap();
            assert!(uuid::Uuid::parse_str(id).is_ok());
            assert_eq!(fs::read(backup).unwrap(), original);
            let changed = PracticeProperty { start_gauge: 88, ..saved };
            save(&path, &changed).unwrap();
            assert_eq!(load(&path).unwrap(), changed);
            assert_eq!(siblings(&path).len(), 2, "valid subsequent save must not add backups");
            assert_eq!(fs::read(backup).unwrap(), original);
        }
    }

    #[test]
    fn missing_and_valid_practice_files_save_without_backups_or_temporary_files() {
        let (_data, path) = fixture();
        assert!(load(&path).is_none());
        assert!(siblings(&path).is_empty());
        for value in [20, 119] {
            let saved = PracticeProperty { start_gauge: value, ..Default::default() };
            save(&path, &saved).unwrap();
            assert_eq!(load(&path).unwrap(), saved);
            assert_eq!(siblings(&path), vec![path.clone()]);
        }
    }

    #[test]
    fn failed_practice_backup_keeps_original_and_does_not_start_replacement() {
        let (_data, path) = fixture();
        let original = b"\xff{truncated";
        fs::write(&path, original).unwrap();
        let replacement = serde_json::to_vec(&PracticeProperty::default()).unwrap();
        let error = save_after_preserving_invalid(&path, &replacement, |target, bytes| {
            assert_eq!(target, path);
            assert_eq!(bytes, original);
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "injected backup failure").into())
        })
        .unwrap_err();
        assert!(error.to_string().contains("injected backup failure"));
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(siblings(&path), vec![path.clone()]);
    }

    #[test]
    fn failed_atomic_practice_replace_preserves_original_and_cleans_only_its_temp() {
        let (_data, path) = fixture();
        let original =
            serde_json::to_vec(&PracticeProperty { start_gauge: 119, ..Default::default() })
                .unwrap();
        fs::write(&path, &original).unwrap();
        let unrelated = path.with_extension("json.tmp-unrelated");
        fs::write(&unrelated, b"keep").unwrap();
        let replacement = serde_json::to_vec(&PracticeProperty::default()).unwrap();
        let error = atomic_replace(&path, &replacement, |temporary, target| {
            assert_eq!(target, path);
            assert_eq!(temporary.parent(), path.parent());
            assert_eq!(fs::read(temporary).unwrap(), replacement);
            assert_eq!(fs::read(target).unwrap(), original);
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "injected replace failure"))
        })
        .unwrap_err();
        assert!(format!("{error:#}").contains("injected replace failure"));
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read(&unrelated).unwrap(), b"keep");
        let mut expected = vec![path.clone(), unrelated];
        expected.sort();
        assert_eq!(siblings(&path), expected);
    }

    #[test]
    fn unreadable_practice_path_falls_back_but_cannot_be_overwritten() {
        let (_data, path) = fixture();
        // A directory is a deterministic read failure on Windows and Unix, even
        // when tests have elevated permissions that would bypass a read-only ACL.
        fs::create_dir(&path).unwrap();
        let sentinel = path.join("keep");
        fs::write(&sentinel, b"original").unwrap();
        assert!(load(&path).is_none());
        assert!(save(&path, &PracticeProperty::default()).is_err());
        assert_eq!(fs::read(&sentinel).unwrap(), b"original");
        assert_eq!(siblings(&path), vec![path.clone()]);
    }
}
