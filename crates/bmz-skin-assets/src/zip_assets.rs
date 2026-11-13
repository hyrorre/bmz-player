//! Read-only ZIP assets. `archive.zip!skin/entry` never extracts files and does
//! not collide with the song resource registry's `archive.zip!/entry` names.
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use anyhow::{Context, Result, ensure};
use zip::ZipArchive;

const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 32_768;
type Archive = Arc<Mutex<ZipArchive<BufReader<File>>>>;
type CacheEntry = (PathBuf, Option<SystemTime>, u64, Archive);

fn archive(path: &Path) -> Result<Archive> {
    static CACHE: OnceLock<Mutex<VecDeque<CacheEntry>>> = OnceLock::new();
    let meta = fs::metadata(path)?;
    let modified = meta.modified().ok();
    let mut cache = CACHE.get_or_init(Mutex::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, _, _, archive)) =
        cache.iter().find(|(p, m, n, _)| p == path && *m == modified && *n == meta.len())
    {
        return Ok(archive.clone());
    }
    let zip = ZipArchive::new(BufReader::new(File::open(path)?))?;
    ensure!(zip.len() <= MAX_ENTRIES, "skin ZIP contains too many entries");
    let zip = Arc::new(Mutex::new(zip));
    cache.retain(|(p, _, _, _)| p != path);
    while cache.len() >= 4 {
        cache.pop_front();
    }
    cache.push_back((path.to_path_buf(), modified, meta.len(), zip.clone()));
    Ok(zip)
}

/// Some Windows archivers store `Note\N 1.png`; BMZ names entries with `/`.
fn normalized_entry(name: &str) -> String {
    name.replace('\\', "/")
}

fn validate_entry(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && !name.contains(['\\', ':', '\0'])
            && name.split('/').all(|part| !part.is_empty() && part != "." && part != ".."),
        "invalid skin ZIP entry: {name}"
    );
    Ok(())
}

pub(super) fn split_path(path: &Path) -> Result<Option<(PathBuf, String)>> {
    let text = path.to_string_lossy().replace('\\', "/");
    let Some((backing, entry)) = text.split_once("!skin/") else { return Ok(None) };
    if !backing.to_ascii_lowercase().ends_with(".zip") {
        return Ok(None);
    }
    validate_entry(entry)?;
    Ok(Some((PathBuf::from(backing), entry.to_string())))
}

/// Callers must authorize the archive's canonical backing path before listing it.
/// Names use `/`. Entries that cannot be addressed safely (traversal, absolute or
/// drive paths) are left out instead of making the whole archive unusable.
pub fn zip_entry_names(path: &Path) -> Result<Vec<String>> {
    let archive = archive(path)?;
    let zip = archive.lock().unwrap_or_else(|e| e.into_inner());
    let mut names: Vec<_> = zip
        .file_names()
        .map(normalized_entry)
        .filter(|name| !name.ends_with('/') && validate_entry(name).is_ok())
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

pub(super) fn find_entry(path: &Path, requested: &str) -> Result<String> {
    let archive = archive(path)?;
    let zip = archive.lock().unwrap_or_else(|e| e.into_inner());
    // `requested` is validated, so a traversal entry never matches it.
    let found = zip.file_names().find(|name| normalized_entry(name) == requested).or_else(|| {
        zip.file_names().filter(|name| normalized_entry(name).eq_ignore_ascii_case(requested)).min()
    });
    found.map(str::to_string).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("skin ZIP entry not found: {requested}"),
        )
        .into()
    })
}

/// `name` is the stored name returned by [`find_entry`].
pub(super) fn read_entry(path: &Path, name: &str) -> Result<Vec<u8>> {
    validate_entry(&normalized_entry(name))?;
    let archive = archive(path)?;
    let mut zip = archive.lock().unwrap_or_else(|e| e.into_inner());
    let mut entry = zip.by_name(name)?;
    ensure!(entry.size() <= MAX_ENTRY_BYTES, "skin ZIP entry is too large");
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    (&mut entry)
        .take(MAX_ENTRY_BYTES + 1)
        .read_to_end(&mut bytes)
        .context("failed to read skin ZIP entry")?;
    ensure!(bytes.len() as u64 <= MAX_ENTRY_BYTES, "skin ZIP entry exceeds size limit");
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SkinAsset, SkinPathContext};
    use std::io::Write;

    fn write_zip(path: &Path, name: &str, data: &[u8]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(data).unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn zip_assets_keep_case_names_and_refresh_changed_archives_without_extraction() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("skin.zip");
        write_zip(&path, "画像/Band!.png", b"first");
        let context = SkinPathContext::for_directory(root.path()).unwrap();
        let name = context.resolve_asset("skin.zip!skin/画像/band!.PNG").unwrap();
        let asset = SkinAsset::resolve(&name).unwrap();
        assert!(asset.is_archived());
        assert_eq!(asset.read().unwrap(), b"first");
        assert!(!root.path().join("画像").exists());
        // Different size invalidates even on filesystems with coarse timestamps.
        write_zip(&path, "画像/Band!.png", b"replacement");
        assert_eq!(asset.read().unwrap(), b"replacement");
        assert!(context.resolve_optional_asset("skin.zip!skin/missing.png").unwrap().is_none());
    }

    #[test]
    fn zip_assets_reject_unauthorized_backing_files_and_traversal() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = outside.path().join("skin.zip");
        write_zip(&path, "x.png", b"x");
        let context = SkinPathContext::for_directory(root.path()).unwrap();
        assert!(context.resolve_asset(&format!("{}!skin/x.png", path.display())).is_err());
        assert!(
            SkinAsset::resolve_with(
                &PathBuf::from(format!("{}!skin/x.png", path.display())),
                |_| false
            )
            .is_err()
        );
        for name in ["../escape.png", "/absolute.png", "a/../../x", "a\\x", "C:/x", ""] {
            assert!(validate_entry(name).is_err(), "{name}");
        }
        // An unsafe entry is unusable, but does not hide the archive's other images.
        let mut zip = zip::ZipWriter::new(File::create(root.path().join("bad.zip")).unwrap());
        for name in ["../escape.png", "..\\up.png", "C:/drive.png", "ok.png"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(b"x").unwrap();
        }
        zip.finish().unwrap();
        assert_eq!(zip_entry_names(&root.path().join("bad.zip")).unwrap(), ["ok.png"]);
        let context = SkinPathContext::for_directory(root.path()).unwrap();
        assert!(context.resolve_asset("bad.zip!skin/../escape.png").is_err());
        assert!(context.resolve_asset("bad.zip!skin/..\\up.png").is_err());
    }

    #[test]
    fn zip_assets_read_backslash_separated_entries() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("skin.zip");
        write_zip(&path, "Note\\N 1.png", b"note");
        assert_eq!(zip_entry_names(&path).unwrap(), ["Note/N 1.png"]);
        let context = SkinPathContext::for_directory(root.path()).unwrap();
        let name = context.resolve_asset("skin.zip!skin/note/n 1.png").unwrap();
        assert_eq!(SkinAsset::resolve(&name).unwrap().read().unwrap(), b"note");
    }
}
